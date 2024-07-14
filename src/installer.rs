use std::collections::HashMap;
use std::env::Args;
use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use async_trait::async_trait;
use bytes::Bytes;
use reqwest::Client;
use semver::Comparator;
use futures::future::BoxFuture;
use futures::FutureExt;
use crate::cache::{Cache, CACHE_DIR};
use crate::command_handler::CommandHandler;
use crate::constants::LATEST;
use crate::errors::{CommandError, ParseError};
use crate::errors::CommandError::{FailedToCreateFile, FailedToGetTarball};
use crate::types::{DependencyMap, PackageLock, VersionData};
use crate::versions::Versions;
use crate::http::HttpRequest;

#[derive(Default)]
pub struct Installer {
    package_name: String,
    package_version: Option<Comparator>
}

static ACTIVE_TASKS: AtomicUsize = AtomicUsize::new(0);

fn increment_active_tasks() {
    ACTIVE_TASKS.fetch_add(1, Ordering::SeqCst);
}

fn decrement_active_tasks() {
    ACTIVE_TASKS.fetch_sub(1, Ordering::SeqCst);
}

fn get_active_tasks() -> usize {
    ACTIVE_TASKS.load(Ordering::SeqCst)
}

type PackageBytes = (String, Bytes);
type DependencyMapMutex = Arc<Mutex<DependencyMap>>;

impl Installer {
    async fn get_version_data(client: Client, package_name: &String, full_version: Option<&String>, version: Option<&Comparator>) -> Result<VersionData, CommandError> {
        if let Some(v) = full_version {
            return HttpRequest::version_data(client.clone(), package_name, &v).await;
        }

        let mut package_data = HttpRequest::package_data(client.clone(), package_name).await?;
        let package_version = Versions::resolve_partial_version(version, &package_data.versions)?;

        Ok(package_data.versions.remove(&package_version).expect("Failed to find resolved package version in package data"))
    }

    fn already_resolved(name: &String, version: &String, is_latest: bool, dependency_map_mx: DependencyMapMutex) -> bool {
        let mut dependency_map = dependency_map_mx.lock().unwrap();
        let str_v = Versions::stringify(name, version);
        let installed_version = dependency_map.get(&str_v);

        match installed_version {
            Some(_) => true,
            None => {
                dependency_map.insert(str_v, PackageLock::new(is_latest));
                false
            }
        }
    }

    fn append_version(parent_version_name: String, new_version_name: String, dependency_map_mx: DependencyMapMutex) -> Result<(), CommandError> {
        let mut dependency_map = dependency_map_mx.lock().unwrap();
        let parent_versions = dependency_map.entry(parent_version_name.to_string()).or_insert(PackageLock::new(parent_version_name.ends_with(LATEST)));

        parent_versions.dependencies.push(new_version_name);

        Ok(())
    }

    fn extract_tarball(bytes: Bytes, destination: String) -> Result<(), CommandError> {
        let bytes = &bytes.to_vec()[..];
        let gz = flate2::read::GzDecoder::new(bytes);
        let mut archive = tar::Archive::new(gz);

        archive.unpack(&destination).map_err(CommandError::ExtractionFailed)
    }

    fn install_package(client: Client, version_data: VersionData, is_latest: bool, sender: Sender<PackageBytes>, dependency_map_mx: DependencyMapMutex) -> BoxFuture<'static, Result<(), CommandError>> {
        async move {
            println!("Installing '{}@{}' ...", version_data.name, version_data.version);

            if Self::already_resolved(&version_data.name, &version_data.version, is_latest, Arc::clone(&dependency_map_mx)) {
                println!("Package already installed");
                return Ok(());
            }

            let package_dest = format!("{}/{}", *CACHE_DIR, Versions::stringify(&version_data.name, &version_data.version));
            let tarball_url = version_data.dist.tarball.clone();
            let stringified_parent = Versions::stringify(&version_data.name, &version_data.version);

            increment_active_tasks();

            let bytes = HttpRequest::get_bytes(client.clone(), tarball_url)
                .await
                .map_err(|_| FailedToGetTarball)?;

            sender.send((package_dest, bytes)).expect("Failed to send package to installation thread");

            let dependencies = version_data.dependencies.unwrap_or(HashMap::new());

            for (name, version) in dependencies {
                println!("Installing dependency '{}@{}' ...", name, version);
                let v = Versions::parse_semantic_version(&version).expect("Failed to parse semantic version");
                let v_ref = Some(&v);

                let full_version = Versions::resolve_full_version(v_ref);
                let full_version_ref = full_version.as_ref();
                let is_cached = Cache::exists(&name, full_version_ref, v_ref).await.unwrap();

                if is_cached {
                    println!("Dependency '{}' already installed", name);
                    continue;
                }

                let version_data = Self::get_version_data(client.clone(), &name, full_version_ref, v_ref).await.unwrap();
                let stringified_version = Versions::stringify(&name, &version);

                Self::append_version(stringified_parent.clone(), stringified_version, Arc::clone(&dependency_map_mx)).unwrap();
                Self::install_package(client.clone(), version_data, Versions::is_latest(full_version), sender.clone(), Arc::clone(&dependency_map_mx)).await?;
            }

            decrement_active_tasks();
            Ok(())
        }.boxed()
    }
}

#[async_trait]
impl CommandHandler for Installer {
    fn parse(&mut self, args: &mut Args) -> Result<(), ParseError> {
        let package = args
            .next()
            .ok_or(ParseError::MissingArgument(String::from("package_name")))?;

        let (package_name, package_version) = Versions::parse_semantic_package_details(package)?;
        self.package_name = package_name;
        self.package_version = package_version;

        Ok(())
    }

    async fn execute(&mut self) -> Result<(), CommandError> {
        let client = Client::new();
        let now = std::time::Instant::now();

        let semantic_version_ref = self.package_version.as_ref();
        let full_version = Versions::resolve_full_version(semantic_version_ref);
        let full_version_ref = full_version.as_ref();
        let is_cached = Cache::exists(&self.package_name, full_version_ref, semantic_version_ref).await?;

        if is_cached {
            println!("Package already installed");
            return Ok(());
        }

        let version_data = Self::get_version_data(client.clone(), &self.package_name, full_version_ref, self.package_version.as_ref()).await?;
        let (sender, receiver) = channel::<PackageBytes>();

        tokio::task::spawn_blocking(move || {
            increment_active_tasks();

            while let Ok((package_dest, bytes)) = receiver.recv() {
                Installer::extract_tarball(bytes, package_dest).unwrap();
            }

            decrement_active_tasks();
        });

        let dependency_map_mx: DependencyMapMutex = Arc::new(Mutex::new(HashMap::new()));
        Self::install_package(client, version_data, Versions::is_latest(full_version), sender, Arc::clone(&dependency_map_mx)).await?;

        while get_active_tasks() > 0 {}

        let dependency_map = dependency_map_mx.lock().unwrap();
        for (package, package_lock) in dependency_map.iter() {
            let prefix = format!("{}/{}/package", *CACHE_DIR, package);
            std::fs::create_dir_all(&prefix).map_err(FailedToCreateFile)?;
            let mut file = File::create(format!("{}/pie-lock.json", prefix)).map_err(FailedToCreateFile)?;

            let package_lock = serde_json::to_string(package_lock).map_err(CommandError::FailedToSerializePackageLock)?;
            file.write_all(package_lock.as_bytes()).map_err(FailedToCreateFile)?;
        }

        let elapsed = now.elapsed();
        println!("Finished in {} ms", elapsed.as_millis());

        Ok(())
    }
}
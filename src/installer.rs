use std::collections::HashMap;
use std::env::Args;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{channel, Sender};
use async_trait::async_trait;
use bytes::Bytes;
use reqwest::Client;
use semver::{Comparator};

use crate::command_handler::CommandHandler;
use crate::errors::{CommandError, ParseError};
use crate::types::VersionData;
use crate::versions::Versions;
use crate::http::HttpRequest;

#[derive(Default)]
pub struct Installer {
    package_name: String,
    package_version: Option<Comparator>
}

static ACTIVE_TASKS: AtomicUsize = AtomicUsize::new(0);

type PackageBytes = (String, Bytes);
type InstalledVersionsMutex = Arc<Mutex<HashMap<String, String>>>;

impl Installer {
    async fn get_version_data(client: Client, package_name: &String, version: Option<&Comparator>) -> Result<VersionData, CommandError> {
        if let Some(v) = Versions::resolve_full_version(version) {
            return HttpRequest::version_data(client.clone(), package_name, &v).await;
        }

        let mut package_data = HttpRequest::package_data(client.clone(), package_name).await?;
        let package_version = Versions::resolve_partial_version(version, &package_data.versions)?;

        Ok(package_data.versions.remove(&package_version).expect("Failed to find resolved package version in package data"))
    }

    fn already_resolved(name: &String, version: &String, installed_dependencies_mx: InstalledVersionsMutex) -> bool {
        let mut installed_dependencies = installed_dependencies_mx.lock().unwrap();
        let installed_version = installed_dependencies.get(name);

        match installed_version {
            Some(v) => v == version,
            None => {
                installed_dependencies.insert(name.to_string(), version.to_string());
                false
            }
        }
    }

    fn extract_tarball(bytes: Bytes, destination: String) -> Result<(), CommandError> {
        let bytes = &bytes.to_vec()[..];
        let gz = flate2::read::GzDecoder::new(bytes);
        let mut archive = tar::Archive::new(gz);

        archive.unpack(&destination).map_err(CommandError::ExtractionFailed)
    }

    fn install_package(client: Client, version_data: VersionData, sender: Sender<PackageBytes>,installed_dependencies_mx: InstalledVersionsMutex) -> Result<(), CommandError> {
        if Self::already_resolved(&version_data.name, &version_data.version, Arc::clone(&installed_dependencies_mx)) {
            return Ok(());
        }

        let cache_dir = dirs::cache_dir().expect("Could not find cache directory");
        let package_dest = format!("{}/node-cache/{}@{}",
                                   cache_dir.to_str().expect("Couldn't convert PathBuf to &str"),
                                   version_data.name,
                                   version_data.version);

        let tarball_url = version_data.dist.tarball.clone();

        tokio::spawn(async move {
            ACTIVE_TASKS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            let bytes = HttpRequest::get_bytes(client.clone(), tarball_url)
                .await
                .unwrap();

            sender.send((package_dest, bytes)).unwrap();

            let dependencies = version_data.dependencies.unwrap_or(HashMap::new());

            for (name, version) in dependencies {
                let v = Versions::parse_semantic_version(&version).expect("Failed to parse semantic version");
                let version_data = Self::get_version_data(client.clone(), &name, Some(&v)).await.unwrap();

                Self::install_package(client.clone(), version_data, sender.clone(), Arc::clone(&installed_dependencies_mx)).unwrap();
            }

            ACTIVE_TASKS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        });

        Ok(())
    }
}

#[async_trait]
impl CommandHandler for Installer {
    fn parse(&mut self, args: &mut Args) -> Result<(), ParseError> {
        let package = args
            .next()
            .ok_or(ParseError::MissingArgument(String::from("package_name")))?;

        let (package_name, package_version) = Versions::parse_package(package)?;
        self.package_name = package_name;
        self.package_version = package_version;

        Ok(())
    }

    async fn execute(&mut self) -> Result<(), CommandError> {
        println!("Installing '{}' ...", self.package_name);

        let client = Client::new();
        let now = std::time::Instant::now();

        let version_data = Self::get_version_data(client.clone(), &self.package_name, self.package_version.as_ref()).await?;
        let (sender, receiver) = channel::<PackageBytes>();

        tokio::task::spawn_blocking(move || {
            ACTIVE_TASKS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            while let Ok((package_dest, bytes)) = receiver.recv() {
                Self::extract_tarball(bytes, package_dest).unwrap();
            }

            ACTIVE_TASKS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        });

        let installed_dependencies_mx: InstalledVersionsMutex = Arc::new(Mutex::new(HashMap::new()));
        Self::install_package(client, version_data, sender, installed_dependencies_mx)?;

        while ACTIVE_TASKS.load(std::sync::atomic::Ordering::SeqCst) > 0 {}

        let elapsed = now.elapsed();
        println!("Finished in {} ms", elapsed.as_millis());

        Ok(())
    }
}
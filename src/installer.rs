use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;
use bytes::Bytes;
use reqwest::Client;
use semver::Comparator;
use crate::cache::{Cache, CACHE_DIR};
use crate::utils::{LATEST, TaskAllocator};
use crate::errors::{CommandError};
use crate::types::{DependencyMap, PackageLock, VersionData};
use crate::versions::Versions;
use crate::http::HttpRequest;

pub type PackageBytes = (String, Bytes);
pub type DependencyMapMutex = Arc<Mutex<DependencyMap>>;

#[derive(Clone)]
pub struct InstallContext {
    pub client: Client,
    pub sender: Sender<PackageBytes>,
    pub dependency_map_mx: DependencyMapMutex,
}

pub struct PackageInfo {
    pub version_data: VersionData,
    pub is_latest: bool,
    pub stringified: String,
}

pub struct Installer;
impl Installer {
    pub async fn get_version_data(client: Client, package_name: &String, full_version: Option<&String>, version: Option<&Comparator>) -> Result<VersionData, CommandError> {
        if let Some(v) = full_version {
            return HttpRequest::version_data(client.clone(), package_name, v).await;
        }

        let mut package_data = HttpRequest::package_data(client.clone(), package_name).await?;
        let package_version = Versions::resolve_partial_version(version, &package_data.versions)?;

        Ok(package_data.versions.remove(&package_version).expect("Failed to find resolved package version in package data"))
    }

    fn already_resolved(context: &InstallContext, package_info: &PackageInfo) -> bool {
        let mut dependency_map = context.dependency_map_mx.lock().unwrap();
        let stringified = Versions::stringify(&package_info.version_data.name, &package_info.version_data.version);

        let installed_versions = dependency_map.get(&stringified);

        match installed_versions {
            Some(_) => true,
            None => {
                dependency_map.insert(stringified, PackageLock::new(package_info.is_latest));
                false
            }
        }
    }

    fn append_version(parent_version_name: &String, new_version_name: String, dependency_map_mx: DependencyMapMutex) -> Result<(), CommandError> {
        let mut dependency_map = dependency_map_mx.lock().unwrap();
        let parent_versions = dependency_map.entry(parent_version_name.to_string()).or_insert(PackageLock::new(parent_version_name.ends_with(LATEST)));

        parent_versions.dependencies.push(new_version_name);

        Ok(())
    }

    pub fn install_package(context: InstallContext, package_info: PackageInfo) -> Result<(), CommandError> {
        if Self::already_resolved(&context, &package_info) {
            println!("Package '{}' already resolved", package_info.stringified);
            return Ok(());
        }

        println!("Launching task to download package '{}'", package_info.stringified);
        TaskAllocator::add_task(async move {
            println!("Downloading package '{}'", package_info.stringified);
            let version_data = package_info.version_data;
            let package_bytes = HttpRequest::get_bytes(context.client.clone(), version_data.dist.tarball).await.unwrap();
            println!("Downloaded package '{}'", package_info.stringified);

            println!("Sending package '{}' to extraction task", package_info.stringified);
            let package_destination = format!("{}/{}", *CACHE_DIR, package_info.stringified);
            context.sender.send((package_destination, package_bytes)).unwrap();

            let dependencies = version_data.dependencies.unwrap_or(HashMap::new());

            println!("Installing dependencies for '{}'", package_info.stringified);
            Self::install_dependencies(&package_info.stringified, context, dependencies).await;
        });

        Ok(())
    }

    async fn install_dependencies(parent: &String, context: InstallContext, dependencies: HashMap<String, String>) {
        for (name, version) in dependencies {
            let c = Versions::parse_semantic_version(&version).unwrap();
            let comparator_ref = Some(&c);

            let full_version = Versions::resolve_full_version(comparator_ref);
            let full_version_ref = full_version.as_ref();

            let (is_cached, cached_version) = Cache::exists(&name, full_version_ref, comparator_ref).await.unwrap();

            if is_cached {
                let version = full_version.or(cached_version).expect("Failed to get version of cached package");
                let stringified = Versions::stringify(&name, &version);
                Cache::load_cached_version(stringified);
                continue;
            }

            let version_data = Self::get_version_data(context.client.clone(), &name, full_version_ref, comparator_ref).await.unwrap();
            let stringified = Versions::stringify(&name, &version_data.version);
            Self::append_version(parent, stringified.to_string(), Arc::clone(&context.dependency_map_mx)).unwrap();

            let package_info = PackageInfo {
                version_data,
                is_latest: Versions::is_latest(Some(&stringified)),
                stringified,
            };

            Self::install_package(context.clone(), package_info).unwrap();
        }
    }
}
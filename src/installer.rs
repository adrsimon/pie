use crate::cache::{Cache, CACHE_DIR};
use crate::errors::CommandError;
use crate::http::HttpRequest;
use crate::types::{DependencyMap, PackageLock, VersionData};
use crate::utils::{TaskAllocator, LATEST};
use crate::versions::Versions;
use bytes::Bytes;
use reqwest::Client;
use semver::Comparator;
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

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
    pub async fn get_version_data(
        client: Client,
        package_name: &String,
        full_version: Option<&String>,
        version: Option<&Comparator>,
    ) -> Result<VersionData, CommandError> {
        if let Some(v) = full_version {
            return HttpRequest::version_data(client.clone(), package_name, v).await;
        }

        let mut package_data = HttpRequest::package_data(client.clone(), package_name).await?;
        let package_version = Versions::resolve_partial_version(version, &package_data.versions)?;

        Ok(package_data
            .versions
            .remove(&package_version)
            .expect("Failed to find resolved package version in package data"))
    }

    fn already_resolved(context: &InstallContext, package_info: &PackageInfo) -> bool {
        let mut dependency_map = context.dependency_map_mx.lock().unwrap();
        let stringified = Versions::stringify(
            &package_info.version_data.name,
            &package_info.version_data.version,
        );

        let installed_versions = dependency_map.get(&stringified);

        match installed_versions {
            Some(_) => true,
            None => {
                dependency_map.insert(stringified, PackageLock::new(package_info.is_latest));
                false
            }
        }
    }

    fn append_version(
        parents_mux: Arc<Mutex<Vec<String>>>,
        new_version_name: String,
        dependency_map_mx: DependencyMapMutex,
    ) -> Result<(), CommandError> {
        let mut dependency_map = dependency_map_mx.lock().unwrap();
        let parents = parents_mux.lock().unwrap();

        for parent in parents.iter() {
            let parent_version = dependency_map
                .entry(parent.to_string())
                .or_insert(PackageLock::new(parent.ends_with(LATEST)));
            parent_version
                .dependencies
                .push(new_version_name.to_string());
        }

        Ok(())
    }

    pub fn install_package(
        context: InstallContext,
        package_info: PackageInfo,
        parents_mux: Arc<Mutex<Vec<String>>>,
    ) -> Result<(), CommandError> {
        if Self::already_resolved(&context, &package_info) {
            println!("Package '{}' already resolved", package_info.stringified);
            return Ok(());
        }

        Self::append_version(
            Arc::clone(&parents_mux),
            package_info.stringified.to_string(),
            Arc::clone(&context.dependency_map_mx),
        )
        .unwrap();
        {
            let mut parents = parents_mux.lock().unwrap();
            parents.push(package_info.stringified.to_string());
        }

        println!(
            "Launching task to download package '{}'",
            package_info.stringified
        );
        TaskAllocator::add_task(async move {
            println!("Downloading package '{}'", package_info.stringified);
            let version_data = package_info.version_data;
            let package_bytes =
                HttpRequest::get_bytes(context.client.clone(), version_data.dist.tarball)
                    .await
                    .unwrap();
            println!("Downloaded package '{}'", package_info.stringified);

            println!(
                "Sending package '{}' to extraction task",
                package_info.stringified
            );
            let package_destination = format!("{}/{}", *CACHE_DIR, package_info.stringified);
            context
                .sender
                .send((package_destination, package_bytes))
                .unwrap();

            let dependencies = version_data.dependencies.unwrap_or(HashMap::new());

            println!("Installing dependencies for '{}'", package_info.stringified);
            Self::install_dependencies(parents_mux, context, dependencies).await;
        });

        Ok(())
    }

    async fn install_dependencies(
        parents_mux: Arc<Mutex<Vec<String>>>,
        context: InstallContext,
        dependencies: HashMap<String, String>,
    ) {
        for (name, version) in dependencies {
            let c = Versions::parse_semantic_version(&version).unwrap();
            let comparator = Some(&c);

            let full_version = Versions::resolve_full_version(comparator);
            let full_version = full_version.as_ref();

            let (is_cached, cached_version) = Cache::exists(&name, full_version, comparator)
                .await
                .unwrap();

            if is_cached {
                let version = cached_version.expect("Failed to get cached version");
                let stringified = Versions::stringify(&name, &version);

                let dependency_map = context.dependency_map_mx.lock().unwrap();
                if dependency_map.get(stringified.as_str()).is_none() {
                    Cache::load_cached_version(stringified);
                    continue;
                }
            }

            let version_data =
                Self::get_version_data(context.client.clone(), &name, full_version, comparator)
                    .await
                    .unwrap();
            let stringified = Versions::stringify(&name, &version_data.version);

            let package_info = PackageInfo {
                version_data,
                is_latest: Versions::is_latest(Some(&stringified)),
                stringified,
            };

            Self::install_package(context.clone(), package_info, Arc::clone(&parents_mux)).unwrap();
        }
    }
}

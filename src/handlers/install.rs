use crate::cache::{Cache, CACHE_DIR};
use crate::command_handler::CommandHandler;
use crate::errors::{CommandError, ParseError};
use crate::installer::{DependencyMapMutex, InstallContext, Installer, PackageBytes, PackageInfo};
use crate::utils;
use crate::utils::TaskAllocator;
use crate::versions::Versions;
use async_trait::async_trait;
use reqwest::Client;
use semver::Comparator;
use std::collections::HashMap;
use std::env::Args;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct InstallHandler {
    package_name: String,
    package_version: Option<Comparator>,
}

impl InstallHandler {
    pub fn write_lockfiles(dependency_map_mx: DependencyMapMutex) -> Result<(), CommandError> {
        let dependency_map = dependency_map_mx.lock().unwrap();

        for (package_name, lock) in dependency_map.iter() {
            let path = format!("{}/{}/package/", *CACHE_DIR, package_name);
            fs::create_dir_all(path.clone()).map_err(CommandError::FailedToCreateDir)?;
            let mut file = File::create(format!("{path}/pie-lock.json"))
                .map_err(CommandError::FailedToCreateFile)?;
            let lock =
                serde_json::to_string(lock).map_err(CommandError::FailedToSerializePackageLock)?;
            file.write_all(lock.as_bytes())
                .map_err(CommandError::FailedToWriteFile)?;
        }

        Ok(())
    }
}

#[async_trait]
impl CommandHandler for InstallHandler {
    fn parse(&mut self, args: &mut Args) -> Result<(), ParseError> {
        let package = args
            .next()
            .ok_or(ParseError::MissingArgument(String::from("package_name")))?;

        let (package_name, package_version) = Versions::parse_semantic_package_details(package)?;
        self.package_name = package_name;
        self.package_version = package_version;

        Ok(())
    }

    async fn execute(&self) -> Result<(), CommandError> {
        println!("Installing '{}' ...", self.package_name);
        let client = Client::new();

        let semantic_version_ref = self.package_version.as_ref();
        let full_version = Versions::resolve_full_version(semantic_version_ref);
        let full_version_ref = full_version.as_ref();
        let (is_cached, cached_version) =
            Cache::exists(&self.package_name, full_version_ref, semantic_version_ref).await?;

        utils::create_node_modules_dir();

        if is_cached {
            let version = cached_version.expect("Failed to get cached version");
            Cache::load_cached_version(Versions::stringify(&self.package_name, &version));
            return Ok(());
        }

        let version_data = Installer::get_version_data(
            client.clone(),
            &self.package_name,
            full_version_ref,
            semantic_version_ref,
        )
        .await?;

        let (sender, receiver) = channel::<PackageBytes>();

        // TODO: find a better way to handle this
        // forced to use this to make sure that at least one task is received
        // if not, the program might exit before the task is received
        // which ends up in caching a package without the actual code
        let task_received = Arc::new(AtomicBool::new(false));
        TaskAllocator::add_blocking_task(move || {
            let task_received = Arc::clone(&task_received);
            println!("Starting extraction task...");
            while !task_received.load(std::sync::atomic::Ordering::Relaxed) {
                while let Ok((package_dest, bytes)) = receiver.recv() {
                    task_received.store(true, std::sync::atomic::Ordering::Relaxed);
                    println!("Extracting package to '{}'", package_dest);
                    utils::extract_tarball(bytes, package_dest).unwrap()
                }
            }
        });

        let dependency_map_mutex = Arc::new(Mutex::new(HashMap::new()));

        let install_context = InstallContext {
            client,
            sender,
            dependency_map_mx: Arc::clone(&dependency_map_mutex),
        };

        let stringified = Versions::stringify(&version_data.name, &version_data.version);
        let package_info = PackageInfo {
            version_data,
            is_latest: Versions::is_latest(full_version_ref),
            stringified: stringified.clone(),
        };

        println!("Installing the package");
        Installer::install_package(
            install_context,
            package_info,
            Arc::new(Mutex::new(Vec::new())),
        )?;
        TaskAllocator::block_until_done();
        println!("All tasks are done!");

        println!("Writing lockfiles...");
        Self::write_lockfiles(dependency_map_mutex)?;
        Cache::load_cached_version(stringified);

        println!("Package '{}' installed successfully!", self.package_name);
        Ok(())
    }
}

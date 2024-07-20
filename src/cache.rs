use crate::errors::CommandError;
use crate::utils::{EMPTY_VERSION, LATEST};
use crate::versions::Versions;
use lazy_static::lazy_static;
use semver::{Comparator, Version};
use std::collections::HashMap;
use std::fs::{self as fs_sync, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::str::FromStr;
use std::string::String;
use tokio::fs;

lazy_static! {
    pub static ref CACHE_DIR: String = format!(
        "{}/pie",
        dirs::cache_dir()
            .expect("Could not find cache directory")
            .to_str()
            .expect("Couldn't convert cache directory path to string")
    );
    pub static ref CACHED_VERSIONS: CachedVersions = Cache::get_cached_versions();
}

pub struct CachedVersion {
    pub version: String,
    pub is_latest: bool,
}

pub type CachedVersions = HashMap<String, CachedVersion>;

pub struct Cache;
impl Cache {
    pub async fn exists(
        package_name: &String,
        version: Option<&String>,
        sem_ver: Option<&Comparator>,
    ) -> Result<(bool, Option<String>), CommandError> {
        if let Some(version) = version {
            if version == LATEST {
                let latest_version = Self::get_latest_version_in_cache(package_name);
                return Ok((latest_version.is_some(), latest_version));
            }

            return Ok((
                Self::is_in_cache(package_name, &version),
                Some(version.to_string()),
            ));
        }

        println!("{}", CACHE_DIR.to_string());
        let mut cache_entries = fs::read_dir(CACHE_DIR.to_string())
            .await
            .map_err(CommandError::NoCacheDirectory)?;
        let sem_ver = sem_ver.expect("Failed to get semver");

        while let Some(cache_entry) = cache_entries
            .next_entry()
            .await
            .map_err(CommandError::FailedDirectoryEntry)
            .unwrap()
        {
            let filename = cache_entry.file_name().to_string_lossy().to_string();

            if !filename.starts_with(package_name) {
                continue;
            }

            let (_, entry_version) = Versions::parse_raw_package_details(filename);
            let version = &Version::from_str(entry_version.as_str()).unwrap_or(EMPTY_VERSION);

            if sem_ver.matches(version) {
                return Ok((true, Some(entry_version)));
            }
        }

        Ok((false, None))
    }

    pub fn get_cached_versions() -> CachedVersions {
        let dir = fs_sync::read_dir(CACHE_DIR.to_string()).expect("Failed to read cache directory");
        let mut cached_versions = HashMap::new();

        for entry in dir {
            let entry = entry.expect("Failed to get cache entry");
            let filename = entry.file_name().to_string_lossy().to_string();

            let mut lock = File::open(format!("{}/{}/package/pie-lock.json", *CACHE_DIR, filename))
                .expect("Failed to open lock file");

            let start_byte = 12;
            let end_byte = 15;

            let mut buf = vec![0; end_byte - start_byte + 1];
            lock.seek(SeekFrom::Start(start_byte as u64)).unwrap();
            lock.read_exact(&mut buf).unwrap();

            let is_latest = String::from_utf8(buf).unwrap() == "true";

            let (name, version) = Versions::parse_raw_package_details(filename);
            cached_versions.insert(name, CachedVersion { version, is_latest });
        }

        cached_versions
    }

    pub fn get_latest_version_in_cache(package_name: &String) -> Option<String> {
        let versions = CACHED_VERSIONS.get(package_name);
        match versions {
            Some(v) if v.is_latest => Some(v.version.clone()),
            _ => None,
        }
    }

    pub fn is_in_cache(package: &String, version: &String) -> bool {
        let cached_version = CACHED_VERSIONS.get(package);
        match cached_version {
            Some(v) if &v.version == version => true,
            _ => false,
        }
    }

    pub fn load_cached_version(package: String) {
        let raw =
            fs_sync::read_to_string(format!("{}/{}/package/pie-lock.json", *CACHE_DIR, package))
                .expect("Failed to read lock file");
        let lock = serde_json::from_str::<PackageLock>(raw.as_str()).unwrap();

        let mut dependencies = lock.dependencies;
        dependencies.push(package);

        for d in dependencies {
            let (name, _) = Versions::parse_raw_package_details(d.to_string());

            let link = symlink::symlink_dir(
                format!("{}/{}/package", *CACHE_DIR, d),
                format!("./node_modules/{}", name),
            );

            match link {
                Ok(_) => continue,
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("Failed to create symlink: {}", e),
            }
        }
    }
}

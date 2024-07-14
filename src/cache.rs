use std::collections::HashMap;
use std::fs::{self as fs_sync, File};
use std::io::{Read, Seek, SeekFrom};
use std::str::FromStr;
use std::string::String;
use tokio::fs;
use lazy_static::lazy_static;
use semver::{Comparator, Version};
use crate::constants::{EMPTY_VERSION, LATEST};
use crate::errors::CommandError;
use crate::versions::Versions;

lazy_static! {
    pub static ref CACHE_DIR: String = format!(
        "{}/pie",
        dirs::cache_dir()
            .expect("Could not find cache directory")
            .to_str()
            .expect("Couldn't convert cache directory path to string")
    );

    pub static ref CACHED_VERSIONS: HashMap<String, bool> = Cache::get_cached_versions();
}

pub struct Cache;
impl Cache {
    pub async fn exists(package_name: &String, version: Option<&String>, sem_ver: Option<&Comparator>) -> Result<bool, CommandError> {
        if let Some(v) = version {
            if v == LATEST {
                return Ok(Self::latest_is_cached(package_name))
            }

            let str_version = Versions::stringify(&package_name, &v);
            return Ok(
                fs::metadata(format!("{}/{}", &*CACHE_DIR, str_version)).await.is_ok()
            )
        }

        println!("{}", CACHE_DIR.to_string());
        let mut cache_entries = fs::read_dir(CACHE_DIR.to_string()).await.map_err(CommandError::NoCacheDirectory)?;
        let sem_ver = sem_ver.expect("Failed to get semver");

        while let Some(cache_entry) = cache_entries.next_entry().await.map_err(CommandError::FailedDirectoryEntry).unwrap() {
            let filename = cache_entry.file_name().to_string_lossy().to_string();

            if !filename.starts_with(package_name) {
                continue;
            }

            let (_, entry_version) = Versions::parse_raw_package_details(filename);
            let version = &Version::from_str(entry_version.as_str()).unwrap_or(EMPTY_VERSION);

            if sem_ver.matches(version) {
                return Ok(true)
            }
        }

        Ok(false)
    }

    pub fn get_cached_versions() -> HashMap<String, bool> {
        let dir = fs_sync::read_dir(CACHE_DIR.to_string()).expect("Failed to read cache directory");
        let mut cached_versions = HashMap::new();

        for entry in dir {
            let entry = entry.expect("Failed to get cache entry");
            let filename = entry.file_name().to_string_lossy().to_string();

            let mut lock = File::open(format!("{}/{}/package/pie-lock.json", *CACHE_DIR, filename)).expect("Failed to open lock file");

            let start_byte = 12;
            let end_byte = 15;

            let mut buf = vec![0; end_byte - start_byte + 1];
            lock.seek(SeekFrom::Start(start_byte as u64)).unwrap();
            lock.read_exact(&mut buf).unwrap();

            let is_latest = String::from_utf8(buf).unwrap() == "true";
            cached_versions.insert(filename, is_latest);
        }

        cached_versions
    }

    pub fn latest_is_cached(package_name: &String) -> bool {
        CACHED_VERSIONS.get(package_name).unwrap_or(&false).clone()
    }
}
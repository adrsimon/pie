use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct PackageData {
    pub versions: HashMap<String, VersionData>,
}

#[derive(Debug, Deserialize)]
pub struct VersionData {
    pub name: String,
    pub version: String,
    pub dependencies: Option<HashMap<String, String>>,
    pub dist: Dist,
}

#[derive(Debug, Deserialize)]
pub struct Dist {
    pub tarball: String,
}

#[derive(Serialize, Deserialize)]
pub struct PackageLock {
    #[serde(rename = "isLatest")]
    pub is_latest: bool,
    pub dependencies: Vec<String>,
}

impl PackageLock {
    pub fn new(is_latest: bool) -> Self {
        Self {
            is_latest,
            dependencies: Vec::new(),
        }
    }
}

pub type DependencyMap = HashMap<String, PackageLock>;

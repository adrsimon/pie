use std::collections::HashMap;
use serde::Deserialize;

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
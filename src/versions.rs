use std::collections::HashMap;
use std::str::FromStr;
use semver::{BuildMetadata, Comparator, Op, Prerelease, Version, VersionReq};
use crate::errors::{CommandError, ParseError};
use crate::types::VersionData;

const EMPTY_VERSION: Version = Version {
    major: 0,
    minor: 0,
    patch: 0,
    pre: Prerelease::EMPTY,
    build: BuildMetadata::EMPTY,
};

type PackageDetails = (String, Option<Comparator>);

pub struct Versions;
impl Versions {
    pub fn parse_semantic_version(raw_version: &String) -> Result<Comparator, ParseError> {
        let mut version =
            VersionReq::parse(raw_version).map_err(ParseError::InvalidVersionNotation)?;
        Ok(version.comparators.remove(0))
    }

    pub fn parse_package(package: String) -> Result<PackageDetails, ParseError> {
        let mut sp = package.split("@");

        let name = match sp.next() {
            Some(n) => n.to_string(),
            None => return Err(ParseError::MissingArgument("package_name".to_string())),
        };

        let raw = match sp.next() {
            Some(v) if v == "latest" => return Ok((name, None)),
            Some(v) => v,
            None => return Ok((name, None))
        };

        let c = Self::parse_semantic_version(&raw.to_string())?;

        Ok((name, Some(c)))
    }

    pub fn resolve_full_version(semantic_version: Option<&Comparator>) -> Option<String> {
        let latest = String::from("latest");

        let semantic_version = match semantic_version {
            Some(semantic_version) => semantic_version,
            None => return Some(latest),
        };

        let (minor, patch) = match (semantic_version.minor, semantic_version.patch) {
            (Some(minor), Some(patch)) => (minor, patch),
            _ => return None,
        };

        match semantic_version.op {
            Op::Greater | Op::GreaterEq | Op::Wildcard => Some(latest), Op::Exact | Op::LessEq | Op::Tilde | Op::Caret => {
                Some(Self::string(semantic_version.major, minor, patch))
            }
            _ => None,
        }
    }

    pub fn resolve_partial_version(semantic_version: Option<&Comparator>, available_versions: &HashMap<String, VersionData>, ) -> Result<String, CommandError> {
        let semantic_version = semantic_version
            .expect("Function should not be called as the version can be resolved to 'latest'");

        let mut versions = available_versions.iter().collect::<Vec<_>>();

        Self::sort(&mut versions);

        if semantic_version.op == Op::Less {
            if let (Some(minor), Some(patch)) = (semantic_version.minor, semantic_version.patch) {
                let version_pos = versions
                    .iter()
                    .position(|(v, _)| {
                        v == &&Self::string(semantic_version.major, minor, patch)
                    })
                    .ok_or(CommandError::InvalidVersion)?;

                return Ok(versions.get(version_pos - 1).expect("No previous version found").0.to_string());
            }
        }

        for (version, _) in versions.iter().rev() {
            let version = Version::from_str(version.as_str()).unwrap_or(EMPTY_VERSION);

            if semantic_version.matches(&version) {
                return Ok(version.to_string());
            }
        }

        Err(CommandError::InvalidVersion)
    }

    fn sort(versions: &mut Vec<(&String, &VersionData)>) {
        versions.sort_by(|a, b| {
            let a = Version::parse(a.0).expect("Failed to parse version");
            let b = Version::parse(b.0).expect("Failed to parse version");

            a.cmp(&b)
        });
    }

    fn string(major: u64, minor: u64, patch: u64) -> String {
        format!("{}.{}.{}", major, minor, patch)
    }
}
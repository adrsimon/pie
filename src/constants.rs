use semver::{BuildMetadata, Prerelease, Version};

pub const REGISTRY_URL: &str = "https://registry.npmjs.org";

pub const EMPTY_VERSION: Version = Version {
    major: 0,
    minor: 0,
    patch: 0,
    pre: Prerelease::EMPTY,
    build: BuildMetadata::EMPTY,
};

pub const LATEST: &str = "latest";
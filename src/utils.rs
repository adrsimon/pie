use crate::errors::CommandError;
use bytes::Bytes;
use flate2::bufread::GzDecoder;
use semver::{BuildMetadata, Prerelease, Version};
use std::future::Future;
use std::path::Path;
use std::sync::atomic::AtomicUsize;
use tar::Archive;
use tokio::task::JoinHandle;

pub const REGISTRY_URL: &str = "https://registry.npmjs.org";

pub const EMPTY_VERSION: Version = Version {
    major: 0,
    minor: 0,
    patch: 0,
    pre: Prerelease::EMPTY,
    build: BuildMetadata::EMPTY,
};

pub const LATEST: &str = "latest";

pub fn extract_tarball(bytes: Bytes, destination: String) -> Result<(), CommandError> {
    let bytes = &bytes.to_vec()[..];
    let gz = GzDecoder::new(bytes);
    let mut archive = Archive::new(gz);

    archive
        .unpack(&destination)
        .map_err(CommandError::ExtractionFailed)
        .expect("Failed to extract tarball");

    Ok(())
}

pub fn create_node_modules_dir() {
    if Path::new("node_modules").exists() {
        return;
    }

    std::fs::create_dir("./node_modules").expect("Failed to create node_modules directory");
}

pub static ACTIVE_TASKS: AtomicUsize = AtomicUsize::new(0);

pub struct TaskAllocator;
impl TaskAllocator {
    pub fn add_task<T>(future: T) -> JoinHandle<T::Output>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        tokio::spawn(async move {
            Self::increment_tasks();
            let task_result = future.await;
            Self::decrement_tasks();

            task_result
        })
    }

    pub fn add_blocking_task<F, R>(f: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        tokio::task::spawn_blocking(move || {
            Self::increment_tasks();
            let task_result = f();
            Self::decrement_tasks();

            task_result
        })
    }

    pub fn block_until_done() {
        while Self::active_tasks() != 0 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    fn increment_tasks() {
        ACTIVE_TASKS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    fn decrement_tasks() {
        ACTIVE_TASKS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }

    fn active_tasks() -> usize {
        ACTIVE_TASKS.load(std::sync::atomic::Ordering::SeqCst)
    }
}

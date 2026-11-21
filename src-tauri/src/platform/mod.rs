#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::{detect, install, launch};
#[cfg(all(test, target_os = "macos"))]
pub(crate) use macos::validate_pkg;
#[cfg(target_os = "macos")]
pub use macos::{detect, install, launch};
#[cfg(all(test, windows))]
pub(crate) use windows::install_on_disposable_runner;
#[cfg(windows)]
pub use windows::{detect, install, launch};

#[cfg(any(windows, target_os = "linux"))]
use std::path::Path;
use std::path::PathBuf;

use serde::Serialize;

use crate::releases::version;

#[derive(Clone, Serialize)]
pub struct Installed {
    pub version: String,
    #[serde(skip)]
    pub path: PathBuf,
}

pub enum InstallResult {
    Finished,
    Cancelled,
    #[cfg_attr(not(windows), allow(dead_code))]
    RestartRequired,
}

#[cfg(any(windows, target_os = "macos"))]
pub fn newest(candidates: impl Iterator<Item = Installed>) -> Option<Installed> {
    candidates.max_by(|left, right| {
        version(&left.version)
            .unwrap()
            .cmp_precedence(&version(&right.version).unwrap())
    })
}

pub fn same_version(left: &str, right: &str) -> bool {
    version(left)
        .zip(version(right))
        .is_some_and(|(left, right)| left.cmp_precedence(&right).is_eq())
}

#[cfg(any(windows, target_os = "linux"))]
pub fn working_directory(path: &Path) -> &Path {
    path.parent().expect("installed executable directory")
}

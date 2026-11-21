#[cfg(any(windows, target_os = "macos"))]
mod copies;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(all(test, target_os = "linux"))]
pub use linux::detect;
#[cfg(target_os = "linux")]
pub use linux::{install, launch};
#[cfg(all(test, target_os = "macos"))]
pub(crate) use macos::{check_disposable_launch, install_on_disposable_runner};
#[cfg(target_os = "macos")]
pub use macos::{install, launch};
#[cfg(all(test, windows))]
pub(crate) use windows::{detect, install_on_disposable_runner};
#[cfg(windows)]
pub use windows::{install, launch};

use std::path::Path;
use std::path::PathBuf;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    catalog::AppId,
    releases::{Target, version},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    Native,
    AppImage,
    RetainedCopy,
}

#[derive(Clone)]
pub struct Installed {
    pub version: String,
    pub can_uninstall: bool,
    pub path: PathBuf,
    pub origin: Origin,
}

impl Installed {
    pub fn key(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(self.version.as_bytes());
        hash.update([0]);
        hash.update(self.path.as_os_str().as_encoded_bytes());
        hex::encode(hash.finalize())
    }
}

pub fn installed_versions(id: AppId, directory: &Path, target: Target) -> Vec<Installed> {
    #[cfg(target_os = "linux")]
    let mut versions = linux::versions(id, directory, target);
    #[cfg(target_os = "macos")]
    let mut versions = macos::versions(id, directory, target);
    #[cfg(windows)]
    let mut versions: Vec<_> = windows::detect(id, directory, target).into_iter().collect();
    #[cfg(any(windows, target_os = "macos"))]
    versions.extend(copies::versions(id, directory));
    versions.sort_by(|left, right| {
        compare_versions(right, left).then_with(|| {
            (left.origin == Origin::RetainedCopy).cmp(&(right.origin == Origin::RetainedCopy))
        })
    });
    versions
}

pub fn uninstall(
    id: AppId,
    directory: &Path,
    installed: &Installed,
) -> Result<InstallResult, String> {
    #[cfg(any(windows, target_os = "macos"))]
    if installed.origin == Origin::RetainedCopy {
        return copies::uninstall(id, directory, installed);
    }
    #[cfg(target_os = "linux")]
    return linux::uninstall(id, directory, installed);
    #[cfg(target_os = "macos")]
    return macos::uninstall(id, directory, installed);
    #[cfg(windows)]
    return windows::uninstall(id, directory, installed);
}

pub fn can_retain(id: AppId, installed: &Installed) -> bool {
    if installed.origin != Origin::Native {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        let _ = id;
        false
    }
    #[cfg(target_os = "macos")]
    {
        let _ = id;
        macos::can_retain(installed)
    }
    #[cfg(windows)]
    windows::can_retain(id, installed)
}

pub fn retain(id: AppId, directory: &Path, installed: &Installed) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let _ = (id, directory, installed);
        Err("Installed AppImages already keep their versions separately.".into())
    }
    #[cfg(any(windows, target_os = "macos"))]
    copies::retain(id, directory, installed)
}

pub enum InstallResult {
    Finished,
    Cancelled,
    #[cfg_attr(not(windows), allow(dead_code))]
    RestartRequired,
}

#[cfg(windows)]
pub fn newest(candidates: impl Iterator<Item = Installed>) -> Option<Installed> {
    candidates.max_by(compare_versions)
}

fn compare_versions(left: &Installed, right: &Installed) -> std::cmp::Ordering {
    version(&left.version)
        .expect("verified installed version")
        .cmp_precedence(&version(&right.version).expect("verified installed version"))
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

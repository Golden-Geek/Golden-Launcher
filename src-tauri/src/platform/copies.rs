use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{catalog::AppId, releases::version};

#[cfg(target_os = "macos")]
use super::macos::{copy_installed, read_copy};
#[cfg(windows)]
use super::windows::{copy_installed, read_copy};
use super::{InstallResult, Installed, Origin, can_retain, same_version};

fn root(id: AppId, directory: &Path) -> PathBuf {
    directory.join("retained").join(id.slug())
}

fn installed_copy(id: AppId, directory: &Path) -> Option<Installed> {
    let mut app = read_copy(id, directory)?;
    app.origin = Origin::RetainedCopy;
    app.can_uninstall = true;
    Some(app)
}

pub fn versions(id: AppId, directory: &Path) -> Vec<Installed> {
    fs::read_dir(root(id, directory))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_name().to_string_lossy().starts_with("copy-")
                && entry.file_type().is_ok_and(|kind| kind.is_dir())
        })
        .filter_map(|entry| installed_copy(id, &entry.path()))
        .collect()
}

pub fn retain(id: AppId, directory: &Path, installed: &Installed) -> Result<(), String> {
    if !can_retain(id, installed) {
        return Err("The selected installation cannot be copied safely.".into());
    }
    let version =
        version(&installed.version).ok_or("The installed version could not be verified.")?;
    let root = root(id, directory);
    fs::create_dir_all(&root).map_err(copy_error)?;
    let destination = root.join(format!("copy-{version}"));
    if destination.try_exists().map_err(copy_error)? {
        return Err("There is already a saved copy of this version.".into());
    }
    let stage = tempfile::Builder::new()
        .prefix(".pending-")
        .tempdir_in(&root)
        .map_err(copy_error)?;
    copy_installed(id, installed, stage.path())?;
    if !installed_copy(id, stage.path())
        .is_some_and(|copy| same_version(&copy.version, &installed.version))
    {
        return Err("The copied app did not match the installed version.".into());
    }
    fs::rename(stage.path(), destination).map_err(copy_error)?;
    Ok(())
}

pub fn uninstall(
    id: AppId,
    directory: &Path,
    installed: &Installed,
) -> Result<InstallResult, String> {
    let folder = installed
        .path
        .parent()
        .ok_or("The saved copy has no installation folder.")?;
    let root = root(id, directory);
    let canonical_root = root.canonicalize().map_err(copy_error)?;
    if folder.parent() != Some(root.as_path())
        || !folder
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("copy-"))
        || !fs::symlink_metadata(folder)
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.is_symlink())
        || !folder
            .canonicalize()
            .is_ok_and(|path| path.parent() == Some(canonical_root.as_path()))
    {
        return Err("The saved copy's installation folder could not be verified.".into());
    }
    fs::remove_dir_all(folder)
        .map_err(|error| format!("Could not remove the saved copy ({error})."))?;
    Ok(InstallResult::Finished)
}

fn copy_error(error: std::io::Error) -> String {
    format!("Could not save the installed version ({error}).")
}

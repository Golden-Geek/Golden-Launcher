use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    catalog::AppId,
    releases::{PackageKind, Release, Target, appimage_header, version},
    state::write_json,
};

use super::{InstallResult, Installed, Origin, working_directory};

#[derive(Deserialize, Serialize)]
struct Receipt {
    app: AppId,
    version: String,
    generation: String,
    sha256: String,
}

fn app_dir(directory: &Path, id: AppId) -> PathBuf {
    directory.join("apps").join(id.slug())
}

fn read_receipt(path: &Path, id: AppId) -> Option<Receipt> {
    let receipt: Receipt = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    let valid_generation = receipt.generation.starts_with("image-")
        && receipt
            .generation
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    (receipt.app == id && valid_generation && version(&receipt.version).is_some())
        .then_some(receipt)
}

fn image_path(root: &Path, id: AppId, receipt: &Receipt) -> PathBuf {
    root.join(&receipt.generation)
        .join(format!("{}.AppImage", id.slug()))
}

fn verify_image(id: AppId, root: &Path, receipt: Receipt, target: Target) -> Option<Installed> {
    let path = image_path(root, id, &receipt);
    if !working_directory(&path)
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.is_dir())
    {
        return None;
    }
    let metadata = path.symlink_metadata().ok()?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o100 == 0 {
        return None;
    }
    let mut image = File::open(&path).ok()?;
    let mut header = [0; 64];
    image.read_exact(&mut header).ok()?;
    if !appimage_header(&header, target) {
        return None;
    }
    let mut hasher = Sha256::new();
    hasher.update(header);
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = image.read(&mut buffer).ok()?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    (hex::encode(hasher.finalize()) == receipt.sha256).then_some(Installed {
        version: receipt.version,
        can_uninstall: true,
        path,
        origin: Origin::AppImage,
    })
}

pub fn versions(id: AppId, directory: &Path, target: Target) -> Vec<Installed> {
    let root = app_dir(directory, id);
    let mut receipts = Vec::new();
    // Preserve installations created with the original launcher receipt format.
    receipts.extend(read_receipt(&root.join("current.json"), id));
    receipts.extend(
        fs::read_dir(&root)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter_map(|entry| {
                let receipt = read_receipt(&entry.path().join("receipt.json"), id)?;
                (entry.file_name() == std::ffi::OsStr::new(&receipt.generation)).then_some(receipt)
            }),
    );
    receipts.sort_by(|left, right| left.generation.cmp(&right.generation));
    receipts.dedup_by(|left, right| left.generation == right.generation);
    receipts
        .into_iter()
        .filter_map(|receipt| verify_image(id, &root, receipt, target))
        .collect()
}

#[cfg(test)]
pub fn detect(id: AppId, directory: &Path, target: Target) -> Option<Installed> {
    super::installed_versions(id, directory, target)
        .into_iter()
        .next()
}

pub fn uninstall(
    id: AppId,
    directory: &Path,
    installed: &Installed,
) -> Result<InstallResult, String> {
    let root = app_dir(directory, id);
    let folder = working_directory(&installed.path);
    let receipt_path = folder.join("receipt.json");
    let legacy_path = root.join("current.json");
    let is_selected = |receipt: Receipt| {
        image_path(&root, id, &receipt) == installed.path && receipt.version == installed.version
    };
    let confirmed = read_receipt(&receipt_path, id).is_some_and(is_selected);
    let legacy = read_receipt(&legacy_path, id).is_some_and(is_selected);
    if folder.parent() != Some(root.as_path()) || (!confirmed && !legacy) {
        return Err("The managed installation changed. Refresh the library and try again.".into());
    }
    fs::remove_file(&installed.path)
        .map_err(|error| format!("Could not remove the AppImage ({error})."))?;
    if confirmed {
        let _ = fs::remove_file(receipt_path);
    }
    if legacy {
        let _ = fs::remove_file(legacy_path);
    }
    let _ = fs::remove_dir(folder);
    Ok(InstallResult::Finished)
}

pub fn install(
    id: AppId,
    directory: &Path,
    path: &Path,
    release: &Release,
    digest: &str,
    _: bool,
    _: Target,
) -> Result<InstallResult, String> {
    if release.package.kind != PackageKind::AppImage {
        return Err("This target requires an AppImage.".into());
    }
    let root = app_dir(directory, id);
    fs::create_dir_all(&root).map_err(file_error)?;
    let generation = tempfile::Builder::new()
        .prefix("image-")
        .tempdir_in(&root)
        .map_err(file_error)?;
    let receipt = Receipt {
        app: id,
        version: release.version.clone(),
        generation: generation
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        sha256: digest.to_owned(),
    };
    let next = image_path(&root, id, &receipt);
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(file_error)?;
    fs::rename(path, &next).map_err(file_error)?;
    File::open(generation.path())
        .and_then(|file| file.sync_all())
        .map_err(file_error)?;
    // Publish only after the new image is durable. Other verified versions stay available.
    write_json(&generation.path().join("receipt.json"), &receipt).map_err(file_error)?;
    let _ = generation.keep();
    File::open(&root)
        .and_then(|file| file.sync_all())
        .map_err(file_error)?;
    Ok(InstallResult::Finished)
}

fn file_error(error: io::Error) -> String {
    format!("Could not write the managed AppImage ({error}).")
}

fn fuse_available() -> bool {
    // Loading libfuse checks the same name the AppImage runtime uses.
    OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/fuse")
        .is_ok()
        && unsafe { libloading::Library::new("libfuse.so.2") }.is_ok()
}

pub fn launch(installed: &Installed) -> Result<u32, String> {
    let path = &installed.path;
    if fuse_available() {
        let mut child = Command::new(path)
            .current_dir(working_directory(path))
            .spawn()
            .map_err(|error| format!("Could not open the AppImage ({error})."))?;
        let pid = child.id();
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        return Ok(pid);
    }

    // Type 2 runtimes support extracting themselves, including ones that predate extract-and-run.
    let extracted = tempfile::tempdir().map_err(file_error)?;
    let output = Command::new(path)
        .arg("--appimage-extract")
        .current_dir(extracted.path())
        .stdout(Stdio::null())
        .output()
        .map_err(|error| format!("Could not extract the AppImage ({error})."))?;
    if !output.status.success() {
        return Err("FUSE is unavailable and this AppImage could not extract itself.".into());
    }
    let app_dir = extracted.path().join("squashfs-root");
    let mut child = Command::new(app_dir.join("AppRun"))
        .current_dir(working_directory(path))
        .env("APPDIR", &app_dir)
        .env("APPIMAGE", path)
        .spawn()
        .map_err(|error| format!("Could not open the extracted AppImage ({error})."))?;
    let pid = child.id();
    std::thread::spawn(move || {
        let _ = child.wait();
        drop(extracted);
    });
    Ok(pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::releases::Package;

    #[test]
    fn failed_managed_update_retains_verified_version() {
        let dir = tempfile::tempdir().unwrap();
        let id = AppId::Chataigne;
        let target = Target::LinuxX64;
        let package = Package {
            url: String::new(),
            kind: PackageKind::AppImage,
            sha256: None,
            size: None,
        };
        let release = Release {
            version: "1.0.0".into(),
            beta: false,
            notes: Vec::new(),
            package,
        };
        let original = dir.path().join("original.AppImage");
        let mut header = [0; 64];
        header[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        header[8..11].copy_from_slice(b"AI\x02");
        header[18] = 62;
        fs::write(&original, header).unwrap();
        let digest = hex::encode(Sha256::digest(header));
        install(id, dir.path(), &original, &release, &digest, false, target).unwrap();
        let before = detect(id, dir.path(), target).unwrap();
        // A library first installed by the earlier launcher has the receipt at the app root.
        fs::rename(
            working_directory(&before.path).join("receipt.json"),
            app_dir(dir.path(), id).join("current.json"),
        )
        .unwrap();
        assert_eq!(detect(id, dir.path(), target).unwrap().key(), before.key());
        let update = Release {
            version: "1.1.0".into(),
            ..release
        };
        assert!(install(id, dir.path(), &original, &update, &digest, false, target).is_err());
        let after = detect(id, dir.path(), target).unwrap();
        assert_eq!(before.path, after.path);
        assert_eq!(after.version, "1.0.0");
        fs::write(&after.path, [0; 64]).unwrap();
        assert!(detect(id, dir.path(), target).is_none());
        fs::write(&after.path, header).unwrap();
        fs::write(&original, header).unwrap();
        install(id, dir.path(), &original, &update, &digest, false, target).unwrap();
        assert_eq!(versions(id, dir.path(), target).len(), 2);
        let unrelated = dir.path().join("user-project");
        fs::write(&unrelated, b"project data").unwrap();
        assert!(uninstall(AppId::BenTo, dir.path(), &after).is_err());
        assert!(detect(id, dir.path(), target).is_some());
        uninstall(id, dir.path(), &after).unwrap();
        assert!(!after.path.exists());
        assert_eq!(detect(id, dir.path(), target).unwrap().version, "1.1.0");
        assert!(unrelated.exists());
    }
}

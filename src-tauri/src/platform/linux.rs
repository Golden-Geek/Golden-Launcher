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

use super::{InstallResult, Installed, working_directory};

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

fn active_receipt(root: &Path, id: AppId) -> Option<Receipt> {
    let receipt: Receipt =
        serde_json::from_slice(&fs::read(root.join("current.json")).ok()?).ok()?;
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

pub fn detect(id: AppId, directory: &Path, target: Target) -> Option<Installed> {
    let root = app_dir(directory, id);
    let receipt = active_receipt(&root, id)?;
    let path = image_path(&root, id, &receipt);
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
        path,
    })
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
    let previous = active_receipt(&root, id);
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
    // The receipt is the activation point. Until its atomic rename the old image remains active.
    write_json(&root.join("current.json"), &receipt).map_err(file_error)?;
    generation.keep();
    if File::open(&root).and_then(|file| file.sync_all()).is_ok()
        && let Some(previous) = previous
    {
        let _ = fs::remove_file(image_path(&root, id, &previous));
        let _ = fs::remove_dir(root.join(previous.generation));
    }
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

pub fn launch(installed: &Installed) -> Result<(), String> {
    let path = &installed.path;
    if fuse_available() {
        let mut child = Command::new(path)
            .current_dir(working_directory(path))
            .spawn()
            .map_err(|error| format!("Could not open the AppImage ({error})."))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        return Ok(());
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
    std::thread::spawn(move || {
        let _ = child.wait();
        drop(extracted);
    });
    Ok(())
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
            preview: false,
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
        let update = Release {
            version: "1.1.0".into(),
            ..release
        };
        assert!(install(id, dir.path(), &original, &update, &digest, false, target).is_err());
        let after = detect(id, dir.path(), target).unwrap();
        assert_eq!(before.path, after.path);
        assert_eq!(after.version, "1.0.0");
        fs::write(after.path, [0; 64]).unwrap();
        assert!(detect(id, dir.path(), target).is_none());
    }
}

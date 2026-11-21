use std::{
    ffi::CString,
    fs::{self, File},
    io::Read,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::Command,
};

use walkdir::WalkDir;

use crate::{
    catalog::AppId,
    releases::{PackageKind, Release, Target, version},
};

use super::{InstallResult, Installed, newest, same_version};

fn bundle_id(id: AppId) -> &'static str {
    match id {
        AppId::Chataigne => "benkuper.Chataigne",
        AppId::Chataigne2 => "com.goldengeek.chataigne2",
        AppId::MapGyver => "com.goldengeek.mapgyver",
        AppId::MapGyver2 => "com.goldengeek.mapgyver2",
        AppId::Blux => "benkuper.Blux",
        AppId::BlinderKitten => "norbertrostaing.BlinderKitten",
        AppId::BenTo => "benkuper.Bento",
        AppId::Lgml => "benkuper.LGML",
    }
}

fn user_applications() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Applications"))
}

fn bundle(id: AppId, path: PathBuf) -> Option<(Installed, PathBuf)> {
    if path.extension()?.to_str()? != "app" || !path.is_dir() {
        return None;
    }
    let info = plist::Value::from_file(path.join("Contents/Info.plist")).ok()?;
    let fields = info.as_dictionary()?;
    let value = |key| fields.get(key).and_then(plist::Value::as_string);
    if !value("CFBundleIdentifier")?.eq_ignore_ascii_case(bundle_id(id)) {
        return None;
    }
    let found = ["CFBundleShortVersionString", "CFBundleVersion"]
        .into_iter()
        .filter_map(value)
        .find(|value| version(value).is_some())?;
    let executable = value("CFBundleExecutable")?;
    if Path::new(executable).file_name()?.to_str()? != executable {
        return None;
    }
    let binary = path.join("Contents/MacOS").join(executable);
    binary.is_file().then_some((
        Installed {
            version: found.into(),
            path,
        },
        binary,
    ))
}

pub fn detect(id: AppId, _: &Path, _: Target) -> Option<Installed> {
    let roots = user_applications()
        .into_iter()
        .chain([PathBuf::from("/Applications")]);
    newest(
        roots
            .filter_map(|root| fs::read_dir(root).ok())
            .flatten()
            .filter_map(|entry| bundle(id, entry.ok()?.path()).map(|(app, _)| app)),
    )
}

fn mac_arch(target: Target) -> (&'static str, u32) {
    match target {
        Target::MacIntel => ("x86_64", 0x0100_0007),
        Target::MacSilicon => ("arm64", 0x0100_000c),
        _ => unreachable!("macOS target"),
    }
}

fn mach_supports(path: &Path, target: Target) -> Option<bool> {
    let mut file = File::open(path).ok()?;
    let mut header = [0; 8];
    file.read_exact(&mut header).ok()?;
    let parse = match &header[..4] {
        [0xcf, 0xfa, 0xed, 0xfe] | [0xbe, 0xba, 0xfe, 0xca] | [0xbf, 0xba, 0xfe, 0xca] => {
            u32::from_le_bytes
        }
        [0xfe, 0xed, 0xfa, 0xcf] | [0xca, 0xfe, 0xba, 0xbe] | [0xca, 0xfe, 0xba, 0xbf] => {
            u32::from_be_bytes
        }
        _ => return Some(false),
    };
    let field = parse(header[4..].try_into().ok()?);
    let (_, cpu) = mac_arch(target);
    if matches!(header[0], 0xcf | 0xfe) {
        return Some(field == cpu);
    }
    let stride = if header[0] == 0xbf || header[3] == 0xbf {
        32
    } else {
        20
    };
    if field == 0 || field > 64 {
        return Some(false);
    }
    for _ in 0..field {
        let mut entry = [0; 32];
        file.read_exact(&mut entry[..stride]).ok()?;
        if parse(entry[..4].try_into().ok()?) == cpu {
            return Some(true);
        }
    }
    Some(false)
}

fn run(command: &mut Command, label: &str) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|error| format!("{label} ({error})."))?;
    if !output.status.success() {
        let details = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{label} ({})",
            details.trim().chars().take(350).collect::<String>()
        ));
    }
    Ok(())
}

fn quarantine(path: &Path) -> Result<(), String> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    run(
        Command::new("/usr/bin/xattr")
            .args(["-w", "com.apple.quarantine"])
            .arg(format!("0081;{timestamp:x};Golden Launcher;"))
            .arg(path),
        "Could not mark the download for Gatekeeper",
    )
}

pub(crate) fn validate_pkg(
    id: AppId,
    path: &Path,
    release: &Release,
    target: Target,
) -> Result<(), String> {
    if id == AppId::MapGyver2 {
        return Err(
            "MapGyver 2 requires a disk image with its own bundle identity and destination.".into(),
        );
    }
    let work = tempfile::tempdir().map_err(|error| error.to_string())?;
    let expanded = work.path().join("expanded");
    run(
        Command::new("/usr/sbin/pkgutil")
            .arg("--expand")
            .arg(path)
            .arg(&expanded),
        "Could not inspect the installer package",
    )?;
    let distribution = expanded.join("Distribution");
    if distribution.is_file() {
        let xml = fs::read_to_string(distribution).map_err(|error| error.to_string())?;
        let doc = roxmltree::Document::parse(&xml).map_err(|error| error.to_string())?;
        for arch in doc
            .descendants()
            .filter(|node| node.has_tag_name("options"))
            .filter_map(|node| node.attribute("hostArchitectures"))
        {
            if !arch
                .split(',')
                .any(|arch| arch.trim() == mac_arch(target).0)
            {
                return Err("The installer package targets a different macOS architecture.".into());
            }
        }
    }
    for info in WalkDir::new(&expanded)
        .max_depth(4)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name() == "PackageInfo")
    {
        let xml = fs::read_to_string(info.path()).map_err(|error| error.to_string())?;
        let doc = roxmltree::Document::parse(&xml).map_err(|error| error.to_string())?;
        let found = doc
            .descendants()
            .filter(|node| node.has_tag_name("bundle"))
            .any(|node| {
                node.attribute("CFBundleIdentifier")
                    .or_else(|| node.attribute("id"))
                    .is_some_and(|found| found.eq_ignore_ascii_case(bundle_id(id)))
                    && ["CFBundleShortVersionString", "CFBundleVersion"]
                        .into_iter()
                        .filter_map(|key| node.attribute(key))
                        .any(|found| same_version(found, &release.version))
            });
        if found {
            return Ok(());
        }
    }
    Err("The installer does not declare the expected app identity and version.".into())
}

struct MountedImage(PathBuf);

impl Drop for MountedImage {
    fn drop(&mut self) {
        let _ = Command::new("/usr/bin/hdiutil")
            .arg("detach")
            .arg(&self.0)
            .output();
    }
}

fn install_dmg(id: AppId, image: &Path, release: &Release, target: Target) -> Result<(), String> {
    let app_dir = user_applications().ok_or("Could not find the user's Applications directory.")?;
    fs::create_dir_all(&app_dir)
        .map_err(|error| format!("Could not create ~/Applications ({error})."))?;
    let volume = tempfile::tempdir().map_err(|error| error.to_string())?;
    run(
        Command::new("/usr/bin/hdiutil")
            .args(["attach", "-readonly", "-nobrowse", "-mountpoint"])
            .arg(volume.path())
            .arg(image),
        "Could not mount the disk image",
    )?;
    let mounted = MountedImage(volume.path().to_owned());
    let expected = |path| {
        bundle(id, path).filter(|(app, executable)| {
            same_version(&app.version, &release.version)
                && mach_supports(executable, target) == Some(true)
        })
    };
    let (source, _) = fs::read_dir(&mounted.0).map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .find_map(|entry| expected(entry.path()))
        .ok_or("The disk image has no bundle matching the published identity, architecture, and version.")?;
    let filename = format!("{}.app", id.executable());
    let stage = tempfile::tempdir_in(&app_dir).map_err(|error| error.to_string())?;
    let staged = stage.path().join(&filename);
    let destination = app_dir.join(&filename);
    run(
        Command::new("/usr/bin/ditto")
            .arg(&source.path)
            .arg(&staged),
        "Could not copy the app into ~/Applications",
    )?;
    if expected(staged.clone()).is_none() {
        return Err("The staged app did not pass installation verification.".into());
    }
    quarantine(&staged)?;
    match destination.symlink_metadata() {
        Ok(metadata) if metadata.is_dir() && bundle(id, destination.clone()).is_some() => {
            let from =
                CString::new(staged.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
            let to = CString::new(destination.as_os_str().as_bytes())
                .map_err(|error| error.to_string())?;
            // Exchange the two existing paths atomically on the same volume.
            if unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_SWAP) } != 0 {
                return Err(format!(
                    "Could not replace the app ({})",
                    std::io::Error::last_os_error()
                ));
            }
        }
        Ok(_) => {
            return Err(format!(
                "{} is already occupied by a different app.",
                destination.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::rename(staged, &destination).map_err(|error| error.to_string())?;
        }
        Err(error) => return Err(error.to_string()),
    }
    Ok(())
}

pub fn install(
    id: AppId,
    _: &Path,
    path: &Path,
    release: &Release,
    _: &str,
    _: bool,
    target: Target,
) -> Result<InstallResult, String> {
    quarantine(path)?;
    match release.package.kind {
        PackageKind::Dmg => install_dmg(id, path, release, target)?,
        PackageKind::Pkg => {
            validate_pkg(id, path, release, target)?;
            run(
                Command::new("/usr/bin/open")
                    .args(["-W", "-n", "-a", "Installer"])
                    .arg(path),
                "Could not open macOS Installer",
            )?;
        }
        _ => return Err("This target requires a macOS package or disk image.".into()),
    }
    Ok(InstallResult::Finished)
}

pub fn launch(installed: &Installed) -> Result<(), String> {
    run(
        Command::new("/usr/bin/open").arg(&installed.path),
        "Could not open the installed app",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapgyver_versions_use_distinct_bundle_ids_and_destinations() {
        assert_ne!(bundle_id(AppId::MapGyver), bundle_id(AppId::MapGyver2));
        assert_ne!(AppId::MapGyver.executable(), AppId::MapGyver2.executable());
    }
}

use std::{
    ffi::OsStr,
    fs, io,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::Command,
};

use pelite::{
    FileMap, PeFile,
    image::{IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_I386},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_CANCELLED},
    System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject},
    UI::{
        Shell::{SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
        WindowsAndMessaging::SW_SHOWNORMAL,
    },
};
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY},
};

use crate::{
    catalog::AppId,
    releases::{PackageKind, Release, Target, version},
};

use super::{InstallResult, Installed, newest, same_version, working_directory};

const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
const INNO_QUIET_ARGS: &str = "/VERYSILENT /SP- /NORESTART /RESTARTEXITCODE=3010";

fn names(id: AppId) -> &'static [&'static str] {
    match id {
        AppId::Chataigne2 => &["Chataigne2", "Chataigne 2"],
        AppId::MapGyver2 => &["MapGyver2", "MapGyver 2"],
        AppId::Lgml => &["LGML", "Le Grand Mechant Loop"],
        AppId::Chataigne => &["Chataigne"],
        AppId::MapGyver => &["MapGyver"],
        AppId::Blux => &["Blux"],
        AppId::BlinderKitten => &["BlinderKitten"],
        AppId::BenTo => &["BenTo"],
    }
}

fn matches_name(id: AppId, name: &str) -> bool {
    names(id)
        .iter()
        .any(|expected| name.trim().eq_ignore_ascii_case(expected))
}

#[derive(Default)]
struct VersionStrings {
    product: String,
    product_version: String,
    file_version: String,
    description: String,
}

impl pelite::resources::version_info::Visit<'_> for VersionStrings {
    fn string(&mut self, key: &[u16], value: &[u16]) {
        let key = String::from_utf16_lossy(key);
        let value = String::from_utf16_lossy(value);
        let field = match key.as_str() {
            "ProductName" => &mut self.product,
            "ProductVersion" => &mut self.product_version,
            "FileVersion" => &mut self.file_version,
            "FileDescription" => &mut self.description,
            _ => return,
        };
        if field.is_empty() {
            *field = value.trim_end_matches('\0').trim().into();
        }
    }
}

fn version_strings(image: &PeFile<'_>) -> Option<VersionStrings> {
    let mut strings = VersionStrings::default();
    image
        .resources()
        .ok()?
        .version_info()
        .ok()?
        .visit(&mut strings);
    Some(strings)
}

fn version_from(strings: &VersionStrings) -> Option<String> {
    [&strings.product_version, &strings.file_version]
        .into_iter()
        .find(|value| version(value).is_some())
        .cloned()
}

fn installed_exe(id: AppId, path: PathBuf) -> Option<Installed> {
    let named_correctly = names(id).iter().any(|name| {
        path.file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|file| file.eq_ignore_ascii_case(&format!("{name}.exe")))
    });
    if !path.is_absolute() || !named_correctly {
        return None;
    }
    let map = FileMap::open(&path).ok()?;
    let image = PeFile::from_bytes(map.as_ref()).ok()?;
    if image.file_header().Machine != IMAGE_FILE_MACHINE_AMD64 {
        return None;
    }
    let strings = version_strings(&image)?;
    matches_name(id, &strings.product).then_some(Installed {
        path,
        version: version_from(&strings)?,
    })
}

fn icon_to_path(value: &str) -> Option<PathBuf> {
    let clean = value.trim();
    let path = if let Some(quoted) = clean.strip_prefix('"') {
        quoted.split_once('"')?.0
    } else {
        clean
            .rsplit_once(',')
            .filter(|(_, index)| index.trim().parse::<i32>().is_ok())
            .map_or(clean, |(path, _)| path)
    };
    Some(PathBuf::from(path))
}

pub fn detect(id: AppId, _: &Path, _: Target) -> Option<Installed> {
    let mut candidates = Vec::new();
    let uninstaller_names = names(id)
        .iter()
        .flat_map(|name| [format!("{name}_is1"), (*name).to_owned()]);
    for name in uninstaller_names {
        for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            for bitness in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
                let Ok(key) = RegKey::predef(hive)
                    .open_subkey_with_flags(format!(r"{UNINSTALL}\{name}"), KEY_READ | bitness)
                else {
                    continue;
                };
                if !key
                    .get_value::<String, _>("DisplayName")
                    .is_ok_and(|display| matches_name(id, &display))
                {
                    continue;
                }
                if let Ok(icon) = key.get_value::<String, _>("DisplayIcon")
                    && let Some(path) = icon_to_path(&icon)
                {
                    candidates.push(path);
                }
                if let Ok(location) = key.get_value::<String, _>("InstallLocation") {
                    let folder = Path::new(location.trim().trim_matches('"'));
                    for name in names(id) {
                        candidates.push(folder.join(format!("{name}.exe")));
                    }
                }
            }
        }
    }
    for key in ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"] {
        if let Some(root) = std::env::var_os(key).map(PathBuf::from) {
            for name in names(id) {
                candidates.push(root.join(name).join(format!("{}.exe", id.executable())));
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    newest(
        candidates
            .into_iter()
            .filter_map(|path| installed_exe(id, path)),
    )
}

fn validate_installer(id: AppId, path: &Path, release: &Release) -> Result<(), String> {
    let map = FileMap::open(path)
        .map_err(|error| format!("Could not read the Windows installer ({error})."))?;
    let bytes = map.as_ref();
    let image =
        PeFile::from_bytes(bytes).map_err(|_| "The Windows installer is not a valid PE file.")?;
    // Inno and NSIS may use a 32-bit loader to install 64-bit applications.
    if !matches!(
        image.file_header().Machine,
        IMAGE_FILE_MACHINE_I386 | IMAGE_FILE_MACHINE_AMD64
    ) {
        return Err("The Windows installer has an unexpected CPU architecture.".into());
    }
    let marker = match release.package.kind {
        PackageKind::Inno => b"Inno Setup Setup Data (" as &[u8],
        PackageKind::Nsis => b"\xef\xbe\xad\xdeNullsoftInst",
        _ => return Err("This target requires an Inno Setup or NSIS installer.".into()),
    };
    if !bytes.windows(marker.len()).any(|window| window == marker) {
        return Err("The file does not contain the expected installer format.".into());
    }
    let metadata = version_strings(&image).ok_or("The installer is missing product metadata.")?;
    let is_expected_version =
        version_from(&metadata).is_some_and(|value| same_version(&value, &release.version));
    if !matches_name(id, &metadata.product)
        || !is_expected_version
        || (release.package.kind == PackageKind::Nsis && !matches_name(id, &metadata.description))
    {
        return Err(
            "The installer product or version does not match the publisher release.".into(),
        );
    }
    Ok(())
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain([0]).collect()
}

fn run_installer(path: &Path, arguments: &OsStr) -> Result<InstallResult, String> {
    let mut zone = path.as_os_str().to_owned();
    zone.push(":Zone.Identifier");
    fs::write(zone, "[ZoneTransfer]\r\nZoneId=3\r\n")
        .map_err(|error| format!("Could not mark the installer as downloaded ({error})."))?;
    let file = wide(path.as_os_str());
    let verb = wide(OsStr::new("open"));
    let params = wide(arguments);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: params.as_ptr(),
        nShow: SW_SHOWNORMAL,
        ..SHELLEXECUTEINFOW::default()
    };
    // ShellExecute preserves the native elevation and downloaded-file prompts.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_CANCELLED as i32) {
            return Ok(InstallResult::Cancelled);
        }
        return Err(format!("Could not start the Windows installer ({error})."));
    }
    if info.hProcess.is_null() {
        return Err("Windows did not provide an installer process to verify.".into());
    }
    let mut code = 0;
    let read = unsafe {
        let waited = WaitForSingleObject(info.hProcess, INFINITE) == 0;
        let read = waited && GetExitCodeProcess(info.hProcess, &mut code) != 0;
        CloseHandle(info.hProcess);
        read
    };
    if !read {
        return Err("Could not read the Windows installer result.".into());
    }
    installer_exit(code)
}

fn installer_exit(code: u32) -> Result<InstallResult, String> {
    match code {
        0 => Ok(InstallResult::Finished),
        2 | 5 | ERROR_CANCELLED | 1602 => Ok(InstallResult::Cancelled),
        1641 | 3010 => Ok(InstallResult::RestartRequired),
        _ => Err(format!("The Windows installer exited with code {code}.")),
    }
}

pub fn install(
    id: AppId,
    directory: &Path,
    path: &Path,
    release: &Release,
    _: &str,
    graphical: bool,
    target: Target,
) -> Result<InstallResult, String> {
    validate_installer(id, path, release)?;
    let args = match (release.package.kind, graphical) {
        (PackageKind::Inno, false) => INNO_QUIET_ARGS,
        (PackageKind::Inno, true) => "/NORESTART /RESTARTEXITCODE=3010",
        (PackageKind::Nsis, false) => "/S",
        (PackageKind::Nsis, true) => "",
        _ => unreachable!("validated installer"),
    };
    let mut arguments = std::ffi::OsString::from(args);
    if id == AppId::MapGyver2 {
        let root = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or("Could not find the local app directory.")?;
        let destination = root.join("MapGyver2");
        if destination.exists() {
            let current = detect(id, directory, target);
            if current.as_ref().and_then(|app| app.path.parent()) != Some(destination.as_path())
                || fs::symlink_metadata(&destination).is_ok_and(|metadata| {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0
                })
            {
                return Err(
                    "MapGyver 2 needs its own unused or verified installation directory.".into(),
                );
            }
        }
        if graphical {
            arguments.push("/P");
        }
        arguments.push(" /D=");
        arguments.push(destination);
    }
    run_installer(path, &arguments)
}

pub fn launch(installed: &Installed) -> Result<u32, String> {
    Command::new(&installed.path)
        .current_dir(working_directory(&installed.path))
        .spawn()
        .map(|child| child.id())
        .map_err(|error| format!("Could not open the installed app ({error})."))
}

#[cfg(test)]
pub(crate) fn install_on_disposable_runner(
    id: AppId,
    path: &Path,
    release: &Release,
) -> Result<InstallResult, String> {
    validate_installer(id, path, release)?;
    let status = Command::new(path)
        .args(INNO_QUIET_ARGS.split_whitespace())
        .status()
        .map_err(|error| format!("Could not start the official Inno Setup package ({error})."))?;
    installer_exit(
        status
            .code()
            .ok_or("No installer exit code was available.")? as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_native_windows_outcomes() {
        assert!(matches!(
            installer_exit(3010),
            Ok(InstallResult::RestartRequired)
        ));
        assert!(matches!(
            installer_exit(ERROR_CANCELLED),
            Ok(InstallResult::Cancelled)
        ));
        assert!(installer_exit(1).is_err());
    }
}

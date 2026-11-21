use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

use futures_util::{Stream, StreamExt};
use reqwest::Client;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::{
    catalog::AppId,
    platform::{self, InstallResult, Installed, Origin, same_version},
    releases::{self, PackageKind, Release, Target},
    state::{Library, install_action},
};

const MAX_PACKAGE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Default)]
pub struct Operations {
    active: Mutex<Option<Active>>,
}

struct Active {
    id: AppId,
    cancellation: CancellationToken,
    download: bool,
}

struct Hold<'a>(&'a Operations);

impl Drop for Hold<'_> {
    fn drop(&mut self) {
        *self.0.active.lock().expect("operation state") = None;
    }
}

impl Operations {
    fn begin(&self, id: AppId) -> Result<(Hold<'_>, CancellationToken), String> {
        let mut active = self.active.lock().expect("operation state");
        if active.is_some() {
            return Err("Another installation is in progress.".into());
        }
        let cancellation = CancellationToken::new();
        *active = Some(Active {
            id,
            cancellation: cancellation.clone(),
            download: true,
        });
        Ok((Hold(self), cancellation))
    }

    fn finish_download(&self) -> bool {
        let mut active = self.active.lock().expect("operation state");
        let active = active.as_mut().expect("active installation");
        active.download = false;
        !active.cancellation.is_cancelled()
    }

    fn cancel(&self, id: AppId) -> bool {
        let active = self.active.lock().expect("operation state");
        if let Some(active) = active
            .as_ref()
            .filter(|active| active.download && active.id == id)
        {
            active.cancellation.cancel();
            return true;
        }
        false
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
enum Stage {
    Downloading,
    Verifying,
    Installing,
    Uninstalling,
    Retaining,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    id: AppId,
    stage: Stage,
    downloaded: u64,
    total: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum ResultKind {
    Installed,
    Uninstalled,
    Retained,
    Cancelled,
    RestartRequired,
    Failed,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    kind: ResultKind,
    message: String,
    retry_graphically: bool,
}

fn publish(app: &AppHandle, id: AppId, stage: Stage, downloaded: u64, total: Option<u64>) {
    let _ = app.emit_to(
        "main",
        "install-progress",
        Progress {
            id,
            stage,
            downloaded,
            total,
        },
    );
}

#[tauri::command]
pub fn cancel_download(state: State<'_, Library>, id: AppId) -> bool {
    state.operations.cancel(id)
}

fn lock_directory(directory: &Path) -> Result<File, String> {
    fs::create_dir_all(directory).map_err(file_error)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("installer.lock"))
        .map_err(file_error)?;
    lock.try_lock()
        .map_err(|error| format!("Could not reserve the installer ({error})."))?;
    Ok(lock)
}

#[tauri::command]
pub async fn install(
    app: AppHandle,
    id: AppId,
    version: String,
    graphical: bool,
) -> Result<Outcome, String> {
    let state = app.state::<Library>();
    let (_hold, cancellation) = state.operations.begin(id)?;
    let lock = lock_directory(&state.directory)?;
    let release = state.selected_release(id, &version)?;
    let scan = app.clone();
    let installed =
        tauri::async_runtime::spawn_blocking(move || scan.state::<Library>().rescan_app(id))
            .await
            .map_err(|error| error.to_string())?;
    if install_action(&release, &installed).is_none() {
        return Err("The selected release is already installed or would replace a newer native installation.".into());
    }
    let temporary = tempfile::Builder::new()
        .prefix("download-")
        .tempdir_in(&state.directory)
        .map_err(file_error)?;
    let filename = format!("{}-setup.{}", id.slug(), release.package.kind.extension());
    let destination = temporary.path().join(filename);
    publish(&app, id, Stage::Downloading, 0, release.package.size);
    let downloaded = cancellation
        .run_until_cancelled(download(
            &state.client,
            id,
            &release,
            &destination,
            |bytes, total| {
                publish(&app, id, Stage::Downloading, bytes, total);
            },
        ))
        .await;
    let continue_install = state.operations.finish_download();
    let target = state.target;
    let work = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Hold the file lock and the staged file until native installation and reconciliation finish.
        let (_lock, _temporary) = (lock, temporary);
        let mut native_started = false;
        let attempt = match downloaded.filter(|_| continue_install) {
            None => Ok(InstallResult::Cancelled),
            Some(Err(error)) => Err(error),
            Some(Ok(digest)) => {
                publish(&work, id, Stage::Verifying, 0, None);
                validate_package(&destination, release.package.kind, target).and_then(|()| {
                    publish(&work, id, Stage::Installing, 0, None);
                    native_started = true;
                    let library = work.state::<Library>();
                    platform::install(
                        id,
                        &library.directory,
                        &destination,
                        &release,
                        &digest,
                        graphical,
                        target,
                    )
                })
            }
        };
        let installed = work.state::<Library>().rescan_app(id);
        outcome(
            attempt,
            installed
                .iter()
                .find(|installed| same_version(&installed.version, &release.version)),
            &release.version,
            native_started,
            graphical,
            target,
        )
    })
    .await
    .map_err(|error| error.to_string())
}

fn outcome(
    attempt: Result<InstallResult, String>,
    installed: Option<&Installed>,
    expected: &str,
    native_started: bool,
    graphical: bool,
    target: Target,
) -> Outcome {
    let retry = !graphical && native_started && target == Target::WinX64;
    let (kind, message, retry_graphically) = match attempt {
        Ok(InstallResult::Cancelled) => (
            ResultKind::Cancelled,
            "Installation canceled.".into(),
            retry,
        ),
        Ok(InstallResult::RestartRequired) => (
            ResultKind::RestartRequired,
            "Restart your computer to finish installation.".into(),
            false,
        ),
        Ok(InstallResult::Finished)
            if installed.is_some_and(|app| same_version(&app.version, expected)) =>
        {
            (
                ResultKind::Installed,
                format!("Version {expected} is installed."),
                false,
            )
        }
        Ok(InstallResult::Finished) => (
            ResultKind::Failed,
            format!("Version {expected} could not be verified after the installer closed."),
            retry,
        ),
        Err(error) => (ResultKind::Failed, error, retry),
    };
    Outcome {
        kind,
        message,
        retry_graphically,
    }
}

#[tauri::command]
pub async fn launch(app: AppHandle, id: AppId, key: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Library>();
        let installed = state
            .rescan_app(id)
            .into_iter()
            .find(|installed| key.as_deref().is_none_or(|key| installed.key() == key))
            .ok_or("The installation could not be verified. Refresh the library.")?;
        platform::launch(&installed).map(|_| ())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn uninstall(app: AppHandle, id: AppId, key: String) -> Result<Outcome, String> {
    let state = app.state::<Library>();
    let (_hold, _) = state.operations.begin(id)?;
    let _ = state.operations.finish_download();
    let lock = lock_directory(&state.directory)?;
    let work = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lock = lock;
        let library = work.state::<Library>();
        let installed = library
            .rescan_app(id)
            .into_iter()
            .find(|installed| installed.can_uninstall && installed.key() == key)
            .ok_or("The selected installation changed or has no verified uninstaller.")?;
        let version = &installed.version;
        publish(&work, id, Stage::Uninstalling, 0, None);
        let attempt = platform::uninstall(id, &library.directory, &installed);
        let remains = library.rescan_app(id).iter().any(|app| app.key() == key);
        let (kind, message) = match attempt {
            Ok(InstallResult::Finished) if !remains => (
                ResultKind::Uninstalled,
                format!("Version {version} was uninstalled."),
            ),
            Ok(InstallResult::Finished) => (
                ResultKind::Failed,
                format!("Version {version} is still installed."),
            ),
            Ok(InstallResult::Cancelled) => (ResultKind::Cancelled, "Uninstall canceled.".into()),
            Ok(InstallResult::RestartRequired) => (
                ResultKind::RestartRequired,
                "Restart your computer to finish uninstalling.".into(),
            ),
            Err(error) => (ResultKind::Failed, error),
        };
        Ok(Outcome {
            kind,
            message,
            retry_graphically: false,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn retain_installation(
    app: AppHandle,
    id: AppId,
    key: String,
) -> Result<Outcome, String> {
    let state = app.state::<Library>();
    let (_hold, _) = state.operations.begin(id)?;
    let _ = state.operations.finish_download();
    let lock = lock_directory(&state.directory)?;
    let work = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lock = lock;
        let library = work.state::<Library>();
        let installed = library
            .rescan_app(id)
            .into_iter()
            .find(|installed| installed.key() == key)
            .ok_or("The selected installation changed. Refresh the library.")?;
        publish(&work, id, Stage::Retaining, 0, None);
        let attempt = platform::retain(id, &library.directory, &installed);
        let verified = library.rescan_app(id).iter().any(|entry| {
            entry.origin == Origin::RetainedCopy && same_version(&entry.version, &installed.version)
        });
        attempt?;
        if !verified {
            return Err("The saved copy could not be verified.".into());
        }
        Ok(Outcome {
            kind: ResultKind::Retained,
            message: format!("Kept a copy of version {}.", installed.version),
            retry_graphically: false,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

fn file_error(error: std::io::Error) -> String {
    format!("Could not store the package ({error}).")
}

async fn download(
    client: &Client,
    id: AppId,
    release: &Release,
    path: &Path,
    progress: impl FnMut(u64, Option<u64>),
) -> Result<String, String> {
    let package = &release.package;
    let url = releases::check_package_source(id, package)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Download failed ({error})."))?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(format!(
            "The publisher returned HTTP {} for the download.",
            response.status()
        ));
    }
    if let (Some(declared), Some(actual)) = (package.size, response.content_length())
        && declared != actual
    {
        return Err("The server's package size differs from the publisher metadata.".into());
    }
    let total = package.size.or(response.content_length());
    let file = tokio::fs::File::create(path).await.map_err(file_error)?;
    write_stream(
        response.bytes_stream(),
        file,
        total,
        package.sha256.as_deref(),
        progress,
    )
    .await
}

async fn write_stream<B, E>(
    chunks: impl Stream<Item = Result<B, E>>,
    mut file: tokio::fs::File,
    total: Option<u64>,
    expected_hash: Option<&str>,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<String, String>
where
    B: AsRef<[u8]>,
    E: std::fmt::Display,
{
    if total.is_some_and(|length| length == 0 || length > MAX_PACKAGE_BYTES) {
        return Err("The published package size is outside the supported range.".into());
    }
    let mut hasher = Sha256::new();
    let mut downloaded = 0;
    progress(downloaded, total);
    let mut then = Instant::now();
    futures_util::pin_mut!(chunks);
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|error| format!("The download was interrupted ({error})."))?;
        let bytes = chunk.as_ref();
        downloaded += bytes.len() as u64;
        if downloaded > total.unwrap_or(MAX_PACKAGE_BYTES) {
            return Err("The download exceeded the expected package size.".into());
        }
        file.write_all(bytes).await.map_err(file_error)?;
        hasher.update(bytes);
        if then.elapsed() >= Duration::from_millis(120) {
            progress(downloaded, total);
            then = Instant::now();
        }
    }
    if downloaded == 0 || total.is_some_and(|total| downloaded != total) {
        return Err("The publisher package download was incomplete.".into());
    }
    let digest = hex::encode(hasher.finalize());
    if expected_hash.is_some_and(|expected| !expected.eq_ignore_ascii_case(&digest)) {
        return Err("The download did not match the publisher's SHA-256 checksum.".into());
    }
    file.flush().await.map_err(file_error)?;
    file.sync_all().await.map_err(file_error)?;
    progress(downloaded, total);
    Ok(digest)
}

fn validate_package(path: &Path, kind: PackageKind, target: Target) -> Result<(), String> {
    let mut file = File::open(path).map_err(file_error)?;
    let mut header = [0; 64];
    file.read_exact(&mut header).map_err(file_error)?;
    if !kind.matches_prefix(&header, target) {
        return Err("The download is not the expected package type for this computer.".into());
    }
    if kind == PackageKind::Dmg {
        let mut trailer = [0; 4];
        file.seek(SeekFrom::End(-512))
            .and_then(|_| file.read_exact(&mut trailer))
            .map_err(file_error)?;
        if &trailer != b"koly" {
            return Err("The downloaded disk image is invalid or incomplete.".into());
        }
    }
    if kind == PackageKind::AppImage {
        let (sections, section_size, section_count) = match header[4] {
            1 => (
                u32::from_le_bytes(header[32..36].try_into().unwrap()) as u64,
                u16::from_le_bytes(header[46..48].try_into().unwrap()) as u64,
                u16::from_le_bytes(header[48..50].try_into().unwrap()) as u64,
            ),
            _ => (
                u64::from_le_bytes(header[40..48].try_into().unwrap()),
                u16::from_le_bytes(header[58..60].try_into().unwrap()) as u64,
                u16::from_le_bytes(header[60..62].try_into().unwrap()) as u64,
            ),
        };
        let offset = sections
            .checked_add(section_size * section_count)
            .filter(|offset| *offset >= 64)
            .ok_or("The AppImage filesystem offset is invalid.")?;
        let mut filesystem = [0; 4];
        file.seek(SeekFrom::Start(offset))
            .and_then(|_| file.read_exact(&mut filesystem))
            .map_err(file_error)?;
        if &filesystem != b"hsqs" {
            return Err("The AppImage does not contain the expected SquashFS filesystem.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    #[test]
    fn only_one_download_can_run_and_cancellation_ends_at_verification() {
        let operations = Operations::default();
        let (hold, token) = operations.begin(AppId::Chataigne).unwrap();
        assert!(operations.begin(AppId::BenTo).is_err());
        assert!(!operations.cancel(AppId::BenTo));
        assert!(operations.cancel(AppId::Chataigne));
        assert!(token.is_cancelled());
        assert!(!operations.finish_download());
        assert!(!operations.cancel(AppId::Chataigne));
        drop(hold);
        assert!(operations.begin(AppId::BenTo).is_ok());
    }

    #[test]
    fn stream_rejects_truncation_interruption_and_bad_digest() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let chunks = || stream::iter([Ok::<_, &str>(b"package")]);
            let path = temp.path().join("download");
            let file = tokio::fs::File::create(&path).await.unwrap();
            assert!(
                write_stream(chunks(), file, Some(12), None, |_, _| {})
                    .await
                    .unwrap_err()
                    .contains("incomplete")
            );
            let file = tokio::fs::File::create(&path).await.unwrap();
            let broken = stream::iter([Ok(b"package"), Err("disconnected")]);
            assert!(
                write_stream(broken, file, None, None, |_, _| {})
                    .await
                    .unwrap_err()
                    .contains("interrupted")
            );
            let file = tokio::fs::File::create(&path).await.unwrap();
            assert!(
                write_stream(chunks(), file, Some(7), Some("bad"), |_, _| {})
                    .await
                    .unwrap_err()
                    .contains("SHA-256")
            );
        });
    }

    #[test]
    fn installer_exit_requires_exact_verified_version_for_success() {
        let installed = Installed {
            version: "1.0.0".into(),
            can_uninstall: true,
            path: "old-app".into(),
            origin: Origin::Native,
        };
        let verified = Some(&installed);
        let failed = outcome(
            Ok(InstallResult::Finished),
            verified,
            "1.0.1",
            true,
            false,
            Target::WinX64,
        );
        assert!(matches!(failed.kind, ResultKind::Failed));
        assert!(failed.retry_graphically);
        let cancelled = outcome(
            Ok(InstallResult::Cancelled),
            verified,
            "1.0.1",
            true,
            false,
            Target::WinX64,
        );
        assert!(matches!(cancelled.kind, ResultKind::Cancelled));
        let denied = outcome(
            Err("Permission denied.".into()),
            verified,
            "1.0.1",
            true,
            false,
            Target::WinX64,
        );
        assert!(matches!(denied.kind, ResultKind::Failed));
        let success = outcome(
            Ok(InstallResult::Finished),
            verified,
            "1.0.0",
            true,
            false,
            Target::WinX64,
        );
        assert!(matches!(success.kind, ResultKind::Installed));
    }

    #[test]
    fn unrecognized_package_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("invalid.AppImage");
        fs::write(&path, [0u8; 64]).unwrap();
        assert!(validate_package(&path, PackageKind::AppImage, Target::LinuxX64).is_err());
    }

    #[test]
    #[ignore = "downloads official packages and installs on a disposable desktop"]
    fn official_chataigne_platform_smoke() {
        assert_eq!(std::env::var("GOLDEN_DISPOSABLE_SMOKE").as_deref(), Ok("1"));
        let id = AppId::Chataigne;
        let target = Target::current();
        let work = tempfile::tempdir().unwrap();
        let client = releases::client().unwrap();
        assert!(
            platform::installed_versions(id, work.path(), target).is_empty(),
            "use a fresh test environment without Chataigne"
        );

        for version in ["1.10.4", "1.10.5b14"] {
            let release = releases::chataigne_smoke_release(version, target);
            let path = work.path().join(format!(
                "Chataigne-{}.{}",
                release.version,
                release.package.kind.extension()
            ));
            let digest =
                tauri::async_runtime::block_on(download(&client, id, &release, &path, |_, _| {}))
                    .unwrap();
            validate_package(&path, release.package.kind, target).unwrap();
            #[cfg(target_os = "macos")]
            {
                let _ = digest;
                platform::install_on_disposable_runner(id, &path, &release, target).unwrap();
                let versions = platform::installed_versions(id, work.path(), target);
                assert_eq!(versions.len(), 1, "expected one native installation");
                let installed = &versions[0];
                assert!(same_version(&installed.version, version));
                println!(
                    "Verified installed Chataigne {} at {}",
                    installed.version,
                    installed.path.display()
                );
                platform::check_disposable_launch(id, installed, target).unwrap();
            }
            #[cfg(not(target_os = "macos"))]
            {
                #[cfg(target_os = "linux")]
                let result =
                    platform::install(id, work.path(), &path, &release, &digest, false, target);
                #[cfg(windows)]
                let result = {
                    let _ = digest;
                    println!(
                        "Running verified official Chataigne {version} on the disposable Windows runner."
                    );
                    platform::install_on_disposable_runner(id, &path, &release)
                };
                let result = result.unwrap();
                assert!(matches!(result, InstallResult::Finished));
                let found = platform::detect(id, work.path(), target).expect("installed Chataigne");
                assert!(same_version(&found.version, version));
                println!(
                    "Verified installed Chataigne {} at {}",
                    found.version,
                    found.path.display()
                );
                #[cfg(windows)]
                if version == "1.10.4" {
                    platform::retain(id, work.path(), &found).unwrap();
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let versions = platform::installed_versions(id, work.path(), target);
            assert_eq!(versions.len(), 2, "both versions should remain installed");
            assert!(same_version(&versions[0].version, "1.10.5b14"));
            assert!(same_version(&versions[1].version, "1.10.4"));
            for installed in &versions {
                let pid = platform::launch(installed).unwrap();
                let found = (0..20).any(|_| {
                    std::thread::sleep(Duration::from_millis(500));
                    chataigne_running(pid)
                });
                assert!(found, "Chataigne process did not start");
                std::thread::sleep(Duration::from_secs(2));
                assert!(chataigne_running(pid), "Chataigne quit during launch");
                println!("Launched Chataigne {} (PID {pid}).", installed.version);
                stop_smoke_app(pid);
                assert!(
                    (0..20).any(|_| {
                        std::thread::sleep(Duration::from_millis(250));
                        !chataigne_running(pid)
                    }),
                    "Chataigne did not exit after the test"
                );
            }
            assert!(matches!(
                platform::uninstall(id, work.path(), &versions[1]),
                Ok(InstallResult::Finished)
            ));
            let remaining = platform::installed_versions(id, work.path(), target);
            assert_eq!(remaining.len(), 1);
            assert_eq!(remaining[0].key(), versions[0].key());
            println!("Removed Chataigne 1.10.4 and verified 1.10.5b14 is still installed.");
        }
    }

    #[cfg(target_os = "linux")]
    fn chataigne_running(pid: u32) -> bool {
        fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|name| name.trim() == "Chataigne")
    }

    #[cfg(windows)]
    fn chataigne_running(pid: u32) -> bool {
        std::process::Command::new("tasklist.exe")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .is_ok_and(|output| {
                String::from_utf8_lossy(&output.stdout).contains("\"Chataigne.exe\"")
            })
    }

    #[cfg(target_os = "linux")]
    fn stop_smoke_app(pid: u32) {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
    }

    #[cfg(windows)]
    fn stop_smoke_app(pid: u32) {
        let _ = std::process::Command::new("taskkill.exe")
            .args(["/F", "/PID", &pid.to_string()])
            .status();
    }
}

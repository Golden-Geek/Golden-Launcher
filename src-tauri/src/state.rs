use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::{
    catalog::{AppId, PRODUCTS},
    operations::Operations,
    platform::{self, Installed, Origin, same_version},
    releases::{self, Channel, Release, Resolved, Target},
};

#[derive(Default, Deserialize, Serialize)]
struct SavedState {
    channels: HashMap<AppId, Channel>,
    releases: HashMap<AppId, Resolved>,
    target: String,
}

#[derive(Default)]
struct Session {
    saved: SavedState,
    fresh: HashMap<AppId, Result<(), String>>,
    installed: HashMap<AppId, Vec<Installed>>,
}

pub struct Library {
    pub(crate) directory: PathBuf,
    pub(crate) client: Client,
    pub(crate) target: Target,
    pub(crate) operations: Operations,
    session: Mutex<Session>,
    scanning: Mutex<()>,
    refreshing: AtomicBool,
}

impl Library {
    pub fn new(directory: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let target = Target::current();
        let mut saved: SavedState = fs::read(directory.join("library.json"))
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        if saved.target != format!("{target:?}") {
            saved.releases.clear();
            saved.target = format!("{target:?}");
        }
        let library = Self {
            directory,
            client: releases::client()?,
            target,
            operations: Operations::default(),
            session: Mutex::new(Session {
                saved,
                ..Session::default()
            }),
            scanning: Mutex::new(()),
            refreshing: AtomicBool::new(false),
        };
        library.rescan();
        Ok(library)
    }

    fn rescan(&self) {
        let _scan = self.scanning.lock().expect("scan state");
        let installed = PRODUCTS
            .iter()
            .map(|product| {
                (
                    product.id,
                    platform::installed_versions(product.id, &self.directory, self.target),
                )
            })
            .collect();
        self.session.lock().expect("library state").installed = installed;
    }

    pub(crate) fn rescan_app(&self, id: AppId) -> Vec<Installed> {
        let _scan = self.scanning.lock().expect("scan state");
        let found = platform::installed_versions(id, &self.directory, self.target);
        self.session
            .lock()
            .expect("library state")
            .installed
            .insert(id, found.clone());
        found
    }

    pub(crate) fn selected_release(&self, id: AppId, requested: &str) -> Result<Release, String> {
        let session = self.session.lock().expect("library state");
        let channel = session.saved.channels.get(&id).copied().unwrap_or_default();
        session
            .saved
            .releases
            .get(&id)
            .and_then(|builds| builds.for_channel(channel))
            .filter(|release| release.version == requested)
            .cloned()
            .ok_or_else(|| {
                "The selected release changed. Refresh the library and try again.".into()
            })
    }

    pub(crate) fn snapshot(&self) -> Vec<AppView> {
        let session = self.session.lock().expect("library state");
        PRODUCTS
            .iter()
            .map(|product| {
                let id = product.id;
                let channel = session.saved.channels.get(&id).copied().unwrap_or_default();
                let resolved = session.saved.releases.get(&id);
                let freshness = session.fresh.get(&id);
                let available = resolved.and_then(|builds| builds.for_channel(channel));
                let installed = session.installed.get(&id).map_or(&[][..], Vec::as_slice);
                let installed_versions: Vec<_> = installed
                    .iter()
                    .map(|app| InstalledView {
                        key: app.key(),
                        version: app.version.clone(),
                        origin: app.origin,
                        can_uninstall: app.can_uninstall,
                        can_retain: platform::can_retain(id, app)
                            && !installed.iter().any(|copy| {
                                copy.origin == Origin::RetainedCopy
                                    && same_version(&copy.version, &app.version)
                            }),
                    })
                    .collect();
                let availability = match (available, resolved) {
                    (Some(_), _) => Availability::Available,
                    (None, Some(builds))
                        if channel == Channel::Stable && builds.preview.is_some() =>
                    {
                        Availability::PreviewOnly
                    }
                    (None, Some(builds))
                        if !builds.published
                            && matches!(id, AppId::Chataigne2 | AppId::MapGyver2) =>
                    {
                        Availability::AwaitingRelease
                    }
                    (None, Some(_)) => Availability::Unavailable,
                    _ => Availability::Unknown,
                };
                AppView {
                    id,
                    name: product.name,
                    description: product.description,
                    channel,
                    availability,
                    stale: resolved.is_some() && !matches!(freshness, Some(Ok(()))),
                    refresh_error: freshness.and_then(|result| result.as_ref().err()).cloned(),
                    installed: installed_versions.first().cloned(),
                    installed_versions,
                    install_action: available
                        .and_then(|release| install_action(release, installed)),
                    available: available.map(|release| ReleaseView {
                        version: release.version.clone(),
                        preview: release.preview,
                        notes: release.notes.clone(),
                    }),
                }
            })
            .collect()
    }

    fn save(&self, saved: &SavedState) -> Result<(), String> {
        write_json(&self.directory.join("library.json"), saved)
            .map_err(|error| format!("Could not save launcher data ({error})."))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallAction {
    Install,
    Update,
    Alongside,
}

pub(crate) fn install_action(release: &Release, installed: &[Installed]) -> Option<InstallAction> {
    let Some(current) = installed.first() else {
        return Some(InstallAction::Install);
    };
    if installed
        .iter()
        .any(|app| same_version(&release.version, &app.version))
    {
        return None;
    }
    if installed
        .iter()
        .all(|app| app.origin == Origin::RetainedCopy)
    {
        return Some(InstallAction::Alongside);
    }
    if releases::newer(&release.version, &current.version) {
        return Some(InstallAction::Update);
    }
    (current.origin == Origin::AppImage).then_some(InstallAction::Alongside)
}

pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let parent = path.parent().expect("state file directory");
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut temporary, value)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    Available,
    PreviewOnly,
    AwaitingRelease,
    Unavailable,
    Unknown,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseView {
    version: String,
    preview: bool,
    notes: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledView {
    key: String,
    version: String,
    origin: Origin,
    can_uninstall: bool,
    can_retain: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppView {
    id: AppId,
    name: &'static str,
    description: &'static str,
    channel: Channel,
    availability: Availability,
    stale: bool,
    refresh_error: Option<String>,
    installed: Option<InstalledView>,
    installed_versions: Vec<InstalledView>,
    install_action: Option<InstallAction>,
    available: Option<ReleaseView>,
}

#[tauri::command]
pub fn library(state: State<'_, Library>) -> Vec<AppView> {
    state.snapshot()
}

struct RefreshGuard<'a>(&'a AtomicBool);

impl Drop for RefreshGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[tauri::command]
pub async fn refresh(app: AppHandle) -> Result<Vec<AppView>, String> {
    let state = app.state::<Library>();
    if state.refreshing.swap(true, Ordering::Acquire) {
        return Err("A refresh is already running.".into());
    }
    let _refresh = RefreshGuard(&state.refreshing);
    let scan = app.clone();
    tauri::async_runtime::spawn_blocking(move || scan.state::<Library>().rescan())
        .await
        .map_err(|error| error.to_string())?;
    let results = releases::refresh_all(&state.client, state.target).await;
    {
        let mut session = state.session.lock().expect("library state");
        for (id, result) in results {
            let fresh = result.map(|resolved| {
                session.saved.releases.insert(id, resolved);
            });
            session.fresh.insert(id, fresh);
        }
        state.save(&session.saved)?;
    }
    Ok(state.snapshot())
}

#[tauri::command]
pub fn set_channel(
    state: State<'_, Library>,
    id: AppId,
    channel: Channel,
) -> Result<Vec<AppView>, String> {
    {
        let mut session = state.session.lock().expect("library state");
        let previous = session.saved.channels.insert(id, channel);
        if let Err(error) = state.save(&session.saved) {
            if let Some(previous) = previous {
                session.saved.channels.insert(id, previous);
            } else {
                session.saved.channels.remove(&id);
            }
            return Err(error);
        }
    }
    Ok(state.snapshot())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alongside_installation_does_not_downgrade_a_native_install() {
        let mut installed = Installed {
            version: "1.10.5b14".into(),
            can_uninstall: true,
            path: "installed".into(),
            origin: Origin::Native,
        };
        let stable = releases::chataigne_smoke_release("1.10.4", Target::WinX64);
        assert_eq!(install_action(&stable, &[]), Some(InstallAction::Install));
        assert_eq!(
            install_action(&stable, std::slice::from_ref(&installed)),
            None
        );
        installed.origin = Origin::RetainedCopy;
        assert_eq!(
            install_action(&stable, std::slice::from_ref(&installed)),
            Some(InstallAction::Alongside)
        );
        installed.origin = Origin::AppImage;
        assert_eq!(
            install_action(&stable, std::slice::from_ref(&installed)),
            Some(InstallAction::Alongside)
        );
        let preview = releases::chataigne_smoke_release("1.10.5b14", Target::LinuxX64);
        assert_eq!(
            install_action(&preview, std::slice::from_ref(&installed)),
            None
        );
        installed.version = stable.version.clone();
        assert_eq!(
            install_action(&preview, &[installed]),
            Some(InstallAction::Update)
        );
    }
}

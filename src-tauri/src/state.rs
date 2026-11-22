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

    fn cached_releases(&self, id: AppId) -> Option<Resolved> {
        self.session
            .lock()
            .expect("library state")
            .saved
            .releases
            .get(&id)
            .cloned()
    }

    pub(crate) async fn selected_release(
        &self,
        id: AppId,
        channel: Channel,
        requested: &str,
    ) -> Result<Release, String> {
        let resolved = self
            .cached_releases(id)
            .ok_or("Release data is unavailable. Refresh the library and try again.")?;
        releases::compatible_version(&self.client, id, self.target, &resolved, channel, requested)
            .await?.ok_or_else(|| "The selected version has no compatible installer available. Refresh the library and try again.".into())
    }

    pub(crate) fn snapshot(&self) -> Vec<AppView> {
        let session = self.session.lock().expect("library state");
        PRODUCTS
            .iter()
            .map(|product| {
                let id = product.id;
                let resolved = session.saved.releases.get(&id);
                let freshness = session.fresh.get(&id);
                let installed = session.installed.get(&id).map_or(&[][..], Vec::as_slice);
                let release_view = |channel| {
                    resolved
                        .and_then(|builds| builds.for_channel(channel))
                        .map(|release| ReleaseView::new(release, installed))
                };
                let stable = release_view(Channel::Stable);
                let beta = release_view(Channel::Beta);
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
                let availability = match resolved {
                    _ if stable.is_some() || beta.is_some() => Availability::Available,
                    Some(builds)
                        if !builds.published
                            && matches!(id, AppId::Chataigne2 | AppId::MapGyver2) =>
                    {
                        Availability::AwaitingRelease
                    }
                    Some(_) => Availability::Unavailable,
                    _ => Availability::Unknown,
                };
                AppView {
                    id,
                    name: product.name,
                    description: product.description,
                    availability,
                    stale: resolved.is_some() && !matches!(freshness, Some(Ok(()))),
                    refresh_error: freshness.and_then(|result| result.as_ref().err()).cloned(),
                    installed: installed_versions.first().cloned(),
                    installed_versions,
                    releases: ReleaseChoices { stable, beta },
                }
            })
            .collect()
    }

    fn save(&self, saved: &SavedState) -> Result<(), String> {
        write_json(&self.directory.join("library.json"), saved)
            .map_err(|error| format!("Could not save launcher data ({error})."))
    }
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InstallAction {
    Install,
    Update,
    Alongside,
    Replace { key: String, version: String },
    Installed,
    Unavailable,
}

pub(crate) fn install_action(release: &Release, installed: &[Installed]) -> InstallAction {
    if installed.is_empty() {
        return InstallAction::Install;
    }
    if installed
        .iter()
        .any(|app| same_version(&release.version, &app.version))
    {
        return InstallAction::Installed;
    }
    let Some(current) = installed
        .iter()
        .find(|app| app.origin != Origin::RetainedCopy)
    else {
        return InstallAction::Alongside;
    };
    if releases::newer(&release.version, &current.version) {
        return InstallAction::Update;
    }
    if current.origin == Origin::AppImage {
        return InstallAction::Alongside;
    }
    if releases::newer(&current.version, &release.version) {
        return InstallAction::Replace {
            key: current.key(),
            version: current.version.clone(),
        };
    }
    InstallAction::Unavailable
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
    AwaitingRelease,
    Unavailable,
    Unknown,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseView {
    version: String,
    notes: Vec<String>,
    action: InstallAction,
}

impl ReleaseView {
    fn new(release: &Release, installed: &[Installed]) -> Self {
        Self {
            version: release.version.clone(),
            notes: release.notes.clone(),
            action: install_action(release, installed),
        }
    }
}

#[derive(Serialize)]
struct ReleaseChoices {
    stable: Option<ReleaseView>,
    beta: Option<ReleaseView>,
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
    availability: Availability,
    stale: bool,
    refresh_error: Option<String>,
    installed: Option<InstalledView>,
    installed_versions: Vec<InstalledView>,
    releases: ReleaseChoices,
}

#[tauri::command]
pub fn library(state: State<'_, Library>) -> Vec<AppView> {
    state.snapshot()
}

#[derive(Deserialize, Serialize)]
pub struct VersionCursor {
    channel: Channel,
    version: String,
}

#[derive(Serialize)]
struct PublishedVersionView {
    channel: Channel,
    release: ReleaseView,
}

#[derive(Default, Serialize)]
pub struct VersionPage {
    versions: Vec<PublishedVersionView>,
    next: Option<VersionCursor>,
    incomplete: bool,
}

#[tauri::command]
pub async fn release_versions(
    app: AppHandle,
    id: AppId,
    after: Option<VersionCursor>,
) -> Result<VersionPage, String> {
    const PAGE_SIZE: usize = 12;
    let state = app.state::<Library>();
    let Some(resolved) = state.cached_releases(id) else {
        return Ok(VersionPage::default());
    };
    let known = resolved.versions();
    let start = match after {
        None => 0,
        Some(cursor) => known
            .iter()
            .position(|&(channel, version)| channel == cursor.channel && version == cursor.version)
            .map(|index| index + 1)
            .ok_or("The version list changed. Reopen it to see the latest versions.")?,
    };
    let end = (start + PAGE_SIZE).min(known.len());
    let client = &state.client;
    let target = state.target;
    let outcomes =
        futures_util::future::join_all(known[start..end].iter().map(|&(channel, version)| {
            let resolved = &resolved;
            async move {
                (
                    channel,
                    releases::compatible_version(client, id, target, resolved, channel, version)
                        .await,
                )
            }
        }))
        .await;
    let session = state.session.lock().expect("library state");
    let installed = session.installed.get(&id).map_or(&[][..], Vec::as_slice);
    let mut page = VersionPage::default();
    for (channel, outcome) in outcomes {
        match outcome {
            Ok(Some(release)) => page.versions.push(PublishedVersionView {
                channel,
                release: ReleaseView::new(&release, installed),
            }),
            Ok(None) => (),
            Err(_) => page.incomplete = true,
        }
    }
    if end < known.len() {
        let (channel, version) = known[end - 1];
        page.next = Some(VersionCursor {
            channel,
            version: version.into(),
        });
    }
    Ok(page)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_release_needs_native_replacement_and_can_install_alongside_an_appimage() {
        let mut installed = Installed {
            version: "1.10.5b14".into(),
            can_uninstall: true,
            path: "installed".into(),
            origin: Origin::Native,
        };
        let stable = releases::chataigne_smoke_release("1.10.4", Target::WinX64);
        assert_eq!(install_action(&stable, &[]), InstallAction::Install);
        assert_eq!(
            install_action(&stable, std::slice::from_ref(&installed)),
            InstallAction::Replace {
                key: installed.key(),
                version: installed.version.clone()
            }
        );
        installed.origin = Origin::RetainedCopy;
        assert_eq!(
            install_action(&stable, std::slice::from_ref(&installed)),
            InstallAction::Alongside
        );
        installed.origin = Origin::AppImage;
        assert_eq!(
            install_action(&stable, std::slice::from_ref(&installed)),
            InstallAction::Alongside
        );
        let beta = releases::chataigne_smoke_release("1.10.5b14", Target::LinuxX64);
        assert_eq!(
            install_action(&beta, std::slice::from_ref(&installed)),
            InstallAction::Installed
        );
        installed.version = stable.version.clone();
        assert_eq!(install_action(&beta, &[installed]), InstallAction::Update);
    }
}

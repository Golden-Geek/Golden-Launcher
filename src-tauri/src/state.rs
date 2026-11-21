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
use tauri::State;

use crate::{
    catalog::{AppId, PRODUCTS},
    releases::{self, Channel, Resolved, Target},
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
}

pub struct Library {
    directory: PathBuf,
    pub(crate) client: Client,
    pub(crate) target: Target,
    session: Mutex<Session>,
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
        Ok(Self {
            directory,
            client: releases::client()?,
            target,
            session: Mutex::new(Session {
                saved,
                fresh: HashMap::new(),
            }),
            refreshing: AtomicBool::new(false),
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
pub async fn refresh(state: State<'_, Library>) -> Result<Vec<AppView>, String> {
    if state.refreshing.swap(true, Ordering::Acquire) {
        return Err("A refresh is already running.".into());
    }
    let _refresh = RefreshGuard(&state.refreshing);
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

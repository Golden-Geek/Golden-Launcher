use std::{sync::Arc, time::Duration};

use tauri::{AppHandle, Manager, utils::platform::bundle_type};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

use crate::{operations::lock_directory, state::Library};

const RELEASE_ASSETS: &str = "https://github.com/Golden-Geek/Golden-Launcher/releases/download/";

struct Package {
    update: Update,
    bytes: Vec<u8>,
}

#[derive(Default)]
pub(crate) struct StagedUpdate(Mutex<Option<Arc<Package>>>);

#[tauri::command]
pub async fn prepare_launcher_update(app: AppHandle) -> Result<Option<String>, String> {
    if cfg!(debug_assertions) || bundle_type().is_none() {
        return Ok(None);
    }

    let staged = app.state::<StagedUpdate>();
    let mut package = staged.0.lock().await;
    if let Some(package) = package.as_ref() {
        return Ok(Some(package.update.version.clone()));
    }

    let updater = app
        .updater_builder()
        .timeout(Duration::from_secs(20))
        .configure_client(|builder| builder.https_only(true))
        .build()
        .map_err(|error| error.to_string())?;
    let Some(mut update) = updater.check().await.map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    let expected_prefix = format!("{RELEASE_ASSETS}v{}/", update.version);
    if !update.download_url.as_str().starts_with(&expected_prefix) {
        return Err("The launcher update was not published with the matching release.".into());
    }
    update.timeout = Some(Duration::from_secs(180));
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())?;
    let version = update.version.clone();
    *package = Some(Arc::new(Package { update, bytes }));
    Ok(Some(version))
}

#[tauri::command]
pub async fn install_launcher_update(app: AppHandle) -> Result<(), String> {
    let staged = app.state::<StagedUpdate>();
    let mut pending = staged.0.lock().await;
    let package = pending
        .as_ref()
        .cloned()
        .ok_or_else(|| "No launcher update is ready.".to_owned())?;
    let lock = lock_directory(&app.state::<Library>().directory)?;
    tauri::async_runtime::spawn_blocking(move || {
        // Keep the installer locked until the native update completes.
        let _lock = lock;
        package
            .update
            .install(&package.bytes)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())??;

    *pending = None;
    app.request_restart();
    Ok(())
}

//! Updates from the releases of the project on GitHub. The app asks `latest.json` of the latest
//! release whether there is a newer version; the files are signed with a key whose public half is
//! in `tauri.conf.json`, so only a build made by the owner of the private key is installed.

use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const AVAILABLE_EVENT: &str = "update-available";
/// The first check waits a little so it does not compete with the start of the app.
const STARTUP_DELAY: Duration = Duration::from_secs(15);

/// A newer version that can be installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// What changed, as written in the release.
    pub notes: Option<String>,
}

impl UpdateInfo {
    pub fn new(version: &str, current_version: &str, notes: Option<&str>) -> UpdateInfo {
        UpdateInfo {
            version: version.to_string(),
            current_version: current_version.to_string(),
            notes: notes
                .map(str::trim)
                .filter(|notes| !notes.is_empty())
                .map(str::to_string),
        }
    }
}

/// The update found by the last check, kept until it is installed.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

impl PendingUpdate {
    fn set(&self, update: Option<Update>) {
        if let Ok(mut pending) = self.0.lock() {
            *pending = update;
        }
    }

    fn take(&self) -> Option<Update> {
        self.0.lock().ok().and_then(|mut pending| pending.take())
    }
}

/// Asks whether a newer version exists; the update is remembered for [`install`].
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(|error| error.to_string())?;
    let found = updater.check().await.map_err(|error| error.to_string())?;
    let info = found.as_ref().map(|update| {
        UpdateInfo::new(
            &update.version,
            &update.current_version,
            update.body.as_deref(),
        )
    });
    if let Some(pending) = app.try_state::<PendingUpdate>() {
        pending.set(found);
    }
    Ok(info)
}

/// Downloads and installs the update that the last check found (or checks again), then restarts
/// the app.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let pending = app
        .try_state::<PendingUpdate>()
        .and_then(|pending| pending.take());
    let update = match pending {
        Some(update) => update,
        None => {
            let updater = app.updater().map_err(|error| error.to_string())?;
            updater
                .check()
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "There is no newer version".to_string())?
        }
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())?;
    app.restart()
}

/// Checks once, a little after the app starts, and tells the window when there is a newer version.
pub fn check_in_background(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(STARTUP_DELAY);
        let found = tauri::async_runtime::block_on(check(&app));
        if let Ok(Some(info)) = found {
            let _ = app.emit(AVAILABLE_EVENT, info);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_update_serializes_with_camel_case_keys() {
        let info = UpdateInfo::new("0.2.0", "0.1.0", Some("Faster lookups"));

        assert_eq!(
            serde_json::to_value(info).expect("json"),
            serde_json::json!({
                "version": "0.2.0",
                "currentVersion": "0.1.0",
                "notes": "Faster lookups"
            })
        );
    }

    #[test]
    fn blank_notes_are_no_notes_and_other_notes_are_trimmed() {
        assert_eq!(UpdateInfo::new("1", "0", Some("  \n ")).notes, None);
        assert_eq!(UpdateInfo::new("1", "0", None).notes, None);
        assert_eq!(
            UpdateInfo::new("1", "0", Some("  Fixes\n")).notes,
            Some("Fixes".to_string())
        );
    }
}

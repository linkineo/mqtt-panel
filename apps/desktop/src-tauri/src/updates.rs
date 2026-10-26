//! Self-update of the direct macOS download (docs/UPDATER.md). Only the Developer ID DMG variant is built with the
//! cargo feature `self-update`; without it not one line of updater code is compiled, `update_status` reports
//! `selfUpdate: false` and the other commands answer a plain error. The window never calls the plugin: it uses these
//! commands through the engine adapter like every other feature.

use crate::state::{lock, AppState};
use mqttpanel_client::{msg, UserMessage};
use mqttpanel_profiles::Settings;
use serde::Serialize;
use tauri::{AppHandle, Manager};

/// What Settings → Updates shows (serialised like `UpdateStatus` in packages/engine-client/src/types.ts).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    /// This build updates itself (the DMG variant); false in store and developer builds.
    pub self_update: bool,
    pub auto: bool,
    /// The last successful check, RFC 3339 (UTC).
    pub last_check: Option<String>,
    /// idle | checking | latest | available | downloading | ready | error
    pub state: String,
    /// The version offered, its date (YYYY-MM-DD) and its notes (plain text).
    pub version: Option<String>,
    pub date: Option<String>,
    pub notes: Option<String>,
    /// Download progress in bytes; `total` is null while the server does not say.
    pub downloaded: u64,
    pub total: Option<u64>,
    /// False when the app cannot replace its own bundle (run from the DMG, translocated, read-only folder): the
    /// panel then says "Move MQTT Panel to Applications first" instead of offering the install.
    pub can_install: bool,
    pub error: Option<UserMessage>,
}

impl UpdateStatus {
    fn idle(settings: &Settings, self_update: bool) -> Self {
        UpdateStatus {
            self_update,
            auto: settings.updates.auto,
            last_check: settings.updates.last_check.clone(),
            state: "idle".into(),
            version: None,
            date: None,
            notes: None,
            downloaded: 0,
            total: None,
            can_install: true,
            error: None,
        }
    }
}

/// The status of a build without the updater (store builds, developer builds).
pub fn store_status(settings: &Settings) -> UpdateStatus {
    UpdateStatus::idle(settings, false)
}

/// True when the last check is missing, unreadable or at least 24 hours before `now`.
#[cfg_attr(not(feature = "self-update"), allow(dead_code))]
pub fn check_due(last_check: Option<&str>, now: time::OffsetDateTime) -> bool {
    use time::format_description::well_known::Rfc3339;
    match last_check.and_then(|s| time::OffsetDateTime::parse(s, &Rfc3339).ok()) {
        Some(last) => now - last >= time::Duration::hours(24) || last > now + time::Duration::hours(1),
        None => true,
    }
}

/// True when the app can replace its own bundle (an executable outside a `.app` bundle never can). Translocated copies (opened from Downloads without being moved)
/// and copies run from the DMG sit on read-only file systems; a folder the user cannot write to is left to the
/// plugin, which asks for an administrator password as macOS does when an app is moved to Applications.
#[cfg_attr(not(feature = "self-update"), allow(dead_code))]
pub fn can_replace_bundle(exe: &std::path::Path) -> bool {
    let Some(bundle) = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")) else {
        // A developer build outside a bundle: the plugin would replace the folder of the executable, never that.
        return false;
    };
    if bundle.to_string_lossy().contains("/AppTranslocation/") {
        return false;
    }
    let Some(parent) = bundle.parent() else { return false };
    let probe = parent.join(format!(".mqttpanel-update-probe-{}", std::process::id()));
    match std::fs::create_dir(&probe) {
        Ok(()) => {
            let _ = std::fs::remove_dir(&probe);
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => true,
        Err(_) => false,
    }
}

/// True when this build carries the updater and it is configured (AppInfo `selfUpdate`).
pub fn enabled(app: &AppHandle) -> bool {
    imp::enabled(app)
}

fn settings_of(app: &AppHandle) -> Settings {
    lock(&app.state::<AppState>().settings).clone()
}

#[tauri::command]
pub fn update_status(app: AppHandle) -> UpdateStatus {
    imp::status(&app)
}

/// `manual`: "Check now" (always asks, reports every outcome). Otherwise the launch check: only when the automatic
/// check is on and the last check is older than 24 hours, silent unless an update is found.
#[tauri::command]
pub async fn update_check(app: AppHandle, manual: bool) -> Result<UpdateStatus, UserMessage> {
    imp::check(&app, manual).await
}

/// "Download and install": downloads with progress, the plugin verifies the signature and replaces the bundle.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<UpdateStatus, UserMessage> {
    imp::install(&app).await
}

/// "Restart now", once the new version is in place.
#[tauri::command]
pub fn update_restart(app: AppHandle) -> Result<(), UserMessage> {
    imp::restart(&app)
}

/// The switch "Check for updates automatically" (saved in settings.json, `updates.auto`).
#[tauri::command]
pub fn update_set_auto(app: AppHandle, on: bool) -> Result<UpdateStatus, UserMessage> {
    imp::enabled_or_err(&app)?;
    let st = app.state::<AppState>();
    let mut s = lock(&st.settings).clone();
    s.updates.auto = on;
    let saved = st.store.save_settings(&s)?;
    *lock(&st.settings) = saved;
    Ok(imp::status(&app))
}

#[cfg(not(feature = "self-update"))]
mod imp {
    use super::*;

    pub fn enabled(_app: &AppHandle) -> bool {
        false
    }
    pub fn enabled_or_err(_app: &AppHandle) -> Result<(), UserMessage> {
        Err(msg!("update.storeBuild"))
    }
    pub fn status(app: &AppHandle) -> UpdateStatus {
        store_status(&settings_of(app))
    }
    pub async fn check(_app: &AppHandle, _manual: bool) -> Result<UpdateStatus, UserMessage> {
        Err(msg!("update.storeBuild"))
    }
    pub async fn install(_app: &AppHandle) -> Result<UpdateStatus, UserMessage> {
        Err(msg!("update.storeBuild"))
    }
    pub fn restart(_app: &AppHandle) -> Result<(), UserMessage> {
        Err(msg!("update.storeBuild"))
    }
}

#[cfg(feature = "self-update")]
pub use imp::register;

#[cfg(feature = "self-update")]
mod imp {
    use super::*;
    use crate::types::EngineEvent;
    use std::sync::Mutex;
    use tauri::{Emitter, Runtime};
    use tauri_plugin_updater::{Error as UpdErr, Update, UpdaterExt};
    use time::format_description::well_known::Rfc3339;

    /// The updater state, managed only when the plugin is registered.
    pub struct Updates {
        status: Mutex<Option<UpdateStatus>>,
        pending: Mutex<Option<Update>>,
    }

    /// Registers the plugin when the configuration carries `plugins.updater` (the DMG overlay
    /// `tauri.dmg.conf.json`, or the test lab overlay). A feature build without it starts without the updater
    /// instead of failing at launch.
    pub fn register<R: Runtime>(builder: tauri::Builder<R>, context: &tauri::Context<R>) -> tauri::Builder<R> {
        if !context.config().plugins.0.contains_key("updater") {
            eprintln!("[mqttpanel] self-update: no plugins.updater configuration, updater off");
            return builder;
        }
        builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .manage(Updates { status: Mutex::new(None), pending: Mutex::new(None) })
    }

    pub fn enabled(app: &AppHandle) -> bool {
        app.try_state::<Updates>().is_some()
    }

    pub fn enabled_or_err(app: &AppHandle) -> Result<(), UserMessage> {
        if enabled(app) {
            Ok(())
        } else {
            Err(msg!("update.storeBuild"))
        }
    }

    fn current(app: &AppHandle, u: &Updates) -> UpdateStatus {
        let s = settings_of(app);
        let mut st = lock(&u.status).clone().unwrap_or_else(|| UpdateStatus::idle(&s, true));
        st.self_update = true;
        st.auto = s.updates.auto;
        st.last_check = s.updates.last_check.clone();
        st
    }

    fn set(app: &AppHandle, u: &Updates, st: &UpdateStatus) {
        *lock(&u.status) = Some(st.clone());
        let _ = app.emit("engine-event", EngineEvent::Update { status: st.clone() });
    }

    pub fn status(app: &AppHandle) -> UpdateStatus {
        match app.try_state::<Updates>() {
            Some(u) => {
                let mut st = current(app, &u);
                if st.state == "available" {
                    st.can_install = std::env::current_exe().map(|e| can_replace_bundle(&e)).unwrap_or(false);
                }
                st
            }
            None => store_status(&settings_of(app)),
        }
    }

    fn now() -> time::OffsetDateTime {
        time::OffsetDateTime::now_utc()
    }

    /// Plain-words message for a failed check, download or install.
    fn message(e: &UpdErr) -> UserMessage {
        match e {
            UpdErr::Minisign(_) | UpdErr::Base64(_) | UpdErr::SignatureUtf8(_) | UpdErr::SignedVersionMismatch { .. } | UpdErr::MissingSignedVersion => {
                msg!("update.badSignature")
            }
            UpdErr::TempDirNotOnSameMountPoint => msg!("update.moveFirst"),
            UpdErr::Io(io) if io.kind() == std::io::ErrorKind::ReadOnlyFilesystem => msg!("update.moveFirst"),
            UpdErr::Io(io) if io.kind() == std::io::ErrorKind::PermissionDenied => msg!("update.notAllowed"),
            UpdErr::PreviousAppNotRestored(p) => msg!("update.previousKept", path = p.display().to_string()),
            UpdErr::TargetNotFound(_) | UpdErr::TargetsNotFound(_) => msg!("update.noBuild"),
            _ => msg!("update.unreachable"),
        }
    }

    fn is_location_error(e: &UpdErr) -> bool {
        matches!(e, UpdErr::TempDirNotOnSameMountPoint) || matches!(e, UpdErr::Io(io) if io.kind() == std::io::ErrorKind::ReadOnlyFilesystem)
    }

    async fn ask(app: &AppHandle) -> Result<Option<Update>, UpdErr> {
        let mut b = app.updater_builder().timeout(std::time::Duration::from_secs(30));
        // Debug and test builds only: the test lab points the check at its own server (docs/UPDATER.md, Tests).
        if cfg!(debug_assertions) {
            if let Ok(url) = std::env::var("MQTTPANEL_UPDATE_ENDPOINT") {
                b = b.endpoints(vec![url.parse::<tauri::Url>()?])?;
            }
        }
        b.build()?.check().await
    }

    pub async fn check(app: &AppHandle, manual: bool) -> Result<UpdateStatus, UserMessage> {
        let Some(u) = app.try_state::<Updates>() else { return Err(msg!("update.storeBuild")) };
        let mut st = current(app, &u);
        if matches!(st.state.as_str(), "checking" | "downloading" | "ready") {
            return Ok(st);
        }
        if !manual && (!st.auto || !check_due(st.last_check.as_deref(), now()) || st.state == "available") {
            return Ok(st);
        }
        st.state = "checking".into();
        st.error = None;
        set(app, &u, &st);
        let res = ask(app).await;
        if res.is_ok() {
            let state = app.state::<AppState>();
            let mut s = lock(&state.settings).clone();
            s.updates.last_check = now().format(&Rfc3339).ok();
            if let Ok(saved) = state.store.save_settings(&s) {
                *lock(&state.settings) = saved;
            }
        }
        let mut st = current(app, &u);
        st.downloaded = 0;
        st.total = None;
        match res {
            Ok(Some(update)) => {
                st.state = "available".into();
                st.version = Some(update.version.clone());
                st.date = update.date.map(|d| {
                    let d = d.date();
                    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
                });
                st.notes = update.body.clone().filter(|b| !b.trim().is_empty());
                st.can_install = std::env::current_exe().map(|e| can_replace_bundle(&e)).unwrap_or(false);
                st.error = None;
                *lock(&u.pending) = Some(update);
            }
            Ok(None) => {
                st.state = if manual { "latest" } else { "idle" }.into();
                st.version = None;
                st.error = None;
                *lock(&u.pending) = None;
            }
            Err(e) => {
                // One log line; the launch check shows nothing (docs/UPDATER.md, Behaviour).
                eprintln!("[mqttpanel] update check failed: {e}");
                if manual {
                    st.state = "error".into();
                    st.error = Some(message(&e));
                } else {
                    st.state = "idle".into();
                }
            }
        }
        set(app, &u, &st);
        Ok(st)
    }

    pub async fn install(app: &AppHandle) -> Result<UpdateStatus, UserMessage> {
        let Some(u) = app.try_state::<Updates>() else { return Err(msg!("update.storeBuild")) };
        let Some(update) = lock(&u.pending).clone() else { return Err(msg!("update.nothing")) };
        let mut st = current(app, &u);
        if st.state == "downloading" || st.state == "ready" {
            return Ok(st);
        }
        if !std::env::current_exe().map(|e| can_replace_bundle(&e)).unwrap_or(false) {
            st.can_install = false;
            set(app, &u, &st);
            return Err(msg!("update.moveFirst"));
        }
        st.state = "downloading".into();
        st.downloaded = 0;
        st.total = None;
        st.error = None;
        set(app, &u, &st);
        let progress = Mutex::new((st.clone(), std::time::Instant::now()));
        let res = update
            .download_and_install(
                |chunk, total| {
                    let mut p = lock(&progress);
                    p.0.downloaded += chunk as u64;
                    p.0.total = total;
                    if p.1.elapsed() >= std::time::Duration::from_millis(100) {
                        p.1 = std::time::Instant::now();
                        let snapshot = p.0.clone();
                        drop(p);
                        set(app, &u, &snapshot);
                    }
                },
                || {},
            )
            .await;
        let mut st = lock(&progress).0.clone();
        match res {
            Ok(()) => {
                st.state = "ready".into();
                if let Some(t) = st.total {
                    st.downloaded = t;
                }
                *lock(&u.pending) = None;
                set(app, &u, &st);
                Ok(st)
            }
            Err(e) => {
                eprintln!("[mqttpanel] update install failed: {e}");
                let m = message(&e);
                st.state = "available".into();
                st.downloaded = 0;
                st.total = None;
                if is_location_error(&e) {
                    st.can_install = false;
                }
                st.error = Some(m.clone());
                set(app, &u, &st);
                Err(m)
            }
        }
    }

    pub fn restart(app: &AppHandle) -> Result<(), UserMessage> {
        let Some(u) = app.try_state::<Updates>() else { return Err(msg!("update.storeBuild")) };
        if current(app, &u).state != "ready" {
            return Err(msg!("update.nothing"));
        }
        app.restart()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u02_store_status_reports_no_self_update() {
        let s = Settings::default();
        let st = store_status(&s);
        assert!(!st.self_update);
        assert!(st.auto);
        assert_eq!(st.state, "idle");
        let json = serde_json::to_value(&st).unwrap();
        assert_eq!(json["selfUpdate"], false);
        assert_eq!(json["canInstall"], true);
    }

    #[test]
    fn u03_check_due_after_24_hours() {
        use time::format_description::well_known::Rfc3339;
        let now = time::OffsetDateTime::parse("2026-10-26T12:00:00Z", &Rfc3339).unwrap();
        assert!(check_due(None, now));
        assert!(check_due(Some("garbage"), now));
        assert!(!check_due(Some("2026-10-26T08:00:00Z"), now));
        assert!(check_due(Some("2026-10-25T12:00:00Z"), now));
        assert!(check_due(Some("2026-10-30T12:00:00Z"), now), "a clock set back does not stop the checks");
    }

    #[test]
    fn u04_bundle_on_read_only_or_translocated_path_cannot_be_replaced() {
        let exe = std::path::Path::new("/private/var/folders/x/AppTranslocation/ABC/d/MQTT Panel.app/Contents/MacOS/MQTT Panel");
        assert!(!can_replace_bundle(exe));
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("MQTT Panel.app/Contents/MacOS/MQTT Panel");
        assert!(can_replace_bundle(&exe));
        assert!(!can_replace_bundle(std::path::Path::new("/tmp/target/debug/mqtt-panel")), "no bundle, no install");
    }
}

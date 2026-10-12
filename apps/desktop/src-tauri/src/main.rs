//! MQTT Panel desktop shell: Tauri 2 with native menus and dialogs; the engine, client and broker run in this process.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod broker;
mod commands;
mod files;
mod payload;
mod state;
#[cfg(test)]
mod tests;
mod types;

use mqttpanel_profiles::{KeychainSecrets, MemorySecrets, SecretStore, Store};
use state::{lock, AppState};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::menu::{AboutMetadataBuilder, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use types::EngineEvent;

fn item<R: Runtime>(app: &AppHandle<R>, id: &str, text: &str, accel: Option<&str>) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(app, id, text, true, accel)
}

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let about = AboutMetadataBuilder::new()
        .name(Some("MQTT Panel"))
        .version(Some(env!("CARGO_PKG_VERSION")))
        .copyright(Some("mqtt-panel.com · Apache License 2.0"))
        .build();
    let settings = item(app, "open-settings", "Settings…", Some("CmdOrCtrl+,"))?;
    let new_conn = item(app, "new-connection", "New Connection…", Some("CmdOrCtrl+N"))?;
    let import = item(app, "import", "Import Connections…", None)?;
    let export = item(app, "export", "Export Data…", Some("CmdOrCtrl+E"))?;
    let sep = || PredefinedMenuItem::separator(app);

    #[cfg(target_os = "macos")]
    let app_menu = Submenu::with_items(
        app,
        "MQTT Panel",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About MQTT Panel"), Some(about.clone()))?,
            &sep()?,
            &settings,
            &sep()?,
            &PredefinedMenuItem::services(app, None)?,
            &sep()?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &sep()?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    let file = Submenu::with_items(app, "File", true, &[&new_conn, &import, &sep()?, &export, &sep()?, &PredefinedMenuItem::close_window(app, None)?])?;
    #[cfg(not(target_os = "macos"))]
    let file = {
        file.append(&sep()?)?;
        file.append(&PredefinedMenuItem::quit(app, None)?)?;
        file
    };
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &sep()?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
            &sep()?,
            &item(app, "find", "Find Topic", Some("CmdOrCtrl+F"))?,
        ],
    )?;
    #[cfg(not(target_os = "macos"))]
    {
        edit.append(&sep()?)?;
        edit.append(&settings)?;
    }
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &item(app, "go-connect", "Connect", Some("CmdOrCtrl+1"))?,
            &item(app, "go-explore", "Explore", Some("CmdOrCtrl+2"))?,
            &item(app, "go-publish", "Publish", Some("CmdOrCtrl+3"))?,
            &item(app, "go-charts", "Charts", Some("CmdOrCtrl+4"))?,
            &item(app, "go-broker", "Broker", Some("CmdOrCtrl+5"))?,
            &sep()?,
            &item(app, "command-palette", "Command Palette…", Some("CmdOrCtrl+K"))?,
            &sep()?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;
    let connection = Submenu::with_items(
        app,
        "Connection",
        true,
        &[&item(app, "switch-connection", "Switch Connection…", Some("CmdOrCtrl+Shift+L"))?, &sep()?, &item(app, "clear-history", "Clear History…", None)?],
    )?;
    let window = Submenu::with_items(app, "Window", true, &[&PredefinedMenuItem::minimize(app, None)?, &PredefinedMenuItem::maximize(app, None)?])?;
    #[cfg(target_os = "macos")]
    let help = Submenu::with_items(app, "Help", true, &[&item(app, "shortcuts", "Keyboard Shortcuts", None)?])?;
    #[cfg(not(target_os = "macos"))]
    let help = Submenu::with_items(app, "Help", true, &[&item(app, "shortcuts", "Keyboard Shortcuts", None)?, &PredefinedMenuItem::about(app, Some("About MQTT Panel"), Some(about))?])?;

    #[cfg(target_os = "macos")]
    return Menu::with_items(app, &[&app_menu, &file, &edit, &view, &connection, &window, &help]);
    #[cfg(not(target_os = "macos"))]
    Menu::with_items(app, &[&file, &edit, &view, &connection, &window, &help])
}

/// Emits a tick when something changed, at most 20 times a second, and a heartbeat every second.
fn spawn_ticker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last_seq: HashMap<String, u64> = HashMap::new();
        let mut last_emit = std::time::Instant::now();
        let mut last_status = String::new();
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(50));
        loop {
            interval.tick().await;
            let st = app.state::<AppState>();
            let now = types::now_ms();
            let skip = lock(&st.settings).skip_verify;
            let conns: Vec<_> = lock(&st.conns).values().cloned().collect();
            let mut changed = HashMap::new();
            let mut statuses = Vec::new();
            let mut repeats = Vec::new();
            let mut sent_updates = Vec::new();
            let mut dirty = false;
            for c in &conns {
                state::sweep_requests(c, now);
                let seq = lock(&c.engine).seq();
                if last_seq.get(&c.id) != Some(&seq) {
                    dirty = true;
                    last_seq.insert(c.id.clone(), seq);
                }
                changed.insert(c.id.clone(), seq);
                statuses.push(c.status(skip, now));
                if let Some(r) = lock(&c.repeat).as_ref() {
                    repeats.push(r.status.clone());
                }
                for e in lock(&c.updates).drain(..) {
                    sent_updates.push(types::SentUpdate { connection_id: c.id.clone(), entry: e });
                }
            }
            last_seq.retain(|k, _| changed.contains_key(k));
            let broker = broker::status_json(&st.broker);
            let status_sig = serde_json::to_string(&(&statuses.iter().map(|s| (&s.state, &s.error, s.reconnect_attempt, s.subscription_errors.len())).collect::<Vec<_>>(), &broker)).unwrap_or_default();
            if status_sig != last_status {
                dirty = true;
                last_status = status_sig;
            }
            if !sent_updates.is_empty() || !repeats.is_empty() {
                dirty = true;
            }
            if dirty || last_emit.elapsed() >= std::time::Duration::from_secs(1) {
                last_emit = std::time::Instant::now();
                let ev = EngineEvent::Tick { connections: statuses, changed, repeats, sent_updates, broker, broker_log_seq: broker::log_seq(&st.broker) };
                let _ = app.emit("engine-event", ev);
            }
        }
    });
}

/// MQTTPANEL_LOG=1 prints page loads and the name of every IPC command to stderr (diagnostics, no payloads).
fn log_enabled() -> bool {
    std::env::var("MQTTPANEL_LOG").as_deref() == Ok("1")
}

fn logged<R: Runtime>(handler: impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    let on = log_enabled();
    move |invoke| {
        if on {
            eprintln!("[mqttpanel] ipc {}", invoke.message.command());
        }
        handler(invoke)
    }
}

fn secrets() -> Arc<dyn SecretStore> {
    if std::env::var("MQTTPANEL_SECRETS").as_deref() == Ok("memory") {
        return Arc::new(MemorySecrets::new());
    }
    Arc::new(KeychainSecrets::new())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = mqttpanel_profiles::default_dir();
            let _ = std::fs::create_dir_all(&dir);
            let store = Store::new(dir, secrets());
            let settings = store.settings();
            app.manage(AppState {
                store,
                settings: Mutex::new(settings),
                conns: Mutex::new(HashMap::new()),
                pending_trust: Mutex::new(HashMap::new()),
                broker: broker::BrokerState::default(),
            });
            let handle = app.handle().clone();
            let menu = build_menu(&handle)?;
            app.set_menu(menu)?;
            app.on_menu_event(|app, event| {
                let _ = app.emit("engine-event", EngineEvent::Menu { id: event.id().0.clone() });
            });
            spawn_ticker(handle);
            Ok(())
        })
        .on_page_load(|webview, payload| {
            if log_enabled() {
                eprintln!("[mqttpanel] page {:?}: {}", payload.event(), payload.url());
            }
            let _ = webview;
        })
        .invoke_handler(logged(tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::save_settings,
            commands::list_profiles,
            commands::new_profile,
            commands::save_profile,
            commands::delete_profile,
            commands::duplicate_profile,
            commands::reorder_profiles,
            commands::set_password,
            commands::import_profiles,
            commands::export_profiles,
            commands::pick_file,
            commands::inspect_file,
            commands::set_profile_file,
            commands::add_proto_schema,
            commands::remove_proto_schema,
            commands::connect_profile,
            commands::test_profile,
            commands::trust_certificate,
            commands::disconnect,
            commands::close_connection,
            commands::list_connections,
            commands::tree_slice,
            commands::table_slice,
            commands::set_expanded,
            commands::expand_to_depth,
            commands::expand_changed,
            commands::collapse_all,
            commands::get_expanded,
            commands::set_expanded_set,
            commands::set_pinned,
            commands::reveal_topic,
            commands::next_changed,
            commands::known_topics,
            commands::topic_detail,
            commands::message,
            commands::history,
            commands::diff,
            commands::set_topic_buffer,
            commands::clear_history,
            commands::retained_under,
            commands::memory,
            commands::series,
            commands::export_estimate,
            commands::export_data,
            commands::export_chart_csv,
            commands::save_png,
            commands::export_sent_log,
            commands::publish,
            commands::delete_retained,
            commands::delete_retained_many,
            commands::sent_log,
            commands::clear_sent_log,
            commands::start_repeat,
            commands::stop_repeat,
            commands::list_templates,
            commands::save_template,
            commands::delete_template,
            commands::encode_payload,
            broker::broker_status,
            broker::broker_config,
            broker::broker_start,
            broker::broker_stop,
            broker::broker_restart,
            broker::validate_port,
            broker::add_listener,
            broker::update_listener,
            broker::remove_listener,
            broker::set_listener_login,
            broker::set_anonymous,
            broker::set_verify_against,
            broker::add_user,
            broker::set_user_password,
            broker::remove_user,
            broker::issued_certs,
            broker::issue_client_cert,
            broker::import_csr,
            broker::export_client_cert,
            broker::revoke_client_cert,
            broker::broker_certs,
            broker::export_ca,
            broker::regenerate_broker_certs,
            broker::import_broker_cert,
            broker::import_client_ca,
            broker::rotate_ca,
            broker::broker_clients,
            broker::disconnect_client,
            broker::broker_log,
            broker::connect_explorer_here,
        ]))
        .build(tauri::generate_context!())
        .expect("error while building MQTT Panel")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let st = app.state::<AppState>();
                tauri::async_runtime::block_on(broker::shutdown(&st.broker));
            }
        });
}

//! MQTT Panel desktop shell: Tauri 2 with native menus and dialogs; the engine, client and broker run in this process.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod broker;
mod charts;
mod commands;
mod files;
mod mcp;
mod menu;
mod payload;
mod state;
#[cfg(test)]
mod tests;
mod types;

use mqttpanel_profiles::{MemorySecrets, SecretStore, Store};
use state::{lock, AppState};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use types::EngineEvent;

/// The assistant socket while the app runs.
struct McpSocket(Mutex<Option<mcp::server::Listening>>);

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
                statuses.push(c.status(now));
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

/// The secret store: `secrets.enc` in the data folder, its master key in the key file or the single keychain item as
/// the build says (SPEC 6, change set 10). The master key is read on first secret use, never here.
fn secrets(dir: &std::path::Path) -> Arc<dyn SecretStore> {
    if std::env::var("MQTTPANEL_SECRETS").as_deref() == Ok("memory") {
        return Arc::new(MemorySecrets::new());
    }
    mqttpanel_profiles::open_app_secrets(dir)
}

fn main() {
    // `<app> mcp`: the MCP stdio shim, so a host can start the app's own executable (docs/MCP.md section 2).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "mcp" {
        std::process::exit(mqttpanel_shim::run(mqttpanel_shim::Launch::SelfExe));
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = mqttpanel_profiles::default_dir();
            let _ = std::fs::create_dir_all(&dir);
            let secrets = secrets(&dir);
            let store = Store::new(dir, secrets);
            let settings = store.settings();
            app.manage(AppState {
                store,
                settings: Mutex::new(settings),
                conns: Mutex::new(HashMap::new()),
                pending_trust: Mutex::new(HashMap::new()),
                broker: broker::BrokerState::default(),
                assistant: mcp::Hub::default(),
                charts: Mutex::new(Default::default()),
            });
            let handle = app.handle().clone();
            let menu = menu::build_menu(&handle, "en")?;
            app.set_menu(menu)?;
            app.on_menu_event(|app, event| {
                let _ = app.emit("engine-event", EngineEvent::Menu { id: event.id().0.clone() });
            });
            spawn_ticker(handle.clone());
            // The assistant socket (docs/MCP.md section 2): enabled by default, same user only, no network port.
            let link: Arc<dyn mcp::AppLink> = Arc::new(mcp::commands::TauriLink(handle.clone()));
            let mcp_dir = mqttpanel_profiles::default_dir();
            tauri::async_runtime::spawn(async move {
                match mcp::server::listen(link, &mcp_dir).await {
                    Ok(l) => {
                        // Kept for the life of the app; the socket file goes when the app quits.
                        handle.manage(McpSocket(Mutex::new(Some(l))));
                    }
                    Err(e) => {
                        if log_enabled() {
                            eprintln!("[mqttpanel] assistant socket: {e}");
                        }
                    }
                }
            });
            Ok(())
        })
        .on_page_load(|webview, payload| {
            if log_enabled() {
                eprintln!("[mqttpanel] page {:?}: {}", payload.event(), payload.url());
            }
            let _ = webview;
        })
        .invoke_handler(logged(tauri::generate_handler![
            menu::set_menu_language,
            commands::app_info,
            commands::key_store_kind,
            commands::get_settings,
            commands::save_settings,
            commands::list_profiles,
            commands::new_profile,
            commands::save_profile,
            commands::delete_profile,
            commands::duplicate_profile,
            commands::reorder_profiles,
            commands::set_password,
            commands::profile_password,
            commands::import_profiles,
            commands::export_profiles,
            commands::pick_file,
            commands::inspect_file,
            commands::set_profile_file,
            commands::set_client_cert_way,
            commands::profile_generate_client_key,
            commands::profile_client_csr,
            commands::profile_import_signed_cert,
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
            broker::validate_port,
            broker::add_listener,
            broker::update_listener,
            broker::remove_listener,
            broker::set_broker_bind,
            broker::set_anonymous,
            broker::set_require_client_cert,
            broker::set_verify_against,
            broker::add_user,
            broker::set_user_password,
            broker::remove_user,
            broker::broker_user_password,
            broker::issued_certs,
            broker::issue_client_cert,
            broker::import_csr,
            broker::export_client_cert,
            broker::revoke_client_cert,
            broker::broker_certs,
            broker::export_ca,
            broker::export_server_cert,
            broker::regenerate_broker_certs,
            broker::import_broker_cert,
            broker::import_client_ca,
            broker::rotate_ca,
            broker::broker_clients,
            broker::disconnect_client,
            broker::broker_log,
            broker::app_broker_step,
            mcp::commands::assistant_status,
            mcp::commands::assistant_accept,
            mcp::commands::assistant_decline,
            mcp::commands::assistant_add_to_claude,
            mcp::commands::assistant_add_to_chatgpt,
            mcp::commands::assistant_log,
            mcp::commands::assistant_clear_log,
            mcp::commands::assistant_export_log,
            mcp::commands::assistant_answer,
            mcp::commands::assistant_pick_save,
            mcp::commands::assistant_pick_folder,
            charts::sync_charts,
        ]))
        .build(tauri::generate_context!())
        .expect("error while building MQTT Panel")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let st = app.state::<AppState>();
                tauri::async_runtime::block_on(broker::shutdown(&st.broker));
                if let Some(s) = app.try_state::<McpSocket>() {
                    lock(&s.0).take();
                }
            }
        });
}

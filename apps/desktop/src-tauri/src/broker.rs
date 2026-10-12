//! Embedded broker commands (filled in once crates/broker lands).

use mqttpanel_client::{msg, UserMessage};

#[derive(Default)]
pub struct BrokerState {}

type R<T> = Result<T, UserMessage>;

fn not_ready<T>() -> R<T> {
    Err(msg!("broker.notRunning"))
}

macro_rules! stub {
    ($($name:ident ( $($arg:ident : $ty:ty),* ) -> $ret:ty;)*) => {
        $(
            #[tauri::command]
            #[allow(unused_variables)]
            pub fn $name($($arg: $ty),*) -> R<$ret> { not_ready() }
        )*
    };
}

stub! {
    broker_status() -> serde_json::Value;
    broker_config() -> serde_json::Value;
    broker_start() -> serde_json::Value;
    broker_stop() -> serde_json::Value;
    broker_restart() -> serde_json::Value;
    validate_port(port: u32, listener_id: Option<String>) -> Option<UserMessage>;
    add_listener(request: serde_json::Value) -> serde_json::Value;
    update_listener(listener: serde_json::Value) -> serde_json::Value;
    remove_listener(listener_id: String) -> serde_json::Value;
    set_listener_login(listener_id: String, username: String, password: Option<String>) -> serde_json::Value;
    set_anonymous(on: bool) -> serde_json::Value;
    set_verify_against(source: String) -> serde_json::Value;
    add_user(username: String, password: String) -> serde_json::Value;
    set_user_password(username: String, password: String) -> serde_json::Value;
    remove_user(username: String) -> serde_json::Value;
    issued_certs() -> serde_json::Value;
    issue_client_cert(request: serde_json::Value) -> serde_json::Value;
    import_csr(name: Option<String>, valid_years: u32) -> serde_json::Value;
    export_client_cert(name: String, format: String, passphrase: String) -> serde_json::Value;
    revoke_client_cert(name: String) -> serde_json::Value;
    broker_certs() -> serde_json::Value;
    export_ca() -> serde_json::Value;
    regenerate_broker_certs(sans: Vec<String>, key_type: String) -> serde_json::Value;
    import_broker_cert() -> serde_json::Value;
    import_client_ca() -> serde_json::Value;
    rotate_ca() -> serde_json::Value;
    broker_clients() -> serde_json::Value;
    disconnect_client(client_id: String) -> ();
    broker_log(level: String, after_seq: u64) -> serde_json::Value;
    connect_explorer_here(listener_id: Option<String>) -> serde_json::Value;
}

pub fn status_json(_s: &BrokerState) -> Option<serde_json::Value> {
    None
}

pub fn log_seq(_s: &BrokerState) -> u64 {
    0
}

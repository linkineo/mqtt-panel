//! Profiles, settings, templates and secrets for MQTT Panel. Shared by the desktop app and the CLI.

pub mod connect;
pub mod import;
pub mod keystore;
pub mod model;
pub mod secrets;
pub mod secrets_file;
pub mod store;

pub use connect::connect_options;
pub use model::*;
pub use secrets::{MemorySecrets, SecretError, SecretStore};
pub use keystore::{FileMasterKey, KeyStoreKind, MigratingMasterKey, KEY_FILE};
pub use secrets_file::{key_store_kind, open_app_secrets, FileSecrets, FixedMasterKey, KeychainMasterKey, MasterKeySource, SECRETS_FILE};
pub use store::{default_dir, Store};

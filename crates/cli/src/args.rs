//! Command-line arguments (clap derive). Help screens are our own texts (see `help.rs`), so clap's help and
//! version flags are disabled; `-h` is the host.

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum Format {
    #[default]
    Text,
    Json,
    Ndjson,
    Raw,
}

#[derive(Debug, Parser)]
#[command(
    name = "mqttpanel",
    disable_help_flag = true,
    disable_version_flag = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(flatten)]
    pub global: Global,
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Options accepted before or after the command.
#[derive(Debug, Clone, Args, Default)]
pub struct Global {
    #[arg(long, global = true)]
    pub profile: Option<String>,
    #[arg(short = 'h', long, global = true)]
    pub host: Option<String>,
    #[arg(short = 'p', long, global = true, value_parser = clap::value_parser!(u16).range(1..))]
    pub port: Option<u16>,
    #[arg(long, global = true)]
    pub tls: bool,
    #[arg(short = 'u', long, global = true)]
    pub user: Option<String>,
    #[arg(long, global = true, value_enum)]
    pub format: Option<Format>,
}

impl Global {
    pub fn format(&self) -> Format {
        self.format.unwrap_or_default()
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Sub(SubArgs),
    Pub(PubArgs),
    Broker(BrokerArgs),
    Profiles(ProfilesArgs),
}

/// TLS options of sub and pub (`mqttpanel sub --help tls`). Any of them implies TLS.
#[derive(Debug, Clone, Args, Default)]
pub struct TlsArgs {
    #[arg(long)]
    pub cafile: Option<PathBuf>,
    #[arg(long)]
    pub cert: Option<PathBuf>,
    #[arg(long)]
    pub key: Option<PathBuf>,
    #[arg(long)]
    pub key_passphrase: Option<String>,
    #[arg(long)]
    pub insecure: bool,
    #[arg(long)]
    pub trust_fingerprint: Vec<String>,
    #[arg(long)]
    pub sni: Option<String>,
    #[arg(long)]
    pub alpn: Option<String>,
}

impl TlsArgs {
    /// True when any option asks for TLS.
    pub fn any(&self) -> bool {
        self.cafile.is_some()
            || self.cert.is_some()
            || self.key.is_some()
            || self.insecure
            || !self.trust_fingerprint.is_empty()
            || self.sni.is_some()
            || self.alpn.is_some()
    }
}

#[derive(Debug, Clone, Args)]
pub struct SubArgs {
    #[arg(short = 't', long = "topic")]
    pub topics: Vec<String>,
    #[arg(short = 'q', long, default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=2))]
    pub qos: u8,
    #[arg(long)]
    pub retained_only: bool,
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    pub count: Option<u64>,
    #[arg(long)]
    pub timeout: Option<f64>,
    #[command(flatten)]
    pub tls: TlsArgs,
}

#[derive(Debug, Clone, Args)]
pub struct PubArgs {
    #[arg(short = 't', long = "topic")]
    pub topic: Option<String>,
    /// Payload bytes as given (any bytes on Unix, not only UTF-8).
    #[arg(short = 'm', long = "message", allow_hyphen_values = true, value_parser = clap::builder::OsStringValueParser::new())]
    pub message: Option<std::ffi::OsString>,
    #[arg(long)]
    pub file: Option<PathBuf>,
    #[arg(long)]
    pub stdin: bool,
    #[arg(short = 'q', long, default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=2))]
    pub qos: u8,
    #[arg(short = 'r', long)]
    pub retain: bool,
    #[arg(long)]
    pub delete_retained: bool,
    #[arg(long)]
    pub repeat: Option<f64>,
    #[command(flatten)]
    pub tls: TlsArgs,
}

#[derive(Debug, Clone, Args)]
pub struct BrokerArgs {
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long)]
    pub bind: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct ProfilesArgs {
    #[command(subcommand)]
    pub command: Option<ProfilesCommand>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ProfilesCommand {
    List,
    Export {
        #[arg(long, conflicts_with = "passphrase")]
        without_secrets: bool,
        #[arg(long)]
        passphrase: Option<String>,
    },
    Import {
        file: PathBuf,
        #[arg(long)]
        passphrase: Option<String>,
    },
}

/// Commands, for the help pre-scan and error hints.
pub const COMMANDS: [&str; 4] = ["sub", "pub", "broker", "profiles"];

/// Options that take a value (the help pre-scan skips their values).
pub const VALUE_OPTIONS: [&str; 26] = [
    "--profile",
    "-h",
    "--host",
    "-p",
    "--port",
    "-u",
    "--user",
    "--format",
    "-t",
    "--topic",
    "-q",
    "--qos",
    "--count",
    "--timeout",
    "-m",
    "--message",
    "--file",
    "--repeat",
    "--cafile",
    "--cert",
    "--key",
    "--key-passphrase",
    "--trust-fingerprint",
    "--sni",
    "--alpn",
    "--config",
];

/// Further value options (kept apart so the array above stays readable).
pub const VALUE_OPTIONS_MORE: [&str; 2] = ["--bind", "--passphrase"];

pub fn takes_value(opt: &str) -> bool {
    VALUE_OPTIONS.contains(&opt) || VALUE_OPTIONS_MORE.contains(&opt)
}

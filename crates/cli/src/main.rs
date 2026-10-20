//! `mqttpanel`: the command line of MQTT Panel (docs/SPEC.md section 8).
//!
//! Exit codes: 0 ok, 1 connection or TLS error, 2 bad arguments, 3 refused by the broker.
//! Errors are one sentence on stderr from the shared catalog, in the language of the environment
//! (LC_ALL, LC_MESSAGES, LANG).
//!
//! Environment:
//! - `MQTTPANEL_HOME`: data folder of the desktop app (profiles, broker settings); for tests and portable use.
//! - `MQTTPANEL_PASSWORD`: password for `-u` instead of the terminal prompt (scripts).
//! - `MQTTPANEL_SECRETS=memory` and `MQTTPANEL_SECRETS_SEED`: test hook, see [`conn::secret_store`].
//! - `MQTTPANEL_READY_HINT`: test hook; `sub` prints "ready" on stderr once every subscription is acknowledged.

mod args;
mod broker;
mod conn;
mod help;
mod i18n;
mod output;
mod profiles;
mod publish;
mod sub;

use args::{Cli, Command};
use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::Parser;
use i18n::{t, Failure};
use mqttpanel_client::catalog::Lang;
use std::io::Write;

/// What the pre-scan found before clap runs (help works after any command, even with otherwise bad arguments).
enum Early {
    Help(&'static str),
    Version,
}

fn help_for(command: Option<&str>, topic: Option<&str>) -> &'static str {
    let topic = topic.filter(|t| *t == "tls" || args::COMMANDS.contains(t));
    match (command, topic) {
        (_, Some("tls")) => help::TLS,
        (None, Some(c)) | (Some(c), _) => match c {
            "sub" => help::SUB,
            "pub" => help::PUB,
            "broker" => help::BROKER,
            "profiles" => help::PROFILES,
            _ => help::MAIN,
        },
        (None, None) => help::MAIN,
    }
}

fn prescan(argv: &[String]) -> Option<Early> {
    let mut command: Option<&str> = None;
    let mut help: Option<Option<&str>> = None;
    let mut version = false;
    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].as_str();
        if a == "--" {
            break;
        }
        if a == "--help" {
            let next = argv
                .get(i + 1)
                .map(String::as_str)
                .filter(|n| !n.starts_with('-'));
            if next.is_some() {
                i += 1;
            }
            help = Some(next);
        } else if a == "-V" || a == "--version" {
            version = true;
        } else if args::takes_value(a) {
            i += 1;
        } else if !a.starts_with('-') && command.is_none() && args::COMMANDS.contains(&a) {
            command = Some(a);
        }
        i += 1;
    }
    if let Some(topic) = help {
        return Some(Early::Help(help_for(command, topic)));
    }
    version.then_some(Early::Version)
}

/// The command named in the arguments (for the "Run mqttpanel <command> --help" hint).
fn command_of(argv: &[String]) -> &str {
    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].as_str();
        if args::takes_value(a) {
            i += 2;
            continue;
        }
        if args::COMMANDS.contains(&a) {
            return a;
        }
        i += 1;
    }
    ""
}

fn ctx(e: &clap::Error, k: ContextKind) -> Option<String> {
    match e.get(k)? {
        ContextValue::String(s) => Some(s.clone()),
        ContextValue::Strings(v) => v.first().cloned(),
        _ => None,
    }
}

/// "--qos <QOS>" → "--qos".
fn arg_name(s: String) -> String {
    s.split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_end_matches(['=', '.'])
        .to_string()
}

/// One sentence for a clap error (clap's own multi-line text never reaches the user).
fn clap_detail(e: &clap::Error, lang: Lang) -> String {
    let arg = ctx(e, ContextKind::InvalidArg)
        .map(arg_name)
        .unwrap_or_default();
    if arg.is_empty()
        && !matches!(
            e.kind(),
            ErrorKind::InvalidSubcommand
                | ErrorKind::MissingSubcommand
                | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        )
    {
        return t(lang, "args.invalid", &[]);
    }
    match e.kind() {
        ErrorKind::UnknownArgument => {
            if arg.starts_with('-') {
                t(lang, "args.unknownOption", &[("arg", &arg)])
            } else {
                t(lang, "args.unexpected", &[("arg", &arg)])
            }
        }
        ErrorKind::InvalidSubcommand => {
            let sub = ctx(e, ContextKind::InvalidSubcommand).unwrap_or_default();
            t(lang, "args.unknownCommand", &[("arg", &sub)])
        }
        ErrorKind::InvalidValue | ErrorKind::ValueValidation | ErrorKind::InvalidUtf8 => {
            let value = ctx(e, ContextKind::InvalidValue).unwrap_or_default();
            if value.is_empty() {
                t(lang, "args.missingValue", &[("arg", &arg)])
            } else {
                t(
                    lang,
                    "args.invalidValue",
                    &[("value", &value), ("arg", &arg)],
                )
            }
        }
        ErrorKind::NoEquals
        | ErrorKind::WrongNumberOfValues
        | ErrorKind::TooFewValues
        | ErrorKind::TooManyValues => t(lang, "args.missingValue", &[("arg", &arg)]),
        ErrorKind::ArgumentConflict => {
            let other = ctx(e, ContextKind::PriorArg)
                .map(arg_name)
                .unwrap_or_default();
            if other.is_empty() || other == arg {
                t(lang, "args.repeated", &[("arg", &arg)])
            } else {
                t(lang, "args.conflict", &[("arg", &arg), ("other", &other)])
            }
        }
        ErrorKind::MissingSubcommand | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            t(lang, "args.noCommand", &[])
        }
        _ => t(lang, "args.invalid", &[]),
    }
}

async fn run(cli: Cli, lang: Lang) -> Result<(), Failure> {
    let g = &cli.global;
    match &cli.command {
        Some(Command::Sub(a)) => sub::run(g, a, lang).await,
        Some(Command::Pub(a)) => publish::run(g, a, lang).await,
        Some(Command::Broker(a)) => broker::run(g, a, lang).await,
        Some(Command::Profiles(a)) => profiles::run(g, a.command.as_ref(), lang),
        None => Err(Failure::args_t("args.noCommand", &[], "", lang)),
    }
}

fn finish(code: u8) -> ! {
    let _ = std::io::stdout().flush();
    // Exit right away: a blocking stdin read (pub --stdin) must not hold the process open.
    std::process::exit(code as i32)
}

fn main() {
    let lang = Lang::from_env();
    let argv: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if argv.is_empty() {
        print!("{}", help::MAIN);
        finish(0);
    }
    // `mqttpanel mcp`: the MCP stdio shim for assistants (docs/MCP.md section 2); stdout belongs to the host.
    if argv.len() == 1 && argv[0] == "mcp" {
        std::process::exit(mqttpanel_shim::run(mqttpanel_shim::Launch::FindApp));
    }
    match prescan(&argv) {
        Some(Early::Help(text)) => {
            print!("{text}");
            finish(0);
        }
        Some(Early::Version) => {
            println!("mqttpanel {}", env!("CARGO_PKG_VERSION"));
            finish(0);
        }
        None => {}
    }
    let cli = match Cli::try_parse_from(std::env::args_os()) {
        Ok(c) => c,
        Err(e) => {
            let f = Failure::args(&clap_detail(&e, lang), command_of(&argv), lang);
            eprintln!("{}", f.text);
            finish(f.code);
        }
    };
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!(
                "{}",
                i18n::cat(
                    lang,
                    "error.internal",
                    &[("detail", serde_json::json!(e.kind().to_string()))]
                )
            );
            finish(1);
        }
    };
    let code = match rt.block_on(run(cli, lang)) {
        Ok(()) => 0,
        Err(f) => {
            eprintln!("{}", f.text);
            f.code
        }
    };
    finish(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn help_prescan() {
        let h = |a: &[&str]| match prescan(&v(a)) {
            Some(Early::Help(t)) => t,
            _ => "",
        };
        assert_eq!(h(&["--help"]), help::MAIN);
        assert_eq!(h(&["sub", "--help"]), help::SUB);
        assert_eq!(h(&["sub", "--help", "tls"]), help::TLS);
        assert_eq!(h(&["--help", "pub"]), help::PUB);
        assert_eq!(h(&["-h", "sub", "pub", "--help"]), help::PUB);
        assert_eq!(h(&["profiles", "list", "--help"]), help::PROFILES);
        assert!(matches!(prescan(&v(&["-V"])), Some(Early::Version)));
        assert!(prescan(&v(&["pub", "-m", "-V", "-t", "x"])).is_none());
    }

    #[test]
    fn clap_errors_are_one_sentence() {
        let e = Cli::try_parse_from(["mqttpanel", "sub", "--bogus"]).unwrap_err();
        assert_eq!(clap_detail(&e, Lang::En), "Unknown option --bogus.");
        let e = Cli::try_parse_from(["mqttpanel", "sub", "-q", "7", "-t", "x"]).unwrap_err();
        assert_eq!(clap_detail(&e, Lang::En), "Invalid value 7 for --qos.");
        let e = Cli::try_parse_from(["mqttpanel", "frob"]).unwrap_err();
        assert_eq!(clap_detail(&e, Lang::En), "Unknown command frob.");
        let e = Cli::try_parse_from(["mqttpanel", "--format", "xml", "sub"]).unwrap_err();
        assert_eq!(clap_detail(&e, Lang::En), "Invalid value xml for --format.");
        let e = Cli::try_parse_from(["mqttpanel", "sub", "-t"]).unwrap_err();
        assert_eq!(clap_detail(&e, Lang::En), "--topic needs a value.");
    }
}

//! User-facing text of the command line.
//!
//! Errors come from the shared catalog (packages/ui/src/i18n/<lang>/errors.json) through
//! [`mqttpanel_client::catalog`]. The few CLI-only strings below (argument problems, trust summary labels,
//! broker console lines) have no catalog key yet; they live here in English, French and German and need the same
//! native review as the catalog before release.

use mqttpanel_client::catalog::{render_with, Lang, UserMessage};

/// A failure that ends the command: one rendered text for stderr and the exit code.
#[derive(Debug, Clone)]
pub struct Failure {
    pub code: u8,
    pub text: String,
}

/// Exit codes (docs/SPEC.md section 8).
pub const EXIT_CONNECTION: u8 = 1;
pub const EXIT_ARGS: u8 = 2;
pub const EXIT_REFUSED: u8 = 3;

/// Exit code for a catalog key: refusals by the broker 3, bad arguments and unusable files 2, the rest 1.
pub fn exit_code_for(key: &str) -> u8 {
    match key {
        "error.badLogin"
        | "error.badLoginAnonymous"
        | "error.notAuthorized"
        | "error.clientIdRejected"
        | "error.serverUnavailable"
        | "error.protocolVersion"
        | "error.connackOther"
        | "error.publishRefused"
        | "error.subscribeRefused"
        | "error.packetTooLarge" => EXIT_REFUSED,
        "tls.wrongPassphrase" => EXIT_ARGS,
        k if k.starts_with("cli.") || k.starts_with("file.") => EXIT_ARGS,
        _ => EXIT_CONNECTION,
    }
}

impl Failure {
    /// From a catalog message, rendered in `lang`.
    pub fn msg(m: &UserMessage, lang: Lang) -> Failure {
        Failure {
            code: exit_code_for(&m.key),
            text: m.render(lang),
        }
    }

    /// A bad-arguments failure: `detail` (already rendered) plus the hint to the command's help (cli.badArgs).
    pub fn args(detail: &str, command: &str, lang: Lang) -> Failure {
        let mut p = serde_json::Map::new();
        p.insert(
            "detail".into(),
            serde_json::Value::String(detail.to_string()),
        );
        p.insert(
            "command".into(),
            serde_json::Value::String(command.to_string()),
        );
        let text = render_with(lang, "cli.badArgs", &p);
        // Without a command the template would read "mqttpanel  --help".
        let text = text.replace("mqttpanel  --help", "mqttpanel --help");
        Failure {
            code: EXIT_ARGS,
            text,
        }
    }

    /// A bad-arguments failure from a CLI-only string of [`t`].
    pub fn args_t(key: &str, params: &[(&str, &str)], command: &str, lang: Lang) -> Failure {
        Failure::args(&t(lang, key, params), command, lang)
    }
}

/// Renders a catalog key in `lang`.
pub fn cat(lang: Lang, key: &str, params: &[(&str, serde_json::Value)]) -> String {
    UserMessage::new(key, params).render(lang)
}

/// CLI-only strings live in the shared catalog under "cli.<key>" (packages/ui/src/i18n/<lang>/errors.json).
fn template(lang: Lang, key: &str) -> String {
    let full = format!("cli.{key}");
    let text = mqttpanel_client::catalog::render_with(lang, &full, &serde_json::Map::new());
    if text == full {
        key.to_string()
    } else {
        text
    }
}

/// Renders a CLI-only string with `{name}` parameters.
pub fn t(lang: Lang, key: &str, params: &[(&str, &str)]) -> String {
    let mut s = template(lang, key);
    for (k, v) in params {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes() {
        assert_eq!(exit_code_for("error.badLogin"), 3);
        assert_eq!(exit_code_for("error.dns"), 1);
        assert_eq!(exit_code_for("cli.noPayload"), 2);
        assert_eq!(exit_code_for("tls.untrusted"), 1);
    }

    #[test]
    fn bad_args_without_command() {
        let f = Failure::args_t("args.noCommand", &[], "", Lang::En);
        assert_eq!(
            f.text,
            "Choose a command: sub, pub, broker or profiles. Run mqttpanel --help for the options."
        );
        assert_eq!(f.code, 2);
    }

    #[test]
    fn every_key_in_three_languages() {
        for k in [
            "args.unknownOption",
            "args.wildcardTopic",
            "trust.hint",
            "profiles.imported",
        ] {
            for l in [Lang::En, Lang::Fr, Lang::De] {
                assert_ne!(t(l, k, &[]), k);
            }
        }
        assert_eq!(
            t(Lang::Fr, "args.unknownOption", &[("arg", "--x")]),
            "Option inconnue\u{202f}: --x."
        );
    }
}

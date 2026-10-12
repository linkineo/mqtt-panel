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

const NB: &str = "\u{a0}";

fn template(lang: Lang, key: &str) -> String {
    let (en, fr, de): (&str, String, &str) = match key {
        "args.unknownOption" => ("Unknown option {arg}.", format!("Option inconnue{NB}: {{arg}}."), "Unbekannte Option {arg}."),
        "args.unexpected" => ("Unexpected argument {arg}.", format!("Argument inattendu{NB}: {{arg}}."), "Unerwartetes Argument {arg}."),
        "args.invalidValue" => ("Invalid value {value} for {arg}.", "Valeur {value} non valable pour {arg}.".into(), "Ungültiger Wert {value} für {arg}."),
        "args.missingValue" => ("{arg} needs a value.", "{arg} attend une valeur.".into(), "{arg} braucht einen Wert."),
        "args.unknownCommand" => ("Unknown command {arg}.", format!("Commande inconnue{NB}: {{arg}}."), "Unbekannter Befehl {arg}."),
        "args.noCommand" => (
            "Choose a command: sub, pub, broker or profiles.",
            format!("Choisissez une commande{NB}: sub, pub, broker ou profiles."),
            "Wähle einen Befehl: sub, pub, broker oder profiles.",
        ),
        "args.conflict" => ("{arg} cannot be combined with {other}.", "{arg} ne peut pas être combiné avec {other}.".into(), "{arg} lässt sich nicht mit {other} kombinieren."),
        "args.repeated" => ("{arg} is given more than once.", "{arg} est indiqué plus d’une fois.".into(), "{arg} ist mehrfach angegeben."),
        "args.invalid" => ("These options are not valid.", "Ces options ne sont pas valables.".into(), "Diese Optionen sind ungültig."),
        "args.noTopic" => ("Give at least one topic filter with -t.", "Indiquez au moins un filtre de topic avec -t.".into(), "Gib mindestens einen Topic-Filter mit -t an."),
        "args.noPubTopic" => ("Give the topic with -t.", "Indiquez le topic avec -t.".into(), "Gib das Topic mit -t an."),
        "args.wildcardTopic" => (
            "The topic {arg} contains + or #; a message can only be published to a topic without wildcards.",
            "Le topic {arg} contient + ou #{NB}; on ne publie que sur un topic sans caractères génériques.".replace("{NB}", NB),
            "Das Topic {arg} enthält + oder #; veröffentlichen geht nur auf ein Topic ohne Platzhalter.",
        ),
        "args.onePayload" => (
            "Use only one of -m, --file, --stdin and --delete-retained.",
            "Utilisez une seule option parmi -m, --file, --stdin et --delete-retained.".into(),
            "Nutze nur eine der Optionen -m, --file, --stdin und --delete-retained.",
        ),
        "args.repeatStdin" => (
            "--repeat works with -m or --file, not with --stdin.",
            "--repeat fonctionne avec -m ou --file, pas avec --stdin.".into(),
            "--repeat funktioniert mit -m oder --file, nicht mit --stdin.",
        ),
        "args.positiveSeconds" => (
            "{arg} needs a number of seconds greater than 0.",
            "{arg} attend un nombre de secondes supérieur à 0.".into(),
            "{arg} braucht eine Anzahl Sekunden größer als 0.",
        ),
        "args.keyWithoutCert" => ("--key needs --cert.", "--key nécessite --cert.".into(), "--key braucht --cert."),
        "args.badBind" => (
            "Invalid address {arg} for --bind. Use an IP address such as 127.0.0.1.",
            "Adresse {arg} non valable pour --bind. Indiquez une adresse IP comme 127.0.0.1.".into(),
            "Ungültige Adresse {arg} für --bind. Gib eine IP-Adresse wie 127.0.0.1 an.",
        ),
        "args.badHost" => (
            "Invalid broker address {arg}. Use a host name, an IP address or a URL such as mqtts://host:8883.",
            "Adresse de broker {arg} non valable. Indiquez un nom d’hôte, une adresse IP ou une URL comme mqtts://host:8883.".into(),
            "Ungültige Broker-Adresse {arg}. Gib einen Hostnamen, eine IP-Adresse oder eine URL wie mqtts://host:8883 an.",
        ),
        "trust.subject" => ("Subject", "Sujet".into(), "Inhaber"),
        "trust.issuer" => ("Issuer", "Émetteur".into(), "Aussteller"),
        "trust.names" => ("Names", "Noms".into(), "Namen"),
        "trust.valid" => ("Valid", "Validité".into(), "Gültig"),
        "trust.fingerprint" => ("Fingerprint (SHA-256)", "Empreinte (SHA-256)".into(), "Fingerabdruck (SHA-256)"),
        "trust.hint" => (
            "Compare the fingerprint with the one shown on the broker, then run again with --trust-fingerprint {fp}, or with --insecure in a lab.",
            "Comparez l’empreinte avec celle affichée sur le broker, puis relancez avec --trust-fingerprint {fp}, ou avec --insecure en laboratoire.".into(),
            "Vergleiche den Fingerabdruck mit dem auf dem Broker, dann starte erneut mit --trust-fingerprint {fp}, oder im Labor mit --insecure.",
        ),
        "broker.listening" => ("{kind} listener on {addr}", "Listener {kind} sur {addr}".into(), "{kind}-Listener auf {addr}"),
        "broker.running" => (
            "Broker running. Press Ctrl-C to stop it.",
            "Broker démarré. Appuyez sur Ctrl-C pour l’arrêter.".into(),
            "Broker läuft. Drücke Ctrl-C, um ihn zu beenden.",
        ),
        "broker.stopped" => ("Broker stopped.", "Broker arrêté.".into(), "Broker beendet."),
        "profiles.none" => ("No saved profiles.", "Aucun profil enregistré.".into(), "Keine gespeicherten Profile."),
        "profiles.imported" => ("Imported {name}", format!("Importé{NB}: {{name}}"), "Importiert: {name}"),
        _ => (key, key.to_string(), key),
    };
    match lang {
        Lang::En => en.to_string(),
        Lang::Fr => fr,
        Lang::De => de.to_string(),
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
            "Option inconnue\u{a0}: --x."
        );
    }
}

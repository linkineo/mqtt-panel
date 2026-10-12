//! Help screens. MAIN, SUB and PUB reproduce docs/SPEC.md section 8 byte for byte (tested in tests/cli.rs).

pub const MAIN: &str = r##"MQTT Panel command line

Usage: mqttpanel <command> [options]

Commands:
  sub        Subscribe to topics and print messages
  pub        Publish one message (or one per line from stdin)
  broker     Run the embedded broker
  profiles   List, export and import saved connection profiles

Options:
  --profile <name>   Use a saved connection profile (from the desktop app)
  -h, --host <host>  Broker host (default: localhost)
  -p, --port <port>  Broker port (default: 1883, or 8883 with --tls)
  --tls              Connect with TLS
  -u, --user <name>  Username (password is asked for, or read from the keychain)
  --format <fmt>     Output: text (default), json, ndjson, raw
  -V, --version      Print the version
  --help             Show this help; also works after any command
"##;

pub const SUB: &str = r##"Subscribe to topics and print messages

Usage: mqttpanel sub [options] -t <filter> [-t <filter> ...]

Options:
  -t, --topic <filter>   Topic filter, repeatable ("#" for everything)
  -q, --qos <0|1|2>      Subscription QoS (default: 0)
  --retained-only        Print retained messages only, then exit
  --count <n>            Exit after n messages
  --timeout <s>          Exit after s seconds without a message
  --cafile, --cert, --key, --insecure   TLS options (see mqttpanel sub --help tls)
"##;

pub const PUB: &str = r##"Publish one message, or one message per line from stdin

Usage: mqttpanel pub [options] -t <topic> (-m <payload> | --stdin | --file <path>)

Options:
  -q, --qos <0|1|2>      QoS (default: 0)
  -r, --retain           Set the retain flag
  --delete-retained      Clear the retained message on the topic
  --repeat <s>           Send again every s seconds until Ctrl-C
"##;

pub const TLS: &str = r##"TLS options of sub and pub

Usage: mqttpanel sub [options] --tls -t <filter>

Options:
  --tls                      Connect with TLS (implied by the options below)
  --cafile <file>            CA certificate(s) to trust, PEM or DER
  --cert <file>              Client certificate, PEM, or a PKCS#12 bundle (.p12, .pfx)
  --key <file>               Client private key, PEM (not needed with PKCS#12)
  --key-passphrase <p>       Passphrase of the key or of the PKCS#12 bundle
  --insecure                 Skip certificate verification (lab only, unsafe)
  --trust-fingerprint <fp>   Trust the server certificate with this SHA-256 fingerprint
  --sni <name>               Server name sent and checked in the handshake
  --alpn <tags>              Protocol tags (ALPN), comma-separated, e.g. mqtt
"##;

pub const BROKER: &str = r##"Run the embedded broker

Usage: mqttpanel broker [options]

Options:
  --config <file>        Broker configuration file (TOML); default: the desktop app's settings
  --bind <addr>          Listen on this address only, e.g. 127.0.0.1 (all listeners)
  --format <fmt>         Log output: text (default), json, ndjson
"##;

pub const PROFILES: &str = r##"List, export and import saved connection profiles

Usage: mqttpanel profiles (list | export [options] | import <file> [options])

Commands:
  list                   Name, URL and MQTT version of each profile
  export                 Print every profile as JSON (redirect it to a file)
  import <file>          Add the profiles of an export file

Options:
  --without-secrets      Export without passwords and keys (default)
  --passphrase <p>       Export or import passwords and keys, encrypted with this passphrase
  --format <fmt>         list output: text (default), json, ndjson
"##;

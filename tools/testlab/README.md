# Test lab

Command-line scripts to test MQTT Panel by hand against the official mosquitto broker and
the official Eclipse Paho client. Nothing here runs in CI.

| Script | Direction | Use |
|---|---|---|
| `mosquitto_broker.py` | app → mosquitto | Start a mosquitto broker in one of five security modes; the script generates the certificates and prints what to enter and which files to pick in the app |
| `load_client.py` | Paho → app's broker | Connect standard clients to the embedded broker with the certificates exported from the app; generate 500 messages/s over 50 topics for throughput, chart and export tests; report rates |
| `external_clients.py` | Paho → `mqttpanel broker` | T-31: the external-client matrix (CA.pem only, issued certificates, revocation, CA rotation, extra names) against the command-line broker on a temporary home; prints PASS / FAIL / SKIP per case |

## Setup, once

```
brew install mosquitto                          # macOS; Linux: apt install mosquitto mosquitto-clients
python3 -m venv tools/testlab/.venv
tools/testlab/.venv/bin/pip install paho-mqtt cryptography
```

Below, `py` stands for `tools/testlab/.venv/bin/python`.

## A. Connect the app to mosquitto

```
py tools/testlab/mosquitto_broker.py plain                   # port 1883, no TLS
py tools/testlab/mosquitto_broker.py tls                     # port 8883, private CA
py tools/testlab/mosquitto_broker.py tls-password            # + username and password
py tools/testlab/mosquitto_broker.py mtls --p12-password x   # + client certificate (PEM and .p12)
py tools/testlab/mosquitto_broker.py self-signed             # no CA: trust by fingerprint
```

Options: `--port`, `--ws-port` (adds a WebSocket listener), `--host <name or IP>` (extra names in the
server certificate, for another machine), `--user/--password`, `--seed` (a few retained demo topics),
`--keep` (reuse the certificates), `--verbose`. Files land in `tools/testlab/out/<mode>/` (git-ignored).

The script prints the connection values for "New connection > Another broker" and the files to pick with
Browse, plus a `mosquitto_sub` command to cross-check from a terminal. Ctrl-C stops the broker.

## B. Connect standard clients to the app's broker

Start the broker in the app, then:

```
py tools/testlab/load_client.py --check                                   # plain listener, round trip
py tools/testlab/load_client.py --port 8883 --cafile ~/Downloads/CA.pem   # encrypted listener
py tools/testlab/load_client.py --port 8883 --cafile CA.pem --user mqtt-panel --password '...'
py tools/testlab/load_client.py --port 8884 --cafile CA.pem --cert device.pem --key device-key.pem
py tools/testlab/load_client.py --port 8884 --cafile CA.pem --p12 device.p12 --p12-password '...'
```

Export the files from the app: Broker > Broker certificates > Export CA.pem; Broker > Client
certificates > Export (PEM or PKCS#12). `--insecure` skips certificate checks (lab only).

Load options: `--rate 500` messages/s in total, `--topics 50`, `--skew 1.0` (Zipf exponent, 0 = even),
`--duration 60`, `--report 5`, `--no-sub`, `--mqtt3`, `--ws`, `--prefix mqttpanel-lab`.

Every report line shows published and received rate, totals, lag (published minus received) and the
mean delivery latency of JSON messages. The subscriber uses QoS 1 on `<prefix>/#`.

Topic kinds (every 6th topic, 50 topics by default):

| Kind | Topic | Payload | Charts |
|---|---|---|---|
| climate | `sensors/<room>/climate` | JSON: temperature sine, humidity random walk, online bool, mode text | temperature, humidity, online (0/1) |
| energy | `meters/<n>/energy` | JSON: power sawtooth, total counter, phase text; QoS 1 | power_w, total_kwh |
| number | `machines/<n>/speed` | plain number, random walk | yes |
| onoff | `relays/<n>/state` | `on` / `off` text | 0/1 step |
| status | `devices/<n>/status` | `ok` / `warning` / `error` | not plottable |
| telemetry | `devices/<n>/telemetry` | nested JSON with counters; retained | counters.sent, device.rssi |

Retained topics stay on the broker after a run: `py tools/testlab/load_client.py --clear-retained`.

## C. External clients with the broker's own chain (T-31)

```
py tools/testlab/external_clients.py            # builds the CLI and the helper, runs the matrix, prints the table
py tools/testlab/external_clients.py --keep     # keeps the temporary home (CA.pem, certificates, broker.json)
py tools/testlab/external_clients.py --no-build # uses target/debug as it is
```

The script starts `mqttpanel broker` with `MQTTPANEL_HOME` on a new temporary folder, TLS and secure WebSocket
listeners on free ports and secrets in memory (`MQTTPANEL_SECRETS=memory`, seeded from the helper). The command line
cannot issue certificates, so the Rust helper `crates/broker/examples/lab_home.rs` prepares and changes that home
between broker runs, exactly as the app would: setup, "Required" on, issue (PEM, or PKCS#12 with a passphrase),
revoke, rotate the CA, regenerate with extra names; it writes `CA.pem`, `server-chain.pem` and the issued files
there. The helper can be run alone:

```
cargo run -p mqttpanel-broker --example lab_home -- <home> setup <tls port> <wss port>
cargo run -p mqttpanel-broker --example lab_home -- <home> issue device-1 p12 'passphrase'
cargo run -p mqttpanel-broker --example lab_home -- <home> regen broker.example.com 203.0.113.7
```

Paho connects with `CA.pem` as its only trust anchor, by localhost, 127.0.0.1, the host name, its .local name and
the first LAN address, over TLS and WSS, MQTT 3.1.1 and 5, with a QoS 0, 1 and 2 round trip each. Names that do
not resolve (the extra name, sometimes the host name) are checked with a hosts-style override: the connection goes
to 127.0.0.1 and TLS verifies the given name. A PKCS#12 bundle is opened with its passphrase (Python `cryptography`)
as a device would. A broker change needs a restart of `mqttpanel broker`, so "the open session is dropped on
revocation" is reported as SKIP here and covered live by `crates/broker/tests/external_chain.rs` (T-29) in CI.

## Public brokers

Never point `load_client.py` at a public broker with the default rate. The project rule for public
brokers is a throwaway prefix, at most 50 messages and no retained messages left behind.

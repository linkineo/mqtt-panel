# Test lab

Command-line scripts to test MQTT Panel by hand against the official mosquitto broker and
the official Eclipse Paho client. Nothing here runs in CI.

| Script | Direction | Use |
|---|---|---|
| `mosquitto_broker.py` | app → mosquitto | Start a mosquitto broker in one of five security modes; the script generates the certificates and prints what to enter and which files to pick in the app |
| `load_client.py` | Paho → app's broker | Connect standard clients to the embedded broker with the certificates exported from the app; generate 500 messages/s over 50 topics for throughput, chart and export tests; report rates |
| `mcp_prompts/run.py` | Claude Code → app (MCP) | Plain English prompts to Claude Code with MQTT Panel as its MCP server, against Mosquitto and the built-in broker; each outcome checked by machine (M-19 to M-35, `docs/MCP_PROMPTS.md`) |
| `external_clients.py` | Paho → `mqttpanel broker` | T-31: the external-client matrix (CA.pem only, issued certificates, revocation, CA rotation, extra names) against the command-line broker on a temporary home; prints PASS / FAIL / SKIP per case |
| `update_server.py` | app → local update server | U-10: serve a `latest.json` and an archive signed with a throwaway key, so a debug bundle of the direct macOS download finds, downloads and installs an update; modes offer, none, fail, bad-signature (`docs/UPDATER.md`, Tests) |

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

## D. Prompts to an assistant (MCP, Claude Code as the host)

```
py tools/testlab/mcp_prompts/run.py                    # builds the debug app, runs every case, writes the report
py tools/testlab/mcp_prompts/run.py --case C07 C08     # some cases
py tools/testlab/mcp_prompts/run.py --no-build --keep  # target/debug as it is; keep the temporary folders
```

Needs Claude Code (`claude`, signed in) and mosquitto. Each case and each attempt gets new certificates, a Mosquitto
in the mode it needs on free ports, and the debug app (`target/debug/mqtt-panel`) on a new temporary
`MQTTPANEL_HOME` with `MQTTPANEL_SECRETS=memory`, English, and the assistant acknowledgement already accepted in its
`settings.json` (except case C15). Claude Code runs `claude -p "<prompt>"` with a temporary `--mcp-config` (the app
executable with the argument `mcp`), `--strict-mcp-config`, only `mcp__mqttpanel__*` allowed and no built-in tools,
so nothing asks for permission and nothing outside MQTT Panel is touched; the real app data folder, the keychain and
the real assistant configurations are never used.

After the session the runner reads the app through its own MCP socket (`list_connections`, `broker_status`,
`list_charts`), looks at what a Paho subscriber on `lab/#` received, the Mosquitto log, the files written and the tool
calls Claude made, and runs the case's check (`mcp_prompts/cases.py`). A failed case runs once more with the same
prompt; both attempts are kept. Results: `tools/testlab/mcp_prompts/results/<timestamp>/` (git-ignored) with
`REPORT.md` (case, prompt, PASS/FAIL, retries, tool calls, notes, every answer) and per attempt `transcript.jsonl`
(Claude's stream-json output), `verify.json`, `mosquitto.log` and `app.log`. Passwords and passphrases are replaced by
••• in every result file. The built-in broker cases use its default ports (1883, 8883, 8083): stop any other broker on
them first.

The debug build loads its window from the dev server, which the runner does not start: the window stays blank and
the cases use only the app side. The in-app way of giving secrets (`ask_in_app`) needs a person in the window, so
every case gives passwords and files directly in the prompt. Actions that wait for Allow or Deny in the window
(`docs/MCP.md` section 3) are answered by nobody and end as "denied" after 60 s; the current cases write their files
to temporary folders and need none. A debug app started with `MQTTPANEL_TEST_CONFIRM=allow` (or `deny`) answers every
confirmation at once, for cases that need one; release builds ignore it.

## E. Self-update of the direct macOS download (U-10)

`update_server.py` needs only Python 3 (no venv). The full run (throwaway key, test overlay, an installed 1.0.1 and a
1.0.2 archive, both debug bundles) is in its header and in `docs/UPDATER.md`, Tests. Files land in
`tools/testlab/out/update/` (git-ignored); the throwaway key never leaves that folder.

```
python3 tools/testlab/update_server.py overlay --pubkey tools/testlab/out/update/test.key.pub
python3 tools/testlab/update_server.py serve --version 1.0.2 --archive "target/debug/bundle/macos/MQTT Panel.app.tar.gz"
python3 tools/testlab/update_server.py serve --version 1.0.2 --archive "…" --mode bad-signature   # or none, fail
```

## Public brokers

Never point `load_client.py` at a public broker with the default rate. The project rule for public
brokers is a throwaway prefix, at most 50 messages and no retained messages left behind.

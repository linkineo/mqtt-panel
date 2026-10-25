# MCP prompts: plain English prompts to an assistant, checked by machine

The assistant interface (`docs/MCP.md`) tested end to end with Claude Code as the host. Each case is a prompt a user
would type, run once against MQTT Panel through its MCP server, and checked by machine: the app's state read through
its own MCP socket after the session, what an external Eclipse Paho client received or could do, the Mosquitto log,
the files written, the tool calls the assistant made and its answer. Test plan rows M-19 to M-35, level "lab".

Runner: `tools/testlab/mcp_prompts/run.py` (setup and options in `tools/testlab/README.md`, section D). The cases,
their lab and their checks are in `tools/testlab/mcp_prompts/cases.py`; this page lists them for people.

## How the cases run

- Host: Claude Code, `claude -p "<prompt>"`, a temporary `--mcp-config` with the debug app executable and the argument
  `mcp`, `--strict-mcp-config`, only `mcp__mqttpanel__*` allowed, no built-in tools, at most 25 turns. No system
  prompt or hint is added: the prompt is all the model gets besides the tool list and the server's instructions.
- App: a new temporary `MQTTPANEL_HOME` per case and attempt, `MQTTPANEL_SECRETS=memory`, English, the
  acknowledgement accepted in `settings.json` (except C15). The real data folder, keychain and assistant
  configurations are never used.
- Lab: certificates generated per run (`labcerts.py`), Mosquitto on free ports in the mode the case needs
  (`mosquitto_broker.py`), a Paho subscriber on `lab/#`, a Paho publisher for the data cases.
- Placeholders below are filled per run: `<port>`, `<ws_port>`, `<ca>`, `<cert>`, `<key>`, `<p12>` (absolute paths in
  the lab folder), `<user>`, `<password>`, `<wrong_password>`, `<p12_pass>` (generated, shown as ••• in the results),
  `<fingerprint>`, `<folder>`, `<file>`.
- A failed case runs once more with the same prompt; both attempts are kept in the results.

Not covered here, by design: the in-app way of giving secrets and picking files (`ask_in_app`) needs a person to
type or click in MQTT Panel's window, so every prompt gives passwords, passphrases and file paths directly (the warned
way). For the same reason `broker_issue_client_cert` (its key files always go to a folder picked in the app) and the
PNG chart export (drawn by the window) are not in the lab; the Rust tests M-11, M-14 and M-16 cover them with a
scripted window. The debug build's window stays blank (no dev server), so only the app side runs.

## Cases

### C01 (M-19): Mosquitto over TCP

```text
Connect MQTT Panel to my Mosquitto broker on localhost, port <port>, no encryption. What temperature does the hall sensor report? Then publish the text "hello from C01" to the topic lab/c01/greeting.
```

- Lab: Mosquitto, no TLS, retained demo topics.
- Exercises: create_connection, connect, list_topics or get_topic, publish.
- Expected: Connected over mqtt; the answer gives 21.5; Paho receives the greeting.

### C02 (M-20): Mosquitto over WebSocket

```text
My Mosquitto broker also takes MQTT over WebSocket on localhost, port <ws_port>, no encryption. Connect MQTT Panel to it over WebSocket. What temperature does the hall sensor report? Then publish "hello from C02" to lab/c02/greeting.
```

- Lab: Mosquitto, no TLS, WebSocket listener, retained demo topics.
- Exercises: protocol ws, port, ws path default.
- Expected: Connected over ws on the WebSocket port; 21.5; greeting received.

### C03 (M-21): TLS with a CA file

```text
Connect MQTT Panel to the broker at localhost, port <port>. It uses TLS with a certificate from our own CA; the CA certificate is the file <ca>. Then publish "hello from C03" to lab/c03/greeting.
```

- Lab: Mosquitto, TLS from the lab CA, retained demo topics.
- Exercises: protocol mqtts, set_ca_certificate (direct file).
- Expected: Connected over mqtts with the CA set, nothing pinned; greeting received.

### C04 (M-22): TLS with username and password

```text
Connect to the broker at localhost, port <port>, over TLS. The CA certificate is <ca>. Log in as <user> with the password <password>. Then publish "hello from C04" to lab/c04/greeting.
```

- Lab: Mosquitto, TLS and a password file, retained demo topics.
- Exercises: set_connection_password with the password given (direct way, warning).
- Expected: Connected with the user and the stored password; greeting received.

### C05 (M-23): Mutual TLS, PEM certificate and key

```text
The broker at localhost, port <port>, uses TLS and asks for a client certificate. The CA certificate is <ca>. My client certificate is <cert> and its key is <key>. Connect MQTT Panel to it and publish "hello from C05" to lab/c05/greeting.
```

- Lab: Mosquitto, TLS, client certificate required, retained demo topics.
- Exercises: set_client_certificate cert_file and key_file.
- Expected: Connected with the client certificate; greeting received.

### C06 (M-24): Mutual TLS, PKCS#12 bundle

```text
The broker at localhost, port <port>, uses TLS and asks for a client certificate. The CA certificate is <ca>. My client certificate is the PKCS#12 file <p12>, its passphrase is <p12_pass>. Connect MQTT Panel to it and publish "hello from C06" to lab/c06/greeting.
```

- Lab: Mosquitto, TLS, client certificate required, client.p12 with a passphrase, retained demo topics.
- Exercises: set_client_certificate with a .p12 and its passphrase.
- Expected: Connected with the bundle; greeting received.

### C07 (M-25): Self-signed broker, pin the certificate

```text
Connect MQTT Panel to the broker at localhost, port <port>, over TLS. It uses a self-signed certificate; its SHA-256 fingerprint is <fingerprint>. I have checked it, so trust that certificate. Then publish "hello from C07" to lab/c07/greeting.
```

- Lab: Mosquitto, self-signed server certificate, retained demo topics.
- Exercises: test_connection or connect, trust_certificate pin.
- Expected: trust_certificate pin; the lab fingerprint pinned; connected; greeting received.

### C08 (M-26): Wrong password

```text
Connect to the broker at localhost, port <port>, over TLS. The CA certificate is <ca>. Log in as <user> with the password <wrong_password>.
```

- Lab: Mosquitto, TLS and a password file.
- Exercises: the login refusal is reported, no blind retries.
- Expected: Not connected; the answer says the broker refused the login (bad username or password); at most three attempts, no password guessing.

### C09 (M-27): Expired broker certificate

```text
Connect MQTT Panel to the broker at localhost, port <port>, over TLS. The CA certificate is <ca>.
```

- Lab: Mosquitto, TLS from the lab CA, server certificate expired yesterday.
- Exercises: the certificate question is explained, nothing trusted or skipped.
- Expected: Not connected; the answer says the certificate expired; no trust_certificate call.

### C10 (M-28): Host name mismatch

```text
Connect MQTT Panel to the broker at localhost, port <port>, over TLS. The CA certificate is <ca>.
```

- Lab: Mosquitto, TLS from the lab CA, server certificate for other.example only.
- Exercises: the name mismatch is explained, nothing trusted or skipped.
- Expected: Not connected; the answer names other.example; no trust_certificate call.

### C11a (M-29): MQTT 3.1.1 chosen in the prompt

```text
Connect MQTT Panel to the broker on localhost, port <port>, no encryption, using MQTT 3.1.1. Then publish "hello from C11a" to lab/c11a/greeting.
```

- Lab: Mosquitto, no TLS.
- Exercises: mqtt_version 3.1.1.
- Expected: Profile version 3.1.1; Mosquitto logs the client with p4; greeting received.

### C11b (M-30): MQTT 5 chosen in the prompt

```text
Connect MQTT Panel to the broker on localhost, port <port>, no encryption, using MQTT 5. Then publish "hello from C11b" to lab/c11b/greeting.
```

- Lab: Mosquitto, no TLS.
- Exercises: mqtt_version 5.
- Expected: Profile version 5; Mosquitto logs the client with p5; greeting received.

### C12a (M-31): Built-in broker, client certificates required

```text
Set up MQTT Panel's built-in broker so that devices must connect over TLS with a client certificate. Connect MQTT Panel itself to it, and save the broker's CA certificate into the folder <folder>.
```

- Lab: built-in broker only (default ports 1883, 8883, 8083).
- Exercises: broker_set_client_cert_rule or broker_add_listener mtls, connect_builtin_broker, broker_export_ca (folder).
- Expected: Broker running, rule required, Local broker connected, CA.pem in the folder; an external client verifies the broker with CA.pem and is refused without a client certificate.

### C12b (M-32): Built-in broker, user and password over TLS

```text
Set up MQTT Panel's built-in broker so my sensors can log in over TLS as user sensor1 with the password <password>. Save the broker's CA certificate into the folder <folder>, and connect MQTT Panel to the broker too.
```

- Lab: built-in broker only (default ports 1883, 8883, 8083).
- Exercises: broker_add_user (direct password), broker_export_ca (folder), connect_builtin_broker.
- Expected: User sensor1, CA.pem in the folder; an external Paho client with CA.pem and sensor1 publishes and MQTT Panel receives it.

### C13 (M-33): Busiest topic and a chart

```text
Connect MQTT Panel to the broker on localhost, port <port>, no encryption. Which topic gets the most messages? Make a chart of its temperature.
```

- Lab: Mosquitto, no TLS, Paho publisher: boiler 10/s, office 1/2 s, garage 1/5 s, status 1/5 s.
- Exercises: list_topics (rates), get_topic, create_chart and add_series or set_topic_chart.
- Expected: The answer names lab/c13/plant/boiler; list_charts shows a series of its temperature.

### C14 (M-34): History and export to a file

```text
Connect MQTT Panel to the broker on localhost, port <port>, no encryption. Show me the last 5 values received on lab/c14/meter, then save everything received on that topic as a CSV file at <file>.
```

- Lab: Mosquitto, no TLS, Paho publisher: lab/c14/meter 5/s.
- Exercises: get_history, export_data with a file path.
- Expected: get_history on the topic; the CSV file exists with a header and rows.

### C15 (M-35): Before the acknowledgement

```text
Connect MQTT Panel to the broker on localhost, port <port>, and publish "hello" to lab/c15/greeting.
```

- Lab: Mosquitto, no TLS, acknowledgement not accepted.
- Exercises: writing tools refused until the user accepts in the app.
- Expected: Writing calls refused; no connection; the answer tells the user to accept the acknowledgement in MQTT Panel, Settings, Assistant.

## Results, 25 Oct 2026

Claude Code 2.1.310, model claude-opus-5-5, MQTT Panel 1.0.0 debug build on macOS.

| Case | M | Result | Retries | Tool calls made |
|---|---|---|---|---|
| C01 TCP | M-19 | PASS | 0 | list_connections, create_connection, connect, list_topics, publish |
| C02 WebSocket | M-20 | PASS | 0 | list_connections, create_connection, connect, list_topics, publish |
| C03 TLS, CA file | M-21 | PASS | 0 | list_connections, create_connection, set_ca_certificate, connect, publish |
| C04 TLS, password | M-22 | PASS | 0 | create_connection, set_ca_certificate, set_connection_password, connect, publish |
| C05 mutual TLS, PEM | M-23 | PASS | 0 | create_connection, set_ca_certificate, set_client_certificate, connect, publish, get_topic |
| C06 mutual TLS, PKCS#12 | M-24 | PASS | 0 | create_connection, set_ca_certificate, set_client_certificate, connect, publish |
| C07 self-signed, pin | M-25 | PASS | 0 | list_connections, create_connection, test_connection, trust_certificate (pin), connect, publish |
| C08 wrong password | M-26 | PASS | 0 | create_connection, set_connection_password, set_ca_certificate, connect, broker_status |
| C09 expired certificate | M-27 | PASS | 0 | list_connections, create_connection, set_ca_certificate, connect |
| C10 host name mismatch | M-28 | PASS | 0 | list_connections, create_connection, set_ca_certificate, connect, test_connection |
| C11a MQTT 3.1.1 | M-29 | PASS | 0 | list_connections, create_connection, connect, publish |
| C11b MQTT 5 | M-30 | PASS | 0 | list_connections, create_connection, connect, publish |
| C12a built-in, certificates required | M-31 | PASS | 0 | broker_status, broker_set_client_cert_rule, broker_update_listener ×2, connect_builtin_broker, broker_export_ca, broker_status |
| C12b built-in, user over TLS | M-32 | PASS | 0 | broker_status, broker_add_user, broker_export_ca, connect_builtin_broker, broker_status |
| C13 busiest topic, chart | M-33 | PASS | 0 | list_connections, create_connection, connect, list_topics ×2, create_chart, add_series, show_chart |
| C14 history, CSV export | M-34 | PASS | 0 | list_connections, create_connection, connect, get_history, export_data |
| C15 no acknowledgement | M-35 | PASS | 0 | list_connections, create_connection (refused), show_settings |

Every prompt is the first and only wording: no case needed a rewrite or an extra hint. Each case took 10 to 25 s
(C15 75 s: the refused call waits 60 s for the acknowledgement).

### What the first run found, and the fixes

- **A password already in the prompt was sometimes asked again in the window.** In the first full run, C08 (first
  attempt) and C12b called `set_connection_password` and `broker_add_user` with `ask_in_app: true` although the user
  had written the password in the prompt; with nobody at the window the call waited 120 s and C08 failed (the assistant
  then asked the user to choose). C04 and the retry of C08 used the password directly. The descriptions said "use the
  direct way only when the user insists". Fix: the four tools with a secret field (`set_connection_password`,
  `set_client_certificate`, `broker_add_user`, `broker_issue_client_cert`) now add: when the user has already written
  the value in the chat, use it directly, since it has already passed through the chat and asking again only makes them
  type it twice. The in-app way stays the recommendation otherwise. After the fix every password and passphrase case
  passed first time, in two runs. Test: `registry_rules` checks the sentence on every tool with a secret field.
- **`connect` said `certificateVerified: true` on a plain connection.** Nothing was verified on mqtt:// or ws://, and
  an assistant could have repeated it. Fix: `certificateVerified` is null without TLS (test M-01 asserts it on the plain
  listener).

### Notes and open points

- **Host name mismatch (C10).** The app's error says "set Server name (SNI)", but `update_connection` has no server
  name field, so the assistant correctly said it could not change it and pointed to the window. Proposed: add
  `server_name` to `update_connection` (a spec change, not done here).
- **Expired certificate (C09) and mismatch (C10).** The assistant stopped, explained the reason in plain words and
  offered the trust choices without picking one; it never called `trust_certificate` unasked. In one run, for the expired
  certificate, it presented pinning as a "reasonable stopgap for a test lab", which is fair but worth watching.
- **Wrong password (C08).** One connect (sometimes one `test_connection` too), then a clear report of "bad username or
  password" with reason code 0x87; no guessing. One run also read `broker_status` to rule out the built-in broker,
  which is harmless.
- **Built-in broker (C12a).** Asked only for "TLS with a client certificate", the assistant also switched off the
  plain TCP and WebSocket listeners and said so; reasonable, and reversible.
- **Busiest topic (C13).** The `ratePerSecond` of `list_topics` was enough to pick the topic within a second of
  connecting; the chart filled from the history already received.
- **Before the acknowledgement (C15).** The refusal sentence was relayed and the assistant opened Settings, Assistant
  with `show_settings`, then stopped.
- The assistant warned about secrets given in the chat in every password case, as the result's warning asks.

### Verdict

Simple prompts are enough. With the current tool descriptions, a user who names the broker, the port, the files and
the secret in ordinary words gets a working, verified connection in one go, including mutual TLS, PKCS#12, pinning
a self-signed certificate and the built-in broker; failures are explained rather than worked around. The one weak spot
was the model hesitating between the in-app and the direct way for a secret already in the prompt, fixed in the
descriptions. These results come from one host and one model; ChatGPT (Codex configuration) and Claude Desktop were
not run here.

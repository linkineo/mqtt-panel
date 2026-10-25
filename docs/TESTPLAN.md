# MQTT Panel test plan

Version 1. Companion to `docs/SPEC.md` and `docs/FEATURES.md`. Every test has an ID, a level, and an automation tag. The plan runs **standalone** (nothing but the repo, the embedded broker and generated certificates) and, when the network allows, **against public brokers** too.

Levels: **U** unit (cargo test / vitest), **I** integration (real sockets, embedded broker or a local Mosquitto), **E** end-to-end (the built app driven by Playwright or tauri-driver), **M** manual (walkthrough with a checklist; results recorded in `docs/testlog/<date>.md`).

Automation tags: `auto` runs in CI on every push; `nightly` runs in the nightly job (public brokers, performance); `manual` is executed before each release; `lab` runs on a developer machine with the test lab tools (`tools/testlab/`), never in CI.

Pass rule: a release needs every `auto` and `nightly` test green on macOS, Windows and Linux, and every `manual` test ticked for the release build.

---

## 0. Fixtures and harness

| ID | What | Notes |
|---|---|---|
| F-01 | **Local CA kit** generated at test start with `rcgen`: root CA (ECDSA P-256, 5 y), intermediate CA, server certs for `localhost`, `127.0.0.1`, `::1` and the machine hostname (2 y), one expired server cert (not-after yesterday), one not-yet-valid cert, one cert for the wrong name (`other.example`), client certs `client-ok`, `client-revoked`, `client-other-ca` (signed by a second, unrelated CA), RSA-2048 variants of server and client certs, a PKCS#12 bundle with passphrase `test-passphrase` and one without. | Written to a temp dir; never committed. |
| F-02 | **Embedded broker harness**: starts `crates/broker` in-process with chosen listeners and auth, returns ports. Can start several brokers on random free ports. | Used by all I tests. |
| F-03 | **Mosquitto harness** (optional): if `mosquitto` is on PATH (brew), start it with a generated config for TLS, mTLS, password file and WebSocket; skip the dependent tests with a visible SKIP otherwise. | Public brokers cover the "foreign broker" angle when Mosquitto is absent. |
| F-04 | **Packet corpus** (`crates/engine/tests/corpus/*.json`): recorded sequences of PUBLISH events with timestamps for replay into the engine. Includes `datapoints.json` (section 3), `tree-100k.json` (section 8), `sparkplug.json`, `cbor.json`, `proto.json`. | Replayed in U tests; same vectors reused by UI tests through the mock engine. |
| F-05 | **Public broker table** (`tests/public-brokers.toml`): `test.mosquitto.org` (1883 plain, 8883 TLS with its published CA, 8884 TLS requiring a client cert issued by its own CA tool, 8080/8081 WebSocket), `broker.emqx.io` (1883, 8883, 8083 ws, 8084 wss), `broker.hivemq.com` (1883, 8000 ws). Each entry has the ports, which features it supports, and a `verified_on` date. **Facts to verify at first run**; a broker that does not answer marks its tests SKIP, not FAIL. | Topics used on public brokers are always under `mqttpanel-test/<random-id>/…` with small payloads, QoS 0 or 1, no retained messages left behind (every retained test publishes an empty retained message afterwards), at most 50 messages per test. |
| F-06 | **Fake clock** in the engine for rate, age and expiry tests. | |
| F-07 | **Screenshot baselines** under `tests/visual/<os>/<theme>/<lang>/` for every mockup screen, compared with a 0.5 % pixel tolerance. | Re-recorded only by an explicit command, reviewed in a gallery. |

---

## 1. Connections and profiles

| ID | Level | Tag | Test | Expected |
|---|---|---|---|---|
| C-01 | U | auto | Profile round trip: save, reload, export without secrets, import. | Fields identical; `credentialRef` present, no password in the file; export has no secret. |
| C-02 | I | auto | Keychain: store, read, delete a password and a key passphrase. On Linux without Secret Service, the encrypted-file fallback with passphrase prompt. | Secrets never in plain files; fallback clearly reported in Settings. |
| C-03 | I | auto | Connect plain TCP to embedded broker, MQTT 3.1.1 and 5 and "auto". | Pill shows negotiated version; "auto" against a 3.1.1-only broker (Mosquitto started with `max_protocol_version` or a harness stub) shows 3.1.1. |
| C-04 | I | auto | Connect over WebSocket (ws) and secure WebSocket (wss) with path `/mqtt`. | Works; wrong path gives "The broker closed the WebSocket: path /x not accepted (HTTP 404)". |
| C-05 | I | auto | Several profiles connected at once; switch in the pill. | Explorer, Publish and Charts scope to the active one; the others keep receiving. |
| C-06 | I | auto | Reconnect policy: broker stopped and started again while connected. | Pill shows "reconnecting", subscriptions restored, counters continue, no duplicate subscriptions. |
| C-07 | E | auto | "Test" success message content. | Latency, server cert subject/issuer/validity, negotiated version, max packet size. |
| C-08 | E | auto | Subscriptions default: new profile has one chip `# · QoS 0`; remove it; warning "No subscriptions…" appears; add `building/#`. | As specified. |
| C-09 | E | manual | File pickers: Browse opens the native dialog; drop a PEM on the field; drop a .txt. | Parsed details shown; the .txt gives "This file is not a certificate". No typed path anywhere. |
| C-10 | E | auto | Import from other tools: a predecessor-tool settings file and an MQTTX export fixture (synthetic, committed under `tests/fixtures/import/`). | Profiles created with host, port, scheme, subscriptions; secrets not imported; a summary lists what was skipped. |
| C-12 | E | auto | "New connection" menu: "New broker…" opens the empty form without a Target row; "Built-in broker" shows "Use an existing listener" and "Add a listener…"; the first-launch cards route to the same places | As specified. |
| C-13 | E | auto | "Built-in broker" > "Add a listener…": Add listener steps 1 and 2 (no login row), then the dialog's steps run (user, certificate if the listener requires one, start if stopped, connect) and the Explorer opens on "Local broker"; with Anonymous on no user is created and the dialog says so | As specified. |
| C-14 | E | auto | Connected card filled with the accent colour and "connected" top right; reconnecting card outlined in clay; card menu Connect/Disconnect, Test, Duplicate, Delete with confirmation; header has Delete, Test, Connect and no Duplicate | As specified. |
| C-15 | E | auto | Connection bar: appears with the first open connection, one tab per open connection with name, dot and figures; click opens the Explorer on it; ✕ disconnects; hidden when none is open; title bar pill shows only broker state and lab flags | As specified. |
| C-16 | I | auto | "Local broker" profile follows the broker: change its listener's port, the client-certificate requirement, the mqtt-panel password and Anonymous; the profile is updated at once; when connected it reconnects at once if its own listener or login changed and stays connected otherwise | As specified. |
| C-17 | E | auto | Connect on the profile "Local broker" while the broker is stopped: the broker starts (card "starting broker…", then "connecting"), the connection opens; "Test" does the same; a broker that cannot start fails the connection with its reason | As specified. |
| C-18 | E | auto | Stop button on cards: a connected, connecting or reconnecting card shows the round ■ button; clicking it disconnects that connection only, the broker keeps running, other connections stay | As specified. |
| C-19 | E | auto | Form folds: Security and Advanced closed at start, Basics open; fold headers show the summary ("verify on · CA · client certificate"; "keep-alive 60 s · 3 subscriptions"); state kept while navigating; no helper sentences next to controls; overflow check in EN, FR, DE | As specified. |
| C-20 | E | auto | No parsed chips next to CA, client certificate, client key or password (no "until", "CN=", "Keychain"); a wrong file still shows the plain error; "mTLS" and "Mutual TLS" appear nowhere in the three catalogs; cards show TLS, WSS or no TLS | grep of the catalogs and the rendered form. |
| C-21 | E | auto | Play button on idle cards: an idle card shows the round ▶ button where active cards show ■, never both; clicking it connects that profile (for "Local broker" the broker starts first); after the connection opens the card shows ■ | As specified. |
| C-22 | E | auto | Form order and address prefiller: Basics rows read Name, Protocol, MQTT version, Host and port, (WebSocket path), Client ID, Username, Password; choosing ws rewrites the field prefix to "ws://" and the port box to 8083 when it held the previous default; typing "mqtt://" selects mqtt; typing ":1884" at the end sets the port box to 1884; editing the port box rewrites the suffix; pasting "wss://demo.emqx.example:8084/mqtt" sets protocol, host, port and path; "[::1]:1883" keeps its brackets; a deleted prefix comes back on blur; the saved profile holds host without scheme or port | As specified. |
| C-23 | E | auto | Security disabled: with mqtt or ws the Security fold is greyed, does not open and reads "Not encrypted: choose mqtts or wss to use certificates." (EN, FR, DE); switching to mqtts reopens it with the previous values intact; the old "Certificates only apply…" line is gone | As specified. |
| C-24 | E | auto | Wording (superseded by C-26 for the name itself): "this app's broker", « cette app » and „dieser App“ appear nowhere in the three catalogs | grep of the catalogs and the rendered screens. |
| C-25 | E | auto | No import entry point in the UI: no "Import" line under the list, no drop zone, no welcome import card, no native menu item, no palette entry; the import crate tests still pass | As specified; crate tests unchanged. |
| C-26 | E | auto | Wording: menu "New broker…" and "Built-in broker", dialog title "Connect to the built-in broker", Client ID button "Generate"; FR catalog has « Connecter », « Générer », « Nouveau broker… », « Broker intégré » and no « Se connecter », « Aléatoire »; "MQTT Panel broker" and "Another broker" appear nowhere in the three catalogs | grep of the catalogs and the rendered screens (change set 6). |
| C-27 | E | auto | Card layout: three lines (name with state and play or stop; one protocol chip mqtt / mqtts / ws / wss with the ⚠ icon when verification is skipped or a fingerprint pinned; "host:port" in monospace, no prefix); no version, subscriptions, last-used or meta text; ⚠ tooltip texts in EN, FR, DE | As specified. |
| C-28 | E | auto | Auto name: a new profile's Name follows the host field (myhost.com → "myhost", broker.plant.example → "broker", 10.20.4.17 → "10.20.4.17", [::1] → "::1", localhost → "localhost"); typing a name stops the following, clearing it resumes; an existing profile is not renamed when its host changes | As specified. |
| C-29 | E | auto | Per-connection skip: the Advanced switch opens the prompt; "Always for this connection" saves `tls.skipVerify` on and the Advanced summary reads "skip verification"; "Only this time" leaves it off, shows "next connection only" and the next attempt skips once; turning it off needs no prompt; the ⚠ icon shows on the card and the tab while the connection is open; Settings has no skip switch | As specified. |
| C-31 | E | auto | Every info button is the round ⓘ (one component, 18 px circle); no pill-shaped info chip remains; screenshots light and dark | As specified. |
| C-32 | E | auto | Accepted formats are written after Browse (CA "PEM, CRT, DER"; client certificate "PEM, CRT, DER, P12, PFX"; key "PEM, KEY") and match the picker filters; the password eye reveals the typed and the stored password while pressed; rows never wrap in EN, FR, DE | As specified. |
| C-33 | E+I | auto | Generate key and request a certificate: Generate key stores an ECDSA (or RSA) key in the secret store and shows its fingerprint; Download CSR writes a PEM request with the chosen common name that the broker's "Import CSR…" signs; Import signed certificate accepts the signed certificate and refuses one for another key; the connection then authenticates with the generated key against the embedded broker with client certificates required; Start over clears; switching segments with material asks first; the fold summary reads "(generated key)" or "(imported)" | As specified. |
| C-34 | E | auto | Session expiry field shows only the unit; its ⓘ explains 0 and empty; an empty field saves as 0 | As specified. |
| C-30 | E | auto | Certificate prompt: unverifiable certificate on Connect or Test opens the list with "Trust this certificate" preselected, the two skip choices and, only with a private CA chain, "Trust anything this CA signs"; the risk sentence is shown; Connect applies the choice and retries; "only this time" saves nothing; "always" sets the profile switch | As specified. |
| C-11 | I | nightly | Public brokers plain: connect to each F-05 broker on 1883, subscribe `mqttpanel-test/<id>/#`, publish 3 messages, receive them. | Received within 2 s; SKIP if unreachable. |

## 2. Certificates and TLS (the combination matrix)

Each row is run against the embedded broker (TLS listener configured with the named server cert) and, where marked P, also against public brokers.

| ID | Level | Tag | Server side | Client side | Expected |
|---|---|---|---|---|---|
| T-01 | I | auto | Cert from local root CA | CA file set | Connects; Properties shows chain. |
| T-02 | I | auto | Cert from intermediate CA, server sends chain | Root CA only | Connects (intermediate taken from the handshake). |
| T-03 | I | auto | Cert from intermediate CA, server sends leaf only; also a leaf signed directly by another CA (T-03b) | Root CA only | Trust prompt (unknown CA), never a hard failure (CONN-001): "Reason: the certificate is signed by CN=…, which is not the CA set in this profile. The broker may also have left out its intermediate certificate. …"; pinning the fingerprint then connects. |
| T-04 | I | auto | Self-signed | nothing | Trust prompt with subject, SAN list, SHA-256 fingerprint, validity, reason "self-signed"; one action "Trust this certificate"; after trusting, connects; fingerprint stored per profile. |
| T-05 | I | auto | Self-signed, then the server cert is replaced | fingerprint trusted earlier | Connection stops; prompt returns saying the fingerprint changed, old and new shown. |
| T-06 | I | auto | Chain ending in a private CA, unknown to the client | nothing | Prompt shows the second action "Trust anything this CA signs"; choosing it fills the CA field; reconnect works; a renewed server cert from the same CA connects without prompt. |
| T-07 | I | auto | Expired cert | CA set | "The broker's certificate expired on <date>." with the trust-by-fingerprint option offered and marked unsafe. |
| T-08 | I | auto | Not-yet-valid cert | CA set | "…is not valid before <date>. Check the clock on the broker or this computer." |
| T-09 | I | auto | Cert for `other.example` | CA set, host `localhost` | "The certificate is for other.example, not localhost." with the hint to set Server name (SNI) if the broker is reached through an alias. |
| T-10 | I | auto | Cert for `broker.lab` only | CA set, host is the IP, SNI = `broker.lab` | Connects (SNI used for both the handshake and name check). |
| T-11 | I | auto | Server requires client cert | none provided | "The broker requires a client certificate and none is set in this profile." |
| T-12 | I | auto | Server requires client cert | `client-ok` PEM + key | Connects; broker log shows CN. |
| T-13 | I | auto | Server requires client cert | `client-other-ca` | "The broker rejected the client certificate (not issued by a CA it trusts)." |
| T-14 | I | auto | Server requires client cert | `client-revoked` | Rejected at handshake after revocation; message names the certificate. |
| T-15 | I | auto | Server requires client cert | PKCS#12 with passphrase | Passphrase asked once, stored in keychain, connects; wrong passphrase: "The passphrase for client.p12 is wrong." |
| T-16 | I | auto | Server requires client cert | PKCS#12 without passphrase | Imports without a prompt. |
| T-17 | I | auto | RSA-2048 server cert | ECDSA client cert, and the reverse | Both connect. |
| T-18 | I | auto | TLS 1.2-only server (Mosquitto option, SKIP without it) | default | Connects; Properties shows TLS 1.2. |
| T-19 | I | auto | ALPN required (harness server that rejects without `mqtt`) | ALPN empty, then `mqtt` | First fails with "The broker closed the TLS handshake; it may require a Protocol tag (ALPN) such as mqtt."; second connects. |
| T-20 | I | auto | Any TLS failure | profile switch "Skip unverified broker certificate" on, or "only this time" | Connects; the tab and card show the ⚠ icon; Properties marks the chain unverified; "only this time" covers one attempt and leaves the profile unchanged. |
| T-21 | I | nightly, P | test.mosquitto.org:8883 with its published CA | CA file | Connects. SKIP if the CA download or the broker is unavailable. |
| T-22 | I | nightly, P | test.mosquitto.org:8884 (client cert required) | cert obtained from the broker's own tool, if available | Connects; without a cert, T-11's message. |
| T-23 | I | nightly, P | broker.emqx.io:8883 and 8084 (public CA) | nothing (system roots) | Connects; chain shows a public CA. |
| T-24 | I | nightly, P | A public broker reached via IP with SNI empty | nothing | Name mismatch message T-09, then success after setting SNI. |
| T-25 | U | auto | Certificate parsing: PEM with CRLF, PEM bundle with several certs, DER, PKCS#8 and traditional keys, encrypted keys, garbage. | Each parsed or refused with the specific reason. |
| T-26 | E | auto | Every security field shows its ⓘ bubble with the agreed text in EN, FR, DE. | Text matches the catalog. |
| T-28 | I | auto | External client with the broker's own chain (`crates/broker/tests/external_chain.rs` with the Rust client as the outsider): TLS and WSS by localhost, by the host name and by a LAN address with CA.pem only; MQTT 3.1.1 and 5; QoS 0, 1, 2 round trips | Connected and messages received; a wrong CA and a wrong host name are refused. |
| T-29 | I | auto | External client with a client certificate issued by the broker, PEM and PKCS#12 (with passphrase), with Required on; without a certificate refused; after revocation refused at the next handshake and the open session dropped | As specified. |
| T-30 | I | auto | CA rotation: the old client certificate is refused, a newly issued one accepted, the exported CA.pem after rotation verifies the server; server certificate regeneration with an extra name: a client connecting by that name verifies | As specified. |
| T-31 | I | lab | Same matrix with Paho (`tools/testlab/external_clients.py`) against `mqttpanel broker` started on a temporary home with the exported CA.pem and issued certificates | Report of PASS/FAIL per case; SKIP without mosquitto/paho. |
| T-27 | I | auto | Built-in broker with an imported server identity from another CA: PEM leaf + key, PKCS#12 with the CA; TLS and WSS listeners | "Built-in broker" flow (user → cert → trust → broker → connect); an external client with that CA | The flow connects over TLS and WSS (the profile trusts the imported CA, or pins the served certificate when the import had no CA); the external client with the CA connects; a re-import, Regenerate or a CA rotation makes "Local broker" follow and reconnect (CONN-001; B-12, C-17, `tests/connectivity/extended.py` `app-imported-*`). |

## 3. Charts and datapoints (the variety matrix)

Replayed from `datapoints.json` into the engine (U) and verified on screen (E) for the series `$.v` unless stated.

| ID | Level | Tag | Payload sequence | Expected series |
|---|---|---|---|---|
| D-01 | U | auto | Integers `0, 1, -5, 4294967296, 9007199254740993` | Points as f64; last loses precision, flagged "precision" in the series stats. |
| D-02 | U | auto | Floats `21.5, -0.0, 1e-7, 1.5e300` | Points; `-0.0` plots as 0. |
| D-03 | U | auto | Numeric strings `"21.5"`, `" 21.5 "`, `"21,5"`, `"1e3"`, `"0x1A"`, `"21.5°C"` | `21.5, 21.5, skipped, 1000, skipped, skipped`; skipped counted; cast rule shows "parsed number". `"21,5"` is not parsed (locale ambiguity) and the picker explains it. |
| D-04 | U | auto | Booleans `true,false,true` | `1,0,1`, step style by default. |
| D-05 | U | auto | Boolean-like text `"true","False","T","f","ON","off","Yes","no","1","0"` | `1,0,1,0,1,0,1,0,1,0`; case-insensitive; cast rule "boolean → 0/1". |
| D-06 | U | auto | Mixed over time: `21.5, "22", true, "n/a", null, {}, [1,2]` | `21.5, 22, 1, skipped, skipped, skipped, skipped`; the series stays alive; skipped = 4 with the first reason listed. |
| D-07 | U | auto | Nested path `$.supply.temp` and array index `$.readings[2].v` | Both resolved; a missing index is skipped, not an error. |
| D-08 | U | auto | Field absent in some messages | Gaps, no interpolation across a gap longer than 5× the median interval (line breaks), configurable. |
| D-09 | U | auto | Whole-payload raw number `"23"` on a topic | Series without path; cast "parsed number". |
| D-10 | U | auto | NaN and Infinity as JSON strings `"NaN"`, `"Infinity"` and as bare tokens (invalid JSON) | Skipped; invalid JSON payload shows as text, series untouched. |
| D-11 | U | auto | Timestamps: message with ISO `ts` field vs. receive time | Chart uses receive time by default; a series option "use field as time" is v1.1, not tested. |
| D-12 | U | auto | 5 000-point cap | Oldest dropped; memory estimate within 10 % of the measured size. |
| D-13 | U | auto | Same topic, two series with different paths | Independent casts and stats. |
| D-14 | U | auto | Messages arriving out of order (older timestamp after newer, via replay with fake clock) | Appended in receive order; no sorting (documented). |
| D-15 | U | auto | Very large payload (2 MB JSON) with the field at the end | Field extracted; render limit respected in the UI. |
| D-16 | E | auto | Add-to-chart picker on `ahu01/state` sample | Lists `supply.temp, supply.fan, setpoint, alarm.active` checkable; `mode, ts` greyed "text, not plottable"; "Other path" accepts `$.supply.pressure`. |
| D-17 | E | auto | Right-click a key in Payload and in Diff; hover icon | "Add <field> to chart" opens the picker preselected; text field item disabled. |
| D-18 | E | auto | Series editor: click a key in the last message | Path written into the field; greyed fields not clickable. |
| D-19 | E | auto | Two axes, step vs line, unit labels, hover readout at a shared time | Readout lists every series with its unit; boolean shows "1 (true)". |
| D-20 | E | auto | Pause and resume | No points lost while paused; view frozen. |
| D-21 | E | auto | Export chart CSV and PNG | CSV has `time` plus one column per series, empty cells for gaps; PNG renders in the current theme. |
| D-22 | E | auto | Sparkline in tree and table equals the last 10 cast values of the chart series | Pixel comparison of the sparkline path against the chart's last 10 points. |
| D-24 | E | auto | Charts screen: series edited in place without an edit mode (switch, colour, name, field and cast, style, axis, unit, Remove); the chart title renamed in place with Enter and with focus loss; a renamed chart keeps its series | As specified. |
| D-23 | I | nightly, P | Subscribe to a public broker's `$SYS/#` where offered (test.mosquitto.org) and chart `$SYS/broker/load/messages/received/1min` | Parsed number series grows over 2 minutes. |

## 4. Explorer: tree, table, inspection

| ID | Level | Tag | Test | Expected |
|---|---|---|---|---|
| X-01 | U | auto | Topic store with 100 000 topics from `tree-100k.json`: insert, lookup, slice rows a..b, search substring/wildcard/regex. | Slice of 60 rows under 2 ms; search under 50 ms. |
| X-02 | U | auto | Per-topic ring buffer at 10, 200 and 10 000 | Oldest dropped; count since connect keeps growing beyond the buffer. |
| X-03 | U | auto | Count since connect per node = own + subtree; resets on reconnect and Clear history | Exact totals on a known replay. |
| X-04 | E | auto | New-message highlight on the row and all collapsed ancestors; reduced motion shows the dot only | Class present for 1.2 s, then removed. |
| X-05 | E | auto | Collapsed by default; "Expand to depth 2"; state saved with the view | Reopen the view: same expansion. |
| X-06 | E | auto | Table view columns: sort by each column, hide/show, reorder, export visible set | Sorted correctly incl. numeric "Since connect"; export matches rows shown. |
| X-07 | E | auto | Pinned chips and saved views per profile | Survive restart. |
| X-08 | E | auto | Follow mode and pause with missed-messages counter | Counter correct. |
| X-09 | E | auto | Quick publish row: send `21.5` QoS 1 retained to the selected topic | Message arrives; sent-log entry; retained flag set. |
| X-10 | E | auto | Context menu: copy topic/payload/CLI; delete retained (single and subtree with preview) | CLI command reproduces the message when run; subtree preview lists exactly the retained topics. |
| X-11 | U/E | auto | Diff: added, removed, changed keys; nested; arrays; baseline from history | Highlights correct; line diff for text. |
| X-12 | U | auto | Decoders: JSON (incl. BOM, comments rejected), UTF-8 text, binary → hex, base64, image/png preview, CBOR (`cbor.json`), MessagePack, Sparkplug B NBIRTH/NDATA with metrics and aliases, Protobuf with a user `.proto` incl. imports and a wrong message type | Each decodes or reports "Cannot decode as Protobuf plant.Telemetry: field 3 wire type mismatch" style errors with raw bytes shown. |
| X-13 | E | auto | Properties tab: negotiated version first; MQTT 5 properties; 3.1.1 shows no MQTT 5 block | As specified. |
| X-15 | E | auto | Tree row: name, payload on the available width with the R marker, then sparkline, messages, throughput, subtopics at the end; the three figures carry their tooltips; no age text; the row never wraps in EN, FR, DE | As specified. |
| X-16 | E | auto | Sparkline rule: a bare number payload draws the sparkline by itself; a JSON payload draws nothing until "Use as sparkline" is chosen on a value; picking a second value replaces the first; the pick survives a profile reload; a text value shows the item disabled | As specified. |
| X-17 | E | auto | Payload click menu: a click on a key or value opens "Use as sparkline", "Add … to chart…", "Copy path", "Copy value"; no hover icon and no trailing menu in the JSON; the context menu on the topic is unchanged | As specified. |
| X-18 | E | auto | Detail tabs are Payload and Properties only; "Show diff to previous" is on by default, highlights changes against the previous message with the baseline time shown, off shows the plain payload; selecting a History row shows that message and diffs it against its predecessor | As specified. |
| X-19 | E | auto | Topic chart under the payload: empty until a field is added; "+ Add field…" opens the picker with this topic's fields checked; adding and removing chips updates the chart; the fields survive a profile reload; "Also add to" sends the checked fields to a dashboard chart or a new chart | As specified. |
| X-20 | E | auto | History section replaces Recent: every buffered message listed, selectable, buffer size shown and raisable for this topic | As specified. |
| X-21 | E | auto | Detail header: star button toggles Pin (☆ / ★, tooltips), the Publish icon button opens the composer on the topic, ⋯ holds the rest; the quick publish row is a boxed strip with separators | As specified. |
| X-23 | E | auto | Empty connection bar: no text and no button while nothing is open; the strip keeps its height | As specified. |
| X-24 | U+E | auto | Smoothed rates: the tab figure and the tree throughput are one-second averages refreshed once a second, written "n msg/s"; with 50 messages in one second the figure reads 50 msg/s and does not change between the one-second ticks | Unit test of the averaging, e2e on the rendered text. |
| X-25 | E | auto | Sparkline right-click: "Hide sparkline" removes it for that topic and saves `sparklineOff`; the topic context menu shows "Show sparkline" and brings it back; a new "Use as sparkline" pick brings it back too | As specified. |
| X-26 | E | auto | History: each row expands with its chevron to show the full payload in the current JSON or Tree form; "Show <field> in History" from the click menu, or the filter button, puts the field's path in the column header and its value per row, with the chip next to the title; the same entry, the chip's ✕ or the button cancels it | As specified. |
| X-27 | E | auto | JSON / Tree segment: Tree shows folding key rows with coloured scalar values and the diff colours; the click menu works on a tree row; the choice applies to expanded History rows and is kept per connection while the app runs | As specified. |
| X-28 | E | auto | Tree row columns: the sparkline, messages, rate and subtopics of every row share the same four right-aligned columns; rows without a sparkline or subtopics leave the column empty; checked by comparing the x positions across rows in EN, FR, DE | As specified. |
| X-22 | E | auto | Connection bar always shown inside the main column, right of the rail; the rail reaches the title bar; with nothing open the strip reads "Choose a connection" with a + that opens the New connection menu; screenshots light and dark, EN, FR, DE | As specified. |
| X-14 | E | auto | Payload above 256 KB switches to the paged raw view; "format anyway" works | No freeze longer than 100 ms. |

## 5. Publish

| ID | Level | Tag | Test | Expected |
|---|---|---|---|---|
| P-01 | I | auto | Send QoS 0, 1, 2 with and without retain; sent log status `sent`, `acked`; PUBACK/PUBCOMP tracked | Statuses correct within 1 s. |
| P-02 | I | auto | Publish to a topic the user is not authorised for (embedded broker user with no ACL for it is v1.1; use Mosquitto ACL when available, else a harness broker returning reason 0x87) | Sent-log row `failed · not authorized`, reason code shown, MQTT 5 reason string shown when sent. |
| P-03 | I | auto | Delete retained: confirm dialog shows current value; after confirm, a fresh subscriber receives nothing | Verified with a second client. |
| P-04 | I | auto | Repeat every 1 s, count 10; disconnect mid-way | Exactly the sent messages logged; repeat stops on disconnect with a notice. |
| P-05 | I | auto | Request and response on MQTT 5: harness responder echoes on the response topic with the correlation data | Reply shown with round-trip time; a reply with a wrong correlation ID is ignored and logged. |
| P-06 | E | auto | Request and response section absent on a 3.1.1 connection, replaced by the one-line note | As specified. |
| P-07 | E | auto | Sending never creates a template; "Save as template" from composer and from a sent-log row does; placeholders `{room}` prompt on send | As specified. |
| P-08 | I | auto | Payload formats: JSON validation errors shown with line/column; hex with spaces and `0x`; base64 with and without padding | Bytes on the wire match. |
| P-09 | I | auto | Payload larger than the broker's max packet size (MQTT 5 CONNACK property) | Refused before sending: "This broker accepts at most 1 MB per message." |
| P-10 | I | nightly, P | Publish and receive on each public broker under the test prefix; clean up retained | No retained message left (verified by a fresh subscriber). |
| P-11 | E | auto | "Send ⌘↵" is in the composer header top right; "Send and clear", repeat and Delete retained stay at the bottom | As specified. |

## 6. Embedded broker

| ID | Level | Tag | Test | Expected |
|---|---|---|---|---|
| B-01 | I | auto | Start with defaults: TCP 1883, TLS 8883, WS 8083 on all interfaces, WSS off; local CA and server cert generated with the machine's names | All listeners accept; cert SANs correct. |
| B-02 | I | auto | Add listener flow: each of the four setups creates the right listener, cert and auth combination | Mutual TLS turns on "require client certificate" for that listener only. |
| B-03 | I | auto | Port validation: 80, 1023, 1024, 65535, 65536, a port in use | Refused with the exact messages; in-use reported before start. |
| B-04 | I | auto | Network vs Localhost switch per listener | Bound address verified by connecting from 127.0.0.1 and from the LAN address of the machine. |
| B-05 | I | auto | Users: add, change password, remove; argon2 at rest; login with right and wrong password on TCP and TLS | CONNACK 0x05 (3.1.1) / 0x86 (5) on wrong password, with the broker log line. |
| B-06 | I | auto | Shared users are accepted on every listener (TCP, TLS, WS, WSS); no per-listener login exists in the config or the UI | As specified. |
| B-07 | I | auto | Anonymous on: users, logins and client certs ignored; mTLS listener accepts a client without a certificate; pill shows "anonymous"; Anonymous off restores | As specified. |
| B-08 | I | auto | Issue client cert, export PEM and PKCS#12, connect with each; revoke; the revoked one is refused at the next handshake | As specified. |
| B-09 | I | auto | Import CSR, sign it, connect with the device-held key | As specified. |
| B-10 | I | auto | Rotate CA: existing client certs refused after rotation with a clear log line; the next "Built-in broker" connection re-issues the desktop cert | As specified. |
| B-11 | I | auto | Retained messages count; disconnect a client from the Clients table; last-will delivery on an abrupt disconnect | As specified. |
| B-12 | I | auto | "Built-in broker" > "Use an existing listener" preselects the most secure enabled listener, creates the user "mqtt-panel" once (kept on later runs), issues "mqttpanel-desktop" only when required, starts the broker only when it is stopped, connects, and updates the single profile "Local broker" | No duplicate profiles or users after 3 runs; connected within 5 s with the broker stopped beforehand. |
| B-13 | I | auto | CLI `mqttpanel broker` with the desktop config and with `--config` file; `--bind 127.0.0.1` | Same behaviour as the app. |
| B-14 | I | nightly | 1 000 clients (harness) connected, 5 000 msg/s fan-in, 20 000 msg/s fan-out for 60 s | No drops at QoS 1; memory stable. |
| B-15 | I | auto | Live apply, with one client on TCP 1883 (password), one on TLS 8883 (password) and one on TLS 8883 (client certificate): add a WS listener (nobody drops; it accepts); change the TLS port to 8884 (only the two TLS clients drop; they reconnect on 8884); switch reachability both ways (nobody drops; new binds take effect); regenerate the server certificate (nobody drops; a new TLS handshake sees the new certificate); rotate the CA (only the client-certificate session drops); Optional → Required (only the password session on TLS drops); Required → Optional (nobody); revoke the certificate (its session drops at once); remove the user (its sessions drop); Anonymous on then off with an anonymous client (only that one drops on off); add a listener on a taken port (its row shows the error, the others keep serving) | Exactly the listed sessions drop and no other; the broker never stops; one log line per applied change. |
| B-26 | E | auto | Client certificate rows: one ↓ download button per row that opens a menu "PEM" / "PKCS#12…"; no PEM or PKCS#12 pills; PEM exports at once, PKCS#12… opens the passphrase dialog as before | As specified. |
| B-27 | E | auto | Start is the accent primary button, Stop outlined in brick; the Status tile shows a round play button while stopped and a round stop button while running, each doing what the header button does | As specified. |
| B-28 | I | auto | Server certificate names: generated and regenerated certificates carry localhost, 127.0.0.1, ::1, the host name, its .local name and every non-loopback, non-link-local interface address; "Extra names" typed in the Regenerate dialog are saved and present after a regeneration and after a heal; a client connecting by a LAN address verifies the certificate with CA.pem | As specified. |
| B-29 | E | auto | "Export…" on the Server certificate line saves the served chain as PEM; the file parses and its leaf equals the served certificate | As specified. |
| B-25 | E | auto | No restart in the UI: no Restart button, no prompt, no "Later", no pending chip, no pulsing button; the header holds Start and Stop; a listener that cannot start shows its error on its row; the status has no `restartNeeded` | As specified. |
| B-16 | E | auto | Users table: eye reveals the password, copy puts it on the clipboard, in every password and passphrase field the app owns | As specified. |
| B-18 | E | auto | Client certificate requirement is one broker-wide control under Client certificates (Optional / Required, with "Verify against"); Required shows the device explanation; every encrypted listener row shows the state read-only; Mutual TLS preset sets Required; plain listeners unaffected | As specified. |
| B-19 | E | auto | Clients and log update live without a Refresh button; the log opens collapsed as a title strip at the bottom of the right pane and expands on click; state remembered | As specified. |
| B-20 | M | auto | Broker start with the server key missing from the secret store: a new server certificate is issued from the local CA, logged, TLS listener up; CA key missing too: CA regenerated, logged; secret store refusing: start fails with the reason, no TLS listener without material | Crate test. |
| B-21 | E | auto | Reachability is broker-wide: "This computer only / Whole network" on the Listeners line with its ⓘ; no per-listener switch, no permanent strip; switching applies at once, nothing prompts; the runtime binds every listener accordingly; an old per-listener broker.json loads (network wins) | As specified. |
| B-22 | E | auto | Listener table: columns On, Kind, Port, Setup, ✕ only; ✕ removes the listener at once and drops only its sessions, nothing prompts; plain listeners show "unencrypted" only while Whole network is chosen | As specified. |
| B-23 | E | auto | Users and Client certificates are boxed panels with their actions in the header; the "Advanced security" fold is closed at start and holds the client-certificate requirement, Verify against and the broker certificates; Add listener step 2 has kind and port only | As specified. |
| B-24 | E | auto | Verbosity: no "live" anywhere on the Broker screen; the Clients header has no count; the log strip reads "▸ Log" and adds "· n warnings" only when n > 0; the anonymous strip is one sentence | As specified. |
| B-30 | I | auto | Depth cap (`crates/broker/tests/security.rs`): a SUBSCRIBE with a 60 000-level filter, then a disconnect; the same on 3.1.1; a PUBLISH on 129 levels; a last will on 200 levels; a round trip on 128 levels | MQTT 5 SUBACK 0x8F and the session goes on; 3.1.1 connection closed; DISCONNECT 0x90; CONNACK 0x90; the 128-level message arrives; the broker keeps serving. |
| B-31 | I | auto | Control orders with a full in-flight window: a 3.1.1 client keeps 1 024 QoS 1 deliveries unacknowledged and is taken over by the same client ID; MQTT 5 clients with Receive Maximum 1 and an unacknowledged delivery are dropped by removing their user, revoking their certificate and "Disconnect"; a client never acknowledging (resend 300 ms, close 900 ms in the test) | Each old connection is closed within 1 s; the unacknowledged delivery is sent once more with DUP, then DISCONNECT 0x98 and the log line. |
| B-32 | I | auto | Limits before login: 8 idle connections from one address, then a 9th and 10th; 3 idle with a per-listener cap of 3, then a 4th; a CONNECT of 70 000 bytes and one of 60 000; a wrong password on 3.1.1 and 5; 12 parallel argon2 logins | 9th, 10th and 4th closed at once with one warning; 70 000 refused, 60 000 accepted and a 100 KiB PUBLISH after login goes through; refusal after ≥ 1 s then closed, a right password not delayed; never more than 4 checks at once. |
| B-17 | M | manual | Test lab: `tools/testlab/mosquitto_broker.py` in all five modes, app connects with the printed values and files; `tools/testlab/load_client.py --check` against each listener kind of the app's broker with exported CA, PEM and PKCS#12; 500 msg/s for 60 s with charts and export | Round trips OK; app shows about 500 msg/s with no backlog. |

## 7. Error messages against external brokers

Each case asserts the exact user-facing sentence (EN) and that FR and DE exist in the catalog.

| ID | Level | Tag | Situation (how it is produced) | Message must say |
|---|---|---|---|---|
| E-01 | I | auto | DNS failure (`no-such-host.invalid`) | "Cannot find the host no-such-host.invalid. Check the name or your network." |
| E-02 | I | auto | Connection refused (closed port on localhost) | "Nothing is listening on localhost:1884. Is the broker running on that port?" |
| E-03 | I | auto | Timeout (non-routable 10.255.255.1) | "No answer from 10.255.255.1:1883 after 10 s. A firewall may be blocking the port." |
| E-04 | I | auto | TLS on a plain port (connect mqtts to 1883) | "localhost:1883 did not start TLS. Try mqtt:// instead of mqtts://, or the broker's TLS port." |
| E-05 | I | auto | Plain on a TLS port | "localhost:8883 expects TLS. Try mqtts://." |
| E-06 | I | auto | Bad username/password, 3.1.1 and 5 | "The broker refused the login for user commissioning (bad username or password)." with the reason code. |
| E-07 | I | auto | Not authorised (CONNACK 0x87) | "The broker does not allow this client to connect (not authorized)." |
| E-08 | I | auto | Client ID rejected (0x02 / 0x85) | Names the client ID and suggests "Generate". |
| E-09 | I | auto | Server unavailable (0x03 / 0x88) | "The broker is up but not accepting connections right now." |
| E-10 | I | auto | Protocol version refused (0x01 / 0x84) when forcing MQTT 5 on a 3.1.1 broker | "This broker does not speak MQTT 5. Set the version to 3.1.1 or auto." |
| E-11 | I | auto | Keep-alive timeout (harness broker stops answering PINGREQ) | "The broker stopped answering; reconnecting…" then the reconnect counter. |
| E-12 | I | auto | Broker closes the connection with a DISCONNECT reason (MQTT 5, e.g. 0x8E session taken over) | The reason string: "Disconnected by the broker: session taken over by another client with the same ID." |
| E-13 | I | auto | Subscribe refused (SUBACK 0x80 / 0x87) | Chip turns brick with "Subscription to building/# was refused (not authorized)." |
| E-14 | I | auto | Packet too large (0x95) | Message P-09. |
| E-15 | I | auto | WebSocket handshake failure (HTTP 404 / 403 / wrong subprotocol) | The HTTP status and the hint about the path. |
| E-16 | I | nightly, P | Each public broker with a deliberately wrong port and a wrong scheme | E-02/E-04/E-05 style messages, never a raw library error string. |
| E-17 | U | auto | Every error path has a catalog entry; no raw `Error: …` leaks to the UI | Grep test over the error enum vs catalog keys. |
| E-18 | E | auto | CLI exit codes for E-01, E-02, E-06, E-07 | 1, 1, 3, 3 with the same sentences. |

## 8. Performance and memory

| ID | Level | Tag | Test | Threshold |
|---|---|---|---|---|
| Q-01 | I | nightly | 100 000 topics loaded, tree and table scrolling (automated scroll of 5 000 rows) | Frame time p95 < 16 ms on the CI baseline machine; measured per OS. |
| Q-02 | I | nightly | 5 000 msg/s for 10 min from the harness on 2 000 topics | No UI stall > 100 ms; engine queue never above 1 s of backlog; memory within budget. |
| Q-03 | I | nightly | 10 MB payload | Received and shown paged; no freeze > 200 ms. |
| Q-04 | I | auto | Startup to first interactive frame | < 1 s warm, < 2 s cold on the baseline machine. |
| Q-05 | U | auto | Memory estimate vs. real allocation after a known replay | Within 10 %. |

## 9. User interface, native feel, localisation

| ID | Level | Tag | Test | Expected |
|---|---|---|---|---|
| V-01 | E | auto | Visual regression of every mockup screen, light and dark, EN and DE, on macOS, Windows, Linux | Within tolerance; DE (longest) never wraps a form row, never clips a button. |
| V-02 | E | auto | Automated overflow check: every label, button, chip and helper text has `scrollWidth <= clientWidth` and a single line box | Zero violations in any language. |
| V-03 | M | manual | Native-feel checklist (SPEC §1.2) walked through per OS: menu bar, shortcuts (⌘/Ctrl), file dialogs, window chrome, scrollbars, theme switch live, focus rings, no text cursor on chrome | Every item ticked. |
| V-04 | E | auto | Command palette: lists commands, topics, connections; keyboard only | Every command reachable. |
| V-05 | E | auto | Language switch at runtime EN → FR → DE; French typography (narrow no-break space before `:` `;` `?` `!`), German "du" | Catalog checks plus screenshots. |
| V-06 | M | manual | Native review of FR and DE by a native reader, logged in `docs/COPY_REVIEW.md` | No literal translations left. |
| V-08 | M | manual | macOS: the app name sits on the centre line of the traffic lights; nothing under the lights; Windows and Linux unchanged | By eye on each OS. |
| V-09 | M | manual | macOS unsigned build: no keychain dialog at start-up, at the first secret use, at a broker start or after a rebuild; the master key sits in `master.key` (0600) in the app data folder; a build that still finds the login-keychain item reads it once (one last dialog), writes the file and deletes the item, so the next build family never asks; Deny on that last dialog keeps `keychain.refused` for the session and the move is tried again at the next start | As specified. |
| V-12 | M | manual | **Release gate.** Signed and notarized macOS build on a fresh user account: no keychain dialog at install, first start, first secret use, after an update and from the CLI; Keychain Access shows no "MQTT Panel" item in the login keychain (the key is in the Data Protection keychain). Windows: DPAPI, no dialog. Linux with GNOME or KDE: Secret Service, no dialog; without a keyring: the file fallback, no dialog | As specified (change set 10); failing this blocks the release. |
| V-10 | E | auto | Title bar holds the name and the mark only; the Broker rail icon shows the play badge while the broker runs (clay while starting, nothing when stopped); the Connect icon shows the count bubble with the number of open connections and nothing at zero; "unverified" chip on the tab and card of a connection with verification skipped | Screenshots light and dark. |
| V-11 | E | auto | No auto-capitalisation: typing "myhost" into the host field, a topic or a client ID keeps the lower case; every input and textarea declares autocapitalize, autocorrect and spellcheck off | DOM check over all inputs plus a typed value. |
| V-07 | E | auto | Accessibility: tab order through every screen, screen-reader labels on icon buttons, contrast AA in both themes, reduced motion | axe-core clean; manual VoiceOver/NVDA spot check before release. |

## 10. CLI

| ID | Level | Tag | Test | Expected |
|---|---|---|---|---|
| L-01 | I | auto | `sub -t "#"` against the embedded broker; `--format text|json|ndjson|raw`; `--count`, `--timeout`, `--retained-only` | Output shapes exact; exit 0. |
| L-02 | I | auto | `pub` from `-m`, `--file`, `--stdin` (one message per line); `-r`; `--delete-retained`; `--repeat` stopped by SIGINT | Bytes on the wire exact. |
| L-03 | I | auto | `--profile` uses the app's profile and keychain | Works without re-entering secrets. |
| L-04 | I | auto | TLS flags, `--trust-fingerprint`, `--insecure` | Same outcomes as T-04, T-20. |
| L-05 | I | auto | `--help` on every command; unknown flag | Help text matches SPEC §8; exit 2 with a one-line hint. |
| L-06 | I | nightly, P | `sub`/`pub` against public brokers under the test prefix | As C-11. |

## 11. Export, settings, privacy

| ID | Level | Tag | Test | Expected |
|---|---|---|---|---|
| S-01 | E | auto | Export selected topic, subtree, visible rows; last hour, session, custom; CSV, JSON, NDJSON; MQTT 5 properties on/off; base64 raw | Files parse; row counts match the estimate ± 0; CSV expands numeric JSON fields. |
| S-02 | E | auto | Nothing is written to disk by default: run a session, inspect the app data folder | Only profiles, templates, views, settings and broker config present; no message data. |
| S-03 | E | auto | Clear all history; memory estimate drops | As specified. |
| S-04 | E | auto | Updates page and network calls (the stores update the app): the page shows the one store sentence in EN, FR, DE with no switch, channel or button; network calls observed with a proxy | Only broker traffic; no update check. |
| S-06 | U | auto | Master key store selection: unsigned signature → file store, Developer ID signature → OS store, `MQTTPANEL_KEYSTORE` override wins; the file store creates `master.key` with 0600 and its folder 0700, round-trips the key, refuses a world-readable file with a plain error; the move from a fake keychain source reads once, writes the file, deletes the item, and keeps the old behaviour when the source refuses | As specified. |
| S-05 | E | auto | No app-wide "Skip verification" in Settings; an old settings file with `skipVerify: true` loads and the flag is ignored; an old profile with `verifyServer: false` loads with the skip switch on | As specified. |

## 12. Supply chain and release

| ID | Level | Tag | Test | Expected |
|---|---|---|---|---|
| R-01 | U | auto | `cargo deny` licence allow-list and advisories; `npm audit` | Clean. |
| R-02 | U | auto | `scripts/check-forbidden-words.sh` on the tree and on the built app strings | Clean. |
| R-03 | M | manual | Beta builds (unsigned): install on each OS with the "how to run it" note | Note is accurate. |
| R-04 | M | manual | Store builds: the Mac App Store build passes App Store Connect validation and installs from TestFlight; the MSIX passes the Windows App Certification Kit and Partner Center validation; a store update installs over the previous version | Validation clean; the update keeps profiles and secrets. |

## 13. Assistant

The assistant interface (`docs/MCP.md`). Rust tests in `apps/desktop/src-tauri/src/mcp/tests.rs` use an `rmcp` client over the app's socket against the real engine, client and embedded broker, on a temporary data folder with in-memory secrets.

| ID | Level | Tag | Test | Expected |
|---|---|---|---|---|
| M-01 | E | auto | Over the socket: list the tools, `list_connections` (empty), `create_connection` (name from the host, usual port), `update_connection` (protocol, version, subscriptions), `connect` to the embedded broker through "Local broker", `disconnect`, `delete_connection`; the resources | Every tool of phases 1 and 2 listed; profile fields as asked; connected; a toast per writing call and a log row with the host name; refresh events; the log resource lists the calls. |
| M-02 | E | auto | Writing tool before the acknowledgement; "Not now" while a call waits; Accept while a call waits | Refused with "Not yet allowed: accept the acknowledgement in MQTT Panel, Settings, Assistant.", dialog shown on connect, log row "refused"; reading tools work; Not now refuses at once; Accept lets the waiting call finish and is saved with the date. |
| M-03 | E | auto | `set_connection_password` with the password as an argument, twice, then in a new session | Stored; the warning once per session; the log masks the password (•••) and notes "secret given through the assistant"; no result or log row contains it. |
| M-04 | E | auto | `ask_in_app`: the app answers with a password, cancels, does not answer; `ask_in_app: false` without a value | A request event for the window; stored without warning; "cancelled", "Nobody answered…" after the user limit, "Give password, or set ask_in_app…"; log outcomes ok, cancelled, timeout, failed. |
| M-05 | E | auto | Every tool, both resources, both prompts and the tool list after a password, a passphrase-protected key, a client certificate and a generated key were set | No result contains the password, the passphrase or any private key. |
| M-06 | E | auto | Against the broker's TLS listener: `test_connection` reports the certificate question; `trust_certificate` pin, ca, skip_once (connects at once, nothing saved), skip_always (saved); trust without a pending question | Details and choices in the result; each mode does what the certificate prompt does; the call without a question explains to test or connect first. |
| M-07 | E | auto | A second host connects while one is connected; after the first leaves, a new host | The second gets the JSON-RPC error "Another assistant is connected to MQTT Panel…"; the next host is served and named in the session. |
| M-08 | E | auto | Navigation tools and the app language | Navigate events with the place, the window raised, nothing logged or toasted as a change; texts in French and German after a language switch. |
| M-09 | E | auto | The shim (`crates/cli/tests/shim.rs`, `apps/desktop/src-tauri/tests/shim.rs`): socket present; socket missing (the shim starts the app and waits); the app does not come; the app executable with `mcp` | Bytes forwarded both ways, exit 0 when stdin closes; the app started and the socket used once it appears; exit 1 with a sentence on stderr and nothing on stdout; same forwarding from the app executable. |
| M-10 | U, E | auto | Settings, Assistant and the window side (Vitest `assistant.test.tsx`, Playwright "Assistant", overflow states 17c to 17e): acknowledgement sentence and Accept, "Add to Claude" and "Add to ChatGPT" on the mock's home, snippet copy, log with Clear and Export…, ⓘ per row in EN/FR/DE, acknowledgement dialog, password and file requests, navigation toast | As specified in `docs/MCP.md` section 6; the files keep their other content; no row wraps in EN, FR, DE. Writing the real files is covered by the Rust tests of `mcp/hosts.rs` on a temporary home. |
| M-11 | E | auto | Every broker tool against the in-app broker manager: status, start, add a WS listener (live), change its port, switch it off, remove it, reachability, Anonymous off, a user by `ask_in_app` then direct, remove it, the client certificate rule (an imported CA missing), issue PEM and PKCS#12 into a folder picked in the app, PKCS#12 without a passphrase and without the app, a cancel, list, revoke, export the CA (direct folder, relative path refused) and the server chain (picked), regenerate with extra names, rotate the CA, the log, stop | Files written where picked (certificate, key, CA.pem; the bundle opens with the passphrase typed in the app); nothing issued on cancel; the key, passwords and passphrase in no result; the broker never restarted; toasts and refreshes; reading tools not logged; the direct password masked and noted. |
| M-12 | E | auto | `connect_builtin_broker` with Anonymous off and a mutual-TLS listener; again by kind; with every listener off | Steps user created, certificate issued, CA trusted, broker started, connected; the broker sees the client certificate `mqttpanel-desktop`; the second run keeps everything and there is one "Local broker"; with no listener on, the result says to add one. |
| M-13 | E | auto | Data tools on "Local broker": publish (JSON, retained, invalid JSON and a wildcard refused), list with a filter, get, search, 620 messages in history read with a limit of 5000 and the cursor, default limit, exports (direct CSV, NDJSON picked in the app, relative path and an empty range refused), delete retained (one, subtree, nothing), 250 topics for the bounds, a connection not open | History capped at 500 then the 120 older rows with `nextBefore`, newest first; 200 by default; CSV with a header and 620 rows; list capped at 200 of 250, search at 100; broker retained count goes to 0; "not connected" for the idle connection. |
| M-14 | E | auto | Chart tools: create (the only connected connection), add series (path without "$", unit), the same series again, a second one, update (colour, style, axis; a wrong colour refused), rename, export CSV (direct) and PNG (image from the window, place picked), topic chart (paths and objects, a kept field keeps its settings, empty removes), sparkline (path, off), remove series, delete chart; the new resources and template; the two new prompts; French | The window gets the charts event; CSV `time,t,h` with 3 rows; the PNG written as answered; profile prefs as set; resources readable by id and by name, an unknown connection refused; prompts carry the broker state and the charts; texts in French. |
| M-15 | E | auto | Every broker, data and chart tool, the five resources, both new prompts and the log, after a direct broker password, a direct PKCS#12 passphrase, a PEM issued into a folder and the desktop certificate | No result contains a broker password, the mqtt-panel password, the passphrase, an issued key, the desktop key or the CA key. |
| M-16 | U, E | auto | Window side of the new requests (Vitest `assistant-phase3.test.tsx`, Playwright M-16, overflow state 17f): save place (Choose… cancelled, then chosen), folder, new certificate (PKCS#12 needs folder and passphrase; PEM passphrase optional; no field when the passphrase came directly), broker password (not empty), FR and DE titles | Answers carry only the path and the typed secret; Save stays disabled until ready; no row wraps in EN, FR, DE. |
| M-17 | U, E | auto | Dashboard charts both ways and the chart image (Vitest, Playwright M-17 in Chromium) | A charts event replaces the list and selects a valid chart; window changes go back with `syncCharts`; `show_chart` selects the chart and its connection; `chartPng` shows the chart and answers a PNG (cancelled for an unknown chart), without a dialog. |
| M-18 | E | auto | Rail item AI (Vitest `rail-ai.test.tsx`, Playwright M-18, overflow state 17g): position after Broker, label and aria-label in EN/FR/DE, Enter from the keyboard, click from another screen and from another Settings page | Settings opens on the Assistant page (selected in the list, section in view); the AI item has `aria-current="page"` there and the Settings item does not; on any other Settings page the Settings item is lit and AI is not; on other screens neither; no badge; "AI", « IA », „KI“ on one line. |
| M-19 | E | lab | Claude Code prompt C01, mosquitto over TCP (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): create_connection, connect, list_topics or get_topic, publish | Connected over mqtt; the answer gives 21.5; Paho receives the greeting. |
| M-20 | E | lab | Claude Code prompt C02, mosquitto over WebSocket (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): protocol ws, port, ws path default | Connected over ws on the WebSocket port; 21.5; greeting received. |
| M-21 | E | lab | Claude Code prompt C03, tLS with a CA file (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): protocol mqtts, set_ca_certificate (direct file) | Connected over mqtts with the CA set, nothing pinned; greeting received. |
| M-22 | E | lab | Claude Code prompt C04, tLS with username and password (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): set_connection_password with the password given (direct way, warning) | Connected with the user and the stored password; greeting received. |
| M-23 | E | lab | Claude Code prompt C05, mutual TLS, PEM certificate and key (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): set_client_certificate cert_file and key_file | Connected with the client certificate; greeting received. |
| M-24 | E | lab | Claude Code prompt C06, mutual TLS, PKCS#12 bundle (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): set_client_certificate with a .p12 and its passphrase | Connected with the bundle; greeting received. |
| M-25 | E | lab | Claude Code prompt C07, self-signed broker, pin the certificate (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): test_connection or connect, trust_certificate pin | trust_certificate pin; the lab fingerprint pinned; connected; greeting received. |
| M-26 | E | lab | Claude Code prompt C08, wrong password (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): the login refusal is reported, no blind retries | Not connected; the answer says the broker refused the login (bad username or password); at most three attempts, no password guessing. |
| M-27 | E | lab | Claude Code prompt C09, expired broker certificate (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): the certificate question is explained, nothing trusted or skipped | Not connected; the answer says the certificate expired; no trust_certificate call. |
| M-28 | E | lab | Claude Code prompt C10, host name mismatch (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): the name mismatch is explained, nothing trusted or skipped | Not connected; the answer names other.example; no trust_certificate call. |
| M-29 | E | lab | Claude Code prompt C11a, mQTT 3.1.1 chosen in the prompt (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): mqtt_version 3.1.1 | Profile version 3.1.1; Mosquitto logs the client with p4; greeting received. |
| M-30 | E | lab | Claude Code prompt C11b, mQTT 5 chosen in the prompt (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): mqtt_version 5 | Profile version 5; Mosquitto logs the client with p5; greeting received. |
| M-31 | E | lab | Claude Code prompt C12a, built-in broker, client certificates required (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): broker_set_client_cert_rule or broker_add_listener mtls, connect_builtin_broker, broker_export_ca (folder) | Broker running, rule required, Local broker connected, CA.pem in the folder; an external client verifies the broker with CA.pem and is refused without a client certificate. |
| M-32 | E | lab | Claude Code prompt C12b, built-in broker, user and password over TLS (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): broker_add_user (direct password), broker_export_ca (folder), connect_builtin_broker | User sensor1, CA.pem in the folder; an external Paho client with CA.pem and sensor1 publishes and MQTT Panel receives it. |
| M-33 | E | lab | Claude Code prompt C13, busiest topic and a chart (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): list_topics (rates), get_topic, create_chart and add_series or set_topic_chart | The answer names lab/c13/plant/boiler; list_charts shows a series of its temperature. |
| M-34 | E | lab | Claude Code prompt C14, history and export to a file (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): get_history, export_data with a file path | get_history on the topic; the CSV file exists with a header and rows. |
| M-35 | E | lab | Claude Code prompt C15, before the acknowledgement (`tools/testlab/mcp_prompts/run.py`, prompt in `docs/MCP_PROMPTS.md`): writing tools refused until the user accepts in the app | Writing calls refused; no connection; the answer tells the user to accept the acknowledgement in MQTT Panel, Settings, Assistant. |

---

## Running it

```
# standalone, everything that needs no network
cargo test --workspace
npm test                       # vitest
npm run e2e                    # playwright against the dev build with the mock engine
npm run e2e:app                # tauri-driver smoke on Windows and Linux

# nightly, adds public brokers and performance
MQTTPANEL_PUBLIC_BROKERS=1 cargo test --workspace -- --ignored
MQTTPANEL_PUBLIC_BROKERS=1 npm run e2e:nightly
```

Public-broker tests are `#[ignore]` by default and opt in with the environment variable. They SKIP (with a line in the log) when a broker does not answer within 10 s, and they never leave retained messages or more than 50 messages per run on a public broker.

## Coverage map

Every requirement in `docs/FEATURES.md` is referenced by at least one ID above; `scripts/check-testplan-coverage.sh` (to be written in phase 2) lists FEATURES rows without a test ID.

# MQTT Panel test plan

Version 1, 12 Oct 2026. Companion to `docs/SPEC.md` and `docs/FEATURES.md`. Every test has an ID, a level, and an automation tag. The plan runs **standalone** (nothing but the repo, the embedded broker and generated certificates) and, when the network allows, **against public brokers** too.

Levels: **U** unit (cargo test / vitest), **I** integration (real sockets, embedded broker or a local Mosquitto), **E** end-to-end (the built app driven by Playwright or tauri-driver), **M** manual (walkthrough with a checklist; results recorded in `docs/testlog/<date>.md`).

Automation tags: `auto` runs in CI on every push; `nightly` runs in the nightly job (public brokers, performance); `manual` is executed before each release.

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
| C-06 | I | auto | Reconnect policy: broker restarted while connected. | Pill shows "reconnecting", subscriptions restored, counters continue, no duplicate subscriptions. |
| C-07 | E | auto | "Try connect" success message content. | Latency, server cert subject/issuer/validity, negotiated version, max packet size. |
| C-08 | E | auto | Subscriptions default: new profile has one chip `# · QoS 0`; remove it; warning "No subscriptions…" appears; add `building/#`. | As specified. |
| C-09 | E | manual | File pickers: Browse opens the native dialog; drop a PEM on the field; drop a .txt. | Parsed details shown; the .txt gives "This file is not a certificate". No typed path anywhere. |
| C-10 | E | auto | Import from other tools: a predecessor-tool settings file and an MQTTX export fixture (synthetic, committed under `tests/fixtures/import/`). | Profiles created with host, port, scheme, subscriptions; secrets not imported; a summary lists what was skipped. |
| C-11 | I | nightly | Public brokers plain: connect to each F-05 broker on 1883, subscribe `mqttpanel-test/<id>/#`, publish 3 messages, receive them. | Received within 2 s; SKIP if unreachable. |

## 2. Certificates and TLS (the combination matrix)

Each row is run against the embedded broker (TLS listener configured with the named server cert) and, where marked P, also against public brokers.

| ID | Level | Tag | Server side | Client side | Expected |
|---|---|---|---|---|---|
| T-01 | I | auto | Cert from local root CA | CA file set | Connects; Properties shows chain. |
| T-02 | I | auto | Cert from intermediate CA, server sends chain | Root CA only | Connects (intermediate taken from the handshake). |
| T-03 | I | auto | Cert from intermediate CA, server sends leaf only | Root CA only | Fails with "The broker did not send its intermediate certificate (issuer CN=…). Ask the broker owner to include the chain, or add the intermediate under CA certificate." |
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
| T-20 | I | auto | Any TLS failure | lab-only "Skip verification" on | Connects; pill shows "skip-verify ON"; Properties marks the chain unverified; toggle resets at next launch. |
| T-21 | I | nightly, P | test.mosquitto.org:8883 with its published CA | CA file | Connects. SKIP if the CA download or the broker is unavailable. |
| T-22 | I | nightly, P | test.mosquitto.org:8884 (client cert required) | cert obtained from the broker's own tool, if available | Connects; without a cert, T-11's message. |
| T-23 | I | nightly, P | broker.emqx.io:8883 and 8084 (public CA) | nothing (system roots) | Connects; chain shows a public CA. |
| T-24 | I | nightly, P | A public broker reached via IP with SNI empty | nothing | Name mismatch message T-09, then success after setting SNI. |
| T-25 | U | auto | Certificate parsing: PEM with CRLF, PEM bundle with several certs, DER, PKCS#8 and traditional keys, encrypted keys, garbage. | Each parsed or refused with the specific reason. |
| T-26 | E | auto | Every security field shows its ⓘ bubble with the agreed text in EN, FR, DE. | Text matches the catalog. |

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

## 6. Embedded broker

| ID | Level | Tag | Test | Expected |
|---|---|---|---|---|
| B-01 | I | auto | Start with defaults: TCP 1883, TLS 8883, WS 8083 on all interfaces, WSS off; local CA and server cert generated with the machine's names | All listeners accept; cert SANs correct. |
| B-02 | I | auto | Add listener flow: each of the four setups creates the right listener, cert and auth combination | Mutual TLS turns on "require client certificate" for that listener only. |
| B-03 | I | auto | Port validation: 80, 1023, 1024, 65535, 65536, a port in use | Refused with the exact messages; in-use reported before start. |
| B-04 | I | auto | Network vs Localhost switch per listener | Bound address verified by connecting from 127.0.0.1 and from the LAN address of the machine. |
| B-05 | I | auto | Users: add, change password, remove; argon2 at rest; login with right and wrong password on TCP and TLS | CONNACK 0x05 (3.1.1) / 0x86 (5) on wrong password, with the broker log line. |
| B-06 | I | auto | Per-listener login accepted on its listener only; shared user still accepted (assumption under confirmation) | As specified. |
| B-07 | I | auto | Anonymous on: users, logins and client certs ignored; mTLS listener accepts a client without a certificate; pill shows "anonymous"; Anonymous off restores | As specified. |
| B-08 | I | auto | Issue client cert, export PEM and PKCS#12, connect with each; revoke; the revoked one is refused at the next handshake | As specified. |
| B-09 | I | auto | Import CSR, sign it, connect with the device-held key | As specified. |
| B-10 | I | auto | Rotate CA: existing client certs refused after rotation with a clear log line; "Connect Explorer here" re-issues the desktop cert | As specified. |
| B-11 | I | auto | Retained messages count; disconnect a client from the Clients table; last-will delivery on an abrupt disconnect | As specified. |
| B-12 | I | auto | "Connect Explorer here" picks the most secure enabled listener and reuses the "Local broker" profile | No duplicate profiles after 3 clicks. |
| B-13 | I | auto | CLI `mqttpanel broker` with the desktop config and with `--config` file; `--bind 127.0.0.1` | Same behaviour as the app. |
| B-14 | I | nightly | 1 000 clients (harness) connected, 5 000 msg/s fan-in, 20 000 msg/s fan-out for 60 s | No drops at QoS 1; memory stable. |

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
| E-08 | I | auto | Client ID rejected (0x02 / 0x85) | Names the client ID and suggests Random. |
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
| S-04 | E | auto | Update check on/off: network calls observed with a proxy | Only broker traffic, plus the release feed when enabled. |
| S-05 | E | auto | Skip verification resets at launch | As specified. |

## 12. Supply chain and release

| ID | Level | Tag | Test | Expected |
|---|---|---|---|---|
| R-01 | U | auto | `cargo deny` licence allow-list and advisories; `npm audit` | Clean. |
| R-02 | U | auto | `scripts/check-forbidden-words.sh` on the tree and on the built app strings | Clean. |
| R-03 | M | manual | Beta builds (unsigned): install on each OS with the "how to run it" note | Note is accurate. |
| R-04 | M | manual | Release build: Windows signed, macOS notarized; auto-update from the previous version | Signatures valid; update applies. |

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

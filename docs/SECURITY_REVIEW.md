# Security review

State of the security review of MQTT Panel 1.0, kept current. The full review was made on 25 October 2026 on the repository at tag v1.0.0 by two independent read-only passes (an automated code review and a reviewer with a checklist who also ran dependency checks and proof-of-concept scripts against a scratch broker on loopback). Findings are listed with their state; fixed items keep their test ids so they stay covered.

Reporting a vulnerability: open a private security advisory on GitHub (Security, Report a vulnerability) rather than a public issue.

## Scope

- Embedded broker: packet parsing, authentication, TLS listeners, client certificates and revocation, WebSocket, live apply.
- Client: TLS verification, trust prompt, pinning, skip modes, SNI and ALPN.
- TLS kit: PEM, DER, PKCS#12 parsing, key generation, CSR.
- Profiles and secrets: encrypted secrets file, master key store, host configuration writers.
- MCP server and shim: socket, acknowledgement, tool arguments and results, paths.
- Desktop app: capabilities, CSP, IPC, dialogs, rendering of untrusted payloads.
- CLI, website worker, test lab scripts, supply chain (`cargo deny`, `npm audit`).

## Fixed (change set after 1.0.0; tests B-30 to B-34, Q-06, M-36 to M-43)

| Id | Severity | Area | Finding | Where | Fix |
|---|---|---|---|---|---|
| SEC-01 | high | Broker | Deep topic filter crashes the whole process (stack overflow in the subscription trie) | `crates/broker/src/runtime/router.rs` | Topic and filter depth capped at 128 levels, refused with the proper reason code; iterative unsubscribe. |
| SEC-02 | high | Client engine | One PUBLISH with a deep topic costs gigabytes of memory in the app (quadratic path storage) | `crates/engine/src/engine.rs` | Topic path segments stored once; memory linear in topic length; stored topic length capped per profile (1024 bytes by default). |
| SEC-03 | high | Broker | Kick, takeover and revocation are ignored while a client's in-flight window is full | `crates/broker/src/runtime/session.rs` | Control orders travel on their own channel the session always polls; unacknowledged deliveries are resent once and the connection closes when the window stays full. |
| SEC-04 | medium | Broker | Unauthenticated resource exhaustion: no pre-auth connection cap, 16 MiB CONNECT buffering, unthrottled argon2 | `crates/broker/src/runtime/session.rs` | Pending connections capped per listener and per address, 64 KiB CONNECT limit before login, 1 s delay on a refused login, at most 4 parallel password checks. |
| SEC-05 | medium | MCP server | Assistant tools can silently weaken security and exfiltrate stored credentials after a one-time acknowledgement | `apps/desktop/src-tauri/src/mcp/server.rs` | In-app confirmation for the assistant tools that weaken security, send a stored credential elsewhere or overwrite files. |
| SEC-06 | medium | Broker | WebSocket listeners accept any Origin (cross-site WebSocket access) | `crates/broker/src/runtime/ws.rs` | Browser origins are accepted on listeners with a login; with Anonymous on, an unknown origin is allowed only after the user accepts it in the app; the list is kept per listener. |
| SEC-07 | medium | MCP shim (Windows) | Predictable named pipe lets another local user impersonate MQTT Panel to the assistant host | `crates/shim/src/lib.rs` | Windows named pipe created with an owner-only security descriptor; the shim verifies the server's user (not yet run on Windows). |
| SEC-08 | low | Broker | Last-will topic is not validated (wildcards, empty topic) | `crates/broker/src/runtime/codec.rs` | Last-will topic validated (no wildcards, depth cap). |

## Open, low severity (to review before 1.1)

| Id | Area | Finding | Where | Recommendation |
|---|---|---|---|---|
| SEC-09 | Broker | No caps on sessions, retained messages, subscriptions; MQTT 3.1.1 persistent sessions never expire | `crates/broker/src/runtime/state.rs` | Add limits (sessions, retained count/bytes, subscriptions per client, offline bytes per session) and a default expiry for 3.1.1 persistent sessions; consider binding session takeover to the same login. |
| SEC-10 | Broker | Certificate-only sessions survive switching 'require client certificate' off | `crates/broker/src/runtime/auth.rs` | Pass require_client_cert into login_still_valid and treat Login::Certificate as invalid when it is off (unless Anonymous). |
| SEC-11 | Broker | User-name enumeration by timing | `crates/broker/src/runtime/auth.rs` | Verify against a fixed dummy argon2 hash when the user is unknown. |
| SEC-12 | Profiles | Profile export: profiles not bound to the encrypted secrets; KDF errors ignored; KDF parameters not stored | `crates/profiles/src/store.rs` | Bind the serialized profiles (or a hash of them) as AEAD associated data, propagate KDF/encryption errors, store m/t/p in the file. |
| SEC-13 | Profiles | MQTTPANEL_KEYSTORE can move the master key out of the keychain into a file | `crates/profiles/src/keystore.rs` | Ignore the override in signed release builds (or require a debug build/flag), and never migrate keychain -> file automatically. |
| SEC-14 | Desktop app | Exported private keys and certificate bundles are written with default permissions | `apps/desktop/src-tauri/src/files.rs` | Create key and PKCS#12 files with mode 0600 on Unix (OpenOptions + mode), and encourage a passphrase. |
| SEC-15 | MCP host config | Host configuration files rewritten through a temp file with default permissions | `apps/desktop/src-tauri/src/mcp/hosts.rs` | Create the temp file with 0600 (or the target's mode) from the start; re-read and compare before rename. |
| SEC-16 | CLI | Passphrases accepted as command-line arguments | `crates/cli/src/args.rs` | Prompt without echo when the value is omitted and accept an environment variable or file descriptor instead of argv; warn when given on argv. |
| SEC-17 | Client | 'Skip verification once' also covers every automatic reconnect | `crates/client/src/session.rs` | Clear skip_verify after the first successful handshake, or pin the fingerprint seen in the skipped handshake for the reconnects of that session. |
| SEC-18 | Client | Server name falls back to 'localhost' when the host is not a valid name | `crates/client/src/transport.rs` | Fail with a clear error instead of substituting a name. |
| SEC-19 | TLS kit | Untrusted key/PKCS#12 files control KDF cost | `crates/tls/src/parse.rs` | Check iteration counts and scrypt parameters against sane maxima before decrypting; run inspection off the UI thread with a timeout. |
| SEC-20 | TLS kit | Local CA is unconstrained | `crates/tls/src/kit.rs` | Set pathLen 0 and, where possible, name constraints (localhost, .local, the chosen LAN names); advise trusting it only on test devices. |
| SEC-21 | Client | Client accepts packets up to 256 MB from the broker | `crates/client/src/session.rs` | Advertise and enforce a configurable maximum (for example 16 MiB) and announce it in CONNECT. |
| SEC-22 | Supply chain | No CI workflow in the repository | `.github` | Add a workflow running cargo deny check, npm audit --omit=dev, cargo test, clippy and scripts/check-forbidden-words.sh on every push and on a schedule. |
| SEC-23 | Supply chain | npm audit: undici high advisories (dev-only, via jsdom) | `package-lock.json` | npm audit fix (or bump jsdom) and keep CI auditing with --omit=dev for the shipped tree. |
| SEC-24 | Desktop app | Test hooks active in release builds | `crates/cli/src/conn.rs` | Compile these hooks only under cfg(debug_assertions) or a test feature. |
| SEC-25 | Test lab | Lab writes generated passwords in clear and offers an insecure TLS mode | `tools/testlab/mosquitto_broker.py` | Keep as is for the lab; add a header warning in README and purge results/ before sharing machines. |

## Good practices confirmed

- Worker headers are strict; preview Basic-auth path still present (`website/worker.js`).
- Webview hardening is good (`apps/desktop/src-tauri/tauri.conf.json`).
- Secrets file and master key handling are sound (`crates/profiles/src/secrets_file.rs`).
- TLS verification and pinning are implemented correctly (`crates/client/src/tlsconf.rs`).
- TLS listener and argon2 use (`crates/broker/src/runtime/tls.rs`).
- Socket and secret masking (`apps/desktop/src-tauri/src/mcp/server.rs`).
- cargo deny passes; lockfiles committed; no secrets in history (`deny.toml`).

## Checks run

- cargo deny check (advisories/bans/licences/sources ok)
- npm audit (1 high, dev-only undici)
- cargo clippy --workspace --all-targets (72 style warnings, none security-relevant)
- cargo test -p mqttpanel-broker -p mqttpanel-tls -p mqttpanel-client (all pass)
- local loopback PoCs against the debug CLI broker (deep filter crash, in-flight kick bypass, wildcard will)
- scratch PoC linking mqttpanel-engine (memory per deep topic)
- git history scan for key files

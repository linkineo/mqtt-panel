# Feature specification

Source legend: **Requested** by the owner, **Parity** with the predecessor tool, **Proposed** by the spec. Release is v1.0 unless marked.

## Scope table

| # | Feature | Source | Release |
|---|---|---|---|
| C1 | Connection profiles: create, duplicate, edit, delete, reorder, search | Parity | 1.0 |
| C2 | Transports: mqtt, mqtts, ws, wss; MQTT 3.1.1 and 5 with auto-negotiation; the negotiated version is shown in the title bar pill and in the Properties tab | Parity + Proposed | 1.0 |
| C3 | TLS: CA, client cert and key (PEM, PKCS#12), self-signed and private-CA brokers accepted by trusting the certificate fingerprint per profile, lab-only skip-verification toggle, SNI, ALPN; file pickers and plain-word info bubbles for every security field | Requested | 1.0 |
| C4 | Credentials in the OS keychain; profile export without secrets or with passphrase | Proposed | 1.0 |
| C5 | Per-profile subscriptions with QoS; the default is everything ("#"), written as such; each filter chip has an X to remove it. Last will, keep-alive, clean start, session expiry, reconnect policy | Parity | 1.0 |
| C6 | Connection test ("Try connect") from the form with a readable error explanation (TLS, auth, DNS) | Proposed | 1.0 |
| C7 | Multiple connections open at once, switch in the title bar | Proposed | 1.0 |
| E1 | Topic tree with child counts, inline last value, activity indicator, per-node sparkline, new-message highlight on the row and its collapsed ancestors, and a discreet message count since connection per node | Parity + Proposed | 1.0 |
| E2 | Table view: flat list of topics with sortable columns (topic, value, value trend as a sparkline of the last 10 cast values, updated, rate, messages since connect, size, QoS, retained) | Requested | 1.0 |
| E3 | Search and filter: substring, MQTT wildcard, regex; filter by retained, by activity, by payload content | Parity + Proposed | 1.0 |
| E4 | Pinned topics bar and saved views (filters, layout, pinned set) per profile | Requested | 1.0 |
| E5 | Expand policy: nodes collapse by default, "expand changed", "expand to depth n", remember state per view | Proposed | 1.0 |
| E6 | Follow mode: selected topic stays in view and the detail pane refreshes live; pause and resume | Proposed | 1.0 |
| E7 | Copy topic, copy payload, copy as CLI command, copy as curl-style snippet | Parity + Proposed | 1.0 |
| E8 | Delete retained messages: single topic and recursive subtree with preview of affected topics and confirmation | Parity | 1.0 |
| I1 | Payload views: formatted JSON, raw text, hex dump, base64, image preview for image content types | Parity + Proposed | 1.0 |
| I2 | Decoders: Sparkplug B, CBOR, MessagePack; Protobuf with user .proto files loaded per profile and assigned to topic filters | Parity (Sparkplug, Protobuf) | 1.0 |
| I3 | Diff of current against previous message, and against any message in the per-topic history | Parity | 1.0 |
| I4 | MQTT 5 properties shown for each message (content type, response topic, correlation data, user properties, expiry) | Proposed | 1.0 |
| I5 | Per-topic message history list (time, size, QoS, retained) with the ring buffer size per setting | Parity | 1.0 |
| P1 | Standalone Publish area in the navigation, independent of the tree | Requested | 1.0 |
| P2 | Composer: topic with autocomplete from known topics, payload editor with format (JSON, text, hex, base64), QoS 0/1/2, retain, MQTT 5 properties | Requested | 1.0 |
| P3 | Sent log: every message sent, with status (acked for QoS 1 and 2), re-send and edit-and-resend; kept for the session, exportable | Requested | 1.0 |
| P4 | Delete retained from the composer (sends an empty retained message) with confirmation | Requested | 1.0 |
| P5 | Templates per profile, created only on "Save as template" (sending never creates one): name, folder, duplicate; "Publish to this topic" from any topic prefills the composer | Proposed | 1.0 |
| P6 | Repeat publish every n seconds (for simulation), with a visible stop control | Proposed | 1.0 |
| P7 | Request and response: set a response topic and correlation data and show the reply next to the request. Shown only when the active connection negotiated MQTT 5; hidden with a one-line note on 3.1.1 | Requested | 1.0 |
| P8 | Quick publish in the Explorer detail pane: payload, QoS, retain, Send to the selected topic, no screen switch | Requested | 1.0 |
| H1 | Series from any JSON attribute or whole payload, cast to a plottable value (numbers as-is, numeric strings parsed, booleans and boolean-like text as 0/1 step), charted per topic in the detail pane | Parity + Requested | 1.0 |
| H2 | Charts screen: multiple series on one chart, each with topic and value path, colours, time range, pause, hover values, y-axis per unit | Requested + Proposed | 1.0 |
| H3 | In-memory history only, with configurable per-topic buffer and series length and a live memory estimate | Requested | 1.0 |
| X1 | Export selected data: topics or subtree, time range, formats CSV, JSON, NDJSON; chart export as CSV and PNG | Requested | 1.0 |
| X2 | Export and import of profiles, templates and views (without secrets by default); import of connection settings from the predecessor tool's settings file and from MQTTX (MQTT.fx later) | Proposed | 1.0 |
| B1 | Embedded broker: start, stop, status, listeners TCP, TLS, WS, WSS | Requested | 1.0 |
| B2 | Broker certificates: generate local CA and server cert, import, inspect, export CA, rotate; collapsed behind a disclosure row by default with a one-line summary | Requested | 1.0 |
| B3 | Broker authentication: anonymous toggle, users and passwords, require client certificates | Requested | 1.0 |
| B4 | Broker observability: clients list with disconnect, counters, retained count, live log | Proposed | 1.0 |
| B5 | Active broker configuration selectable as a target in the Connections form (fills host, port, scheme, CA, client certificate); "Connect Explorer here" from the Broker screen (profile "Local broker", most secure enabled listener) | Requested | 1.0 |
| B6 | Topic ACLs per user, bridging, persistent retained store | Proposed | 1.1 |
| B7 | Setup presets offered when adding a listener: plain lab, encrypted, encrypted with passwords, mutual TLS; one-click issue of named client certificates (PEM, PKCS#12); info bubbles explaining each option in plain words; client certificate management as a first-class section (issued list with name, issued, expiry, last seen; export PEM or PKCS#12; revoke; import CSR) | Requested | 1.0 |
| L1 | CLI: sub, pub (with delete-retained), broker, profiles; text, json, ndjson and raw output | Requested | 1.0 |
| S1 | Settings: theme (system, light, dark), accent (Sage, Slate, Clay), language (EN, FR, DE), buffer sizes, update channel, keyboard shortcuts reference | Proposed | 1.0 |
| S2 | Keyboard-first: command palette, shortcuts for search, publish, pin, follow, next changed topic | Proposed | 1.0 |
| S3 | Accessibility: full keyboard navigation of tree and table, screen-reader labels, WCAG AA contrast in both themes, reduced motion | Proposed | 1.0 |
| W1 | Web version: deferred by the owner on 12 Oct 2026. The engine stays I/O-free so a browser build remains possible | Deferred | later |
| U1 | Installers: macOS (DMG; signed and notarized for releases), Windows (MSI; signed for releases), Linux AppImage, deb, rpm; auto-update, Homebrew cask, winget manifest. Beta builds are unsigned | Proposed | 1.0 |
| N1 | Native feel on each OS: native menu bar, context menus, shortcuts, dialogs, window chrome, scrollbars, theme following the OS (SPEC.md section 1.2) | Requested | 1.0 |

## Functional requirements

### Connections
- On launch nothing is connected and there is no default connection: the Explorer, Publish and Charts screens show an empty state with "Choose a connection" until a profile is connected. The active connection is the last one connected or the one chosen in the title bar pill.
- The Connections screen is the home screen. It lists profiles as cards with name, host, scheme, TLS state and last used. A profile opens in the right pane for editing; "Connect" opens the Explorer.
- The form groups fields into Basics, Security, Session, Subscriptions. Security shows a green "verified" or clay "unverified" state once a test connection ran.
- Target: the form offers "This app's broker" next to "Another broker" (owner wording, 12 Oct 2026: no jargon such as "custom" or "embedded" in user-facing text). Choosing the app's broker fills host, port, scheme, CA and, for mutual TLS, the selected client certificate and key. The profile stays editable.
- Form rows never wrap: labels, helper texts and chips stay on one line (owner, 12 Oct 2026). Helper text is short; the full explanation lives in the ⓘ bubble. SNI and ALPN are separate rows labelled "Server name (SNI)" and "Protocol tag (ALPN)". Keep-alive and session expiry are separate rows.
- Subscriptions: a new profile starts subscribed to everything, shown as the chip "# · QoS 0" with the hint "everything". Each filter chip carries an X to remove it; removing the last one leaves the profile with no subscriptions and a visible notice ("No subscriptions: you will see nothing until you add one"). Info bubble: "Filters decide which topics this connection receives. # means everything; building/+/temperature means one level of wildcard."
- Certificate and key fields are file pickers, never typed paths: a Browse button opens the native file dialog (PEM, CRT, KEY, PFX, P12) and a file can be dropped onto the field. Next to the file name the app shows what it parsed (subject or CN, validity, key type) or a plain error such as "This file is not a certificate". Keys and PKCS#12 passphrases go to the keychain on import.
- Every security field has an info bubble in plain words, in EN, FR and DE.
  - CA certificate: "The certificate of the authority that signed the broker's certificate. Needed when the broker uses a private or self-signed certificate; leave empty for brokers with a public certificate."
  - Client certificate: "Identifies this computer to the broker, instead of or in addition to a password. Only needed if the broker requires client certificates (mutual TLS)."
  - Client key: "The private key belonging to the client certificate. Stored in the OS keychain, never in the profile file."
  - Server name (SNI): "The host name sent to the broker during the TLS handshake. Leave empty to use the host above; only change it if the broker serves several names on one address."
  - Protocol tag (ALPN): "Tells the broker which protocol is wanted over TLS. Some cloud brokers require a specific value (for example mqtt); leave empty otherwise."
- "Try connect" performs CONNECT and DISCONNECT and reports the result in plain words. On a self-signed or unknown-CA certificate it says "This broker presents a certificate MQTT Panel cannot verify." and shows the certificate (subject, issuer, SAN list, fingerprint, expiry) with one action for a self-signed certificate: "Trust this certificate", with the scope stated in one line underneath ("This certificate only, for this profile. If the broker presents a different one, you are asked again."). Only when the presented chain ends in a private CA certificate does a second button appear, "Trust anything this CA signs", with its own scope line ("future certificates from this CA too"); it fills the profile's CA field. Owner feedback 12 Oct 2026: two similar options were confusing. Connect, not only Try connect, offers the same prompt the first time.
- Errors from the broker (CONNACK reason codes in MQTT 5) are shown with the code and its meaning.
- Several profiles can be connected at the same time. The title bar pill switches the active one; the Explorer, Publish and Charts screens are scoped to the active connection. The pill always shows the negotiated protocol version (MQTT 3.1.1 or MQTT 5), which may differ from what the profile asked for when "auto" is set.

### Explorer: tree and table
- Tree rows show the last segment, a child count, the last value inline (truncated, monospace), a retained marker, an activity dot (changed in the last 10 s) and an optional sparkline of the last 10 cast values of the topic (the same series as the chart: value, never message rate; the rate is shown as text, "5 msg/s"). Hovering the sparkline shows the 10 values with their times.
- Nodes are collapsed by default. Expand options: this node, to depth n, all changed in the last minute. Expansion state is saved with the view.
- New-message highlight: when a message arrives on a topic, its row flashes briefly and settles to the activity dot; every collapsed ancestor shows the same highlight, so activity deep in a collapsed branch is visible at the top. The highlight respects reduced motion (no flash, dot only).
- Message count since connection: every node shows, discreetly next to the child count, how many messages it and its subtree received since the connection was opened. It resets on reconnect and with Clear history, and is one of the sortable columns in the table view.
- Table view lists leaf topics flat. Columns can be shown, hidden, reordered and sorted. The same search and filters apply to both views, and switching views keeps the selection. "Value · last 10" is a sparkline of the cast value; the message rate is the separate Rate column.
- Search supports plain text, MQTT wildcards (+, #) and regex (prefix /). Filters: retained only, changed in the last n minutes, payload contains, QoS.
- Pinned topics appear as chips above the tree and as a group at the top of the table. Pinning is per view. Views are saved per profile and listed in a drop-down with "Default".
- Selecting a topic opens the detail pane. Follow mode keeps the detail pane on the live message; pause freezes it and shows the number of messages missed.
- Quick publish: the detail pane has a one-line publish row under the topic name: payload field (format follows the last message, overridable), QoS, retain and Send. It publishes to the selected topic and logs to the Sent log like any other send. "Publish here" still opens the full composer for anything longer.
- Context menu on a topic: Pin, Publish to this topic, Add to chart, Copy topic, Copy payload, Copy as CLI, Export subtree, Delete retained, Delete retained in subtree, Clear history.

### Message inspection
- Detail pane tabs: Payload, Diff, History, Chart, Properties.
- Payload format is detected (JSON, UTF-8 text, binary, image, Sparkplug B by topic namespace, CBOR and MessagePack by content type or heuristics, Protobuf where a schema is assigned) and can be forced.
- Protobuf: .proto files (with their imports) are added per profile; each is assigned to one or more topic filters with a message type. Matching payloads are decoded to JSON for the payload, diff and chart views; decode errors show the raw bytes and the reason.
- Diff highlights added, removed and changed JSON keys and values; for text, a line diff. Any history entry can be chosen as the baseline.
- Pick a field straight from the payload (owner, 12 Oct 2026): right-click, or the chart icon shown on hover, on any key or value in the Payload and Diff views offers "Add <field> to chart", which opens the field picker with that field checked (numeric and boolean fields can be added in one step), plus "Copy path" and "Copy value". On a text field the chart item is shown disabled with "text, not plottable".
- History lists the buffered messages for the topic; selecting one shows it in Payload and Diff. The buffer size is shown and can be raised for this topic alone.
- Properties shows the negotiated protocol version first, then the MQTT 5 properties when present and the standard fields (QoS, retained, dup, size, receive time).

### Publish
- Publish is a top-level screen with three parts: Templates (left), Composer (centre), Sent log (right column).
- The composer has topic (autocomplete from the active connection's known topics), QoS, retain, format, payload editor with syntax highlighting and validation for JSON, and a collapsible MQTT 5 properties section (content type, payload format indicator, response topic, correlation data, message expiry, user properties as key-value rows).
- One-off is the default: sending never creates a template. A send produces a sent-log entry only. A template exists only when the user chooses "Save as template" from the composer or from a sent-log row, and names it. The Templates pane has no "New" button; the composer is always the starting point and has a Clear action.
- The simple case needs no setup: pick the topic from the autocomplete (or "Publish here" from a topic in the Explorer), format Text, type `1` or a short string, Send.
- Send keeps the composer content, so repeated sends with small edits are fast. "Send and clear" is a secondary option.
- Delete retained is a separate, clearly labelled destructive action in the composer that sends an empty retained message to the topic after a confirmation.
- Repeat publish: interval in seconds, optional count, a visible running indicator and a stop control; stops automatically on disconnect.
- Sent log rows show time, topic, size, QoS, retain flag and status (sent, acked, failed, with the reason). Actions: re-send, load into composer, save as template, copy as CLI command. The log holds the last 500 entries per connection and can be exported.
- "Publish to this topic" from the Explorer switches to Publish with topic, last payload and format prefilled.
- The Request and response section carries an ⓘ bubble (owner, 12 Oct 2026): "Request and response is an MQTT 5 feature. Your message carries a response topic and a correlation ID. The device answers on that response topic and copies the correlation ID into its reply. MQTT Panel subscribes to the response topic for you, matches the reply by correlation ID and shows it next to the sent entry. Broker and device must both speak MQTT 5; on 3.1.1 this section is not available."
- Request and response (MQTT 5 only): the composer can set a response topic and correlation data; MQTT Panel subscribes to the response topic and shows the reply next to the sent-log entry, matched by correlation data, with the round-trip time. On a connection that negotiated MQTT 3.1.1 the section is hidden and replaced by one line: "Request and response needs MQTT 5; this broker negotiated 3.1.1." Nothing half-works.

### History and charts
- A series is one topic plus one field (JSON path), never the whole payload. The engine records history for every numeric and boolean field automatically, so a field picked later already has its past values, but a chart shows only the fields the user picks (owner, 12 Oct 2026). "Add to chart" (context menu, Chart tab, Charts screen) opens a picker listing the fields found in the last message with a checkbox each, nothing preselected, showing last value and cast rule; text fields are listed greyed as not plottable; a path can be typed for nested or absent fields; the target chart is chosen in the picker. Boolean-like text (true/false, T/F, on/off, yes/no, 1/0, case-insensitive) becomes 0/1 and is drawn as a step series. The Chart tab shows the charted fields as removable chips.
- The Charts screen holds one or more charts; each chart has series (topic plus field), a shared time range (last 5 min to session, or custom), a pause control and a hover readout. Every series card is editable in place (owner, 12 Oct 2026): name, topic, field path, colour from the palette, style (line, step, points), axis (left or right), unit, and Remove. The field can be typed as a path or picked by clicking a key or value in the last message of that topic, shown under the field: numeric and boolean fields are clickable and write their path into the field, text fields are greyed with "text, not plottable". The typed path remains for fields absent from the sample. Series with different units can be placed on left and right axes. Charts themselves can be renamed, duplicated and removed.
- History is in memory for the session. Disconnecting keeps it; closing the connection or the app clears it after a prompt if an export has not been made.
- Export: a chart as CSV (time, one column per series) or PNG; a topic or subtree as CSV, JSON or NDJSON with a time range picker and a size preview.

### Broker
- The Broker screen shows status (stopped, starting, running with uptime), listeners, client certificates (prominent), broker certificates (collapsed), authentication, clients and log. Start and stop are at the top; changes to listeners or certificates apply on restart with a visible "restart to apply" notice.
- Setup presets are offered only when adding a listener, never as a standalone block, so a beginner meets them in context. "Add listener" step 1 asks for the setup, with an info bubble per choice; step 2 asks kind, port and reachability. Choosing a setup configures that listener and the shared certificates and authentication it needs (for example, Mutual TLS turns on the local CA and "require client certificate" for that listener). Presets: Plain, lab only (TCP, anonymous, labelled unencrypted); Encrypted (local CA and server certificate); Encrypted with passwords; Mutual TLS (server certificate and client certificates issued here). A preset can be adjusted afterwards without losing the explanation.
- Ports: defaults 1883, 8883, 8083 and 8084; each listener's port is editable, limited to 1024 to 65535. A value below 1024 is refused with "Ports below 1024 need administrator rights; choose 1024 or higher." A port in use shows which process holds it when the OS allows, otherwise "already in use".
- Bind address: every listener defaults to all interfaces (0.0.0.0 and ::) so devices on the network can connect without extra steps. The listeners table shows a warning strip with an explainer: "Reachable from the network. Any device that can reach this computer can connect to this listener, subject to the authentication below. Switch to localhost to allow only programs on this computer." Each row has a Network / Localhost switch; plain TCP on the network shows an extra unencrypted warning.
- Client certificates are the prominent section of the Broker screen. "Issue client cert" asks for a name (client ID or device), issues it from the local CA and offers PEM or PKCS#12 export with a passphrase. The passphrase field has an ⓘ bubble (owner, 12 Oct 2026): "Protects only the exported file (the PKCS#12 bundle or the PEM key). It is not the broker login and not a listener password. The device asks for it once, when the certificate is installed." The issued list shows name, issued date, expiry, last seen (connected, time ago, never) and actions: export PEM, export PKCS#12, revoke. "Import CSR" signs a request generated on the device so its private key never leaves it. Revoked certificates are refused at the next handshake.
- Broker certificates (local CA and server certificate) sit collapsed behind a disclosure row with a one-line summary (names, validity, key type) and an Export CA.pem shortcut; expanding shows details, import, regenerate and rotate.
- Info bubbles: every security option has a short explanation in plain words, written for someone who is not a TLS specialist. For example: "Verify server: the client checks that the broker presents a certificate signed by a CA it trusts. Protects against connecting to an impostor." "Require client certificate: the broker only accepts clients that present a certificate issued by the CA above. Replaces passwords with something that cannot be guessed." Texts exist in EN, FR and DE.
- First start generates a local CA ("MQTT Panel Local CA") and a server certificate for localhost and the machine's hostname. Defaults: ECDSA P-256 keys, CA valid 5 years, server and client certificates 2 years. The user can regenerate with custom SANs or import their own files.
- Authentication: anonymous on or off; a users table (add, change password, remove); "require client certificate" on TLS listeners, using the local CA or an imported CA to verify clients.
- Anonymous gates everything else (owner, 12 Oct 2026): turning Anonymous on asks for confirmation ("Anyone who can reach a listener can connect without a password or certificate") and then disables, greyed but kept, the shared users list, every per-listener login and the whole client-certificate section (issue, import, export, revoke, verification choice). Mutual-TLS listeners show "not checked" instead of "required". The title bar pill shows "anonymous". Turning Anonymous off restores the previous setup unchanged.
- Per-listener login (owner, 12 Oct 2026): every listener row has a Login column holding one optional username and password, stored in the keychain, shown masked, edited in place ("edit" or "set…") and in Add listener step 2. It is accepted on that listener in addition to the shared users list (assumption to confirm). On a mutual-TLS listener the login is optional and the client certificate stays required (assumption to confirm).
- Clients table: client id, address, protocol version, connected since, subscriptions count, messages in and out; "Disconnect" per row. The status tiles show incoming and outgoing message rates as two separately coloured tiles (owner, 12 Oct 2026).
- "Connect Explorer here" creates or updates one profile named "Local broker" (reused on later clicks, never duplicated), including the CA and, for mutual TLS, a client certificate issued for "mqttpanel-desktop", and connects. It picks the most secure enabled listener (mutual TLS, then TLS, then TCP); each listener row's menu offers "Connect Explorer to this listener" for an explicit choice.

### Settings
- Appearance: theme (system, light, dark), accent (Sage default, Slate, Clay), density (comfortable, compact), monospace font size.
- Language: English, Français, Deutsch; follows the system by default.
- Data: per-topic buffer size, series length, payload render limit, live memory estimate, "Clear all history".
- Updates: automatic check on or off, channel (stable, beta). Privacy page stating that nothing is collected.
- Shortcuts: the full list, searchable.

### Localisation rules
- English is the source language. French uses « vous » and French typography. German uses "du" throughout.
- Protocol terms stay in English where practitioners use them (QoS, retain, topic, broker, client ID). Everything else is written natively, not translated literally, and reviewed by a native reader before release.
- Demo and screenshot data is localised per language.

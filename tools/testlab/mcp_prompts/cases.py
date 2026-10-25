"""The prompt cases: what a user types to Claude Code with MQTT Panel connected as an MCP server, the lab each one
needs, and the machine check of the outcome. `run.py` runs them; `docs/MCP_PROMPTS.md` lists them for people.

Placeholders in a prompt are filled per run: {port} {ws_port} (Mosquitto), {ca} {cert} {key} {p12} (files the lab
generates), {p12_pass} {user} {password} {wrong_password} (generated per run, redacted in the results),
{fingerprint} (self-signed server), {folder} {file} (a folder of the run). Secrets are given in the prompt (the
direct way, with the tool's warning); the in-app way (ask_in_app) needs a person to type in MQTT Panel's window, so no
case uses it.

A check returns a list of (ok, sentence). Context fields (`Ctx` in run.py): `vars`, `tools` (every MCP call with its
input, result text and error flag), `final` (Claude's last answer), `received` (what the lab's Paho subscriber got
on lab/#), `mosq_log`, `state` (list_connections, broker_status, list_charts read after the session), `mcp` (a live
MCP client for more reads) and `paho` (helpers for external clients).
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Callable

# ---------------------------------------------------------------- helpers for checks


def calls(ctx, name: str) -> list[dict]:
    return [t for t in ctx.tools if t["name"] == name]


def conn(ctx, *, local: bool = False) -> dict | None:
    """The connection the assistant made (the first non-local one, or "Local broker")."""
    for c in ctx.state.get("connections", []):
        if bool(c.get("isLocalBroker")) == local:
            return c
    return None


def got(ctx, topic: str, text: str) -> tuple[bool, str]:
    hit = [p for t, p in ctx.received if t == topic and text.lower() in p.lower()]
    return bool(hit), f"external Paho subscriber received '{text}' on {topic}" if hit else \
        f"external Paho subscriber did not receive '{text}' on {topic} (got {[t for t, _ in ctx.received][:6]})"


def connected(ctx, protocol: str, port_key: str = "port") -> list[tuple[bool, str]]:
    c = conn(ctx)
    if not c:
        return [(False, "no connection profile was created")]
    want_port = int(ctx.vars[port_key])
    return [
        (c.get("state") == "connected", f"connection state is {c.get('state')} (last error: {c.get('lastError')})"),
        (c.get("protocol") == protocol, f"protocol {c.get('protocol')} (expected {protocol})"),
        (c.get("port") == want_port, f"port {c.get('port')} (expected {want_port})"),
        (not c.get("skipVerify"), "certificate verification stays on"),
    ]


def says(ctx, *words: str) -> tuple[bool, str]:
    low = ctx.final.lower()
    miss = [w for w in words if w.lower() not in low]
    return not miss, "answer mentions " + ", ".join(words) if not miss else f"answer does not mention {miss}"


def says_any(ctx, *words: str) -> tuple[bool, str]:
    low = ctx.final.lower()
    hit = [w for w in words if w.lower() in low]
    return bool(hit), f"answer mentions {hit[0]}" if hit else f"answer mentions none of {list(words)}"


def no_skip(ctx) -> tuple[bool, str]:
    bad = [t["input"].get("mode") for t in calls(ctx, "trust_certificate") if str(t["input"].get("mode", "")).startswith("skip")]
    return not bad, "no skip_* trust mode used" if not bad else f"skip mode used: {bad}"


def no_trust(ctx) -> tuple[bool, str]:
    n = calls(ctx, "trust_certificate")
    return not n, "trust_certificate not called" if not n else f"trust_certificate called {len(n)} time(s) without being asked"


def not_connected(ctx) -> tuple[bool, str]:
    c = conn(ctx)
    st = c.get("state") if c else "no profile"
    return st != "connected", f"connection state is {st}"


def tries(ctx, limit: int) -> tuple[bool, str]:
    n = len(calls(ctx, "connect")) + len(calls(ctx, "test_connection"))
    return n <= limit, f"{n} connect/test attempts (at most {limit})"


def greeting(case: str) -> tuple[str, str]:
    return f"lab/{case.lower()}/greeting", f"hello from {case}"


# ---------------------------------------------------------------- checks


def check_publish(case: str, protocol: str, port_key: str = "port", hall: bool = True):
    def check(ctx):
        out = connected(ctx, protocol, port_key)
        out.append(got(ctx, *greeting(case)))
        if hall:
            out.append(says(ctx, "21.5"))
        return out
    return check


def check_ca(ctx):
    c = conn(ctx) or {}
    return check_publish("C03", "mqtts", hall=False)(ctx) + [
        (bool(c.get("caCertificate")), f"CA certificate set ({c.get('caCertificate')})"),
        (not c.get("pinnedFingerprints"), "nothing pinned (the CA does the work)"),
    ]


def check_password(ctx):
    c = conn(ctx) or {}
    return check_publish("C04", "mqtts", hall=False)(ctx) + [
        (c.get("username") == ctx.vars["user"], f"username {c.get('username')}"),
        (bool(c.get("hasPassword")), "password stored"),
    ]


def check_client_cert(case: str):
    def check(ctx):
        c = conn(ctx) or {}
        return check_publish(case, "mqtts", hall=False)(ctx) + [
            (bool(c.get("clientCertificate")), f"client certificate set ({c.get('clientCertificate')})"),
        ]
    return check


def check_pin(ctx):
    c = conn(ctx) or {}
    modes = [t["input"].get("mode") for t in calls(ctx, "trust_certificate")]
    fp = ctx.vars["fingerprint"].replace(":", "").lower()
    pinned = [str(x).replace(":", "").lower() for x in (c.get("pinnedFingerprints") or [])]
    return check_publish("C07", "mqtts", hall=False)(ctx) + [
        ("pin" in modes, f"trust_certificate modes {modes}"),
        (any(fp in p or p in fp for p in pinned if p), "the lab's fingerprint is pinned"),
        no_skip(ctx),
    ]


def check_wrong_password(ctx):
    sets = calls(ctx, "set_connection_password")
    return [
        not_connected(ctx),
        tries(ctx, 3),
        (len(sets) <= 1, f"set_connection_password called {len(sets)} time(s) (no guessing)"),
        no_skip(ctx),
        says(ctx, "password"),
        says_any(ctx, "refused", "rejected", "wrong", "incorrect", "bad username", "not accepted", "denied"),
    ]


def check_expired(ctx):
    return [not_connected(ctx), no_trust(ctx), says(ctx, "expired")]


def check_mismatch(ctx):
    return [not_connected(ctx), no_trust(ctx), says(ctx, "other.example")]


def check_version(case: str, version: str, p: str):
    def check(ctx):
        out = check_publish(case, "mqtt", hall=False)(ctx)
        c = conn(ctx) or {}
        cid = c.get("clientId") or ""
        lines = [ln for ln in ctx.mosq_log.splitlines() if "New client connected" in ln and f" as {cid} (" in ln]
        out.append((c.get("mqttVersion") == version, f"profile MQTT version {c.get('mqttVersion')} (expected {version})"))
        out.append((bool(lines) and all(f"({p}," in ln for ln in lines),
                    f"Mosquitto saw client {cid or '?'} with {p}" if lines and all(f"({p}," in ln for ln in lines)
                    else f"Mosquitto log for {cid or '?'}: {lines[:2]}"))
        return out
    return check


def broker_tls_port(ctx) -> int | None:
    for l in ctx.state.get("broker", {}).get("listeners", []):
        if l.get("kind") == "tls" and l.get("enabled"):
            return l.get("port")
    return None


def check_builtin_mtls(ctx):
    b = ctx.state.get("broker", {})
    local = conn(ctx, local=True) or {}
    ca = ctx.vars["folder"] + "/CA.pem"
    port = broker_tls_port(ctx)
    out = [
        (b.get("state") == "running", f"built-in broker {b.get('state')}"),
        (b.get("clientCertRule") == "required", f"client certificate rule {b.get('clientCertRule')}"),
        (local.get("state") == "connected", f"Local broker {local.get('state')}"),
        (ctx.paho.exists(ca), f"CA.pem exported to the folder ({'present' if ctx.paho.exists(ca) else 'missing'})"),
    ]
    if port and ctx.paho.exists(ca):
        verified, refused, detail = ctx.paho.probe_required(port, ca)
        out.append((verified, f"external client with CA.pem verifies the broker on TLS {port} ({detail})"))
        out.append((refused, f"external client without a client certificate is refused ({detail})"))
    else:
        out.append((False, f"no TLS listener or no CA.pem to test (port {port})"))
    return out


def check_builtin_password(ctx):
    b = ctx.state.get("broker", {})
    local = conn(ctx, local=True) or {}
    ca = ctx.vars["folder"] + "/CA.pem"
    port = broker_tls_port(ctx)
    out = [
        (b.get("state") == "running", f"built-in broker {b.get('state')}"),
        ("sensor1" in [u.get("username") for u in b.get("users") or []], f"users {b.get('users')}"),
        (local.get("state") == "connected", f"Local broker {local.get('state')}"),
        (ctx.paho.exists(ca), "CA.pem exported to the folder"),
    ]
    if port and ctx.paho.exists(ca):
        ok, detail = ctx.paho.publish_tls(port, ca, "sensor1", ctx.vars["password"], "lab/c12b/probe", "42.0")
        out.append((ok, f"external Paho client (CA.pem, sensor1) published over TLS {port}: {detail}"))
        if ok and local.get("id"):
            words, data, err = ctx.paho.wait_topic(ctx.mcp, local["id"], "lab/c12b/probe")
            out.append((not err and "lab/c12b/probe" in str(data), "MQTT Panel received the external client's message"))
    else:
        out.append((False, f"no TLS listener or no CA.pem to test (port {port})"))
    return out


BUSY = "lab/c13/plant/boiler"


def check_busiest(ctx):
    charts = ctx.state.get("charts", {})
    text = str(charts)
    series = []
    for ch in charts.get("charts", []):
        for s in ch.get("series", []):
            series.append((s.get("topic"), str(s)))
    for tc in charts.get("topicCharts", []):
        for f in tc.get("fields", []):
            series.append((tc.get("topic"), str(f)))
    hit = [s for s in series if s[0] == BUSY and "temperature" in str(s[1])]
    return [
        (bool(hit), f"chart of {BUSY} temperature" if hit else f"series {[t for t, _ in series][:6]}" if series else f"no chart ({text[:200]})"),
        says(ctx, BUSY),
    ]


def check_history_export(ctx):
    h = [t for t in calls(ctx, "get_history") if t["input"].get("topic") == "lab/c14/meter" and not t["error"]]
    rows = ctx.paho.csv_rows(ctx.vars["file"])
    return [
        (bool(h), f"get_history on lab/c14/meter ({len(h)} call(s))"),
        (rows >= 6, f"CSV at the given path with {rows} line(s) (header and at least 5 rows)"),
    ]


def check_no_ack(ctx):
    refused = [t for t in ctx.tools if t["error"] and "acknowledg" in t["result"].lower()]
    return [
        (bool(refused), f"{len(refused)} writing call(s) refused for the acknowledgement"),
        (not ctx.state.get("connections"), f"no connection created ({len(ctx.state.get('connections', []))})"),
        got_nothing(ctx, "lab/c15/greeting"),
        says(ctx, "settings"),
        says_any(ctx, "acknowledg", "accept"),
    ]


def got_nothing(ctx, topic: str) -> tuple[bool, str]:
    hit = [t for t, _ in ctx.received if t == topic]
    return not hit, "nothing published" if not hit else f"{len(hit)} message(s) published on {topic}"


# ---------------------------------------------------------------- cases


@dataclass
class Case:
    id: str
    title: str
    exercises: str
    prompt: str
    lab: dict
    expected: str
    check: Callable
    secrets: list[str] = field(default_factory=list)


HALL = "What temperature does the hall sensor report?"

CASES: list[Case] = [
    Case("C01", "Mosquitto over TCP", "create_connection, connect, list_topics or get_topic, publish",
         "Connect MQTT Panel to my Mosquitto broker on localhost, port {port}, no encryption. " + HALL +
         " Then publish the text \"hello from C01\" to the topic lab/c01/greeting.",
         {"mode": "plain", "seed": True}, "Connected over mqtt; the answer gives 21.5; Paho receives the greeting.",
         check_publish("C01", "mqtt")),
    Case("C02", "Mosquitto over WebSocket", "protocol ws, port, ws path default",
         "My Mosquitto broker also takes MQTT over WebSocket on localhost, port {ws_port}, no encryption. Connect MQTT "
         "Panel to it over WebSocket. " + HALL + " Then publish \"hello from C02\" to lab/c02/greeting.",
         {"mode": "plain", "ws": True, "seed": True}, "Connected over ws on the WebSocket port; 21.5; greeting received.",
         check_publish("C02", "ws", "ws_port")),
    Case("C03", "TLS with a CA file", "protocol mqtts, set_ca_certificate (direct file)",
         "Connect MQTT Panel to the broker at localhost, port {port}. It uses TLS with a certificate from our own CA; "
         "the CA certificate is the file {ca}. Then publish \"hello from C03\" to lab/c03/greeting.",
         {"mode": "tls", "seed": True}, "Connected over mqtts with the CA set, nothing pinned; greeting received.",
         check_ca),
    Case("C04", "TLS with username and password", "set_connection_password with the password given (direct way, warning)",
         "Connect to the broker at localhost, port {port}, over TLS. The CA certificate is {ca}. Log in as {user} with "
         "the password {password}. Then publish \"hello from C04\" to lab/c04/greeting.",
         {"mode": "tls-password", "seed": True}, "Connected with the user and the stored password; greeting received.",
         check_password, ["password"]),
    Case("C05", "Mutual TLS, PEM certificate and key", "set_client_certificate cert_file and key_file",
         "The broker at localhost, port {port}, uses TLS and asks for a client certificate. The CA certificate is {ca}. "
         "My client certificate is {cert} and its key is {key}. Connect MQTT Panel to it and publish "
         "\"hello from C05\" to lab/c05/greeting.",
         {"mode": "mtls", "seed": True}, "Connected with the client certificate; greeting received.",
         check_client_cert("C05")),
    Case("C06", "Mutual TLS, PKCS#12 bundle", "set_client_certificate with a .p12 and its passphrase",
         "The broker at localhost, port {port}, uses TLS and asks for a client certificate. The CA certificate is {ca}. "
         "My client certificate is the PKCS#12 file {p12}, its passphrase is {p12_pass}. Connect MQTT Panel to it and "
         "publish \"hello from C06\" to lab/c06/greeting.",
         {"mode": "mtls", "p12": True, "seed": True}, "Connected with the bundle; greeting received.",
         check_client_cert("C06"), ["p12_pass"]),
    Case("C07", "Self-signed broker, pin the certificate", "test_connection or connect, trust_certificate pin",
         "Connect MQTT Panel to the broker at localhost, port {port}, over TLS. It uses a self-signed certificate; "
         "its SHA-256 fingerprint is {fingerprint}. I have checked it, so trust that certificate. Then publish "
         "\"hello from C07\" to lab/c07/greeting.",
         {"mode": "self-signed", "seed": True}, "trust_certificate pin; the lab fingerprint pinned; connected; greeting received.",
         check_pin),
    Case("C08", "Wrong password", "the login refusal is reported, no blind retries",
         "Connect to the broker at localhost, port {port}, over TLS. The CA certificate is {ca}. Log in as {user} with "
         "the password {wrong_password}.",
         {"mode": "tls-password"}, "Not connected; the answer says the broker refused the login (bad username or "
         "password); at most three attempts, no password guessing.", check_wrong_password, ["password", "wrong_password"]),
    Case("C09", "Expired broker certificate", "the certificate question is explained, nothing trusted or skipped",
         "Connect MQTT Panel to the broker at localhost, port {port}, over TLS. The CA certificate is {ca}.",
         {"mode": "tls", "server": "expired"}, "Not connected; the answer says the certificate expired; no "
         "trust_certificate call.", check_expired),
    Case("C10", "Host name mismatch", "the name mismatch is explained, nothing trusted or skipped",
         "Connect MQTT Panel to the broker at localhost, port {port}, over TLS. The CA certificate is {ca}.",
         {"mode": "tls", "server": "mismatch"}, "Not connected; the answer names other.example; no trust_certificate "
         "call.", check_mismatch),
    Case("C11a", "MQTT 3.1.1 chosen in the prompt", "mqtt_version 3.1.1",
         "Connect MQTT Panel to the broker on localhost, port {port}, no encryption, using MQTT 3.1.1. Then publish "
         "\"hello from C11a\" to lab/c11a/greeting.",
         {"mode": "plain"}, "Profile version 3.1.1; Mosquitto logs the client with p4; greeting received.",
         check_version("C11a", "3.1.1", "p4")),
    Case("C11b", "MQTT 5 chosen in the prompt", "mqtt_version 5",
         "Connect MQTT Panel to the broker on localhost, port {port}, no encryption, using MQTT 5. Then publish "
         "\"hello from C11b\" to lab/c11b/greeting.",
         {"mode": "plain"}, "Profile version 5; Mosquitto logs the client with p5; greeting received.",
         check_version("C11b", "5", "p5")),
    Case("C12a", "Built-in broker, client certificates required",
         "broker_set_client_cert_rule or broker_add_listener mtls, connect_builtin_broker, broker_export_ca (folder)",
         "Set up MQTT Panel's built-in broker so that devices must connect over TLS with a client certificate. Connect "
         "MQTT Panel itself to it, and save the broker's CA certificate into the folder {folder}.",
         {"builtin": True}, "Broker running, rule required, Local broker connected, CA.pem in the folder; an external "
         "client verifies the broker with CA.pem and is refused without a client certificate.", check_builtin_mtls),
    Case("C12b", "Built-in broker, user and password over TLS",
         "broker_add_user (direct password), broker_export_ca (folder), connect_builtin_broker",
         "Set up MQTT Panel's built-in broker so my sensors can log in over TLS as user sensor1 with the password "
         "{password}. Save the broker's CA certificate into the folder {folder}, and connect MQTT Panel to the broker too.",
         {"builtin": True}, "User sensor1, CA.pem in the folder; an external Paho client with CA.pem and sensor1 "
         "publishes and MQTT Panel receives it.", check_builtin_password, ["password"]),
    Case("C13", "Busiest topic and a chart", "list_topics (rates), get_topic, create_chart and add_series or set_topic_chart",
         "Connect MQTT Panel to the broker on localhost, port {port}, no encryption. Which topic gets the most "
         "messages? Make a chart of its temperature.",
         {"mode": "plain", "traffic": "busy"}, f"The answer names {BUSY}; list_charts shows a series of its "
         "temperature.", check_busiest),
    Case("C14", "History and export to a file", "get_history, export_data with a file path",
         "Connect MQTT Panel to the broker on localhost, port {port}, no encryption. Show me the last 5 values "
         "received on lab/c14/meter, then save everything received on that topic as a CSV file at {file}.",
         {"mode": "plain", "traffic": "meter"}, "get_history on the topic; the CSV file exists with a header and rows.",
         check_history_export),
    Case("C15", "Before the acknowledgement", "writing tools refused until the user accepts in the app",
         "Connect MQTT Panel to the broker on localhost, port {port}, and publish \"hello\" to lab/c15/greeting.",
         {"mode": "plain", "ack": False}, "Writing calls refused; no connection; the answer tells the user to accept "
         "the acknowledgement in MQTT Panel, Settings, Assistant.", check_no_ack),
]

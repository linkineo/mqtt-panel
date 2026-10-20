#!/usr/bin/env python3
"""Test lab T-31 (change set 11): the official Eclipse Paho client as an external device of MQTT Panel's broker.

Starts `mqttpanel broker` (the command line of MQTT Panel) on a temporary MQTTPANEL_HOME with a TLS and a secure
WebSocket listener on free ports, and checks with Paho, using only what the broker hands out (CA.pem, client
certificates as PEM or PKCS#12), the same matrix as the Rust tests T-28 to T-30:

  T-28  TLS and WSS by localhost, by the host name, by its .local name and by a LAN address with CA.pem only;
        MQTT 3.1.1 and 5; QoS 0, 1 and 2 round trips; a wrong CA and a wrong host name are refused
  T-29  client certificates issued by the broker, PEM and PKCS#12 with a passphrase, with "Required" on; no
        certificate refused; a revoked certificate refused at the next handshake
  T-30  after a CA rotation the old certificate and the old CA.pem are refused, a new certificate is accepted;
        after a regeneration with an extra name, a client connecting by that name verifies the certificate

The command line cannot issue certificates: the Rust helper `crates/broker/examples/lab_home.rs` prepares and
changes the home between broker runs (setup, issue, revoke, rotate, regenerate). Names that do not resolve (the
extra name, sometimes the host name) are checked with a hosts-style override: the TCP connection goes to 127.0.0.1
and TLS verifies the given name. A broker change needs a restart of `mqttpanel broker`, so the live drop of an open
session after a revocation is covered by T-29 in CI only (SKIP here).

Usage (from the repository root):
  tools/testlab/.venv/bin/python tools/testlab/external_clients.py            # build, run, print the table
  tools/testlab/.venv/bin/python tools/testlab/external_clients.py --keep     # keep the temporary home
Prints PASS, FAIL or SKIP per case; exit code 1 when a case fails. SKIP everything without paho-mqtt.
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

try:
    import paho.mqtt.client as mqtt
except ImportError:  # pragma: no cover
    print("SKIP  all cases: pip install paho-mqtt cryptography (or use tools/testlab/.venv)")
    sys.exit(0)

try:
    from cryptography.hazmat.primitives.serialization import Encoding, NoEncryption, PrivateFormat, pkcs12
except ImportError:  # pragma: no cover
    pkcs12 = None

RESULTS: list[tuple[str, str, str]] = []


def record(case: str, status: str, detail: str = "") -> None:
    RESULTS.append((case, status, detail))
    print(f"{status:<5} {case}{'  ' + detail if detail else ''}", flush=True)


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


# ---------------------------------------------------------------- broker and helper


class Lab:
    def __init__(self, home: Path):
        self.home = home
        self.tls = free_port()
        self.wss = free_port()
        self.proc: subprocess.Popen | None = None
        self.state: dict = {}

    def helper(self, *args: str) -> dict:
        exe = ROOT / "target" / "debug" / "examples" / "lab_home"
        out = subprocess.run([str(exe), str(self.home), *args], capture_output=True, text=True)
        if out.returncode != 0:
            raise RuntimeError(f"lab_home {' '.join(args)}: {out.stderr.strip()}")
        self.state = json.loads(out.stdout)
        return self.state

    def start(self) -> None:
        env = dict(os.environ, MQTTPANEL_HOME=str(self.home), MQTTPANEL_SECRETS="memory", MQTTPANEL_SECRETS_SEED=self.state["secretsSeed"])
        exe = ROOT / "target" / "debug" / "mqttpanel"
        self.proc = subprocess.Popen([str(exe), "broker"], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        threading.Thread(target=self._drain, daemon=True).start()
        deadline = time.time() + 10
        while time.time() < deadline:
            try:
                with socket.create_connection(("127.0.0.1", self.tls), timeout=0.5), socket.create_connection(("127.0.0.1", self.wss), timeout=0.5):
                    return
            except OSError:
                time.sleep(0.1)
        raise RuntimeError("mqttpanel broker did not open its listeners")

    def _drain(self) -> None:
        assert self.proc and self.proc.stdout
        for _ in self.proc.stdout:
            pass

    def stop(self) -> None:
        if self.proc and self.proc.poll() is None:
            self.proc.send_signal(signal.SIGINT)
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        self.proc = None

    def restart(self) -> None:
        self.stop()
        self.start()

    def file(self, name: str) -> str:
        return str(self.home / name)


# ---------------------------------------------------------------- Paho


class NameContext(ssl.SSLContext):
    """An SSL context that verifies `override` instead of the host connected to (hosts-file style)."""

    override: str | None = None

    def wrap_socket(self, sock, *args, server_hostname=None, **kwargs):  # type: ignore[override]
        return super().wrap_socket(sock, *args, server_hostname=self.override or server_hostname, **kwargs)


def context(cafile: str, cert: str | None = None, key: str | None = None, name: str | None = None) -> ssl.SSLContext:
    ctx = NameContext(ssl.PROTOCOL_TLS_CLIENT)
    ctx.override = name
    ctx.check_hostname = True
    ctx.verify_mode = ssl.CERT_REQUIRED
    ctx.load_verify_locations(cafile=cafile)
    if cert:
        ctx.load_cert_chain(certfile=cert, keyfile=key)
    return ctx


def round_trip(host: str, port: int, transport: str, v5: bool, ctx: ssl.SSLContext, client_id: str, timeout: float = 5.0) -> str | None:
    """Connects, subscribes with QoS 2, publishes QoS 0, 1 and 2 to itself; None when all three come back with their
    QoS, otherwise the reason."""
    proto = mqtt.MQTTv5 if v5 else mqtt.MQTTv311
    c = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2, client_id=client_id, protocol=proto, transport=transport)
    if transport == "websockets":
        c.ws_set_options(path="/mqtt")
    c.tls_set_context(ctx)
    got: dict[str, int] = {}
    connected = threading.Event()
    subscribed = threading.Event()
    reason: list[str] = []
    topic = f"ext/{client_id}"

    def on_connect(client, userdata, flags, rc, props=None):
        if rc == 0 or getattr(rc, "value", 1) == 0:
            client.subscribe(f"{topic}/#", qos=2)
            connected.set()
        else:
            reason.append(f"CONNACK {rc}")
            connected.set()

    def on_subscribe(client, userdata, mid, rcs, props=None):
        subscribed.set()

    def on_message(client, userdata, msg):
        got[msg.topic] = msg.qos

    c.on_connect = on_connect
    c.on_subscribe = on_subscribe
    c.on_message = on_message
    try:
        c.connect(host, port, keepalive=30)
    except (ssl.SSLError, ssl.CertificateError, OSError) as e:
        return f"refused: {type(e).__name__}: {e}"
    c.loop_start()
    try:
        if not connected.wait(timeout) or reason:
            return reason[0] if reason else "no CONNACK"
        if not subscribed.wait(timeout):
            return "no SUBACK"
        for q in (0, 1, 2):
            c.publish(f"{topic}/q{q}", f"payload {q}", qos=q)
        deadline = time.time() + timeout
        while time.time() < deadline and len(got) < 3:
            time.sleep(0.05)
        want = {f"{topic}/q{q}": q for q in (0, 1, 2)}
        return None if got == want else f"received {got}"
    finally:
        c.disconnect()
        c.loop_stop()


def refused(result: str | None) -> bool:
    return result is not None


def p12_to_pem(p12_path: str, passphrase: str, out_dir: Path) -> tuple[str, str]:
    """A device that takes PKCS#12: open the bundle with its passphrase, as the device would."""
    data = Path(p12_path).read_bytes()
    key, cert, _chain = pkcs12.load_key_and_certificates(data, passphrase.encode())
    cert_path = out_dir / (Path(p12_path).stem + "-from-p12.pem")
    key_path = out_dir / (Path(p12_path).stem + "-from-p12.key")
    cert_path.write_bytes(cert.public_bytes(Encoding.PEM))
    key_path.write_bytes(key.private_bytes(Encoding.PEM, PrivateFormat.PKCS8, NoEncryption()))
    return str(cert_path), str(key_path)


def resolves(name: str) -> bool:
    try:
        socket.getaddrinfo(name, None)
        return True
    except OSError:
        return False


# ---------------------------------------------------------------- matrix

TRANSPORTS = [("tls", "tcp"), ("wss", "websockets")]
VERSIONS = [("3.1.1", False), ("5", True)]


def t28(lab: Lab) -> None:
    ca = lab.file("CA.pem")
    auto = lab.state["autoSans"]
    names = [n for n in auto[3:] if not _is_ip(n)]
    lan = next((n for n in auto[3:] if _is_ip(n) and ":" not in n), None)
    targets: list[tuple[str, str, str | None]] = [("localhost", "localhost", None), ("127.0.0.1", "127.0.0.1", None)]
    for n in names:
        # connect by the name when it resolves, otherwise to 127.0.0.1 verifying the name
        targets.append((".local name" if n.endswith(".local") else "host name", n if resolves(n) else "127.0.0.1", n))
    if lan:
        targets.append(("LAN address", lan, None))
    else:
        record("T-28 LAN address", "SKIP", "no usable IPv4 interface address")
    for label, host, name in targets:
        for tl, tr in TRANSPORTS:
            for vl, v5 in VERSIONS:
                port = lab.tls if tl == "tls" else lab.wss
                r = round_trip(host, port, tr, v5, context(ca, name=name), f"t28-{tl}-{vl.replace('.', '')}-{len(RESULTS)}")
                record(f"T-28 {label} {tl.upper()} MQTT {vl} QoS 0/1/2", "PASS" if r is None else "FAIL", r or "")
    for tl, tr in TRANSPORTS:
        port = lab.tls if tl == "tls" else lab.wss
        r = round_trip("localhost", port, tr, True, context(lab.file("other-CA.pem")), f"t28-wrongca-{tl}")
        record(f"T-28 wrong CA refused {tl.upper()}", "PASS" if refused(r) else "FAIL", (r or "accepted")[:90])
        r = round_trip("127.0.0.1", port, tr, True, context(ca, name="not-this-broker.example"), f"t28-wrongname-{tl}")
        record(f"T-28 wrong host name refused {tl.upper()}", "PASS" if refused(r) else "FAIL", (r or "accepted")[:90])


def _is_ip(s: str) -> bool:
    try:
        socket.inet_pton(socket.AF_INET6 if ":" in s else socket.AF_INET, s)
        return True
    except OSError:
        return False


def t29(lab: Lab) -> None:
    lab.helper("require", "on")
    lab.helper("issue", "device-pem", "pem")
    lab.helper("issue", "device-p12", "p12", "p12-pass-31")
    lab.restart()
    ca = lab.file("CA.pem")
    pem = (lab.file("device-pem.pem"), lab.file("device-pem.key"))
    p12 = p12_to_pem(lab.file("device-p12.p12"), "p12-pass-31", lab.home) if pkcs12 else None
    for tl, tr in TRANSPORTS:
        port = lab.tls if tl == "tls" else lab.wss
        for vl, v5 in VERSIONS:
            r = round_trip("localhost", port, tr, v5, context(ca, *pem), f"t29-pem-{tl}-{vl.replace('.', '')}")
            record(f"T-29 PEM client certificate {tl.upper()} MQTT {vl}", "PASS" if r is None else "FAIL", r or "")
            if p12:
                r = round_trip("localhost", port, tr, v5, context(ca, *p12), f"t29-p12-{tl}-{vl.replace('.', '')}")
                record(f"T-29 PKCS#12 (passphrase) {tl.upper()} MQTT {vl}", "PASS" if r is None else "FAIL", r or "")
            else:
                record(f"T-29 PKCS#12 (passphrase) {tl.upper()} MQTT {vl}", "SKIP", "pip install cryptography")
        r = round_trip("localhost", port, tr, True, context(ca), f"t29-none-{tl}")
        record(f"T-29 no certificate refused {tl.upper()}", "PASS" if refused(r) else "FAIL", (r or "accepted")[:90])
    lab.helper("revoke", "device-pem")
    lab.restart()
    for tl, tr in TRANSPORTS:
        port = lab.tls if tl == "tls" else lab.wss
        r = round_trip("localhost", port, tr, True, context(ca, *pem), f"t29-revoked-{tl}")
        record(f"T-29 revoked certificate refused {tl.upper()}", "PASS" if refused(r) else "FAIL", (r or "accepted")[:90])
        if p12:
            r = round_trip("localhost", port, tr, True, context(ca, *p12), f"t29-other-{tl}")
            record(f"T-29 another device still accepted {tl.upper()}", "PASS" if r is None else "FAIL", r or "")
    record("T-29 open session dropped on revocation", "SKIP", "needs a live change; the CLI broker restarts (covered by T-29 in CI)")


def t30(lab: Lab) -> None:
    old_ca = lab.home / "CA-before-rotation.pem"
    shutil.copy(lab.file("CA.pem"), old_ca)
    lab.helper("issue", "device-old", "pem")
    lab.helper("rotate")
    lab.helper("issue", "device-new", "pem")
    lab.restart()
    ca = lab.file("CA.pem")
    old = (lab.file("device-old.pem"), lab.file("device-old.key"))
    new = (lab.file("device-new.pem"), lab.file("device-new.key"))
    for tl, tr in TRANSPORTS:
        port = lab.tls if tl == "tls" else lab.wss
        r = round_trip("localhost", port, tr, True, context(str(old_ca), *new), f"t30-oldca-{tl}")
        record(f"T-30 old CA.pem refused after rotation {tl.upper()}", "PASS" if refused(r) else "FAIL", (r or "accepted")[:90])
        r = round_trip("localhost", port, tr, True, context(ca, *old), f"t30-oldcert-{tl}")
        record(f"T-30 old client certificate refused {tl.upper()}", "PASS" if refused(r) else "FAIL", (r or "accepted")[:90])
        r = round_trip("localhost", port, tr, True, context(ca, *new), f"t30-new-{tl}")
        record(f"T-30 new certificate and re-exported CA.pem accepted {tl.upper()}", "PASS" if r is None else "FAIL", r or "")
    extra = "mqtt.plant.example"
    r = round_trip("127.0.0.1", lab.tls, "tcp", True, context(ca, *new, name=extra), "t30-extra-before")
    record("T-30 extra name refused before regeneration", "PASS" if refused(r) else "FAIL", (r or "accepted")[:90])
    lab.helper("regen", extra, "203.0.113.30")
    lab.restart()
    for tl, tr in TRANSPORTS:
        port = lab.tls if tl == "tls" else lab.wss
        for vl, v5 in VERSIONS:
            r = round_trip("127.0.0.1", port, tr, v5, context(ca, *new, name=extra), f"t30-extra-{tl}-{vl.replace('.', '')}")
            record(f"T-30 by the extra name {tl.upper()} MQTT {vl}", "PASS" if r is None else "FAIL", r or "")
        r = round_trip("localhost", port, tr, True, context(ca, *new), f"t30-local-{tl}")
        record(f"T-30 automatic names kept {tl.upper()}", "PASS" if r is None else "FAIL", r or "")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--keep", action="store_true", help="keep the temporary home and print its path")
    ap.add_argument("--no-build", action="store_true", help="use the binaries already in target/debug")
    a = ap.parse_args()
    if not a.no_build:
        subprocess.run(["cargo", "build", "-q", "-p", "mqttpanel-cli", "--bin", "mqttpanel"], cwd=ROOT, check=True)
        subprocess.run(["cargo", "build", "-q", "-p", "mqttpanel-broker", "--example", "lab_home"], cwd=ROOT, check=True)
    home = Path(tempfile.mkdtemp(prefix="mqttpanel-lab-"))
    lab = Lab(home)
    try:
        lab.helper("setup", str(lab.tls), str(lab.wss))
        lab.helper("other-ca")
        lab.start()
        t28(lab)
        t29(lab)
        t30(lab)
    finally:
        lab.stop()
        if a.keep:
            print(f"home kept: {home}")
        else:
            shutil.rmtree(home, ignore_errors=True)
    fails = [r for r in RESULTS if r[1] == "FAIL"]
    counts = {s: sum(1 for r in RESULTS if r[1] == s) for s in ("PASS", "FAIL", "SKIP")}
    print(f"\n{counts['PASS']} PASS, {counts['FAIL']} FAIL, {counts['SKIP']} SKIP")
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Lab: plain English prompts to Claude Code with MQTT Panel as its MCP server, checked by machine.

For every case of `cases.py` (and every attempt) the runner:
  1. generates the lab's certificates and starts Mosquitto in the mode the case needs (plain, WebSocket, TLS,
     TLS with a password, mutual TLS, self-signed, an expired or a wrongly named server certificate), seeds a few
     retained topics, starts a Paho subscriber on lab/# and, for the data cases, a Paho publisher;
  2. starts the debug app (`target/debug/mqtt-panel`) on a new temporary MQTTPANEL_HOME with MQTTPANEL_SECRETS=memory,
     the language set to English and the assistant acknowledgement accepted (except the case that tests its absence);
  3. writes a temporary MCP configuration (the app executable with the argument `mcp`, the same environment) and
     runs `claude -p "<prompt>"` with only MQTT Panel's tools allowed and no built-in tools;
  4. reads the app's state through its own MCP socket (list_connections, broker_status, list_charts) and runs the
     case's check (state, the tool calls Claude made, its answer, what Paho received, the Mosquitto log);
  5. on a failure, runs the same prompt once more from scratch.

Results go to tools/testlab/mcp_prompts/results/<timestamp>/ (git-ignored): per case and attempt the transcript
(Claude's stream-json output), the check result, the Mosquitto and app logs, plus REPORT.md. Passwords and
passphrases are replaced by ••• everywhere in the results.

Never touches the real app data folder, the keychain or the real assistant configurations.

Usage (from the repository root):
  tools/testlab/.venv/bin/python tools/testlab/mcp_prompts/run.py                 # every case
  tools/testlab/.venv/bin/python tools/testlab/mcp_prompts/run.py --case C07 C08  # some cases
Options: --no-build, --no-retry, --model <alias>, --keep (keep the temporary folders).
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import math
import os
import secrets as rnd
import shutil
import signal
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
LAB = HERE.parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(LAB))
sys.path.insert(0, str(HERE))

import paho.mqtt.client as mqtt  # noqa: E402
from cryptography import x509  # noqa: E402

from cases import CASES, Case  # noqa: E402
from labcerts import Lab, default_hosts, fingerprint  # noqa: E402
from mcp_client import McpClient, wait_ready  # noqa: E402
from mosquitto_broker import build_conf, find_bin, seed_messages, write_passwd  # noqa: E402

EXE = ROOT / "target" / "debug" / "mqtt-panel"
CLAUDE_TIMEOUT = 900


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def wait_port(port: int, timeout: float = 10) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.5):
                return
        except OSError:
            time.sleep(0.1)
    raise RuntimeError(f"nothing listens on {port}")


def word(n: int = 12) -> str:
    alphabet = "abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789"
    return "".join(rnd.choice(alphabet) for _ in range(n))


# ---------------------------------------------------------------- Mosquitto and Paho


class Broker:
    """Mosquitto in one of the lab modes, with the certificates for the case."""

    def __init__(self, case: Case, out: Path, vars: dict):
        self.lab = case.lab
        self.mode = self.lab["mode"]
        self.out = out
        self.vars = vars
        self.proc: subprocess.Popen | None = None
        self.log = out / "mosquitto.log"

    def start(self) -> None:
        mode, out, lab = self.mode, self.out, self.lab
        out.mkdir(parents=True, exist_ok=True)
        port = free_port()
        ws_port = free_port() if lab.get("ws") else None
        self.vars.update(port=str(port), ws_port=str(ws_port or ""))
        certs = Lab(out)
        if mode == "self-signed":
            cert = certs.make_self_signed_server(default_hosts())
            self.vars["fingerprint"] = fingerprint(cert)
        elif mode != "plain":
            certs.make_ca()
            server = lab.get("server")
            hosts = ["other.example"] if server == "mismatch" else default_hosts()
            certs.make_server(hosts, expired=server == "expired")
            certs.make_client("lab-client", p12_password=self.vars["p12_pass"] if lab.get("p12") else None)
            self.vars.update(ca=str(out / "ca.pem"), cert=str(out / "client.pem"), key=str(out / "client-key.pem"),
                             p12=str(out / "client.p12"))
        if mode == "tls-password":
            write_passwd(find_bin("mosquitto_passwd"), out / "passwd", self.vars["user"], self.vars["password"])
        conf = out / "mosquitto.conf"
        conf.write_text(build_conf(mode, port, out, ws_port))
        self.proc = subprocess.Popen([find_bin("mosquitto"), "-c", str(conf)], stdout=self.log.open("w"),
                                     stderr=subprocess.STDOUT)
        wait_port(port)
        if ws_port:
            wait_port(ws_port)
        if lab.get("seed"):
            user = self.vars["user"] if mode == "tls-password" else None
            pw = self.vars["password"] if mode == "tls-password" else None
            seed_messages("localhost", port, mode, out, user, pw, None)

    def client(self, client_id: str) -> mqtt.Client:
        """A Paho client set up for this broker (TLS files, user)."""
        c = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2, client_id=client_id, protocol=mqtt.MQTTv5)
        if self.mode != "plain":
            ctx = ssl.create_default_context()
            ctx.load_verify_locations(str(self.out / ("server.pem" if self.mode == "self-signed" else "ca.pem")))
            if self.mode == "mtls":
                ctx.load_cert_chain(str(self.out / "client.pem"), str(self.out / "client-key.pem"))
            c.tls_set_context(ctx)
        if self.mode == "tls-password":
            c.username_pw_set(self.vars["user"], self.vars["password"])
        return c

    def stop(self) -> str:
        if self.proc and self.proc.poll() is None:
            self.proc.send_signal(signal.SIGINT)
            try:
                self.proc.wait(5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        return self.log.read_text(errors="replace") if self.log.exists() else ""


class Subscriber:
    """Paho on lab/#: what the assistant published must arrive here."""

    def __init__(self, broker: Broker):
        self.received: list[tuple[str, str]] = []
        self.ok = True
        try:
            self.c = broker.client("lab-sub")
            self.c.on_message = lambda _c, _u, m: self.received.append((m.topic, m.payload.decode(errors="replace")))
            self.c.on_connect = lambda c, *_: c.subscribe("lab/#", qos=1)
            self.c.connect("localhost", int(broker.vars["port"]), keepalive=30)
            self.c.loop_start()
        except (OSError, ssl.SSLError):
            self.ok = False  # expired or misnamed server certificate: nothing to receive

    def stop(self) -> None:
        if self.ok:
            self.c.loop_stop()
            self.c.disconnect()


class Traffic(threading.Thread):
    """Paho publisher for the data cases."""

    PLANS = {
        "busy": [("lab/c13/plant/boiler", 0.1), ("lab/c13/plant/office", 2.0), ("lab/c13/plant/garage", 5.0),
                 ("lab/c13/status", 5.0)],
        "meter": [("lab/c14/meter", 0.2)],
    }

    def __init__(self, broker: Broker, plan: str):
        super().__init__(daemon=True)
        self.c = broker.client("lab-traffic")
        self.c.connect("localhost", int(broker.vars["port"]), keepalive=30)
        self.c.loop_start()
        self.plan = self.PLANS[plan]
        self.halt = threading.Event()

    def run(self) -> None:
        nxt = {t: 0.0 for t, _ in self.plan}
        n = 0
        while not self.halt.is_set():
            now = time.time()
            for topic, every in self.plan:
                if now >= nxt[topic]:
                    nxt[topic] = now + every
                    n += 1
                    if topic.endswith("status"):
                        payload = "ok"
                    elif topic == "lab/c14/meter":
                        payload = json.dumps({"power_w": round(1200 + 300 * math.sin(n / 7), 1),
                                              "total_kwh": round(8842 + n / 1000, 3)})
                    else:
                        base = {"boiler": 64.0, "office": 21.0, "garage": 12.0}[topic.rsplit("/", 1)[1]]
                        payload = json.dumps({"temperature": round(base + math.sin(n / 9), 2), "humidity": 40 + n % 7})
                    self.c.publish(topic, payload, qos=0)
            self.halt.wait(0.02)

    def stop(self) -> None:
        self.halt.set()
        self.c.loop_stop()
        self.c.disconnect()


class Paho:
    """External-client helpers for the built-in broker checks."""

    @staticmethod
    def exists(path: str) -> bool:
        return Path(path).is_file()

    @staticmethod
    def csv_rows(path: str) -> int:
        p = Path(path)
        return len([ln for ln in p.read_text(errors="replace").splitlines() if ln.strip()]) if p.is_file() else 0

    @staticmethod
    def probe_required(port: int, ca: str) -> tuple[bool, bool, str]:
        """(server verified with CA.pem, refused without a client certificate, detail)."""
        ctx = ssl.create_default_context(cafile=ca)
        detail = ""
        verified = True
        try:
            with socket.create_connection(("localhost", port), timeout=5) as s, \
                    ctx.wrap_socket(s, server_hostname="localhost") as tls:
                tls.settimeout(3)
                try:
                    tls.recv(1)
                except (ssl.SSLError, OSError, TimeoutError) as e:
                    detail = f"after the handshake: {type(e).__name__}"
        except ssl.SSLCertVerificationError as e:
            verified, detail = False, f"verification failed: {e.verify_message}"
        except (ssl.SSLError, OSError) as e:
            detail = f"handshake refused: {getattr(e, 'reason', None) or e}"
        c = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2, client_id="lab-nocert", protocol=mqtt.MQTTv5)
        c.tls_set_context(ssl.create_default_context(cafile=ca))
        done = threading.Event()
        res: dict = {}
        c.on_connect = lambda _c, _u, _f, rc, _p=None: (res.update(rc=str(rc)), done.set())
        try:
            c.connect("localhost", port, keepalive=10)
            c.loop_start()
            done.wait(5)
            c.loop_stop()
            refused = res.get("rc") != "Success"
        except (ssl.SSLError, OSError) as e:
            refused = True
            res["rc"] = type(e).__name__
        return verified, refused, f"{detail}; MQTT without a certificate: {res.get('rc', 'no answer')}"

    @staticmethod
    def publish_tls(port: int, ca: str, user: str, password: str, topic: str, payload: str) -> tuple[bool, str]:
        c = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2, client_id="lab-sensor", protocol=mqtt.MQTTv5)
        c.tls_set_context(ssl.create_default_context(cafile=ca))
        c.username_pw_set(user, password)
        done = threading.Event()
        res: dict = {}
        c.on_connect = lambda _c, _u, _f, rc, _p=None: (res.update(rc=str(rc)), done.set())
        try:
            c.connect("localhost", port, keepalive=10)
            c.loop_start()
            done.wait(5)
            if res.get("rc") != "Success":
                return False, f"connect: {res.get('rc', 'no answer')}"
            c.publish(topic, payload, qos=1, retain=False).wait_for_publish(5)
            c.disconnect()
            c.loop_stop()
            return True, "ok"
        except (ssl.SSLError, OSError) as e:
            return False, f"{type(e).__name__}: {e}"

    @staticmethod
    def wait_topic(client: McpClient, conn_id: str, topic: str):
        last = ("", {}, True)
        for _ in range(10):
            last = client.call("list_topics", {"connection": conn_id, "filter": topic})
            if topic in str(last[1]):
                return last
            time.sleep(0.5)
        return last


# ---------------------------------------------------------------- one attempt


@dataclass
class Ctx:
    vars: dict
    tools: list = field(default_factory=list)
    final: str = ""
    received: list = field(default_factory=list)
    mosq_log: str = ""
    state: dict = field(default_factory=dict)
    mcp: McpClient | None = None
    paho: type = Paho


def clean_env() -> dict:
    return {k: v for k, v in os.environ.items() if not k.startswith("CLAUDE") and k != "AI_AGENT"}


def parse_stream(path: Path) -> tuple[list[dict], str, dict]:
    """Tool calls (name, input, result, error), the final answer and the result line of a stream-json transcript."""
    calls: dict[str, dict] = {}
    order: list[str] = []
    final, result = "", {}
    for line in path.read_text(errors="replace").splitlines():
        try:
            m = json.loads(line)
        except ValueError:
            continue
        t = m.get("type")
        if t == "assistant":
            for c in m.get("message", {}).get("content", []):
                if c.get("type") == "tool_use":
                    calls[c["id"]] = {"name": c["name"].removeprefix("mcp__mqttpanel__"), "input": c.get("input", {}),
                                      "result": "", "error": False}
                    order.append(c["id"])
                elif c.get("type") == "text":
                    final = c.get("text", "")
        elif t == "user":
            content = m.get("message", {}).get("content", [])
            for c in content if isinstance(content, list) else []:
                if c.get("type") == "tool_result" and c.get("tool_use_id") in calls:
                    body = c.get("content")
                    text = body if isinstance(body, str) else "\n".join(x.get("text", "") for x in body or [])
                    calls[c["tool_use_id"]].update(result=text, error=bool(c.get("is_error")))
        elif t == "result":
            result = m
            final = m.get("result") or final
    return [calls[i] for i in order], final, result


def redact(text: str, secrets: list[str]) -> str:
    for s in secrets:
        if s:
            text = text.replace(s, "•••")
    return text


def attempt(case: Case, out: Path, args) -> dict:
    tmp = Path(tempfile.mkdtemp(prefix=f"mqp-{case.id.lower()}-"))
    home, labdir, folder = tmp / "home", tmp / "lab", tmp / "exports"
    for d in (home, labdir, folder):
        d.mkdir(parents=True)
    vars = {"user": "lab", "password": word(), "wrong_password": word(), "p12_pass": word(10),
            "folder": str(folder), "file": str(folder / "meter.csv")}
    secrets = [vars[k] for k in ("password", "wrong_password", "p12_pass")]
    broker = Broker(case, labdir, vars) if case.lab.get("mode") else None
    sub = traffic = app = client = None
    ctx = Ctx(vars=vars)
    record: dict = {"case": case.id, "started": dt.datetime.now().isoformat(timespec="seconds")}
    try:
        if broker:
            broker.start()
            sub = Subscriber(broker)
            if case.lab.get("traffic"):
                traffic = Traffic(broker, case.lab["traffic"])
                traffic.start()
        settings = {"language": "en"}
        if case.lab.get("ack", True):
            settings["assistantAcknowledged"] = dt.date.today().isoformat()
        (home / "settings.json").write_text(json.dumps(settings))
        env = dict(clean_env(), MQTTPANEL_HOME=str(home), MQTTPANEL_SECRETS="memory", LANG="en_US.UTF-8",
                   LC_ALL="en_US.UTF-8", MQTTPANEL_HOSTS_HOME=str(tmp))
        app = subprocess.Popen([str(EXE)], env=env, stdout=(out / "app.log").open("w"), stderr=subprocess.STDOUT)
        wait_ready(EXE, env)
        time.sleep(0.5)
        cfg = tmp / "mcp.json"
        cfg.write_text(json.dumps({"mcpServers": {"mqttpanel": {
            "type": "stdio", "command": str(EXE), "args": ["mcp"],
            "env": {"MQTTPANEL_HOME": str(home), "MQTTPANEL_SECRETS": "memory", "LANG": "en_US.UTF-8",
                    "LC_ALL": "en_US.UTF-8", "MQTTPANEL_APP": str(EXE)}}}}))
        prompt = case.prompt.format(**vars)
        record["prompt"] = redact(prompt, secrets)
        cmd = ["claude", "-p", prompt, "--mcp-config", str(cfg), "--strict-mcp-config",
               "--allowedTools", "mcp__mqttpanel__*", "--tools", "", "--setting-sources", "local",
               "--output-format", "stream-json", "--verbose", "--max-turns", "25", "--no-session-persistence"]
        if args.model:
            cmd += ["--model", args.model]
        raw = tmp / "transcript.jsonl"
        t0 = time.time()
        try:
            subprocess.run(cmd, cwd=tmp, env=clean_env(), stdin=subprocess.DEVNULL, stdout=raw.open("w"),
                           stderr=(tmp / "claude.err").open("w"), timeout=CLAUDE_TIMEOUT)
        except subprocess.TimeoutExpired:
            record["timeout"] = True
        record["seconds"] = round(time.time() - t0)
        ctx.tools, ctx.final, result = parse_stream(raw)
        record.update(turns=result.get("num_turns"), cost=result.get("total_cost_usd"), stop=result.get("subtype"))
        (out / "transcript.jsonl").write_text(redact(raw.read_text(errors="replace"), secrets))
        time.sleep(1.5)
        if sub:
            ctx.received = list(sub.received)
        client = McpClient(EXE, dict(env, MQTTPANEL_APP="/usr/bin/false"))
        ctx.mcp = client
        for tool, key in (("list_connections", "connections"), ("broker_status", "broker"), ("list_charts", "charts")):
            _, data, _ = client.call(tool)
            ctx.state[key] = data.get(key, []) if key == "connections" else data
        if broker:
            ctx.mosq_log = broker.log.read_text(errors="replace")
        checks = case.check(ctx)
    except Exception as e:  # noqa: BLE001
        checks = [(False, f"harness error: {type(e).__name__}: {e}")]
    finally:
        for stop in (lambda: client and client.close(), lambda: traffic and traffic.stop(), lambda: sub and sub.stop()):
            try:
                stop()
            except Exception:  # noqa: BLE001
                pass
        if app and app.poll() is None:
            app.terminate()
            try:
                app.wait(5)
            except subprocess.TimeoutExpired:
                app.kill()
        if broker:
            broker.stop()
            shutil.copy(broker.log, out / "mosquitto.log")
        if not args.keep:
            shutil.rmtree(tmp, ignore_errors=True)
        else:
            record["tmp"] = str(tmp)
    record.update(
        passed=all(ok for ok, _ in checks),
        checks=[{"ok": ok, "detail": redact(d, secrets)} for ok, d in checks],
        tools=[{"name": t["name"], "input": json.loads(redact(json.dumps(t["input"]), secrets)), "error": t["error"]}
               for t in ctx.tools],
        final=redact(ctx.final, secrets),
        received=[[t, redact(p, secrets)] for t, p in ctx.received if not t.startswith(("lab/c13", "lab/c14"))][:20],
    )
    (out / "verify.json").write_text(json.dumps(record, indent=2, ensure_ascii=False))
    return record


# ---------------------------------------------------------------- report


def cell(s: str) -> str:
    return s.replace("|", "\\|").replace("\n", " ")


def report(dirpath: Path, rows: list[tuple[Case, list[dict]]], model: str) -> str:
    lines = [f"# MCP prompts lab report, {dirpath.name}", "",
             f"Host: Claude Code (`claude -p`, model {model}), MQTT Panel debug build, one fresh temporary home per "
             "attempt. Passwords and passphrases are shown as •••.", "",
             "| Case | Prompt | Result | Retries | Tool calls made | Notes |", "|---|---|---|---|---|---|"]
    for case, tries in rows:
        last = tries[-1]
        result = "PASS" if last["passed"] else "FAIL"
        names = ", ".join(t["name"] + (" (error)" if t["error"] else "") for t in last["tools"]) or "none"
        bad = [c["detail"] for c in last["checks"] if not c["ok"]]
        notes = "; ".join(bad) if bad else "all checks passed"
        if len(tries) > 1:
            first_bad = [c["detail"] for c in tries[0]["checks"] if not c["ok"]]
            notes += f". First attempt failed: {'; '.join(first_bad)}"
        lines.append(f"| {case.id} {cell(case.title)} | {cell(last.get('prompt', ''))} | {result} | {len(tries) - 1} | "
                     f"{cell(names)} | {cell(notes)} |")
    lines += ["", "## Answers", ""]
    for case, tries in rows:
        for i, t in enumerate(tries, 1):
            lines += [f"### {case.id}, attempt {i} ({'PASS' if t['passed'] else 'FAIL'}, {t.get('seconds')} s, "
                      f"{t.get('turns')} turns)", "", "> " + t.get("final", "").replace("\n", "\n> "), ""]
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--case", nargs="*", help="case ids (default: all)")
    ap.add_argument("--no-build", action="store_true")
    ap.add_argument("--no-retry", action="store_true")
    ap.add_argument("--model", help="Claude model alias (default: the account's default)")
    ap.add_argument("--keep", action="store_true", help="keep the temporary folders")
    args = ap.parse_args()
    if not args.no_build:
        subprocess.run(["cargo", "build", "-p", "mqttpanel-desktop"], cwd=ROOT, check=True)
    if not shutil.which("claude"):
        print("SKIP  claude (Claude Code) not found")
        return 0
    cases = [c for c in CASES if not args.case or c.id in args.case]
    stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
    base = HERE / "results" / stamp
    rows = []
    model = args.model or "default"
    for case in cases:
        tries = []
        for n in (1, 2):
            out = base / case.id / f"attempt{n}"
            out.mkdir(parents=True)
            r = attempt(case, out, args)
            tries.append(r)
            print(f"{'PASS' if r['passed'] else 'FAIL'}  {case.id} attempt {n}  {r.get('seconds')} s  "
                  f"{', '.join(t['name'] for t in r['tools'])}", flush=True)
            for c in r["checks"]:
                if not c["ok"]:
                    print(f"      - {c['detail']}", flush=True)
            if r["passed"] or args.no_retry:
                break
        rows.append((case, tries))
        if model == "default":
            try:
                for line in (base / case.id / "attempt1" / "transcript.jsonl").read_text().splitlines():
                    m = json.loads(line)
                    if m.get("type") == "system" and m.get("model"):
                        model = m["model"]
                        break
            except (OSError, ValueError):
                pass
        (base / "REPORT.md").write_text(report(base, rows, model))
    print(f"\nreport: {base / 'REPORT.md'}")
    return 0 if all(t[-1]["passed"] for _, t in rows) else 1


if __name__ == "__main__":
    sys.exit(main())

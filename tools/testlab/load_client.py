#!/usr/bin/env python3
"""Test lab B: official Eclipse Paho clients against MQTT Panel's embedded broker.

Connects a publisher and a subscriber to the broker, generates realistic fake data
(default 500 messages per second spread unevenly over 50 topics) and reports
published and received rates every few seconds. Use it to test throughput,
charts and export in the app, or to check that a listener you configured in the
app accepts a standard client.

Examples
  python3 load_client.py                                   # plain, localhost:1883
  python3 load_client.py --check                            # one pub/sub round trip, then exit
  python3 load_client.py --port 8883 --cafile ~/Downloads/CA.pem
  python3 load_client.py --port 8883 --cafile CA.pem --cert device.pem --key device-key.pem
  python3 load_client.py --port 8883 --cafile CA.pem --p12 device.p12 --p12-password '...'
  python3 load_client.py --user mqtt-panel --password '...' --rate 200 --topics 20 --duration 60

Export from the app: Broker > Broker certificates > Export CA.pem, and Broker >
Client certificates > Export (PEM or PKCS#12). Pass them with --cafile, --cert/--key or --p12.
TLS is switched on automatically when --cafile, --cert, --p12 or --insecure is given.
"""
from __future__ import annotations

import argparse
import json
import math
import os
import random
import signal
import ssl
import sys
import tempfile
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

try:
    import paho.mqtt.client as mqtt
    from paho.mqtt.properties import Properties
    from paho.mqtt.packettypes import PacketTypes
except ImportError:
    raise SystemExit("pip install paho-mqtt cryptography   (or use tools/testlab/.venv)")


# ---------------------------------------------------------------------------
# Data generator: 50 topics of several kinds, Zipf-like traffic split
# ---------------------------------------------------------------------------
ROOMS = ["hall", "office", "lab", "workshop", "server-room", "kitchen", "storage", "roof"]
MODES = ["heat", "cool", "auto", "off"]
STATUS = ["ok", "ok", "ok", "warning", "ok", "error"]


class Topic:
    def __init__(self, idx: int, prefix: str, rng: random.Random):
        self.idx = idx
        self.rng = rng
        kind = idx % 6
        n = idx // 6 + 1
        self.seq = 0
        self.qos = 0
        self.retain = False
        self.walk = rng.uniform(40, 60)
        self.counter = rng.uniform(1000, 9000)
        self.phase = rng.uniform(0, 2 * math.pi)
        if kind == 0:        # JSON climate: sine temperature, random-walk humidity, bool, text
            self.topic = f"{prefix}/sensors/{ROOMS[n % len(ROOMS)]}-{n}/climate"
            self.kind = "climate"
        elif kind == 1:      # JSON energy: sawtooth power, counter
            self.topic = f"{prefix}/meters/{n}/energy"
            self.kind = "energy"
            self.qos = 1
        elif kind == 2:      # plain number: random walk
            self.topic = f"{prefix}/machines/{n}/speed"
            self.kind = "number"
        elif kind == 3:      # boolean-like text: on/off (castable to 0/1 step)
            self.topic = f"{prefix}/relays/{n}/state"
            self.kind = "onoff"
        elif kind == 4:      # text status: not plottable
            self.topic = f"{prefix}/devices/{n}/status"
            self.kind = "status"
        else:                # nested JSON with a counter and a string, retained
            self.topic = f"{prefix}/devices/{n}/telemetry"
            self.kind = "telemetry"
            self.retain = True

    def payload(self, t: float) -> str:
        self.seq += 1
        r = self.rng
        if self.kind == "climate":
            temp = 21 + 3 * math.sin(t / 60 + self.phase) + r.gauss(0, 0.15)
            self.walk = min(80, max(20, self.walk + r.gauss(0, 0.5)))
            return json.dumps({"temperature": round(temp, 2), "humidity": round(self.walk, 1),
                               "online": r.random() > 0.02, "mode": MODES[int(t / 30 + self.idx) % 4],
                               "seq": self.seq, "ts": round(t, 3)})
        if self.kind == "energy":
            power = 500 + 1500 * ((t / 20 + self.phase) % 1.0) + r.gauss(0, 20)
            self.counter += power / 3_600_000
            return json.dumps({"power_w": round(power, 1), "total_kwh": round(self.counter, 4),
                               "phase": "L%d" % (self.idx % 3 + 1), "seq": self.seq, "ts": round(t, 3)})
        if self.kind == "number":
            self.walk = max(0, self.walk + r.gauss(0, 2))
            return f"{self.walk:.1f}"
        if self.kind == "onoff":
            return "on" if math.sin(t / 15 + self.phase) > 0 else "off"
        if self.kind == "status":
            return r.choice(STATUS)
        return json.dumps({"device": {"id": f"dev-{self.idx:02d}", "firmware": "1.4.2",
                                      "uptime_s": int(t) % 100000, "rssi": -40 - int(30 * r.random())},
                           "counters": {"sent": self.seq, "errors": self.seq // 50},
                           "seq": self.seq, "ts": round(t, 3)})


def zipf_weights(n: int, s: float) -> list[float]:
    w = [1.0 / (k ** s) for k in range(1, n + 1)]
    total = sum(w)
    return [x / total for x in w]


# ---------------------------------------------------------------------------
# TLS and clients
# ---------------------------------------------------------------------------
def tls_context(a) -> ssl.SSLContext | None:
    if not (a.cafile or a.cert or a.p12 or a.insecure or a.tls):
        return None
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    ctx.minimum_version = ssl.TLSVersion.TLSv1_2
    if a.insecure:
        ctx.check_hostname = False
        ctx.verify_mode = ssl.CERT_NONE
    else:
        ctx.check_hostname = True
        ctx.verify_mode = ssl.CERT_REQUIRED
        if a.cafile:
            ctx.load_verify_locations(str(a.cafile))
        else:
            ctx.load_default_certs()
    cert, key = a.cert, a.key
    if a.p12:
        from labcerts import pkcs12_to_pem
        tmp = Path(tempfile.mkdtemp(prefix="mqttpanel-lab-"))
        cert, key, ca = pkcs12_to_pem(Path(a.p12), a.p12_password, tmp)
        if ca and not a.cafile and not a.insecure:
            ctx.load_verify_locations(str(ca))
    if cert:
        ctx.load_cert_chain(str(cert), str(key) if key else None)
    return ctx


def make_client(a, client_id: str, ctx: ssl.SSLContext | None):
    proto = mqtt.MQTTv311 if a.mqtt3 else mqtt.MQTTv5
    c = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2, client_id=client_id, protocol=proto,
                    transport="websockets" if a.ws else "tcp")
    if ctx is not None:
        c.tls_set_context(ctx)
    if a.user:
        c.username_pw_set(a.user, a.password)
    c.reconnect_delay_set(1, 5)
    c.max_queued_messages_set(50_000)
    c.max_inflight_messages_set(200)
    return c


def connect(c, a, name: str, subscribe: str | None = None) -> None:
    """Connects and waits for the CONNACK. With `subscribe`, the filter is (re)subscribed inside on_connect,
    so a subscriber comes back with its subscription after a broker outage (fixed 15 Oct 2026: before, the
    subscription was made once after connect and a reconnect left the subscriber silent)."""
    done = threading.Event()
    result = {}

    def on_connect(client, userdata, flags, rc, properties=None):
        result["rc"] = rc
        if subscribe and (rc == 0 or str(rc) in ("Success", "0")):
            client.subscribe(subscribe, qos=1)
            if done.is_set():
                print(f"{name}: reconnected, subscribed again to {subscribe}", flush=True)
        done.set()

    def on_disconnect(client, userdata, flags, rc, properties=None):
        if done.is_set():
            print(f"{name}: disconnected ({rc}); retrying", flush=True)

    c.on_connect = on_connect
    c.on_disconnect = on_disconnect
    try:
        c.connect(a.host, a.port, keepalive=30)
    except (OSError, ssl.SSLError) as e:
        hint = ""
        text = str(e).lower()
        if "certificate verify failed" in text or "self-signed" in text:
            hint = "\n  The server certificate is not trusted: pass the broker's CA with --cafile (or --insecure for a lab)."
        elif "decrypt error" in text or "unknown ca" in text or "bad certificate" in text:
            hint = "\n  The broker rejected the client certificate: it must be issued by the broker's own CA" \
                   " (Broker > Client certificates > Issue, then Export)."
        elif "hostname" in text or "ip address mismatch" in text:
            hint = "\n  The server certificate is not valid for this host name: use the name it was issued for."
        raise SystemExit(f"{name}: cannot connect to {a.host}:{a.port}: {e}{hint}")
    c.loop_start()
    if not done.wait(10):
        raise SystemExit(f"{name}: no CONNACK from {a.host}:{a.port} within 10 s")
    rc = result["rc"]
    if rc != 0 and str(rc) not in ("Success", "0"):
        raise SystemExit(f"{name}: broker refused the connection: {rc}")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--host", default="localhost")
    ap.add_argument("--port", type=int, help="default 1883, or 8883 with TLS")
    ap.add_argument("--ws", action="store_true", help="use WebSocket transport")
    ap.add_argument("--mqtt3", action="store_true", help="MQTT 3.1.1 instead of 5")
    ap.add_argument("--tls", action="store_true", help="TLS with the system trust store")
    ap.add_argument("--cafile", type=Path, help="CA.pem exported from the app's broker")
    ap.add_argument("--cert", type=Path, help="client certificate PEM exported from the app")
    ap.add_argument("--key", type=Path, help="client private key PEM")
    ap.add_argument("--p12", type=Path, help="client certificate as PKCS#12 (.p12/.pfx) instead of --cert/--key")
    ap.add_argument("--p12-password", help="passphrase of the PKCS#12 file")
    ap.add_argument("--insecure", action="store_true", help="skip server certificate checks (lab only)")
    ap.add_argument("--user")
    ap.add_argument("--password")
    ap.add_argument("--prefix", default="mqttpanel-lab", help="topic prefix (default mqttpanel-lab)")
    ap.add_argument("--rate", type=float, default=500, help="messages per second in total (default 500)")
    ap.add_argument("--topics", type=int, default=50, help="number of topics (default 50)")
    ap.add_argument("--skew", type=float, default=1.0, help="Zipf exponent; 0 = even split (default 1.0)")
    ap.add_argument("--duration", type=float, default=0, help="seconds, 0 = until Ctrl-C")
    ap.add_argument("--report", type=float, default=5, help="report interval in seconds")
    ap.add_argument("--no-sub", action="store_true", help="publisher only, no subscriber")
    ap.add_argument("--seed", type=int, default=1, help="random seed (default 1, reproducible)")
    ap.add_argument("--check", action="store_true", help="one pub/sub round trip, then exit 0 or 1")
    ap.add_argument("--clear-retained", action="store_true",
                    help="delete the retained messages this script leaves under the prefix, then exit")
    a = ap.parse_args()
    ctx = tls_context(a)
    if a.port is None:
        a.port = 8883 if ctx is not None else 1883
    if a.p12 and not a.p12_password:
        import getpass
        a.p12_password = getpass.getpass("PKCS#12 passphrase: ")
    if a.user and a.password is None:
        import getpass
        a.password = getpass.getpass(f"Password for {a.user}: ")

    rng = random.Random(a.seed)
    topics = [Topic(i, a.prefix, random.Random(a.seed * 1000 + i)) for i in range(a.topics)]
    weights = zipf_weights(a.topics, a.skew)
    rng.shuffle(weights)  # so the heavy topics are not simply the first ones

    stats = {"pub": 0, "recv": 0, "acked": 0, "lat_sum": 0.0, "lat_n": 0, "errors": 0}
    lock = threading.Lock()
    stop = threading.Event()

    # ---- clear retained ---------------------------------------------------
    if a.clear_retained:
        pub = make_client(a, f"lab-clear-{os.getpid()}", ctx)
        connect(pub, a, "clear")
        for t in topics:
            if t.retain:
                pub.publish(t.topic, b"", qos=1, retain=True).wait_for_publish(5)
        pub.loop_stop(); pub.disconnect()
        print(f"cleared {sum(1 for t in topics if t.retain)} retained topics under {a.prefix}/")
        return 0

    # ---- subscriber -------------------------------------------------------
    sub = None
    if not a.no_sub or a.check:
        sub = make_client(a, f"lab-sub-{os.getpid()}", ctx)

        def on_message(client, userdata, msg):
            now = time.time()
            with lock:
                stats["recv"] += 1
                if msg.payload[:1] == b"{":
                    try:
                        ts = json.loads(msg.payload).get("ts")
                        if ts:
                            stats["lat_sum"] += now - ts
                            stats["lat_n"] += 1
                    except ValueError:
                        pass
        sub.on_message = on_message
        connect(sub, a, "subscriber", subscribe=f"{a.prefix}/#")
        time.sleep(0.3)

    # ---- publisher --------------------------------------------------------
    pub = make_client(a, f"lab-pub-{os.getpid()}", ctx)

    def on_publish(client, userdata, mid, rc=None, properties=None):
        with lock:
            stats["acked"] += 1
    pub.on_publish = on_publish
    connect(pub, a, "publisher")
    ver = "3.1.1" if a.mqtt3 else "5"
    print(f"connected to {a.host}:{a.port} ({'TLS' if ctx else 'plain'}, MQTT {ver}"
          + (f", user {a.user}" if a.user else "") + ")")

    if a.check:
        t0 = time.time()
        pub.publish(f"{a.prefix}/check", json.dumps({"ts": t0, "ok": True}), qos=1).wait_for_publish(5)
        deadline = time.time() + 5
        while time.time() < deadline:
            with lock:
                if stats["recv"] >= 1:
                    break
            time.sleep(0.05)
        ok = stats["recv"] >= 1
        print("round trip OK" if ok else "round trip FAILED: published but nothing received")
        pub.loop_stop(); pub.disconnect()
        if sub:
            sub.loop_stop(); sub.disconnect()
        return 0 if ok else 1

    # retained snapshots first
    for t in topics:
        if t.retain:
            pub.publish(t.topic, t.payload(time.time()), qos=1, retain=True)
            with lock:
                stats["pub"] += 1

    print(f"publishing {a.rate:g} msg/s over {a.topics} topics under {a.prefix}/ "
          f"(skew {a.skew:g}); Ctrl-C stops" + (f"; stops after {a.duration:g} s" if a.duration else ""))
    heavy = sorted(range(a.topics), key=lambda i: -weights[i])[:3]
    print("busiest topics: " + ", ".join(f"{topics[i].topic} ({weights[i] * a.rate:.0f}/s)" for i in heavy))

    def handle_sig(*_):
        stop.set()
    signal.signal(signal.SIGINT, handle_sig)
    signal.signal(signal.SIGTERM, handle_sig)

    tick = 0.01                       # scheduling granularity
    per_tick = a.rate * tick
    carry = 0.0
    start = time.perf_counter()
    next_tick = start
    last_report = start
    last = {"pub": 0, "recv": 0}
    try:
        while not stop.is_set():
            now = time.perf_counter()
            if a.duration and now - start >= a.duration:
                break
            carry += per_tick
            n = int(carry)
            carry -= n
            wall = time.time()
            for _ in range(n):
                t = rng.choices(topics, weights)[0]
                try:
                    info = pub.publish(t.topic, t.payload(wall), qos=t.qos, retain=False)
                    if info.rc != mqtt.MQTT_ERR_SUCCESS:
                        with lock:
                            stats["errors"] += 1
                    else:
                        with lock:
                            stats["pub"] += 1
                except (ValueError, OSError):
                    with lock:
                        stats["errors"] += 1
            if now - last_report >= a.report:
                with lock:
                    s = dict(stats)
                dt = now - last_report
                lat = f"{1000 * s['lat_sum'] / s['lat_n']:.0f} ms" if s["lat_n"] else "n/a"
                recv = f"{(s['recv'] - last['recv']) / dt:6.1f}/s" if sub else "   off "
                print(f"t={now - start:6.0f}s  pub {(s['pub'] - last['pub']) / dt:6.1f}/s  "
                      f"recv {recv}  total {s['pub']}/{s['recv']}  "
                      f"lag {s['pub'] - s['recv'] if sub else 0:5d}  latency {lat}"
                      + (f"  errors {s['errors']}" if s["errors"] else ""), flush=True)
                stats["lat_sum"] = 0.0; stats["lat_n"] = 0
                last = {"pub": s["pub"], "recv": s["recv"]}
                last_report = now
            next_tick += tick
            delay = next_tick - time.perf_counter()
            if delay > 0:
                time.sleep(delay)
            elif delay < -1.0:          # fell far behind (sleep or stall): resync instead of bursting
                next_tick = time.perf_counter()
    finally:
        time.sleep(0.5)
        with lock:
            s = dict(stats)
        print(f"\nstopped: published {s['pub']}, received {s['recv']}, acked {s['acked']}, errors {s['errors']}")
        if any(t.retain for t in topics):
            print(f"retained topics were left on the broker; remove them with --clear-retained")
        pub.loop_stop(); pub.disconnect()
        if sub:
            sub.loop_stop(); sub.disconnect()
    return 0


if __name__ == "__main__":
    sys.exit(main())

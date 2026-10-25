"""A small MCP client for the checks: JSON-RPC over stdio through `<app executable> mcp` (the same shim a host
starts). Used only after the assistant's session has ended, since the app serves one assistant at a time."""
from __future__ import annotations

import json
import os
import re
import subprocess
import threading
import time
from pathlib import Path


class McpClient:
    def __init__(self, exe: Path, env: dict[str, str], name: str = "lab-check"):
        self.proc = subprocess.Popen([str(exe), "mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.DEVNULL, env=env, text=True, bufsize=1)
        self.next_id = 1
        self.lines: list[str] = []
        self.cv = threading.Condition()
        threading.Thread(target=self._read, daemon=True).start()
        self.request("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                    "clientInfo": {"name": name, "version": "1"}}, timeout=15)
        self.notify("notifications/initialized")

    def _read(self) -> None:
        assert self.proc.stdout
        for line in self.proc.stdout:
            with self.cv:
                self.lines.append(line)
                self.cv.notify_all()

    def notify(self, method: str, params: dict | None = None) -> None:
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        assert self.proc.stdin
        self.proc.stdin.write(json.dumps(msg) + "\n")
        self.proc.stdin.flush()

    def request(self, method: str, params: dict, timeout: float = 40) -> dict:
        rid = self.next_id
        self.next_id += 1
        assert self.proc.stdin
        self.proc.stdin.write(json.dumps({"jsonrpc": "2.0", "id": rid, "method": method, "params": params}) + "\n")
        self.proc.stdin.flush()
        deadline = time.time() + timeout
        with self.cv:
            while True:
                for i, line in enumerate(self.lines):
                    try:
                        m = json.loads(line)
                    except ValueError:
                        continue
                    if m.get("id") == rid:
                        del self.lines[i]
                        if "error" in m:
                            raise RuntimeError(f"{method}: {m['error']}")
                        return m["result"]
                left = deadline - time.time()
                if left <= 0 or self.proc.poll() is not None:
                    raise TimeoutError(f"{method}: no answer")
                self.cv.wait(left)

    def call(self, tool: str, args: dict | None = None) -> tuple[str, dict, bool]:
        """Calls a tool; returns (sentences, JSON block or {}, is_error)."""
        r = self.request("tools/call", {"name": tool, "arguments": args or {}})
        texts = [c.get("text", "") for c in r.get("content", []) if c.get("type") == "text"]
        data: dict = {}
        for t in texts:
            m = re.search(r"```json\n(.*)\n```", t, re.S)
            if m:
                try:
                    data = json.loads(m.group(1))
                except ValueError:
                    pass
        words = "\n".join(t for t in texts if not t.startswith("```"))
        return words, data, bool(r.get("isError"))

    def close(self) -> None:
        try:
            assert self.proc.stdin
            self.proc.stdin.close()
            self.proc.wait(timeout=5)
        except Exception:  # noqa: BLE001
            self.proc.kill()


def wait_ready(exe: Path, env: dict[str, str], timeout: float = 30) -> None:
    """Waits until the app answers on its socket (the shim must not start a second app)."""
    probe_env = dict(env, MQTTPANEL_SHIM_WAIT_S="1", MQTTPANEL_APP="/usr/bin/false")
    deadline = time.time() + timeout
    last: Exception | None = None
    while time.time() < deadline:
        try:
            c = McpClient(exe, probe_env, name="lab-probe")
            c.close()
            return
        except Exception as e:  # noqa: BLE001
            last = e
            time.sleep(0.5)
    raise RuntimeError(f"the app did not open its assistant socket: {last}")

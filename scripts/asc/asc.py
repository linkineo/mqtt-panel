"""Tiny App Store Connect API client (standard library only). The bearer token comes from asc-jwt.swift next to this file."""
import json, subprocess, urllib.request, urllib.error, os, pathlib

def _load_env():
    """Reads KEY=VALUE lines from the repo's git-ignored .env.local into os.environ."""
    f = pathlib.Path(__file__).resolve().parents[2] / ".env.local"
    if f.exists():
        for line in f.read_text().splitlines():
            if "=" in line and not line.lstrip().startswith("#"):
                k, v = line.split("=", 1)
                os.environ.setdefault(k.strip(), v.strip().strip('"'))
_load_env()
KEY, ISSUER = os.environ["ASC_KEY_ID"], os.environ["ASC_ISSUER_ID"]
BASE = "https://api.appstoreconnect.apple.com"
_token = None

def token():
    global _token
    if not _token:
        _token = subprocess.check_output(["xcrun", "swift", str(pathlib.Path(__file__).with_name("asc-jwt.swift")), KEY, ISSUER], text=True).strip()
    return _token

def call(method, path, body=None):
    req = urllib.request.Request(BASE + path, method=method, data=json.dumps(body).encode() if body else None,
                                 headers={"Authorization": "Bearer " + token(), "Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req) as r:
            data = r.read()
            return json.loads(data) if data else {}
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"{method} {path} -> {e.code}: {e.read().decode()[:600]}")

get = lambda p: call("GET", p)
post = lambda p, b: call("POST", p, b)
patch = lambda p, b: call("PATCH", p, b)

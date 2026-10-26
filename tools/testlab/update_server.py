#!/usr/bin/env python3
"""Test lab U: a local update server for the self-update of the direct macOS download (docs/UPDATER.md).

Two commands:

  overlay   writes a Tauri config overlay with a throwaway public key and this server as the endpoint
  serve     answers every path under /update/ with a latest.json and serves the signed archive

End-to-end run (docs/UPDATER.md, Tests); debug bundles, since only debug builds read MQTTPANEL_UPDATE_ENDPOINT:

  cargo tauri signer generate --ci -p "" -w tools/testlab/out/update/test.key
  python3 tools/testlab/update_server.py overlay --pubkey tools/testlab/out/update/test.key.pub
  export TAURI_SIGNING_PRIVATE_KEY="$PWD/tools/testlab/out/update/test.key" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""   # absolute path
  # 1. the installed copy: the current version, copied to a folder you can write to
  cargo tauri build --debug --bundles app --features self-update \
    --config tools/testlab/out/update/tauri.updatetest.conf.json
  ditto "target/debug/bundle/macos/MQTT Panel.app" "tools/testlab/out/update/installed/MQTT Panel.app"
  # 2. the new version: same build with version 1.0.2; the bundler signs its archive with the throwaway key
  cargo tauri build --debug --bundles app --features self-update \
    --config tools/testlab/out/update/tauri.updatetest.conf.json --config '{"version":"1.0.2"}'
  python3 tools/testlab/update_server.py serve --version 1.0.2 \
    --archive "target/debug/bundle/macos/MQTT Panel.app.tar.gz"
  # 3. start the installed copy, Settings > Updates > Check now, Download and install, Restart now: About says 1.0.2
  open "tools/testlab/out/update/installed/MQTT Panel.app"

The signature is read from "<archive>.sig" unless --signature says otherwise. Modes: offer (default), none (204: no
update), fail (500: "Could not reach mqtt-panel.com"), bad-signature (a signature that does not match). Every request
is printed: the path carries the platform, the architecture and the version, nothing else. Ctrl-C stops the server.
Files go to tools/testlab/out/update/ (git-ignored). Release builds ignore MQTTPANEL_UPDATE_ENDPOINT.
"""
from __future__ import annotations

import argparse
import base64
import datetime
import http.server
import json
import pathlib
import sys

OUT = pathlib.Path(__file__).resolve().parent / "out" / "update"
ARCHIVE_NAME = "MQTT-Panel-macOS.app.tar.gz"


def write_overlay(pubkey_file: pathlib.Path, port: int) -> pathlib.Path:
    pubkey = pubkey_file.read_text().strip()
    OUT.mkdir(parents=True, exist_ok=True)
    overlay = {
        "bundle": {"createUpdaterArtifacts": True},
        "plugins": {
            "updater": {
                "endpoints": [f"http://127.0.0.1:{port}/update/{{{{target}}}}/{{{{arch}}}}/{{{{current_version}}}}"],
                "pubkey": pubkey,
                "requireSignedVersion": True,
            }
        },
    }
    path = OUT / "tauri.updatetest.conf.json"
    path.write_text(json.dumps(overlay, indent=2) + "\n")
    return path


def manifest(version: str, signature: str, port: int) -> dict:
    url = f"http://127.0.0.1:{port}/{ARCHIVE_NAME}"
    keys = [f"darwin-{arch}{kind}" for arch in ("aarch64", "x86_64") for kind in ("-app", "")]
    return {
        "version": version,
        "notes": f"Test lab build {version}",
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": {k: {"signature": signature, "url": url} for k in keys},
    }


def serve(args: argparse.Namespace) -> None:
    archive = pathlib.Path(args.archive)
    sig_path = pathlib.Path(args.signature) if args.signature else pathlib.Path(str(archive) + ".sig")
    if not archive.is_file() or not sig_path.is_file():
        sys.exit(f"archive or signature not found: {archive}, {sig_path}")
    signature = sig_path.read_text().strip()
    if args.mode == "bad-signature":
        # a well-formed signature of other bytes: the plugin must refuse the archive
        raw = bytearray(base64.b64decode(signature))
        raw[-8] ^= 0xFF
        signature = base64.b64encode(bytes(raw)).decode()
    body = json.dumps(manifest(args.version, signature, args.port), indent=2).encode()
    data = archive.read_bytes()

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, fmt: str, *a: object) -> None:
            print(f"[update-server] {self.command} {self.path} -> {a[1] if len(a) > 1 else ''}", flush=True)

        def do_GET(self) -> None:  # noqa: N802
            if self.path.startswith("/update/") or self.path == "/latest.json":
                if args.mode == "fail":
                    self.send_response(500)
                    self.end_headers()
                    return
                if args.mode == "none":
                    self.send_response(204)
                    self.end_headers()
                    return
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Cache-Control", "no-store")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            elif self.path == f"/{ARCHIVE_NAME}":
                self.send_response(200)
                self.send_header("Content-Type", "application/gzip")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
            else:
                self.send_response(404)
                self.end_headers()

    server = http.server.ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"update server on http://127.0.0.1:{args.port}/update/ (mode {args.mode}, version {args.version})")
    print(f"MQTTPANEL_UPDATE_ENDPOINT=http://127.0.0.1:{args.port}/update/{{{{target}}}}/{{{{arch}}}}/{{{{current_version}}}}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    o = sub.add_parser("overlay", help="write the Tauri config overlay with the throwaway public key")
    o.add_argument("--pubkey", required=True, help="the .pub file written by cargo tauri signer generate")
    o.add_argument("--port", type=int, default=8765)
    s = sub.add_parser("serve", help="serve latest.json and the signed archive")
    s.add_argument("--archive", required=True, help="the .app.tar.gz made by the bundler")
    s.add_argument("--signature", help="the .sig file (default: <archive>.sig)")
    s.add_argument("--version", required=True, help="the version announced, newer than the installed one")
    s.add_argument("--port", type=int, default=8765)
    s.add_argument("--mode", choices=["offer", "none", "fail", "bad-signature"], default="offer")
    args = p.parse_args()
    if args.cmd == "overlay":
        print(write_overlay(pathlib.Path(args.pubkey), args.port))
    else:
        serve(args)


if __name__ == "__main__":
    main()

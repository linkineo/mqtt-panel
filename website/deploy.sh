#!/bin/sh
# Builds the site and deploys it to https://mqtt-panel.com (Cloudflare Worker with static assets).
# WRANGLER: path to a wrangler binary (default: wrangler on PATH). See website/README.md.
set -eu
cd "$(dirname "$0")"
python3 build.py
"${WRANGLER:-wrangler}" deploy

# Website: https://mqtt-panel.com

A static single-page site in English (`/`), French (`/fr/`) and German (`/de/`), with a privacy page and a legal
page per language. It is served by one Cloudflare Worker with static assets (`wrangler.jsonc`); the Worker only
redirects `www.mqtt-panel.com` to the apex and adds security headers. No cookies, no trackers, no external
resources.

## Files

| File | Role |
|---|---|
| `build.py` | Generates `public/` from the texts in the file (EN, FR, DE). Fails if a forbidden word appears in the output. The three store and source-code links are constants at the top: empty means "coming soon". |
| `style.css`, `site.js` | Styles (light and dark) and the small script that plays the clips while visible, never under reduced motion. |
| `worker.js`, `wrangler.jsonc` | The Worker, its routes (`mqtt-panel.com`, `www.mqtt-panel.com` as custom domains) and the assets binding. |
| `third-party.json` | Licences of the shipped Rust crates and npm packages, shown on the legal page. Refresh with `python3 build.py --licences`. |
| `assets/media/` | Demo clips (WebM and MP4), posters and screenshots. |
| `encode-media.sh` | Turns the raw recordings into `assets/media/` (needs `ffmpeg`). |

## Build and preview

```
python3 website/build.py
python3 -m http.server -d website/public 8080
```

## Refresh the clips and screenshots

The clips are recorded from the real interface with the mock engine (light theme, English; the French and German
pages reuse them):

```
npm run dev                                   # UI on http://localhost:1420
cd packages/ui && node e2e/website-media.mjs  # writes website/.media-raw/ (git-ignored)
website/encode-media.sh                       # writes website/assets/media/
```

Each clip stays between 8 and 15 seconds and under about 2 MB.

## Deploy

`deploy.sh` builds the site and runs `wrangler deploy`. Set `WRANGLER` to a wrangler binary, for example the
`node_modules/.bin/wrangler` of another project or a global install (`npm install -g wrangler`):

```
WRANGLER=/path/to/wrangler website/deploy.sh
```

The zone `mqtt-panel.com` must be on the Cloudflare account that wrangler is logged in to (`wrangler whoami`), with
its nameservers pointing to Cloudflare. The first deploy creates the two custom domains and their DNS records.

# Website: https://mqtt-panel.com

A static single-page site in English (`/`), French (`/fr/`) and German (`/de/`), with a privacy page and a legal
page per language. It is served by one Cloudflare Worker with static assets (`wrangler.jsonc`); the Worker only
redirects `www.mqtt-panel.com` to the apex and adds security headers. No cookies, no trackers, no external
resources.

## Files

| File | Role |
|---|---|
| `build.py` | Generates `public/` from the texts in the file (EN, FR, DE). Fails if a forbidden word appears in the output. The three store and source-code links are constants at the top: empty means "coming soon". |
| `style.css`, `site.js`, `theme.js` | Styles (light or dark: the system one until the visitor picks one in the header), the live views, the tabs, the typing terminal, and `theme.js`, which applies a stored choice before the page paints. |
| `/app/` (built) | The app's own interface in demo mode (`npm run build:site -w @mqttpanel/ui`, copied from `packages/ui/dist-site` by `build.py`). The live views load it in frames from the same site, run a few setup steps (`SCENES` in `build.py`) and show it scaled, without interaction; the screenshots are the posters and the fallback (no script, reduced motion, screens under 700 px except the hero, off screen). |
| `worker.js`, `wrangler.jsonc` | The Worker, its routes (`mqtt-panel.com`, `www.mqtt-panel.com` as custom domains) and the assets binding. |
| `third-party.json` | Licences of the shipped Rust crates and npm packages, shown on the legal page. Refresh with `python3 build.py --licences`. |
| `assets/media/` | Screenshots per language and theme, `<id>.<lang>.<light|dark>.jpg`. |
| `encode-media.sh` | Turns the raw screenshots (and clips, when recorded) into `assets/media/` (needs `ffmpeg`). |

## Build and preview

```
npm run build:site -w @mqttpanel/ui
python3 website/build.py
python3 -m http.server -d website/public 8080
```

## Refresh the screenshots

The screenshots are taken from the real interface with the mock engine, in English, French and German, with the
demo data of each language, in the light and dark themes:

```
npm run dev                                   # UI on http://localhost:1420
cd packages/ui && node e2e/website-media.mjs  # writes website/.media-raw/ (git-ignored) and website/hero-areas.json; LANGS=en THEMES=light limit it
website/encode-media.sh                       # writes website/assets/media/
```

The page shows no video since 18 Oct 2026. The clip recordings still exist on request
(`node e2e/website-media.mjs clip-explorer`) but are not published.

## Preview protection

While the app is not released, the Worker asks for HTTP Basic authentication on every request (user name `preview`,
or the `SITE_USER` secret; password in the `SITE_PASSWORD` secret) and sends `X-Robots-Tag: noindex`. To make the
site public, delete the secret:

```
wrangler secret delete SITE_PASSWORD
```

## Deploy

`deploy.sh` builds the app interface for `/app/`, builds the site and runs `wrangler deploy`. Set `WRANGLER` to a wrangler binary, for example the
`node_modules/.bin/wrangler` of another project or a global install (`npm install -g wrangler`):

```
WRANGLER=/path/to/wrangler website/deploy.sh
```

The zone `mqtt-panel.com` must be on the Cloudflare account that wrangler is logged in to (`wrangler whoami`), with
its nameservers pointing to Cloudflare. The first deploy creates the two custom domains and their DNS records.

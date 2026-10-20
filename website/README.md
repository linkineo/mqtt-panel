# Website: https://mqtt-panel.com

A static single-page site in English (`/`), French (`/fr/`) and German (`/de/`), with a privacy page and a legal
page per language. It is served by one Cloudflare Worker with static assets (`wrangler.jsonc`); the Worker only
redirects `www.mqtt-panel.com` to the apex and adds security headers. No cookies, no trackers, no external
resources.

## Files

| File | Role |
|---|---|
| `build.py` | Generates `public/` from the texts in the file (EN, FR, DE). Fails if a forbidden word appears in the output. The download links are constants at the top (see Download section below). |
| `style.css`, `site.js`, `theme.js` | Styles (light or dark: the system one until the visitor picks one in the header), the live views, the tabs, the typing terminal, and `theme.js`, which applies a stored choice before the page paints. |
| `/app/` (built) | The app's own interface in demo mode (`npm run build:site -w @mqttpanel/ui`, copied from `packages/ui/dist-site` by `build.py`). The live views load it in frames from the same site, run a few setup steps (`SCENES` in `build.py`) and show it scaled, without interaction; the screenshots are the posters and the fallback (no script, reduced motion, screens under 700 px except the hero, off screen). |
| `worker.js`, `wrangler.jsonc` | The Worker, its routes (`mqtt-panel.com`, `www.mqtt-panel.com` as custom domains) and the assets binding. |
| `third-party.json` | Licences of the shipped Rust crates and npm packages, shown on the legal page. Refresh with `python3 build.py --licences`. |
| `assets/media/` | Screenshots per language and theme, `<id>.<lang>.<light|dark>.jpg`. |
| `assets/badges/` | The official store badges and the GitHub mark, unaltered (see Download section below). |
| `encode-media.sh` | Turns the raw screenshots (and clips, when recorded) into `assets/media/` (needs `ffmpeg`). |

## Build and preview

The build chain has two steps: `npm run build:site -w @mqttpanel/ui` builds the app interface in demo mode with the
base `/app/` into `packages/ui/dist-site/` (git-ignored; script in `packages/ui/package.json`), then `build.py`
copies it to `public/app/` next to the generated pages. `deploy.sh` runs both.

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
cd packages/ui && node e2e/website-media.mjs  # writes website/.media-raw/ (git-ignored); LANGS=en THEMES=light limit it
website/encode-media.sh                       # writes website/assets/media/
```

Pass ids to record only some screenshots, for example `node e2e/website-media.mjs shot-security shot-broker`.
`encode-media.sh` converts whatever is in `.media-raw/`, so empty it first to leave the other screenshots untouched.

The page shows no video since 18 Oct 2026. The clip recordings still exist on request
(`node e2e/website-media.mjs clip-explorer`) but are not published.

## Download section

The top bar has one Download button (slate `#56697A`, white text) that scrolls to the Download band at the bottom of
the home page (`#download`). The band holds the store badges, the GitHub button and three plain links to the
installers (Windows `.msi`, macOS `.dmg`, Linux `.deb`, `.rpm`, AppImage).

All six targets are constants at the top of `build.py`: `MAC_APP_STORE_URL`, `MICROSOFT_STORE_URL`, `GITHUB_URL`,
`WINDOWS_MSI_URL`, `MACOS_DMG_URL`, `LINUX_PACKAGES_URL`. While a constant is empty, its link is `href="#"` with
`aria-disabled="true"` and a "coming soon" caption (EN, FR, DE).

The badges stay black, as published: Apple and Microsoft do not allow recolouring. Badge files and sources:

| File | Source |
|---|---|
| `assets/badges/microsoft-store.svg` | Microsoft's badge page, https://apps.microsoft.com/badge, English, dark (the file the page links: `https://get.microsoft.com/images/en-us%20dark.svg`). Unaltered, shown 48 px high. |
| `assets/badges/github-mark-white.svg` | GitHub's logo download, https://github.com/logos (`GitHub_Logos.zip`, file `SVG/GitHub_Invertocat_White.svg`). Unaltered, 24 px high in the "View on GitHub" button. |
| `assets/badges/mac-app-store.svg` | **Not in the repository yet.** Apple's "Download on the Mac App Store" badge is only available from Apple's marketing resources after accepting Apple's licence, which the owner does. Save the official black badge, unaltered, at this path: the build then uses it (48 px high; Apple's minimum is 40 px). Until then the band shows a neutral black button with the words "Mac App Store" and no Apple logo. |

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

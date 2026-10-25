// Records the README demo clip from the real UI with the mock engine (1280x800, light, English).
// Usage (dev server on :1420, `npm run dev`): node e2e/readme-demo.mjs
// Output (git-ignored): docs/media/raw/readme-demo.webm and readme-demo.json (trim window in seconds).
// Encode (from docs/media, S and T from readme-demo.json: S = start, T = end - start):
//   ffmpeg -ss S -t T -i raw/readme-demo.webm -vf "fps=12,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" readme-demo.gif
//   ffmpeg -ss S -t T -i raw/readme-demo.webm -vf "scale=1280:-2,format=yuv420p" -c:v libx264 -preset slow -crf 26 -movflags +faststart -an readme-demo.mp4
import { chromium } from '@playwright/test';
import { mkdirSync, writeFileSync, renameSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const RAW = resolve(HERE, '../../../docs/media/raw');
const BASE = 'http://localhost:1420/';
const SIZE = { width: 1280, height: 800 };

// A visible pointer: Playwright videos do not show the system cursor.
const CURSOR = () => {
  addEventListener('DOMContentLoaded', () => {
    const c = document.createElement('div');
    c.style.cssText =
      'position:fixed;left:0;top:0;width:18px;height:18px;margin:-9px 0 0 -9px;border-radius:50%;background:rgba(86,105,122,.35);border:2px solid #56697A;z-index:2147483647;pointer-events:none;transition:transform .12s;transform:translate(-40px,-40px)';
    document.body.appendChild(c);
    let x = -40, y = -40;
    addEventListener('mousemove', (e) => { x = e.clientX; y = e.clientY; c.style.transform = `translate(${x}px,${y}px)`; }, true);
    addEventListener('mousedown', () => { c.style.transform = `translate(${x}px,${y}px) scale(.7)`; }, true);
    addEventListener('mouseup', () => { c.style.transform = `translate(${x}px,${y}px)`; }, true);
  });
};

const wait = (p, ms) => p.waitForTimeout(ms);
const row = (path) => `[data-testid=tree-row][data-path="${path}"]`;

async function go(p, sel, after = 650) {
  const loc = typeof sel === 'string' ? p.locator(sel).first() : sel;
  await loc.waitFor({ state: 'visible' });
  await loc.scrollIntoViewIfNeeded();
  const b = await loc.boundingBox();
  await p.mouse.move(b.x + Math.min(b.width / 2, 40), b.y + b.height / 2, { steps: 12 });
  await wait(p, 100);
  await p.mouse.down();
  await p.mouse.up();
  await wait(p, after);
}

// Connections in the bar, the topic tree with sparklines and a payload, the topic chart, then the Broker screen.
async function run(p) {
  await wait(p, 700);
  await go(p, '[data-profile=lab] [data-testid=card-play]', 1200);
  await go(p, '[data-testid=cbar-tab][data-conn=plant]', 1000);
  await go(p, row('building'), 450);
  await go(p, row('building/f2'), 450);
  await go(p, row('building/f2/r204'), 600);
  await go(p, row('building/f2/r204/temperature'), 1600);
  await go(p, row('building/f2/ahu01'), 500);
  await go(p, row('building/f2/ahu01/state'), 2400);
  await go(p, '[data-testid=rail-broker]', 2600);
}

mkdirSync(RAW, { recursive: true });
const dir = join(RAW, 'readme-demo.tmp');
rmSync(dir, { recursive: true, force: true });
const browser = await chromium.launch();
const ctx = await browser.newContext({ viewport: SIZE, recordVideo: { dir, size: SIZE }, colorScheme: 'light' });
const t0 = Date.now();
await ctx.addInitScript(CURSOR);
const p = await ctx.newPage();
await p.goto(`${BASE}?mock=demo&connect=plant,local&screen=connect&lang=en&theme=light`);
await p.waitForSelector('#app-root');
await wait(p, 1500);
const start = (Date.now() - t0) / 1000;
await run(p);
const end = (Date.now() - t0) / 1000;
const video = p.video();
await ctx.close();
renameSync(await video.path(), join(RAW, 'readme-demo.webm'));
rmSync(dir, { recursive: true, force: true });
writeFileSync(join(RAW, 'readme-demo.json'), JSON.stringify({ start: +start.toFixed(2), end: +end.toFixed(2) }) + '\n');
await browser.close();
console.log(`readme-demo: ${(end - start).toFixed(1)} s`);

// Records the website's demo clips and feature screenshots from the real UI with the mock engine.
// Usage (dev server on :1420, `npm run dev`): node e2e/website-media.mjs [clip-id ...]
// Output (git-ignored): website/.media-raw/<id>.webm (untrimmed Playwright recordings, with <id>.json
// holding the trim window) and website/.media-raw/<id>.png (2x screenshots). website/encode-media.sh then
// writes the published WebM/MP4 clips, posters and screenshots to website/assets/media/.
// Light theme, English; the FR and DE pages reuse the EN media.
import { chromium } from '@playwright/test';
import { mkdirSync, writeFileSync, renameSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const RAW = resolve(HERE, '../../../website/.media-raw');
const SHOTS = RAW;
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

/** Moves the pointer visibly to the element, then clicks it. */
async function go(p, sel, opts = {}) {
  const loc = typeof sel === 'string' ? p.locator(sel).first() : sel;
  await loc.waitFor({ state: 'visible' });
  await loc.scrollIntoViewIfNeeded();
  const b = await loc.boundingBox();
  const x = b.x + (opts.dx ?? Math.min(b.width / 2, 40));
  const y = b.y + b.height / 2;
  await p.mouse.move(x, y, { steps: 14 });
  await wait(p, 120);
  await p.mouse.down({ button: opts.button ?? 'left' });
  await p.mouse.up({ button: opts.button ?? 'left' });
  await wait(p, opts.after ?? 650);
}

async function typeSlow(p, sel, text) {
  await go(p, sel, { after: 200 });
  await p.keyboard.type(text, { delay: 55 });
}

const row = (path) => `[data-testid=tree-row][data-path="${path}"]`;

const CLIPS = {
  // 1. Several connections open: the connection bar's tabs and the profile cards.
  'clip-connections': {
    query: 'mock=demo&connect=plant,local&screen=connect',
    async run(p) {
      await wait(p, 900);
      await go(p, '[data-profile=lab] [data-testid=card-play]', { after: 1400 });
      await go(p, '[data-profile=local]', { after: 1100 });
      await go(p, '[data-testid=cbar-tab][data-conn=lab]', { after: 900 });
      await go(p, '[data-testid=rail-explore]', { after: 1300 });
      await go(p, '[data-testid=cbar-tab][data-conn=plant]', { after: 1300 });
      await go(p, '[data-testid=cbar-tab][data-conn=local]', { after: 1300 });
      await go(p, '[data-testid=rail-connect]', { after: 1200 });
    },
  },
  // 2. A broker with many topics: tree, payload, sparklines, history and the diff to the previous message.
  'clip-explorer': {
    query: 'mock=demo&connect=plant,local&screen=explore',
    async run(p) {
      await wait(p, 900);
      await go(p, row('building'), { after: 600 });
      await go(p, row('building/f2'), { after: 600 });
      await go(p, row('building/f2/r204'), { after: 900 });
      await go(p, row('building/f2/r204/temperature'), { after: 1800 });
      await go(p, '[data-testid=history-row] >> nth=1', { after: 1500 });
      await go(p, row('building/f2/ahu01'), { after: 800 });
      await go(p, row('building/f2/ahu01/state'), { after: 2600 });
    },
  },
  // 3. Charts: a topic chart under the payload, then the dashboard with several series.
  'clip-charts': {
    query: 'mock=demo&connect=plant,local&screen=explore',
    async run(p) {
      await p.fill('[data-testid=search]', 'building/f2/ahu01');
      await go(p, row('building/f2/ahu01/state'), { after: 900 });
      await go(p, '[data-testid=add-field]', { after: 700 });
      // the fields already on the topic chart stay ticked; tick the setpoint as a third series
      await go(p, p.locator('[data-testid=picker-row]', { hasText: 'setpoint' }), { after: 400 });
      await p.selectOption('[data-testid=picker-also]', 'new');
      await wait(p, 400);
      await go(p, '[data-testid=picker-add]', { after: 2600 });
      await go(p, '[data-testid=rail-charts]', { after: 3500 });
    },
  },
  // 4. The built-in broker: listeners, users, client certificates, a change applied live.
  'clip-broker': {
    query: 'mock=demo&screen=broker',
    async run(p) {
      await wait(p, 1200);
      await go(p, '[data-testid=user-pw-mqtt-panel-eye]', { after: 1200 });
      await go(p, '[data-testid=port-l-tls]', { after: 300 });
      await p.fill('[data-testid=port-input-l-tls]', '');
      await p.keyboard.type('8884', { delay: 90 });
      await p.keyboard.press('Enter');
      await wait(p, 1400);
      await go(p, '[data-testid=issue-cert]', { after: 500 });
      await p.keyboard.type('sensor-r206', { delay: 70 });
      await wait(p, 600);
      await go(p, '[data-testid=issue-ok]', { after: 2000 });
      await p.locator('[data-testid=client-certs]').scrollIntoViewIfNeeded();
      await wait(p, 1500);
    },
  },
};

const SHOT_STATES = {
  'shot-explorer': {
    query: 'mock=demo&connect=plant,local&screen=explore',
    async run(p) {
      await p.fill('[data-testid=search]', 'building/f2');
      await p.click(row('building/f2/ahu01/state'));
      await wait(p, 2500);
    },
  },
  'shot-charts': {
    query: 'mock=demo&connect=plant,local&screen=explore',
    async run(p) {
      await p.fill('[data-testid=search]', 'building/f2/ahu01');
      await p.click(row('building/f2/ahu01/state'));
      await p.click('[data-testid=add-field]');
      await p.locator('[data-testid=picker-row]', { hasText: 'setpoint' }).click();
      await p.selectOption('[data-testid=picker-also]', 'new');
      await p.click('[data-testid=picker-add]');
      await p.click('[data-testid=rail-charts]');
      await wait(p, 4000);
    },
  },
  'shot-publish': {
    query: 'mock=demo&connect=plant,local&screen=publish',
    async run(p) {
      await p.locator('[data-testid=template]').nth(2).click();
      await p.click('[data-testid=send]');
      await p.waitForSelector('[data-testid=sent-detail]');
      await wait(p, 1200);
    },
  },
  'shot-broker': {
    query: 'mock=demo&screen=broker',
    async run(p) {
      await wait(p, 1500);
    },
  },
  'shot-security': {
    query: 'mock=demo&connect=plant,local&screen=connect',
    async run(p) {
      await p.click('[data-testid=fold-security]');
      await wait(p, 800);
    },
  },
};

async function recordClip(browser, id, clip) {
  const dir = join(RAW, id + '.tmp');
  rmSync(dir, { recursive: true, force: true });
  const ctx = await browser.newContext({ viewport: SIZE, recordVideo: { dir, size: SIZE }, colorScheme: 'light' });
  const t0 = Date.now();
  await ctx.addInitScript(CURSOR);
  const p = await ctx.newPage();
  await p.goto(`${BASE}?${clip.query}&lang=en&theme=light`);
  await p.waitForSelector('#app-root');
  await wait(p, 1500);
  const start = (Date.now() - t0) / 1000;
  await clip.run(p);
  const end = (Date.now() - t0) / 1000;
  const video = p.video();
  await ctx.close();
  const src = await video.path();
  renameSync(src, join(RAW, id + '.webm'));
  rmSync(dir, { recursive: true, force: true });
  writeFileSync(join(RAW, id + '.json'), JSON.stringify({ start: +start.toFixed(2), end: +end.toFixed(2) }) + '\n');
  console.log(`${id}: ${(end - start).toFixed(1)} s`);
}

async function shoot(browser, id, s) {
  const ctx = await browser.newContext({ viewport: SIZE, deviceScaleFactor: 2, colorScheme: 'light' });
  const p = await ctx.newPage();
  await p.goto(`${BASE}?${s.query}&lang=en&theme=light`);
  await p.waitForSelector('#app-root');
  await wait(p, 1200);
  await s.run(p);
  await p.screenshot({ path: join(SHOTS, id + '.png') });
  await ctx.close();
  console.log(`${id}: ok`);
}

mkdirSync(RAW, { recursive: true });
mkdirSync(SHOTS, { recursive: true });
const only = process.argv.slice(2);
const browser = await chromium.launch();
for (const [id, c] of Object.entries(CLIPS)) if (!only.length || only.includes(id)) await recordClip(browser, id, c);
for (const [id, s] of Object.entries(SHOT_STATES)) if (!only.length || only.includes(id)) await shoot(browser, id, s);
await browser.close();

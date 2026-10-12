// Dev helper: node e2e/shot.mjs <url-query> <out.png> [clicks...]; clicks are CSS selectors (prefix "key:" for a key press, "type:sel=text").
import { chromium } from '@playwright/test';
const [, , q, out, ...steps] = process.argv;
const b = await chromium.launch();
const p = await b.newPage({ viewport: { width: 1280, height: 700 } });
const errs = [];
p.on('pageerror', (e) => errs.push(String(e)));
p.on('console', (m) => m.type() === 'error' && errs.push(m.text()));
await p.goto('http://localhost:1420/' + q);
await p.waitForTimeout(800);
for (const s of steps) {
  if (s.startsWith('key:')) await p.keyboard.press(s.slice(4));
  else if (s.startsWith('wait:')) await p.waitForTimeout(Number(s.slice(5)));
  else if (s.startsWith('type:')) { const i = s.indexOf('=>'); const sel = s.slice(5, i), txt = s.slice(i + 2); await p.fill(sel, txt); }
  else if (s.startsWith('right:')) await p.click(s.slice(6), { button: 'right' });
  else await p.click(s);
  await p.waitForTimeout(400);
}
await p.screenshot({ path: out });
if (errs.length) console.log('ERRORS:\n' + errs.join('\n'));
await b.close();

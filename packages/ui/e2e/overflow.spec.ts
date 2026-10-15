import { test, expect, type Page } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { STATES } from './states';

// V-02: every label, button, chip, segment item, helper text and table header stays on one line and is not clipped.
const SELECTORS = [
  'label',
  '.lbl',
  '.btn',
  '.chip',
  '.seg > *',
  '.hint',
  'th',
  '.sec',
  '.gridtbl .hr > div',
  '.rail button',
  '.fold .ttl',
  '.pnl .hd',
  '.dtabs button',
  '.stat .l',
  '.ctx-item',
  '.modal .mh',
  '.ph h5',
  '.f .row2:not(.wrap)',
  '.ph',
  '.meta',
  '.cbar .tab',
  '.card .state',
  '.lerr',
  '[data-testid=listener-row] td',
  // change set 8: tree rows never wrap (the payload is cut with an ellipsis), the quick publish strip
  '.tree .n',
  '.qp',
  // change set 9: the four figure cells of a tree row, the History filter list
  '.tree .n .v > *',
  '.hfilter-pop .hf-row',
].join(',');

async function violations(page: Page): Promise<string[]> {
  return page.evaluate((sel) => {
    const out: string[] = [];
    const els = Array.from(document.querySelectorAll<HTMLElement>(sel));
    for (const el of els) {
      if (el.closest('[data-overflow-ok], [data-testid=pinned-row], .json, .cm-editor')) continue;
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) continue;
      const style = getComputedStyle(el);
      if (style.visibility === 'hidden' || style.display === 'none') continue;
      // skip elements scrolled out of their scroll container
      const text = (el.innerText || el.getAttribute('aria-label') || '').trim().replace(/\s+/g, ' ').slice(0, 80);
      if (el.scrollWidth > el.clientWidth + 1) out.push(`clipped: "${text}" (${el.scrollWidth} > ${el.clientWidth}) <${el.className}>`);
      // single line box: cluster text rects by vertical centre
      const rects: DOMRect[] = [];
      const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
      for (let n = walker.nextNode(); n; n = walker.nextNode()) {
        if (!n.textContent || !n.textContent.trim()) continue;
        // badges placed over an icon (the rail's count bubble) are not a second line of text
        if (n.parentElement && getComputedStyle(n.parentElement).position === 'absolute' && n.parentElement !== el) continue;
        const range = document.createRange();
        range.selectNodeContents(n);
        rects.push(...Array.from(range.getClientRects()).filter((x) => x.width > 1 && x.height > 1));
      }
      const lh = parseFloat(style.lineHeight) || parseFloat(style.fontSize) * 1.4;
      const centres: number[] = [];
      for (const x of rects) {
        const c = x.top + x.height / 2;
        if (!centres.some((y) => Math.abs(y - c) < lh * 0.75)) centres.push(c);
      }
      if (centres.length > 1 && !el.matches('.ph, .f .row2, .meta')) out.push(`wraps: "${text}" (${centres.length} lines) <${el.className}>`);
    }
    // containers: no pane, card, dialog or form may scroll sideways (content wider than its column)
    const boxes = new Set<HTMLElement>(Array.from(document.querySelectorAll<HTMLElement>('.pc, .pane, .modal, .card, .form, .stat, .callout')));
    // plus every scroll box (table wrappers and the like): sideways scrolling is a layout bug, except for code and payload views
    for (const el of Array.from(document.querySelectorAll<HTMLElement>('body *'))) {
      const ox = getComputedStyle(el).overflowX;
      if (ox === 'auto' || ox === 'scroll') boxes.add(el);
    }
    for (const el of Array.from(boxes)) {
      if (el.closest('[data-overflow-ok], .json, .cm-editor, .uplot')) continue;
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) continue;
      if (el.scrollWidth > el.clientWidth + 1) {
        const label = (el.getAttribute('data-testid') || el.className || el.tagName).toString().slice(0, 60);
        out.push(`container too narrow: <${label}> (${el.scrollWidth} > ${el.clientWidth})`);
      }
    }
    return Array.from(new Set(out));
  }, SELECTORS);
}

for (const lang of ['en', 'de', 'fr'] as const) {
  test.describe(`overflow ${lang}`, () => {
    for (const s of STATES) {
      test(`${s.id} ${lang}`, async ({ page }) => {
        await page.goto(`/?${s.query}&lang=${lang}&theme=light`);
        await page.waitForSelector('#app-root');
        if (s.steps) await s.steps(page);
        await page.waitForTimeout(400);
        const v = await violations(page);
        if (lang !== 'en') {
          mkdirSync('e2e/screenshots', { recursive: true });
          await page.screenshot({ path: `e2e/screenshots/${s.id}-${lang}-light.png` });
        }
        expect(v, v.join('\n')).toEqual([]);
      });
    }
  });
}

test.describe('screenshots light and dark (V-01, EN)', () => {
  for (const theme of ['light', 'dark'] as const) {
    for (const s of STATES) {
      test(`${s.id} ${theme}`, async ({ page }) => {
        await page.goto(`/?${s.query}&lang=en&theme=${theme}`);
        await page.waitForSelector('#app-root');
        if (s.steps) await s.steps(page);
        await page.waitForTimeout(400);
        mkdirSync('e2e/screenshots', { recursive: true });
        await page.screenshot({ path: `e2e/screenshots/${s.id}-en-${theme}.png` });
      });
    }
  }
});

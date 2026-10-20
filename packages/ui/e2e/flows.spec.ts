import { test, expect, type Page } from '@playwright/test';

// End-to-end flows against the mock engine (TESTPLAN E-level tests for phases 2 to 4).
// The mock exposes itself as window.__mqttpanelMock in the browser build only.

async function open(page: Page, query: string) {
  await page.goto(`/?${query}&lang=en&theme=light`);
  await page.waitForSelector('#app-root');
}

async function selectTopic(page: Page, search: string, topic: string) {
  await page.fill('[data-testid=search]', search);
  await page.locator(`[data-testid=tree-row][data-path="${topic}"]`).click();
  await page.waitForSelector('[data-testid=detail]');
}

const copied = (page: Page) => page.evaluate(() => (window as any).__mqttpanelMock.platform.lastCopied() as string | null);
const saves = (page: Page) => page.evaluate(() => (window as any).__mqttpanelMock.engine.savedFiles() as { fileName: string; bytes: number }[]);
const inject = (page: Page, topic: string, payload: string, opts: Record<string, unknown> = {}) =>
  page.evaluate(([t, p, o]) => (window as any).__mqttpanelMock.engine.inject('plant', t, p, o), [topic, payload, opts] as const);

async function menuItem(page: Page, label: RegExp | string) {
  await page.waitForSelector('[data-testid=context-menu]');
  await page.locator('[data-testid=context-menu] .ctx-item', { hasText: label }).first().click();
}

test.describe('Connections', () => {
  test('C-07 "Test" reports latency, certificate and negotiated version', async ({ page }) => {
    await open(page, 'mock=demo');
    await page.click('[data-profile=plant]');
    await expect(page.locator('[data-testid=try-connect]')).toHaveText('Test');
    await page.click('[data-testid=try-connect]');
    const box = page.locator('[data-testid=last-test]');
    await expect(box).toContainText(/ms/, { timeout: 5000 });
    await expect(box).toContainText(/MQTT 5/);
    await expect(box).toContainText(/CN=/);
  });

  test('C-12 "New connection" is a menu; the first-launch cards route to the same places', async ({ page }) => {
    await open(page, 'mock=demo');
    await page.click('[data-testid=new-profile]');
    await expect(page.locator('[data-testid=context-menu] .ctx-label')).toHaveText(['New broker…', 'Built-in broker']);
    await page.locator('[data-testid=context-menu] .ctx-item', { hasText: 'Built-in broker' }).hover();
    await expect(page.locator('[data-testid=context-submenu] .ctx-label')).toHaveText(['Use an existing listener', 'Add a listener…']);
    await page.locator('[data-testid=context-menu] .ctx-item', { hasText: 'New broker…' }).click();
    // change set 6: named after its host (localhost) until a name is typed
    await expect(page.locator('[data-testid=profile-form] h5')).toHaveText('localhost');
    await expect(page.locator('[data-testid=profile-form]')).not.toContainText('Target');
    await expect(page.locator('[data-testid=target-app]')).toHaveCount(0);
    // keyboard: arrow right opens the submenu, Enter picks
    await page.click('[data-testid=new-profile]');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('ArrowRight');
    await expect(page.locator('[data-testid=context-submenu]')).toBeVisible();
    await page.keyboard.press('Enter');
    await expect(page.locator('[data-testid=app-broker]')).toBeVisible();
    await page.click('[data-testid=app-broker] .mf .btn >> nth=0');
    // first launch
    await open(page, 'mock=empty');
    await page.click('[data-testid=welcome-new]');
    await expect(page.locator('[data-testid=context-menu] .ctx-label')).toHaveText(['New broker…', 'Built-in broker']);
    await page.keyboard.press('Escape');
    await page.click('[data-testid=welcome-broker]');
    await expect(page.locator('[data-testid=app-broker]')).toBeVisible();
    await expect(page.locator('[data-testid=appb-listener]').first()).toHaveAttribute('aria-checked', 'true');
  });

  test('C-13 "Built-in broker" > "Add a listener…": no login row, then the steps run and the Explorer opens', async ({ page }) => {
    await open(page, 'mock=demo');
    await page.click('[data-testid=new-profile]');
    await page.locator('[data-testid=context-menu] .ctx-item', { hasText: 'Built-in broker' }).hover();
    await page.locator('[data-testid=context-submenu] .ctx-item', { hasText: 'Add a listener…' }).click();
    await page.click('[data-testid=preset-mtls]');
    await page.click('[data-testid=addl-next]');
    const step2 = page.locator('[data-testid=add-listener-2]');
    await expect(step2).not.toContainText('Login');
    await expect(step2.locator('input[type=password]')).toHaveCount(0);
    await expect(page.locator('[data-testid=addl-after]')).toContainText('starts the broker if it is stopped and connects');
    await expect(page.locator('[data-testid=addl-after]')).not.toContainText('restart');
    await page.fill('[data-testid=addl-port]', '8886');
    await page.click('[data-testid=addl-add]');
    await expect(page.locator('[data-testid=app-broker]')).toBeVisible();
    await expect(page.locator('[data-testid=appb-step-cert]')).toBeVisible();
    await expect(page.locator('[data-testid=explorer]')).toBeVisible({ timeout: 8000 });
    await expect(page.locator('[data-testid=rail-connect-count]')).toHaveText('1');
    await page.click('[data-testid=rail-connect]');
    await expect(page.locator('[data-testid=profile-card]', { hasText: 'Local broker' })).toHaveCount(1);
    await expect(page.locator('[data-profile=local]')).toContainText('localhost:8886');
    // with Anonymous on no user is created and the dialog says so
    await open(page, 'mock=empty&screen=broker');
    await page.click('[data-testid=anonymous]');
    await page.click('[data-testid=confirm-ok]');
    await page.click('[data-testid=rail-connect]');
    await page.click('[data-testid=welcome-broker]');
    await expect(page.locator('[data-testid=appb-what]')).toContainText('Anonymous is on');
    await page.click('[data-testid=appb-connect]');
    await expect(page.locator('[data-testid=explorer]')).toBeVisible({ timeout: 8000 });
    await page.click('[data-testid=rail-broker]');
    await expect(page.locator('[data-testid=user-row]')).toHaveCount(0);
  });

  test('C-05 / C-15 several connections open at once; the connection bar switches the active one', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    const tabs = page.locator('[data-testid=cbar-tab]');
    await expect(tabs).toHaveCount(1);
    await expect(tabs.first()).toContainText('Plant broker');
    await expect(tabs.first()).toContainText(/topics · [\d.,]+ msg\/s · MQTT 5/);
    // no title bar pill (change set 4): the rail's count bubble shows how many are open
    await expect(page.locator('[data-testid=pill]')).toHaveCount(0);
    await expect(page.locator('[data-testid=rail-connect-count]')).toHaveText('1');
    await page.click('[data-testid=rail-connect]');
    await page.click('[data-profile=emqx]');
    await page.click('[data-testid=connect]');
    await expect(page.locator('[data-testid=explorer]')).toBeVisible({ timeout: 5000 });
    await expect(tabs).toHaveCount(2);
    await expect(page.locator('[data-testid=rail-connect-count]')).toHaveText('2');
    await expect(page.locator('[data-testid=cbar-tab][aria-selected=true]')).toContainText('Cloud EMQX');
    // click a tab: Explorer on that connection, tab active
    await page.click('[data-testid=rail-publish]');
    await tabs.filter({ hasText: 'Plant broker' }).click();
    await expect(page.locator('[data-testid=explorer]')).toBeVisible();
    await expect(page.locator('[data-testid=cbar-tab][aria-selected=true]')).toContainText('Plant broker');
    // ⌘⇧L lists the open connections and opens the Explorer on the chosen one
    await page.click('[data-testid=rail-charts]');
    await page.keyboard.press('ControlOrMeta+Shift+L');
    await menuItem(page, /Cloud EMQX/);
    await expect(page.locator('[data-testid=explorer]')).toBeVisible();
    await expect(page.locator('[data-testid=cbar-tab][aria-selected=true]')).toContainText('Cloud EMQX');
    // ✕ disconnects; with the last open connection gone the bar stays, empty (change set 8)
    await page.locator('[data-testid=cbar-tab][aria-selected=true] [data-testid=cbar-close]').click();
    await expect(tabs).toHaveCount(1);
    await expect(page.locator('[data-testid=cbar-tab][aria-selected=true]')).toContainText('Plant broker');
    await page.locator('[data-testid=cbar-close]').click();
    await expect(page.locator('[data-testid=cbar]')).toHaveAttribute('data-empty', 'true');
    await expect(page.locator('[data-testid=rail-connect-count]')).toHaveCount(0);
  });

  test('C-14 connected card in the accent colour, card menu, Delete with confirmation', async ({ page }) => {
    await open(page, 'mock=demo&connect=local&screen=connect');
    const local = page.locator('[data-profile=local]');
    await expect(local).toHaveClass(/live/);
    await expect(local.locator('[data-testid=card-state]')).toHaveText('connected');
    const bg = await local.evaluate((el) => getComputedStyle(el).backgroundColor);
    const accent = await page.evaluate(() => getComputedStyle(document.querySelector('.btn.pri')!).backgroundColor);
    expect(bg).toBe(accent);
    await expect(local.locator('[data-testid=card-state]')).toHaveCSS('text-transform', 'uppercase');
    await page.evaluate(() => (window as any).__mqttpanelMock.engine.setConnectionState('local', 'reconnecting'));
    await expect(local).toHaveClass(/retry/);
    await expect(local.locator('[data-testid=card-state]')).toHaveText('reconnecting');
    await page.evaluate(() => (window as any).__mqttpanelMock.engine.setConnectionState('local', 'connected'));
    // card menu by state
    await local.click({ button: 'right' });
    await expect(page.locator('[data-testid=context-menu] .ctx-label')).toHaveText(['Disconnect', 'Test', 'Duplicate', 'Move up', 'Move down', 'Delete…']);
    await page.keyboard.press('Escape');
    await page.locator('[data-profile=lab]').click({ button: 'right' });
    await expect(page.locator('[data-testid=context-menu] .ctx-label').first()).toHaveText('Connect');
    await menuItem(page, 'Duplicate');
    await expect(page.locator('[data-testid=profile-card]')).toHaveCount(5);
    // header: Delete (outlined brick, normal size), Test, Connect; no Duplicate
    await page.click('[data-profile=plant]');
    await expect(page.locator('[data-testid=profile-form] .ph .row button')).toHaveText(['Delete', 'Test', 'Connect']);
    const del = page.locator('[data-testid=delete-profile-btn]');
    await expect(del).toHaveCSS('font-size', '13px');
    await expect(del).toHaveCSS('padding', '6px 13px');
    // deleting the connected profile from its menu disconnects it first
    await local.click({ button: 'right' });
    await menuItem(page, 'Delete…');
    await expect(page.locator('[data-testid=delete-profile] .mh')).toHaveText('Delete the connection "Local broker"?');
    await page.click('[data-testid=confirm-ok]');
    await expect(page.locator('[data-profile=local]')).toHaveCount(0);
    await expect(page.locator('[data-testid=cbar]')).toHaveAttribute('data-empty', 'true');
  });
});

test.describe('Explorer', () => {
  test('X-05 collapsed by default, "Expand to depth 2" opens two levels', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await page.waitForSelector('[data-testid=tree-row]');
    const before = await page.locator('[data-testid=tree-row]').count();
    await page.click('[data-testid=expand-menu]');
    await menuItem(page, /depth 2/i);
    await expect.poll(async () => page.locator('[data-testid=tree-row]').count()).toBeGreaterThan(before);
    const depths = await page.locator('[data-testid=tree-row]').evaluateAll((rows) => rows.map((r) => (r.getAttribute('data-path') ?? '').split('/').length));
    expect(Math.max(...depths)).toBeLessThanOrEqual(3);
  });

  test('X-06 table sorts by "Since connect" numerically and hides a column', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await page.click('[data-testid=layout-table]');
    await page.waitForSelector('[data-testid=table-row]');
    await page.click('[data-testid=col-messages]');
    await page.waitForTimeout(300);
    const first = async () =>
      page.locator('[data-testid=table-row]').evaluateAll((rows) =>
        rows.filter((r) => !r.textContent?.includes('★')).slice(0, 8).map((r) => Number((r.querySelectorAll(':scope > div')[6]?.textContent ?? '0').replace(/\D/g, ''))),
      );
    let v = await first();
    const sortedDesc = [...v].sort((a, b) => b - a);
    const sortedAsc = [...v].sort((a, b) => a - b);
    expect(JSON.stringify(v) === JSON.stringify(sortedDesc) || JSON.stringify(v) === JSON.stringify(sortedAsc)).toBe(true);
    await page.click('[data-testid=columns-btn]');
    await page.waitForSelector('[data-testid=columns]');
    await page.locator('[data-testid=columns] [role=checkbox]', { hasText: 'Size' }).click();
    await page.keyboard.press('Escape');
    await expect(page.locator('[data-testid=col-size]')).toHaveCount(0);
    v = await first();
    expect(v.length).toBeGreaterThan(0);
  });

  test('X-08 pause follow counts missed messages', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await selectTopic(page, 'building/f2/r204', 'building/f2/r204/setpoint');
    await page.click('[data-testid=follow]');
    await inject(page, 'building/f2/r204/setpoint', '{"value":23}');
    await inject(page, 'building/f2/r204/setpoint', '{"value":24}');
    await expect(page.locator('[data-testid=missed]')).toContainText('2', { timeout: 3000 });
    await page.click('[data-testid=follow]');
    await expect(page.locator('[data-testid=missed]')).toHaveCount(0);
  });

  test('X-09 quick publish sends to the selected topic and logs it', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await selectTopic(page, 'building/f2/r204', 'building/f2/r204/setpoint');
    await page.fill('[data-testid=qp-payload]', '21.5');
    await page.click('[data-testid=qp-qos-1]');
    await page.click('[data-testid=qp-send]');
    await page.click('[data-testid=rail-publish]');
    const row = page.locator('[data-testid=sent-row]').first();
    await expect(row).toContainText('r204/setpoint');
    await expect(row).toContainText(/acked|sent/, { timeout: 5000 });
  });

  test('X-10 context menu copies topic, payload and CLI; delete retained in subtree previews the topics', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await selectTopic(page, 'building/f2/r204', 'building/f2/r204/setpoint');
    const row = page.locator('[data-testid=tree-row][data-path="building/f2/r204/setpoint"]');
    await row.click({ button: 'right' });
    await menuItem(page, /^Copy topic/);
    expect(await copied(page)).toBe('building/f2/r204/setpoint');
    await row.click({ button: 'right' });
    await menuItem(page, /Copy as CLI/);
    expect(await copied(page)).toMatch(/^mqttpanel pub .*-t ['"]?building\/f2\/r204\/setpoint/);
    await page.locator('[data-testid=tree-row][data-path="building/f2/r204"]').click({ button: 'right' });
    await menuItem(page, /Delete retained in subtree/);
    const dlg = page.locator('[data-testid=delete-subtree]');
    await expect(dlg).toBeVisible();
    await expect(dlg).toContainText('setpoint');
    await expect(dlg).not.toContainText('humidity'); // not retained in the demo data
  });

  test('X-11 / X-18 "Show diff to previous" is on by default; a History row is diffed against its predecessor', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await selectTopic(page, 'building/f2/ahu01', 'building/f2/ahu01/state');
    await expect(page.locator('[data-testid=diff-toggle]')).toHaveAttribute('aria-checked', 'true');
    await expect(page.locator('[data-testid=diff-changes]')).toBeVisible();
    await expect(page.locator('[data-testid=payload-json] .add, [data-testid=payload-json] .chg, [data-testid=payload-json] .del').first()).toBeVisible();
    await expect(page.locator('[data-testid=diff-baseline]')).toHaveText(/^vs \d\d:\d\d:\d\d$/);
    await page.click('[data-testid=follow]');
    const rows = page.locator('[data-testid=history-row]');
    const pred = (await rows.nth(3).locator('td').nth(1).textContent())!.slice(0, 8); // change set 9: the chevron cell comes first
    await rows.nth(2).click();
    await expect(page.locator('[data-testid=detail] .callout')).toContainText('Showing the message from');
    await expect(page.locator('[data-testid=diff-baseline]')).toHaveText(`vs ${pred}`);
    await page.click('[data-testid=diff-toggle]');
    await expect(page.locator('[data-testid=payload-json] [data-kind]')).toHaveCount(0);
    await expect(page.locator('[data-testid=diff-baseline]')).toHaveCount(0);
  });

  test('X-14 payloads above the render limit open the paged raw view; "format anyway" works', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    const big = JSON.stringify({ data: 'x'.repeat(300 * 1024), value: 1 });
    await inject(page, 'building/f2/r204/blob', big);
    await selectTopic(page, 'building/f2/r204/blob', 'building/f2/r204/blob');
    await expect(page.locator('[data-testid=paged]')).toBeVisible();
    await page.click('[data-testid=format-anyway]');
    await expect(page.locator('[data-testid=paged]')).toHaveCount(0);
  });
});

test.describe('Charts', () => {
  async function chartWithSeries(page: Page) {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await selectTopic(page, 'building/f2/ahu01', 'building/f2/ahu01/state');
    await page.click('[data-testid=add-field]');
    // the demo topic chart already holds supply.temp and supply.fan; keep supply.temp only and also add it to a new chart
    await page.click('[data-testid="picker-check-supply.fan"]');
    await page.selectOption('[data-testid=picker-also]', 'new');
    await page.click('[data-testid=picker-add]');
    await page.click('[data-testid=rail-charts]');
    await page.waitForSelector('[data-testid=series-card]');
  }

  test('D-18 clicking a key in the last message writes its path into the series', async ({ page }) => {
    await chartWithSeries(page);
    await page.locator('[data-testid=series-more]').first().click();
    await page.waitForSelector('[data-testid=series-sample]');
    await page.locator('[data-testid=series-sample] [data-path="$.supply.fan"]').first().click();
    await expect(page.locator('[data-testid=series-path]')).toHaveValue('$.supply.fan');
    // text fields are not clickable
    const mode = page.locator('[data-testid=series-sample] [data-path="$.mode"]');
    if (await mode.count()) {
      await mode.first().click({ force: true });
      await expect(page.locator('[data-testid=series-path]')).toHaveValue('$.supply.fan');
    }
  });

  test('D-19 hover readout lists every series at the same time', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await selectTopic(page, 'building/f2/ahu01', 'building/f2/ahu01/state');
    await page.click('[data-testid=add-field]');
    await page.click('[data-testid="picker-check-supply.fan"]');
    await page.click('[data-testid="picker-check-alarm.active"]');
    await page.selectOption('[data-testid=picker-also]', 'new');
    await page.click('[data-testid=picker-add]');
    await page.click('[data-testid=rail-charts]');
    await page.waitForSelector('[data-testid=series-card]');
    const plot = page.locator('[data-testid=chart-pane] .u-over').first();
    const box = (await plot.boundingBox())!;
    await page.mouse.move(box.x + box.width - 6, box.y + box.height / 2);
    const readout = page.locator('[data-testid=chart-readout]');
    await expect(readout).toBeVisible();
    await expect(readout).toContainText('supply.temp');
    await expect(readout).toContainText(/alarm\.active/);
    await expect(readout).toContainText(/\((true|false)\)/);
  });

  test('D-20 pause freezes the chart and resume continues', async ({ page }) => {
    await chartWithSeries(page);
    const btn = page.locator('[data-testid=chart-pause]');
    await btn.click();
    await expect(btn).toHaveText(/Resume/);
    await expect(page.locator('[data-testid=chart-pane]')).toContainText(/Paused/);
    await btn.click();
    await expect(btn).toHaveText(/Pause/);
  });

  test('D-21 exports the chart as CSV and PNG', async ({ page }) => {
    await chartWithSeries(page);
    await page.click('[data-testid=chart-export]');
    await menuItem(page, /CSV/);
    await expect.poll(async () => (await saves(page)).map((s) => s.fileName)).toContain('chart.csv');
    await page.click('[data-testid=chart-export]');
    await menuItem(page, /PNG/);
    await expect.poll(async () => (await saves(page)).filter((s) => s.fileName.endsWith('.png') && s.bytes > 1000).length).toBe(1);
  });
});

test.describe('Publish', () => {
  test('P-04 repeat publish runs with a visible stop and logs each send', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=publish');
    await page.locator('[data-testid=template]', { hasText: 'Fake temperature' }).click();
    await page.fill('[data-testid=repeat-s]', '1');
    await page.click('[data-testid=repeat-start]');
    await expect(page.locator('[data-testid=repeat-callout]')).toContainText(/Repeating every 1 s/);
    await page.waitForTimeout(2300);
    await page.click('[data-testid=repeat-stop]');
    await expect(page.locator('[data-testid=repeat-stop]')).toHaveCount(0);
    await expect(page.locator('[data-testid=sent-row]').first()).toContainText('sim/room');
  });

  test('P-03 delete retained from the composer asks first and logs the empty retained message', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=publish');
    await page.fill('[data-testid=pub-topic]', 'building/f2/r204/setpoint');
    await page.keyboard.press('Escape');
    await page.click('[data-testid=delete-retained-btn]');
    await expect(page.locator('[data-testid=delete-retained]')).toContainText('{"value"');
    await page.click('[data-testid=delete-retained-ok]');
    await expect(page.locator('[data-testid=sent-row]').first()).toContainText(/deleted retained/);
  });
});

test.describe('Export, settings, palette', () => {
  test('S-01 export a subtree as CSV and as NDJSON', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await selectTopic(page, 'building/f2/r204', 'building/f2/r204/setpoint');
    await page.click('[data-testid=export-btn]');
    await expect(page.locator('[data-testid=export-estimate]')).toContainText(/messages/);
    await page.click('[data-testid=export-save]');
    await page.click('[data-testid=export-btn]');
    await page.locator('[data-testid=export-dialog] .seg button', { hasText: 'NDJSON' }).click();
    await page.click('[data-testid=export-save]');
    await expect.poll(async () => (await saves(page)).map((s) => s.fileName)).toEqual(['mqttpanel-export.csv', 'mqttpanel-export.ndjson']);
  });

  test('S-03 clear all history lowers the memory estimate', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=settings');
    await page.click('[data-testid=settings-nav-data]');
    const mem = page.locator('[data-testid=settings-data]');
    const read = async () => {
      const m = /([\d.,\s]+)\s*(B|KB|MB) in use/.exec((await mem.innerText()).replace(/ | /g, ' '));
      if (!m) return -1;
      const n = Number(m[1].replace(/[\s,]/g, ''));
      return n * (m[2] === 'MB' ? 1024 * 1024 : m[2] === 'KB' ? 1024 : 1);
    };
    const before = await read();
    expect(before).toBeGreaterThan(0);
    await page.click('[data-testid=clear-all]');
    await page.click('[data-testid=confirm-ok]');
    await expect.poll(read).toBeLessThan(before);
  });

  test('V-04 command palette works from the keyboard alone', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await page.keyboard.press('ControlOrMeta+K');
    await page.keyboard.type('open broker');
    await page.keyboard.press('Enter');
    await expect(page.locator('[data-testid=broker]')).toBeVisible();
    await page.keyboard.press('ControlOrMeta+K');
    await page.keyboard.type('cloud');
    await expect(page.locator('[data-testid=palette-row]').first()).toContainText(/Cloud EMQX/);
    await page.keyboard.press('Escape');
    await expect(page.locator('[data-testid=palette]')).toHaveCount(0);
  });

  test('V-05 language switches at runtime: English, French, German', async ({ page }) => {
    await open(page, 'mock=demo&screen=settings');
    await page.click('[data-testid=settings-nav-language]');
    const seg = page.locator('[data-testid=language]');
    await seg.locator('button', { hasText: 'Français' }).click();
    await expect(page.locator('[data-testid=rail-settings]')).toHaveText(/Réglages/);
    await seg.locator('button', { hasText: 'Deutsch' }).click();
    await expect(page.locator('[data-testid=rail-settings]')).toHaveText(/Einstellungen/);
    await seg.locator('button', { hasText: 'English' }).click();
    await expect(page.locator('[data-testid=rail-settings]')).toHaveText(/Settings/);
  });
});

test.describe('Broker', () => {
  test('B-02 Add listener with "Encrypted + client certificates" sets client certificates to Required', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    const before = await page.locator('[data-testid=listener-row]').count();
    await page.click('[data-testid=add-listener]');
    await page.click('[data-testid=preset-mtls]');
    await page.click('[data-testid=addl-next]');
    await page.fill('[data-testid=addl-port]', '80');
    await expect(page.locator('[data-testid=addl-port-error]')).toContainText(/1024/);
    await page.fill('[data-testid=addl-port]', '8885');
    await page.click('[data-testid=addl-add]');
    await expect(page.locator('[data-testid=listener-row]')).toHaveCount(before + 1);
    const added = page.locator('[data-testid=listener-row]').last();
    await expect(added).toContainText('8885');
    await expect(added).toContainText('Encrypted + client certificates');
    // started at once (change set 7): no prompt, no chip
    await expect(page.locator('[data-testid=restart-prompt]')).toHaveCount(0);
    await expect(page.locator('[data-testid=restart-chip]')).toHaveCount(0);
    await page.click('[data-testid=fold-brokerAdvanced]');
    await expect(page.locator('[data-testid=require-cert-seg] button.on')).toHaveText('Required');
  });

  test('B-12 "Built-in broker" > "Use an existing listener" keeps one profile and one user after 3 runs', async ({ page }) => {
    await open(page, 'mock=empty');
    for (let i = 0; i < 3; i++) {
      await page.click('[data-testid=rail-connect]');
      await page.click(i === 0 ? '[data-testid=welcome-broker]' : '[data-testid=new-profile]');
      if (i > 0) {
        await page.locator('[data-testid=context-menu] .ctx-item', { hasText: 'Built-in broker' }).hover();
        await page.locator('[data-testid=context-submenu] .ctx-item', { hasText: 'Use an existing listener' }).click();
      }
      await expect(page.locator('[data-testid=appb-listener][aria-checked=true]')).toHaveAttribute('data-listener', 'l-tls');
      await page.click('[data-testid=appb-connect]');
      // the broker starts only when it is stopped: a step on the first run only, never a restart (change set 7)
      if (i === 0) await expect(page.locator('[data-testid=appb-step-broker]')).toBeVisible();
      else await expect(page.locator('[data-testid=appb-step-broker]')).toHaveCount(0);
      await expect(page.locator('[data-testid=explorer]')).toBeVisible({ timeout: 8000 });
    }
    await page.click('[data-testid=rail-connect]');
    await expect(page.locator('[data-testid=profile-card]', { hasText: 'Local broker' })).toHaveCount(1);
    await page.click('[data-testid=rail-broker]');
    await expect(page.locator('[data-testid=user-row]')).toHaveCount(1);
    await expect(page.locator('[data-testid=user-row] [data-testid=user-local-tag]')).toHaveText('Local broker');
    await expect(page.locator('[data-testid=connect-here]')).toHaveCount(0);
  });

  test('B-25 no restart: Start and Stop only; a port change applies at once, only that listener restarts', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    const head = page.locator('[data-testid=broker] > .ph');
    await expect(head.locator('button')).toHaveText(['Stop']);
    await expect(page.locator('[data-testid=broker-restart]')).toHaveCount(0);
    await expect(head.locator('.pulse')).toHaveCount(0);
    await page.click('[data-testid=log-toggle]');
    await page.click('[data-testid=port-l-tls]');
    await page.fill('[data-testid=port-input-l-tls]', '8884');
    await page.keyboard.press('Enter');
    await expect(page.locator('[data-testid=port-l-tls]')).toHaveText('8884');
    await expect(page.locator('[data-testid=restart-prompt]')).toHaveCount(0);
    await expect(page.locator('[data-testid=restart-chip]')).toHaveCount(0);
    await expect(page.locator('[data-testid=port-pending]')).toHaveCount(0);
    const log = page.locator('[data-testid=broker-log]');
    await expect(log).toContainText('listener TLS 8883 stopped');
    await expect(log).toContainText(/listener TLS 8884 started · \d+ sessions? dropped: port changed/);
    // reachability, a new listener, users and Anonymous apply the same way, never prompting
    await page.click('[data-testid=reach-localhost]');
    await page.click('[data-testid=add-listener]');
    await page.click('[data-testid=addl-next]');
    await expect(page.locator('[data-testid=addl-after]')).not.toContainText(/restart/i);
    await page.fill('[data-testid=addl-port]', '8887');
    await page.click('[data-testid=addl-add]');
    await expect(log).toContainText('listener TLS 8887 started');
    await page.click('[data-testid=add-user]');
    await page.fill('#cred-user', 'tester');
    await page.fill('#cred-pass', 'pw-tester');
    await page.click('[data-testid=cred-ok]');
    await expect(page.locator('[data-testid=user-row][data-user=tester]')).toBeVisible();
    await page.click('[data-testid=anonymous]');
    await page.click('[data-testid=confirm-ok]');
    await page.click('[data-testid=anonymous]');
    await expect(page.locator('[data-testid=restart-prompt]')).toHaveCount(0);
    await expect(head.locator('button')).toHaveText(['Stop']);
    const st = await page.evaluate(() => (window as any).__mqttpanelMock.engine.brokerStatus());
    expect(st.state).toBe('running');
    expect('restartNeeded' in st).toBe(false);
    // stopped: Start only
    await page.click('[data-testid=broker-stop]');
    await expect(head.locator('button')).toHaveText(['Start']);
  });

  test('B-25 a listener that cannot start shows its error on its row; the others keep running', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    await page.evaluate(() => (window as any).__mqttpanelMock.engine.takePort(8084));
    const row = page.locator('[data-testid=listener-row][data-listener=l-wss]');
    await row.getByRole('switch').click();
    const err = page.locator('[data-testid=listener-error-l-wss]');
    await expect(err).toHaveText('not started');
    await expect(err).toHaveAttribute('title', 'The WSS listener on port 8084 could not start: port already in use');
    expect(await err.evaluate((el) => getComputedStyle(el).color)).toBe(await page.evaluate(() => {
      const d = document.createElement('span');
      d.style.color = 'var(--brick)';
      document.body.appendChild(d);
      const c = getComputedStyle(d).color;
      d.remove();
      return c;
    }));
    // the row stays on one line
    const h = await row.evaluate((el) => el.getBoundingClientRect().height);
    const h0 = await page.locator('[data-testid=listener-row][data-listener=l-tcp]').evaluate((el) => el.getBoundingClientRect().height);
    expect(h).toBeLessThanOrEqual(h0 + 1);
    await expect(page.locator('[data-testid=listener-error-l-tcp]')).toHaveCount(0);
    await expect(page.locator('[data-testid=broker-tiles]')).toContainText('Running');
  });

  test('B-26 client certificate rows: one ↓ button with a menu PEM / PKCS#12…', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    const certs = page.locator('[data-testid=client-certs]');
    await expect(certs.locator('tbody button', { hasText: /^PEM$|^PKCS#12$/ })).toHaveCount(0);
    const rows = certs.locator('[data-testid=issued-row]');
    await expect(certs.locator('[data-testid=cert-export]')).toHaveCount(await rows.count());
    const btn = certs.locator('[data-cert=edge01] [data-testid=cert-export]');
    await expect(btn).toHaveText('↓');
    await expect(btn).toHaveAttribute('title', 'Export…');
    await expect(btn).toHaveAttribute('aria-label', 'Export…');
    await btn.click();
    await expect(page.locator('[data-testid=context-menu] .ctx-label')).toHaveText(['PEM', 'PKCS#12…']);
    await menuItem(page, 'PEM');
    await expect.poll(async () => (await saves(page)).map((f) => f.fileName)).toContain('edge01.pem');
    await btn.click();
    await menuItem(page, 'PKCS#12…');
    await expect(page.locator('.modal', { hasText: 'Export edge01 as PKCS#12' })).toBeVisible();
    await expect(page.locator('[data-testid=p12-passphrase]')).toBeVisible();
    await page.keyboard.press('Escape');
    // the certificate of "Local broker" keeps its tag instead of ✕
    await expect(certs.locator('[data-cert=mqttpanel-desktop] [data-testid=cert-local-tag]')).toHaveText('Local broker');
    await expect(certs.locator('[data-cert=mqttpanel-desktop] [data-testid=cert-export]')).toBeVisible();
  });

  test('C-16 "Local broker" follows its listener at once and stays connected; other changes leave it alone', async ({ page }) => {
    await open(page, 'mock=demo&connect=local&screen=broker');
    // another listener's port: the profile is untouched
    await page.click('[data-testid=port-l-tcp]');
    await page.fill('[data-testid=port-input-l-tcp]', '1884');
    await page.keyboard.press('Enter');
    await expect(page.locator('[data-testid=port-l-tcp]')).toHaveText('1884');
    // its own listener's port: the profile has the new port and is connected again at once
    await page.click('[data-testid=port-l-tls]');
    await page.fill('[data-testid=port-input-l-tls]', '8884');
    await page.keyboard.press('Enter');
    await expect(page.locator('[data-testid=port-l-tls]')).toHaveText('8884');
    await page.click('[data-testid=rail-connect]');
    const local = page.locator('[data-profile=local]');
    await expect(local).toContainText('localhost:8884');
    await expect(local.locator('[data-testid=card-state]')).toHaveText('connected');
    await expect(page.locator('[data-testid=restart-prompt]')).toHaveCount(0);
  });

  test('B-16 eye and copy on every password and passphrase the app owns', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    const v = page.locator('[data-testid=user-pw-commissioning-value]');
    await expect(v).toHaveText('••••••••••••');
    await expect(page.locator('[data-testid=user-pw-commissioning-eye]')).toHaveAttribute('title', 'Show password');
    await page.click('[data-testid=user-pw-commissioning-eye]');
    await expect(v).toHaveText('site…ning');
    await expect(page.locator('[data-testid=user-pw-commissioning-eye]')).toHaveAttribute('title', 'Hide password');
    await page.click('[data-testid=user-pw-commissioning-copy]');
    await expect.poll(() => copied(page)).toBe('site-2026-commissioning');
    // generated PKCS#12 passphrase
    await page.click('[data-testid=issue-cert]');
    await page.locator('[data-testid=issue-cert] button', { hasText: 'Generate' }).click();
    const pass = page.locator('[data-testid=issue-passphrase]');
    await expect(pass).toHaveAttribute('type', 'password');
    await page.click('[data-testid=issue-passphrase-eye]');
    await expect(pass).toHaveAttribute('type', 'text');
    await page.click('[data-testid=issue-passphrase-copy]');
    await expect.poll(() => copied(page)).toBe(await pass.inputValue());
    await page.keyboard.press('Escape');
    // the "Local broker" profile password
    await page.click('[data-testid=rail-connect]');
    await page.click('[data-profile=local]');
    await page.click('[data-testid=profile-pw-copy]');
    await expect.poll(() => copied(page)).toBe('Vq7mZ3xLwT9bN4cHs8kR2p');
    await page.click('[data-profile=plant]');
    await expect(page.locator('[data-testid=profile-pw-eye]')).toHaveCount(0);
  });
});

test.describe('Native feel (N1, SPEC 1.2)', () => {
  test('theme follows the OS live, and "Increase contrast" darkens secondary text and lines', async ({ page }) => {
    await page.goto('/?mock=demo&lang=en');
    await page.waitForSelector('#app-root');
    const bg = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    await page.emulateMedia({ colorScheme: 'light' });
    const light = await bg();
    await page.emulateMedia({ colorScheme: 'dark' });
    await expect.poll(bg).not.toBe(light);
    await page.emulateMedia({ colorScheme: 'light', contrast: 'no-preference' });
    const ink2 = () => page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--ink-2').trim());
    const normal = await ink2();
    await page.emulateMedia({ colorScheme: 'light', contrast: 'more' });
    await expect.poll(ink2).not.toBe(normal);
  });

  test('chrome is not selectable and shows no text cursor; data is selectable', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await page.waitForSelector('[data-testid=tree-row]');
    const style = (sel: string) => page.locator(sel).first().evaluate((el) => ({ sel: getComputedStyle(el).userSelect, cur: getComputedStyle(el).cursor }));
    expect(await style('[data-testid=rail-explore]')).toEqual({ sel: 'none', cur: 'default' });
    expect((await style('.tb .name')).sel).toBe('none');
    expect((await style('[data-testid=tree-row] .name')).sel).toBe('text');
  });

  test('the native menu bar follows the app language', async ({ page }) => {
    await open(page, 'mock=demo&screen=settings');
    const menuLang = () => page.evaluate(() => (window as any).__mqttpanelMock.platform.menuLanguage());
    await expect.poll(menuLang).toBe('en');
    await page.click('[data-testid=settings-nav-language]');
    await page.locator('[data-testid=language] button', { hasText: 'Deutsch' }).click();
    await expect.poll(menuLang).toBe('de');
  });
});

test.describe('Change set 2', () => {
  test('P-11 "Send ⌘↵" sits top right in the composer header', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=publish');
    const head = page.locator('[data-testid=composer] > .ph');
    await expect(head.locator('button').last()).toHaveAttribute('data-testid', 'send');
    await expect(head.locator('[data-testid=send-clear], [data-testid=delete-retained-btn], [data-testid=repeat-s]')).toHaveCount(0);
    const sendBox = await head.locator('[data-testid=send]').boundingBox();
    const paneBox = await page.locator('[data-testid=composer]').boundingBox();
    expect(paneBox!.x + paneBox!.width - (sendBox!.x + sendBox!.width)).toBeLessThan(20);
    await page.fill('[data-testid=pub-topic]', 'building/f2/r204/setpoint');
    await page.keyboard.press('Escape');
    await head.locator('[data-testid=send]').click();
    await expect(page.locator('[data-testid=sent-row]').first()).toContainText('r204/setpoint');
  });

  test('B-18 broker-wide client certificate control; B-19 log collapsed, expands, remembered', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    // change set 4: the requirement and Verify against live in the "Advanced security" fold; listener rows never show it
    await page.click('[data-testid=fold-brokerAdvanced]');
    const adv = page.locator('[data-testid=advanced-security]');
    await expect(adv.locator('[data-testid=require-cert]')).toContainText('Client certificates');
    await expect(adv.locator('[data-testid=require-cert]')).toContainText('on encrypted listeners');
    await expect(adv.locator('[data-testid=verify-against]')).toContainText('Verify against');
    await expect(page.locator('[data-testid=require-cert-hint]')).toHaveCount(0);
    await expect(page.locator('[data-testid=listener-cert]')).toHaveCount(0);
    await page.locator('[data-testid=require-cert-seg] button', { hasText: 'Optional' }).click();
    await expect(page.locator('[data-testid=fold-brokerAdvanced-summary]')).toContainText('client certificates optional');
    await expect(page.locator('[data-testid=restart-prompt]')).toHaveCount(0);
    // B-19: no Refresh, the log is a title strip at the bottom of the right pane until clicked
    const side = page.locator('[data-testid=broker-side]');
    await expect(side.getByText('Refresh')).toHaveCount(0);
    const strip = page.locator('[data-testid=broker-log-strip]');
    await expect(strip).toContainText('Log');
    await expect(strip).not.toContainText('live');
    await expect(strip).toContainText(/\d+ warnings?/);
    const sideBox = await side.boundingBox();
    const stripBox = await strip.boundingBox();
    expect(Math.abs(sideBox!.y + sideBox!.height - (stripBox!.y + stripBox!.height))).toBeLessThan(3);
    await page.click('[data-testid=log-toggle]');
    await expect(page.locator('[data-testid=broker-log]')).toBeVisible();
    await page.click('[data-testid=rail-connect]');
    await page.click('[data-testid=rail-broker]');
    await expect(page.locator('[data-testid=broker-log]')).toBeVisible();
    await page.click('[data-testid=log-toggle]');
    await expect(page.locator('[data-testid=broker-log]')).toHaveCount(0);
  });
});

test.describe('Change set 4', () => {
  const mock = (page: Page, fn: string, ...args: unknown[]) =>
    page.evaluate(([f, a]) => (window as any).__mqttpanelMock.engine[f as string](...(a as unknown[])), [fn, args] as const);

  test('C-17 Connect and Test on "Local broker" start a stopped broker first; a broker that cannot start fails the connection', async ({ page }) => {
    await open(page, 'mock=demo&screen=connect&latency=1200');
    await mock(page, 'brokerStop');
    await expect(page.locator('[data-testid=rail-broker-badge]')).toHaveCount(0);
    const local = page.locator('[data-profile=local]');
    await local.click();
    await page.click('[data-testid=connect]');
    await expect(local.locator('[data-testid=card-state]')).toHaveText('starting broker…');
    await expect(page.locator('[data-testid=rail-broker-badge]')).toHaveAttribute('data-state', 'starting');
    await expect(page.locator('[data-testid=explorer]')).toBeVisible({ timeout: 8000 });
    await expect(page.locator('[data-testid=rail-broker-badge]')).toHaveAttribute('data-state', 'running');
    await expect(page.locator('[data-testid=cbar-tab]')).toContainText('Local broker');
    // Test does the same
    await page.locator('[data-testid=cbar-close]').click();
    await mock(page, 'brokerStop');
    await page.click('[data-testid=rail-connect]');
    await local.click();
    await page.click('[data-testid=try-connect]');
    await expect(page.locator('[data-testid=last-test]')).toContainText(/Connected in \d+ ms/, { timeout: 8000 });
    await expect(page.locator('[data-testid=rail-broker-badge]')).toHaveAttribute('data-state', 'running');
    // a broker that cannot start: the connection fails with the broker's reason
    await mock(page, 'brokerStop');
    await mock(page, 'failBrokerStart', true);
    await page.click('[data-testid=connect]');
    await expect(page.locator('.toast.err')).toContainText('already in use by node', { timeout: 8000 });
    await expect(page.locator('[data-testid=explorer]')).toHaveCount(0);
    await expect(page.locator('[data-testid=cbar]')).toHaveAttribute('data-empty', 'true');
    await expect(page.locator('[data-testid=rail-broker-badge]')).toHaveCount(0);
  });

  test('C-18 the stop button on a card disconnects that connection only, never the broker', async ({ page }) => {
    await open(page, 'mock=demo&connect=local,plant&screen=connect');
    const local = page.locator('[data-profile=local]');
    const plant = page.locator('[data-profile=plant]');
    await expect(local.locator('[data-testid=card-stop]')).toHaveAttribute('title', 'Disconnect');
    await expect(local.locator('[data-testid=card-stop]')).toHaveAttribute('aria-label', 'Disconnect');
    await expect(page.locator('[data-profile=lab] [data-testid=card-stop]')).toHaveCount(0);
    await local.locator('[data-testid=card-stop]').click();
    await expect(local.locator('[data-testid=card-state]')).toHaveCount(0);
    await expect(plant.locator('[data-testid=card-state]')).toHaveText('connected');
    await expect(page.locator('[data-testid=rail-broker-badge]')).toHaveAttribute('data-state', 'running');
    await expect(page.locator('[data-testid=rail-connect-count]')).toHaveText('1');
    // the click does not select the card or open the Explorer
    await expect(page.locator('[data-testid=profile-form]')).toBeVisible();
    // a reconnecting card: the stop button cancels the retries
    await mock(page, 'setConnectionState', 'plant', 'reconnecting');
    await expect(plant.locator('[data-testid=card-state]')).toHaveText('reconnecting');
    await plant.locator('[data-testid=card-stop]').click();
    await expect(plant.locator('[data-testid=card-state]')).toHaveCount(0);
    await expect(page.locator('[data-testid=cbar]')).toHaveAttribute('data-empty', 'true');
  });

  test('C-19 Security and Advanced are folds, closed at start, with a summary; the state is kept while the app runs', async ({ page }) => {
    await open(page, 'mock=demo&screen=connect');
    await page.click('[data-profile=plant]');
    const sec = page.locator('[data-testid=fold-security]');
    const adv = page.locator('[data-testid=fold-advanced]');
    await expect(sec).toHaveAttribute('aria-expanded', 'false');
    await expect(adv).toHaveAttribute('aria-expanded', 'false');
    await expect(page.locator('[data-testid=profile-form] .sec')).toHaveText(['Basics']);
    await expect(page.locator('[data-testid=fold-security-summary]')).toHaveText('CA · client certificate (imported) · verified');
    await expect(page.locator('[data-testid=fold-advanced-summary]')).toHaveText('keep-alive 60 s · 3 subscriptions · 1 schema');
    await expect(page.locator('[data-testid=info-caCert]')).toHaveCount(0);
    await sec.click();
    await expect(page.locator('[data-testid=info-caCert]')).toBeVisible();
    for (const id of ['caCert', 'clientCert', 'clientKey', 'sni', 'alpn']) await expect(page.locator(`[data-testid=info-${id}]`)).toBeVisible();
    // no helper sentences next to the controls
    const form = page.locator('[data-testid=profile-form]');
    for (const s of ['System roots plus the CA below.', 'Only if the broker serves several names.', 'Some cloud brokers require it.']) await expect(form).not.toContainText(s);
    await adv.click();
    await expect(page.locator('[data-testid=profile-form] .sub')).toHaveText(['Session', 'Subscriptions', 'Decoders']);
    // kept while navigating and across profiles
    await page.click('[data-testid=rail-explore]');
    await page.click('[data-testid=rail-connect]');
    // (an encrypted profile: with mqtt or ws the Security fold is off, change set 5)
    await page.click('[data-profile=emqx]');
    await expect(page.locator('[data-testid=fold-security]')).toHaveAttribute('aria-expanded', 'true');
    await expect(page.locator('[data-testid=fold-advanced]')).toHaveAttribute('aria-expanded', 'true');
    // a profile at its defaults has empty summaries
    await page.click('[data-testid=new-profile]');
    await page.locator('[data-testid=context-menu] .ctx-item', { hasText: 'New broker…' }).click();
    await expect(page.locator('[data-testid=fold-advanced-summary]')).toHaveCount(0);
    await page.click('[data-testid=scheme-mqtts]');
    await expect(page.locator('[data-testid=fold-security-summary]')).toHaveCount(0);
    // closed again after a restart of the app
    await open(page, 'mock=demo&screen=connect');
    await expect(page.locator('[data-testid=fold-security]')).toHaveAttribute('aria-expanded', 'false');
  });

  test('C-20 no parsed chips next to file fields or the password; no "mTLS"; cards show the protocol (change set 6)', async ({ page }) => {
    await open(page, 'mock=demo&connect=local&screen=connect');
    await page.click('[data-profile=plant]');
    await page.click('[data-testid=fold-security]');
    for (const f of ['caCert', 'clientCert', 'clientKey']) {
      const field = page.locator(`[data-testid=file-${f}]`);
      await expect(field).toBeVisible();
      await expect(field.locator('.chip')).toHaveCount(0);
      for (const w of ['until', 'CN=', 'Keychain']) await expect(field).not.toContainText(w);
    }
    await expect(page.locator('[data-testid=profile-form]')).not.toContainText('Keychain');
    await page.click('[data-profile=local]');
    await expect(page.locator('[data-testid=local-password] .chip')).toHaveCount(0);
    const chips = await page.locator('[data-testid=card-chip]').allTextContents();
    expect(new Set(chips)).toEqual(new Set(['mqtts', 'mqtt', 'wss']));
    // a wrong file still shows the plain error
    await page.click('[data-profile=plant]');
    const box = (await page.locator('[data-testid=file-caCert]').boundingBox())!;
    await page.evaluate(([x, y]) => (window as any).__mqttpanelMock.platform.simulateDrop(['/dropped/notes.txt'], x, y), [box.x + 20, box.y + box.height / 2]);
    await expect(page.locator('[data-testid=file-error-caCert]')).toHaveText('This file is not a certificate.');
    await expect(page.locator('[data-testid=file-caCert] .chip')).toHaveCount(0);
    for (const lang of ['en', 'fr', 'de']) {
      await page.goto(`/?mock=demo&connect=local&screen=broker&lang=${lang}&theme=light`);
      await page.waitForSelector('#app-root');
      await page.click('[data-testid=add-listener]');
      const text = await page.locator('body').innerText();
      expect(text).not.toMatch(/mTLS|mutual TLS|TLS mutuel/i);
    }
  });

  test('B-21 reachability is one broker-wide choice with an ⓘ; switching applies at once, nothing prompts', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    const line = page.locator('[data-testid=listeners-line]');
    await expect(line.locator('[data-testid=reach-seg] button')).toHaveText(['This computer only', 'Whole network']);
    await expect(page.locator('[data-testid=reach-network]')).toHaveClass(/on/);
    await expect(page.locator('[data-testid=listener-row] .seg')).toHaveCount(0);
    await expect(page.locator('[data-testid=network-warning]')).toHaveCount(0);
    await line.locator('[data-testid=info-reachable]').click();
    await expect(page.locator('[data-testid=bubble-text-reachable]')).toContainText('Whole network: any device that can reach this computer can connect');
    await page.keyboard.press('Escape');
    await expect(page.locator('[data-testid=unencrypted]')).toHaveCount(2);
    await expect(page.locator('[data-testid=bubble-text-reachable]')).toHaveCount(0);
    await page.click('[data-testid=reach-localhost]');
    await expect(page.locator('[data-testid=reach-localhost]')).toHaveClass(/on/);
    await expect(page.locator('[data-testid=unencrypted]')).toHaveCount(0);
    await expect(page.locator('[data-testid=restart-prompt]')).toHaveCount(0);
    await expect(page.locator('[data-testid=restart-chip]')).toHaveCount(0);
    await line.locator('[data-testid=info-reachable]').click();
    await expect(page.locator('[data-testid=bubble-text-reachable]')).not.toContainText('restart');
  });

  test('B-22 listener table: On, Kind, Port, Setup and ✕, which removes at once; nothing prompts', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    await expect(page.locator('[data-testid=listeners] th')).toHaveText(['On', 'Kind', 'Port', 'Setup', '']);
    await expect(page.locator('[data-testid=listener-row] button', { hasText: '⋯' })).toHaveCount(0);
    const before = await page.locator('[data-testid=listener-row]').count();
    await expect(page.locator('[data-testid=remove-listener-l-ws]')).toHaveAttribute('aria-label', 'Remove the WebSocket listener on port 8083');
    await page.click('[data-testid=remove-listener-l-ws]');
    await expect(page.locator('[data-testid=confirm-ok]')).toHaveCount(0);
    await expect(page.locator('[data-testid=listener-row]')).toHaveCount(before - 1);
    await expect(page.locator('[data-testid=restart-prompt]')).toHaveCount(0);
    await page.click('[data-testid=log-toggle]');
    await expect(page.locator('[data-testid=broker-log]')).toContainText(/listener WS 8083 stopped( · \d+ sessions? dropped: listener removed)?/);
  });

  test('B-23 Users and Client certificates are panels; "Advanced security" is folded; Add listener step 2 has kind and port only', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    const users = page.locator('[data-testid=users]');
    await expect(users.locator('.hd')).toContainText('Users');
    await expect(users.locator('.hd [data-testid=anonymous]')).toBeVisible();
    await expect(users.locator('.hd [data-testid=info-anonymous]')).toBeVisible();
    await expect(users.locator('.hd [data-testid=add-user]')).toHaveText('+ Add user');
    await expect(users.locator('th')).toHaveText(['Username', 'Password', '']);
    await expect(users.locator('[data-user=mqtt-panel] [data-testid=user-local-tag]')).toHaveText('Local broker');
    await expect(users.locator('[data-testid=user-change-commissioning]')).toHaveText('✎');
    await page.click('[data-testid=user-remove-dashboard]');
    await expect(users.locator('[data-user=dashboard]')).toHaveCount(0);
    const certs = page.locator('[data-testid=client-certs]');
    await expect(certs.locator('.hd [data-testid=info-clientCerts]')).toBeVisible();
    await expect(certs.locator('.hd button')).toHaveText(['i', 'Import CSR…', 'Issue…']);
    await expect(certs.locator('th')).toHaveText(['Name', 'Expires', 'Last seen', '']);
    await expect(certs.locator('[data-cert=mqttpanel-desktop] [data-testid=cert-local-tag]')).toHaveText('Local broker');
    await expect(certs.locator('[data-cert=mqttpanel-desktop] [data-testid=revoke-mqttpanel-desktop]')).toHaveCount(0);
    await page.click('[data-testid=revoke-edge01]');
    await expect(page.locator('[data-testid=confirm-ok]')).toBeVisible();
    await page.keyboard.press('Escape');
    const fold = page.locator('[data-testid=fold-brokerAdvanced]');
    await expect(fold).toHaveAttribute('aria-expanded', 'false');
    await expect(page.locator('[data-testid=advanced-security]')).toHaveCount(0);
    await fold.click();
    const adv = page.locator('[data-testid=advanced-security]');
    for (const id of ['require-cert', 'verify-against', 'local-ca', 'server-cert']) await expect(adv.locator(`[data-testid=${id}]`)).toBeVisible();
    await expect(adv.locator('[data-testid=local-ca] button')).toHaveText(['Export CA.pem', 'Rotate…']);
    await expect(adv.locator('[data-testid=server-cert] button')).toHaveText(['Import…', 'Regenerate…', 'Export…']);
    await page.click('[data-testid=add-listener]');
    await page.click('[data-testid=addl-next]');
    await expect(page.locator('[data-testid=add-listener-2] .f > .lbl, [data-testid=add-listener-2] .f > label')).toHaveText(['Kind', 'Port']);
  });

  test('B-24 verbosity: no "live", no client count, "▸ Log" with warnings only when there are some, one-sentence anonymous strip', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    const screen = page.locator('.cols');
    await expect(screen).not.toContainText(/\blive\b/);
    await expect(page.locator('[data-testid=clients-title]')).toHaveText('Clients');
    await expect(page.locator('[data-testid=broker-tiles] .stat .l')).toHaveText(['Status', '↓ Messages in', '↑ Messages out', 'Retained']);
    await expect(page.locator('[data-testid=log-toggle]')).toHaveText(/^▸\s*Log\s*· \d+ warnings?$/);
    for (const s of ['Export each certificate', 'one set of users for every listener', 'one per device, issued by the local CA', 'created for the profile', 'Every device needs its own certificate']) {
      await expect(page.locator('[data-testid=broker]')).not.toContainText(s);
    }
    await page.click('[data-testid=anonymous]');
    await page.click('[data-testid=confirm-ok]');
    await expect(page.locator('[data-testid=anon-warning]')).toHaveText('Anonymous access is on: anyone who can reach a listener can connect.');
    await expect(page.locator('[data-testid=fold-brokerAdvanced-summary]')).toHaveText('not checked while Anonymous is on');
    // no warnings: the strip reads "▸ Log" only
    await open(page, 'mock=empty&screen=broker');
    await expect(page.locator('[data-testid=log-toggle]')).toHaveText(/^▸\s*Log$/);
    await expect(page.locator('[data-testid=log-warnings]')).toHaveCount(0);
  });

  test('V-10 title bar holds the name and the mark; rail badges; ⚠ on tab and card (change set 6)', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant,local&screen=connect');
    await expect(page.locator('.tb')).toHaveText('MQTT Panel');
    await expect(page.locator('.tb .mark svg')).toBeVisible();
    await expect(page.locator('[data-testid=pill]')).toHaveCount(0);
    await expect(page.locator('[data-testid=rail-broker-badge]')).toHaveAttribute('data-state', 'running');
    await expect(page.locator('[data-testid=rail-connect-count]')).toHaveText('2');
    await expect(page.locator('[data-testid=rail-connect]')).toHaveAttribute('aria-label', 'Connect, 2 connections open');
    // verified connections carry no ⚠; the demo "Cloud EMQX (WS)" skips the check, so its card does
    await expect(page.locator('[data-testid=cbar-warn]')).toHaveCount(0);
    await expect(page.locator('[data-profile=plant] [data-testid=card-warn]')).toHaveCount(0);
    await expect(page.locator('[data-profile=lab] [data-testid=card-warn]')).toHaveCount(0);
    await expect(page.locator('[data-profile=emqx] [data-testid=card-warn]')).toHaveAttribute('data-kind', 'skip');
    // "anonymous" lives on the Broker screen only
    await mock(page, 'brokerStop');
    await expect(page.locator('[data-testid=rail-broker-badge]')).toHaveCount(0);
    for (const id of ['plant', 'local']) await mock(page, 'disconnect', id);
    await expect(page.locator('[data-testid=rail-connect-count]')).toHaveCount(0);
  });
});

test.describe('Change set 5', () => {
  const mock = (page: Page, fn: string, ...args: unknown[]) =>
    page.evaluate(([f, a]) => (window as any).__mqttpanelMock.engine[f as string](...(a as unknown[])), [fn, args] as const);
  const host = (page: Page) => page.locator('#pf-host');
  const port = (page: Page) => page.locator('[data-testid=pf-port]');
  const scheme = (page: Page, s: string) => page.locator(`[data-testid=scheme-${s}]`);

  test('C-21 idle cards carry a play button that connects; play and stop never show together', async ({ page }) => {
    await open(page, 'mock=demo&connect=local&screen=connect&latency=600');
    const lab = page.locator('[data-profile=lab]');
    await expect(page.locator('[data-profile=local] [data-testid=card-stop]')).toBeVisible();
    await expect(page.locator('[data-profile=local] [data-testid=card-play]')).toHaveCount(0);
    for (const id of ['plant', 'lab', 'emqx']) {
      await expect(page.locator(`[data-profile=${id}] [data-testid=card-play]`)).toHaveAttribute('title', 'Connect');
      await expect(page.locator(`[data-profile=${id}] [data-testid=card-stop]`)).toHaveCount(0);
    }
    await expect(page.locator('[data-profile=plant]')).toHaveAttribute('aria-selected', 'true');
    await lab.locator('[data-testid=card-play]').click();
    await expect(lab.locator('[data-testid=card-play]')).toHaveCount(0);
    await expect(lab.locator('[data-testid=card-state]')).toHaveText('connecting');
    await expect(lab.locator('[data-testid=card-state]')).toHaveText('connected', { timeout: 5000 });
    await expect(lab.locator('[data-testid=card-stop]')).toBeVisible();
    await expect(lab.locator('[data-testid=card-play]')).toHaveCount(0);
    // the click neither selects the card nor leaves the Connections screen
    await expect(page.locator('[data-profile=plant]')).toHaveAttribute('aria-selected', 'true');
    await expect(page.locator('[data-testid=profile-form]')).toBeVisible();
    await expect(page.locator('[data-testid=rail-connect-count]')).toHaveText('2');
    // "Local broker": the broker starts first
    await page.locator('[data-profile=local] [data-testid=card-stop]').click();
    await mock(page, 'brokerStop');
    await expect(page.locator('[data-testid=rail-broker-badge]')).toHaveCount(0);
    await page.locator('[data-profile=local] [data-testid=card-play]').click();
    await expect(page.locator('[data-profile=local] [data-testid=card-state]')).toHaveText('starting broker…');
    await expect(page.locator('[data-profile=local] [data-testid=card-state]')).toHaveText('connected', { timeout: 8000 });
    await expect(page.locator('[data-testid=rail-broker-badge]')).toHaveAttribute('data-state', 'running');
  });

  test('C-22 Basics order and the address prefiller', async ({ page }) => {
    await open(page, 'mock=demo&screen=connect');
    await page.click('[data-profile=lab]');
    const form = page.locator('[data-testid=profile-form]');
    await expect(form.locator('.f > .lbl, .f > label')).toHaveText(['Name', 'Protocol', 'MQTT version', 'Host and port', 'Client ID', 'Username', 'Password']);
    await expect(host(page)).toHaveValue('mqtt://10.20.4.17:1883');
    // choosing ws rewrites the prefix and the default port, and adds the WebSocket path row
    await scheme(page, 'ws').click();
    await expect(host(page)).toHaveValue('ws://10.20.4.17:8083');
    await expect(port(page)).toHaveValue('8083');
    await expect(form.locator('.f > .lbl, .f > label')).toHaveText(['Name', 'Protocol', 'MQTT version', 'Host and port', 'WebSocket path', 'Client ID', 'Username', 'Password']);
    // typing "mqtt://" selects mqtt
    await host(page).fill('mqtt://10.20.4.17');
    await expect(scheme(page, 'mqtt')).toHaveAttribute('aria-checked', 'true');
    await expect(port(page)).toHaveValue('1883');
    // ":1884" at the end sets the port box
    await host(page).press('End');
    await page.keyboard.type(':1884');
    await expect(port(page)).toHaveValue('1884');
    // the port box rewrites the suffix
    await port(page).fill('1885');
    await expect(host(page)).toHaveValue('mqtt://10.20.4.17:1885');
    // a pasted full address
    await host(page).fill('wss://demo.emqx.example:8084/mqtt');
    await host(page).blur();
    await expect(scheme(page, 'wss')).toHaveAttribute('aria-checked', 'true');
    await expect(port(page)).toHaveValue('8084');
    await expect(page.locator('#pf-ws')).toHaveValue('/mqtt');
    await expect(host(page)).toHaveValue('wss://demo.emqx.example:8084');
    // IPv6 keeps its brackets
    await host(page).fill('mqtt://[::1]:1883');
    await host(page).blur();
    await expect(host(page)).toHaveValue('mqtt://[::1]:1883');
    // a deleted prefix comes back on blur; the protocol stays
    await host(page).fill('10.20.4.17:1883');
    await expect(scheme(page, 'mqtt')).toHaveAttribute('aria-checked', 'true');
    await host(page).blur();
    await expect(host(page)).toHaveValue('mqtt://10.20.4.17:1883');
    // the saved profile holds the host without scheme or port
    await expect
      .poll(async () => (await mock(page, 'listProfiles')).find((p: any) => p.id === 'lab'), { timeout: 3000 })
      .toMatchObject({ host: '10.20.4.17', port: 1883, scheme: 'mqtt' });
  });

  test('C-23 Security is off with mqtt and ws, with the reason; values come back with mqtts', async ({ page }) => {
    await open(page, 'mock=demo&screen=connect');
    const fold = page.locator('[data-testid=fold-security]');
    await fold.click();
    await expect(page.locator('[data-testid=file-caCert]')).toContainText('plant-root-ca.pem');
    await scheme(page, 'mqtt').click();
    await expect(fold).toHaveClass('fold off');
    await expect(fold).toHaveAttribute('aria-disabled', 'true');
    await expect(fold).toHaveAttribute('aria-expanded', 'false');
    await expect(page.locator('[data-testid=fold-security-summary]')).toHaveText('Not encrypted: choose mqtts or wss to use certificates.');
    await expect(page.locator('[data-testid=file-caCert]')).toHaveCount(0);
    await fold.click({ force: true });
    await expect(page.locator('[data-testid=file-caCert]')).toHaveCount(0);
    await expect(page.locator('[data-testid=profile-form]')).not.toContainText('Certificates are used only');
    await scheme(page, 'ws').click();
    await expect(fold).toHaveClass('fold off');
    await scheme(page, 'mqtts').click();
    await expect(fold).toHaveAttribute('aria-expanded', 'true');
    await expect(page.locator('[data-testid=file-caCert]')).toContainText('plant-root-ca.pem');
    for (const [lang, text] of [
      ['fr', 'Non chiffré\u202f: choisissez mqtts ou wss pour utiliser des certificats.'],
      ['de', 'Nicht verschlüsselt: Wähle mqtts oder wss, um Zertifikate zu verwenden.'],
    ]) {
      await page.goto(`/?mock=demo&screen=connect&lang=${lang}&theme=light`);
      await page.waitForSelector('#app-root');
      await page.click('[data-profile=lab]');
      await expect(page.locator('[data-testid=fold-security-summary]')).toHaveText(text);
    }
  });

  test('C-24 (superseded by C-26) no "this app’s broker" wording in the rendered screens, in EN, FR and DE', async ({ page }) => {
    for (const lang of ['en', 'fr', 'de']) {
      await page.goto(`/?mock=demo&screen=connect&lang=${lang}&theme=light`);
      await page.waitForSelector('#app-root');
      await page.click('[data-testid=new-profile]');
      await page.locator('[data-testid=context-menu] .ctx-item').nth(1).hover();
      await page.locator('[data-testid=context-submenu] .ctx-item').first().click();
      await expect(page.locator('[data-testid=app-broker]')).toBeVisible();
      const text = await page.locator('body').innerText();
      expect(text).not.toMatch(/this app's broker|cette app\b|dieser App/i);
    }
  });

  test('C-25 no import entry point: no line under the list, no drop zone, no welcome card, no palette entry', async ({ page }) => {
    await open(page, 'mock=demo&screen=connect');
    await expect(page.locator('[data-testid=import]')).toHaveCount(0);
    await expect(page.locator('.pane.side')).not.toContainText('Import');
    const before = (await mock(page, 'listProfiles')).length;
    const box = (await page.locator('.pane.side .list').boundingBox())!;
    await page.evaluate(([x, y]) => (window as any).__mqttpanelMock.platform.simulateDrop(['/dropped/connections.json'], x, y), [box.x + 20, box.y + 20]);
    await page.waitForTimeout(300);
    await expect(page.locator('[data-testid=import-summary]')).toHaveCount(0);
    expect((await mock(page, 'listProfiles')).length).toBe(before);
    await page.keyboard.press('ControlOrMeta+K');
    await page.keyboard.type('import');
    await expect(page.locator('[data-testid=palette-row]', { hasText: /connection/i })).toHaveCount(0);
    await page.keyboard.press('Escape');
    await open(page, 'mock=empty');
    await expect(page.locator('[data-testid=welcome] .big > .card')).toHaveCount(2);
    await expect(page.locator('[data-testid=welcome]')).not.toContainText('Import');
  });
});

test.describe('Change set 6', () => {
  const mock = (page: Page, fn: string, ...args: unknown[]) =>
    page.evaluate(([f, a]) => (window as any).__mqttpanelMock.engine[f as string](...(a as unknown[])), [fn, args] as const);
  const profile = async (page: Page, id: string) => ((await mock(page, 'listProfiles')) as any[]).find((p) => p.id === id);
  const setHost = async (page: Page, text: string) => {
    await page.fill('#pf-host', text);
    await page.locator('#pf-host').blur();
  };

  test('C-26 wording: "New broker…", "Built-in broker", "Connect to the built-in broker", "Generate", « Connecter » in EN, FR and DE', async ({ page }) => {
    for (const [lang, other, app, title, gen, connect] of [
      ['en', 'New broker…', 'Built-in broker', 'Connect to the built-in broker', 'Generate', 'Connect'],
      ['fr', 'Nouveau broker…', 'Broker intégré', 'Connexion au broker intégré', 'Générer', 'Connecter'],
      ['de', 'Neuer Broker…', 'Eingebauter Broker', 'Mit dem eingebauten Broker verbinden', 'Generieren', 'Verbinden'],
    ]) {
      await page.goto(`/?mock=demo&screen=connect&lang=${lang}&theme=light`);
      await page.waitForSelector('#app-root');
      await expect(page.locator('[data-testid=generate-cid]')).toHaveText(gen);
      await expect(page.locator('[data-testid=connect]')).toHaveText(connect);
      await expect(page.locator('[data-profile=plant] [data-testid=card-play]')).toHaveAttribute('aria-label', connect);
      await page.click('[data-testid=new-profile]');
      await expect(page.locator('[data-testid=context-menu] .ctx-label')).toHaveText([other, app]);
      await page.locator('[data-testid=context-menu] .ctx-item').nth(1).hover();
      await page.locator('[data-testid=context-submenu] .ctx-item').first().click();
      await expect(page.locator('[data-testid=app-broker] .mh')).toHaveText(title);
      const text = await page.locator('body').innerText();
      expect(text).not.toMatch(/MQTT Panel broker|Broker MQTT Panel|MQTT Panel Broker|Another broker|Se connecter|Aléatoire|Zufällig|\bRandom\b/);
    }
  });

  test('C-27 cards: name line, protocol chip with ⚠, "host:port"; nothing else; taller', async ({ page }) => {
    await open(page, 'mock=demo&connect=local&screen=connect');
    const expected: [string, string, string][] = [
      ['plant', 'mqtts', 'broker.plant.example:8883'],
      ['local', 'mqtts', 'localhost:8883'],
      ['lab', 'mqtt', '10.20.4.17:1883'],
      ['emqx', 'wss', 'demo.emqx.example:8084'],
    ];
    for (const [id, proto, addr] of expected) {
      const c = page.locator(`[data-profile=${id}]`);
      await expect(c.locator(':scope > div')).toHaveCount(3);
      await expect(c.locator('[data-testid=card-chip]')).toHaveText(proto);
      await expect(c.locator('[data-testid=card-address]')).toHaveText(addr);
      await expect(c).not.toContainText(/MQTT 5|MQTT 3|subscription|used|ago|last week|Built-in/);
    }
    // left aligned chip, monospace address, 13 px padding and 4 px between lines
    const look = await page.locator('[data-profile=lab]').evaluate((el) => {
      const cs = getComputedStyle(el);
      const lines = el.querySelectorAll(':scope > .s');
      const chip = el.querySelector('[data-testid=card-chip]')!.getBoundingClientRect();
      return {
        pad: cs.paddingTop,
        gap: getComputedStyle(lines[0]).marginTop,
        mono: getComputedStyle(lines[1]).fontFamily,
        chipLeft: Math.round(chip.left - el.getBoundingClientRect().left),
      };
    });
    expect(look.pad).toBe('13px');
    expect(look.gap).toBe('4px');
    expect(look.mono).toMatch(/mono|Menlo|Consolas|Courier/i);
    expect(look.chipLeft).toBeLessThanOrEqual(14);
    const warn = page.locator('[data-profile=emqx] [data-testid=card-warn]');
    await expect(warn).toHaveAttribute('title', 'Certificate verification is off for this connection: any broker certificate is accepted, so an impostor could not be told apart.');
    for (const [lang, text] of [
      ['fr', 'La vérification du certificat est désactivée pour cette connexion : n’importe quel certificat est accepté, un usurpateur ne serait pas détecté.'],
      ['de', 'Die Zertifikatsprüfung ist für diese Verbindung aus: Jedes Broker-Zertifikat wird akzeptiert, ein Angreifer wäre nicht zu erkennen.'],
    ]) {
      await page.goto(`/?mock=demo&screen=connect&lang=${lang}&theme=light`);
      await page.waitForSelector('#app-root');
      await expect(page.locator('[data-profile=emqx] [data-testid=card-warn]')).toHaveAttribute('title', text);
    }
  });

  test('C-28 a new profile is named after its host until a name is typed; existing profiles are not renamed', async ({ page }) => {
    await open(page, 'mock=demo&screen=connect');
    await page.click('[data-testid=new-profile]');
    await page.locator('[data-testid=context-menu] .ctx-item', { hasText: 'New broker…' }).click();
    const name = page.locator('#pf-name');
    await expect(name).toHaveValue('localhost');
    for (const [h, n] of [
      ['mqtt://myhost.com', 'myhost'],
      ['mqtt://broker.plant.example', 'broker'],
      ['mqtt://10.20.4.17', '10.20.4.17'],
      ['mqtt://[::1]:1883', '::1'],
      ['mqtt://localhost', 'localhost'],
    ]) {
      await setHost(page, h);
      await expect(name).toHaveValue(n);
    }
    await expect(page.locator('[data-testid=profile-card][aria-selected=true]')).toContainText('localhost');
    await name.fill('Roof gateway');
    await setHost(page, 'mqtt://gw.roof.example');
    await expect(name).toHaveValue('Roof gateway');
    await name.fill('');
    await setHost(page, 'mqtt://edge.roof.example');
    await expect(name).toHaveValue('edge');
    // an existing profile keeps its name
    await page.click('[data-profile=lab]');
    await setHost(page, 'mqtt://10.20.4.18');
    await expect(page.locator('#pf-name')).toHaveValue('Lab Mosquitto');
  });

  test('C-29 per-connection skip: the Advanced switch asks "only this time" or "always"; ⚠ on card and tab; no switch in Settings', async ({ page }) => {
    await open(page, 'mock=demo&screen=connect');
    await page.click('[data-profile=lab]');
    await page.click('[data-testid=scheme-mqtts]');
    // the form saves after a short pause; the card's play button connects what is saved
    await expect.poll(async () => (await profile(page, 'lab')).scheme).toBe('mqtts');
    await page.click('[data-testid=fold-advanced]');
    const sw = page.locator('[data-testid=skip-verify]');
    await expect(sw).toHaveAttribute('aria-label', 'Skip unverified broker certificate');
    // "Only this time": nothing saved, "next connection only" until the next attempt, which skips the check
    await sw.click();
    const dlg = page.locator('[data-testid=skip-dialog]');
    await expect(dlg.locator('.mh')).toHaveText('Skip the broker certificate check?');
    await expect(dlg.locator('[data-testid=skip-risk]')).toHaveText('Without verification, anyone between you and the broker could pose as it and read or change your messages.');
    await expect(dlg.locator('.mf .btn')).toHaveText(['Cancel', 'Only this time', 'Always for this connection']);
    await page.click('[data-testid=skip-once-btn]');
    await expect(sw).toHaveAttribute('aria-checked', 'false');
    await expect(page.locator('[data-testid=skip-once]')).toHaveText('next connection only');
    await expect(page.locator('[data-testid=fold-advanced-summary]')).not.toContainText('skip verification');
    await page.click('[data-profile=lab] [data-testid=card-play]');
    await expect(page.locator('[data-profile=lab] [data-testid=card-stop]')).toBeVisible();
    await expect(page.locator('[data-testid=trust-dialog]')).toHaveCount(0);
    await expect(page.locator('[data-testid=skip-once]')).toHaveCount(0);
    await expect(page.locator('[data-profile=lab] [data-testid=card-warn]')).toHaveAttribute('data-kind', 'skip');
    await expect(page.locator('[data-testid=cbar-tab][data-conn=lab] [data-testid=cbar-warn]')).toHaveAttribute('data-kind', 'skip');
    expect((await profile(page, 'lab')).tls.skipVerify).toBe(false);
    // the next attempt verifies again
    await page.click('[data-profile=lab] [data-testid=card-stop]');
    await expect(page.locator('[data-profile=lab] [data-testid=card-warn]')).toHaveCount(0);
    await page.click('[data-profile=lab] [data-testid=card-play]');
    await expect(page.locator('[data-testid=trust-dialog]')).toBeVisible();
    await page.keyboard.press('Escape');
    // "Always for this connection": saved, summary in clay, ⚠ on the card while idle too
    await sw.click();
    await page.click('[data-testid=skip-always-btn]');
    await expect(sw).toHaveAttribute('aria-checked', 'true');
    await expect(page.locator('[data-testid=advanced-skip]')).toHaveText('skip verification');
    await expect(page.locator('[data-profile=lab] [data-testid=card-warn]')).toHaveAttribute('data-kind', 'skip');
    await expect.poll(async () => (await profile(page, 'lab')).tls.skipVerify).toBe(true);
    // turning it off needs no prompt
    await sw.click();
    await expect(page.locator('[data-testid=skip-dialog]')).toHaveCount(0);
    await expect(sw).toHaveAttribute('aria-checked', 'false');
    await expect.poll(async () => (await profile(page, 'lab')).tls.skipVerify).toBe(false);
    // Settings: trusted fingerprints, no skip switch
    await page.click('[data-testid=rail-settings]');
    await page.click('[data-testid=settings-nav-security]');
    await expect(page.locator('[data-testid=skip-verify]')).toHaveCount(0);
    await expect(page.locator('body')).toContainText('Trusted fingerprints');
  });

  test('C-30 certificate prompt: one list with "Trust this certificate" preselected, the two skips and the CA choice for a private CA', async ({ page }) => {
    await open(page, 'mock=demo&screen=connect');
    await page.click('[data-profile=lab]');
    await page.click('[data-testid=scheme-mqtts]');
    await page.waitForTimeout(500);
    await page.click('[data-testid=connect]');
    const dlg = page.locator('[data-testid=trust-dialog]');
    await expect(dlg.locator('.mh')).toHaveText("This broker's certificate cannot be verified");
    await expect(dlg.locator('[data-testid=trust-risk]')).toHaveText('Without verification, anyone between you and the broker could pose as it and read or change your messages.');
    await expect(dlg.locator('[role=radio]')).toHaveCount(3);
    await expect(dlg.locator('[data-testid=trust-certificate]')).toHaveAttribute('aria-checked', 'true');
    await expect(dlg.locator('[data-testid=trust-certificate]')).toContainText('This certificate only, for the connection "Lab Mosquitto".');
    await expect(dlg.locator('[data-testid=trust-ca]')).toHaveCount(0);
    await expect(dlg.locator('.mf .btn')).toHaveText(['Cancel', 'Connect']);
    // only this time: connects, saves nothing
    await dlg.locator('[data-testid=trust-skipOnce]').click();
    await page.click('[data-testid=trust-connect]');
    await expect(page.locator('[data-testid=cbar-tab][data-conn=lab]')).toBeVisible();
    await expect(page.locator('[data-testid=cbar-tab][data-conn=lab] [data-testid=cbar-warn]')).toHaveAttribute('data-kind', 'skip');
    let p = await profile(page, 'lab');
    expect(p.tls.skipVerify).toBe(false);
    expect(p.tls.trustedFingerprints).toEqual([]);
    // always: the profile switch is on
    await page.click('[data-testid=cbar-tab][data-conn=lab] [data-testid=cbar-close]');
    await expect(page.locator('[data-testid=cbar]')).toHaveAttribute('data-empty', 'true');
    await page.click('[data-testid=rail-connect]');
    await page.click('[data-profile=lab]');
    await page.click('[data-testid=connect]');
    await dlg.locator('[data-testid=trust-skipAlways]').click();
    await page.click('[data-testid=trust-connect]');
    await expect(page.locator('[data-testid=cbar-tab][data-conn=lab] [data-testid=cbar-warn]')).toBeVisible();
    await expect(page.locator('[data-testid=search]')).toBeVisible();
    p = await profile(page, 'lab');
    expect(p.tls.skipVerify).toBe(true);
    // a private CA adds the fourth choice; Test opens the same prompt
    await page.click('[data-testid=rail-connect]');
    await page.click('[data-testid=new-profile]');
    await page.locator('[data-testid=context-menu] .ctx-item', { hasText: 'New broker…' }).click();
    await setHost(page, 'mqtts://private.lab.example:8883');
    await expect(page.locator('#pf-name')).toHaveValue('private');
    await page.waitForTimeout(500);
    await page.click('[data-testid=try-connect]');
    await expect(dlg.locator('[role=radio]')).toHaveCount(4);
    await expect(dlg.locator('[data-testid=trust-ca]')).toContainText('Trust anything this CA signs');
    await dlg.locator('[data-testid=trust-ca]').click();
    await page.click('[data-testid=trust-connect]');
    await expect(page.locator('[data-testid=last-test]')).toContainText(/ms/);
    p = ((await mock(page, 'listProfiles')) as any[]).find((x) => x.name === 'private');
    expect(p.tls.caCert.fileName).toBe('lab-private-ca.pem');
    expect(p.tls.skipVerify).toBe(false);
  });
});

test.describe('Change set 8', () => {
  test('X-15 tree rows: name, payload, R, then sparkline, messages, throughput, subtopics with tooltips; never wrap', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await page.fill('[data-testid=search]', 'building/f2/r204');
    const folder = page.locator('[data-testid=tree-row][data-path="building/f2/r204"]');
    await expect(folder.locator('[data-testid=tree-subtopics]')).toHaveAttribute('title', 'Subtopics below this one');
    await expect(folder.locator('[data-testid=tree-msgs]')).toHaveAttribute('title', 'Messages since the connection opened');
    await expect(folder.locator('[data-testid=tree-rate]')).toHaveAttribute('title', 'Messages per second');
    await expect(folder.locator('[data-testid=tree-rate]')).toHaveText(/^[\d.,\u202f]+ msg\/s$/);
    const temp = page.locator('[data-testid=tree-row][data-path="building/f2/r204/temperature"]');
    await expect(temp.locator('[data-testid=tree-payload] em')).toContainText('{"value":');
    await expect(temp.locator('[data-testid=tree-payload] .chip')).toHaveText('R');
    await expect(temp.locator('.v > *')).toHaveCount(4); // sparkline, messages, throughput, (empty) subtopics: change set 9
    await expect(page.locator('[data-testid=activity-dot]')).toHaveCount(0);
    // every row is one line: its height is the row height and the payload is cut with an ellipsis
    const heights = await page.locator('[data-testid=tree-row]').evaluateAll((els) => els.map((e) => (e as HTMLElement).scrollHeight - (e as HTMLElement).clientHeight));
    expect(heights.every((h) => h <= 0)).toBe(true);
    const em = temp.locator('[data-testid=tree-payload] em');
    await expect(em).toHaveCSS('text-overflow', 'ellipsis');
    await expect(em).toHaveCSS('white-space', 'nowrap');
  });

  test('X-16 / X-17 a click on a JSON value opens the click menu; "Use as sparkline" draws it and survives a reload', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await selectTopic(page, 'building/f2/r205/temperature', 'building/f2/r205/temperature');
    const row = page.locator('[data-testid=tree-row][data-path="building/f2/r205/temperature"]');
    await expect(row.locator('svg.spark')).toHaveCount(0); // JSON: nothing until picked
    await expect(page.locator('[data-testid=detail] .charticon')).toHaveCount(0);
    await page.click('[data-testid=diff-toggle]');
    await page.locator('[data-testid=payload-json] .num[data-path="$.value"]').click();
    const menu = page.locator('[data-testid=json-menu]');
    await expect(menu.locator('.ctx-label')).toHaveText(['Use as sparkline', 'Add value to chart…', 'Show value in History', 'Copy path · $.value', /^Copy value · [\d.]+$/]);
    await menu.locator('.ctx-item', { hasText: 'Use as sparkline' }).click();
    await expect(row.locator('svg.spark')).toHaveCount(1);
    // a text value: the plot items are disabled
    await page.locator('[data-testid=payload-json] .k[data-path="$.unit"]').click();
    await expect(menu.locator('.ctx-item').first()).toHaveAttribute('aria-disabled', 'true');
    await expect(menu.locator('.ctx-item').first()).toHaveText('Use as sparkline · text, not plottable');
    await page.keyboard.press('Escape');
    // right-click on a topic still opens the topic menu
    await row.click({ button: 'right' });
    await expect(page.locator('[data-testid=context-menu] .ctx-label').first()).toHaveText(/^Pin|^Unpin/);
    await page.keyboard.press('Escape');
    const p = ((await page.evaluate(() => (window as any).__mqttpanelMock.engine.listProfiles())) as any[]).find((x) => x.id === 'plant');
    expect(p.topics['building/f2/r205/temperature'].sparklinePath).toBe('$.value');
    // a bare number draws by itself
    await page.fill('[data-testid=search]', 'building/f2/r205/humidity');
    await expect(page.locator('[data-testid=tree-row][data-path="building/f2/r205/humidity"] svg.spark')).toHaveCount(1);
  });

  test('X-19 topic chart: chips, + Add field… with this chart\'s fields checked, Also add to a dashboard chart', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await selectTopic(page, 'building/f2/ahu01', 'building/f2/ahu01/state');
    await expect(page.locator('[data-testid=chart-field]')).toHaveCount(2);
    await expect(page.locator('[data-testid=topic-chart]')).toBeVisible();
    await page.click('[data-testid=add-field]');
    const picker = page.locator('[data-testid=field-picker]');
    await expect(picker).toContainText('Fields on this chart · building/f2/ahu01/state');
    await expect(picker.locator('[role=checkbox][aria-checked=true]')).toHaveCount(2);
    await expect(page.locator('[data-testid=picker-also] option')).toHaveText(['None', 'New chart']);
    await page.click('[data-testid="picker-check-setpoint"]');
    await page.selectOption('[data-testid=picker-also]', 'new');
    await page.click('[data-testid=picker-add]');
    await expect(page.locator('[data-testid=chart-field]')).toHaveCount(3);
    await page.locator('[data-testid=chart-field][data-path="$.supply.fan"] .x').click();
    await expect(page.locator('[data-testid=chart-field]')).toHaveCount(2);
    await page.click('[data-testid=rail-charts]');
    await expect(page.locator('[data-testid=series-card]')).toHaveCount(3);
  });

  test('X-20 History lists every buffered message and the buffer can be raised for this topic', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await selectTopic(page, 'building/f2/r204/humidity', 'building/f2/r204/humidity');
    await page.click('[data-testid=follow]');
    const n = await page.locator('[data-testid=history-row]').count();
    expect(n).toBeGreaterThan(4);
    await expect(page.locator('[data-testid=detail]')).toContainText(`History · ${n}`);
    await expect(page.locator('[data-testid=detail]')).not.toContainText('Recent');
    await page.click('[data-testid=buffer-raise]');
    await page.locator('[role=dialog] input').fill('500');
    await page.locator('[role=dialog] .btn.pri').click();
    await expect(page.locator('[data-testid=buffer-line]')).toContainText('500');
  });

  test('X-21 detail header: star, Publish icon, ⋯; boxed quick publish', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await selectTopic(page, 'building/f2/r204/humidity', 'building/f2/r204/humidity');
    const pin = page.locator('[data-testid=pin-btn]');
    await expect(pin).toHaveText('☆');
    await expect(pin).toHaveAttribute('title', 'Pin');
    await pin.click();
    await expect(pin).toHaveText('★');
    await expect(pin).toHaveAttribute('title', 'Pinned');
    const qp = page.locator('[data-testid=quick-publish]');
    await expect(qp).toHaveCSS('border-top-width', '1px');
    await expect(qp).toHaveCSS('border-top-left-radius', '8px');
    await expect(qp.locator('.sep')).toHaveCount(4);
    await expect(qp).not.toContainText('Quick publish');
    await page.click('[data-testid=publish-here]');
    await expect(page.locator('[data-testid=pub-topic]')).toHaveValue('building/f2/r204/humidity');
  });

  test('X-22 the bar is always there, right of the rail (empty since change set 9, see X-23)', async ({ page }) => {
    await open(page, 'mock=demo&screen=explore');
    const bar = page.locator('[data-testid=cbar]');
    await expect(bar).toHaveAttribute('data-empty', 'true');
    const railBox = (await page.locator('.rail').boundingBox())!;
    const barBox = (await bar.boundingBox())!;
    expect(barBox.x).toBeGreaterThanOrEqual(railBox.x + railBox.width - 1);
    expect(railBox.y).toBeLessThanOrEqual(barBox.y);
  });

  test('D-24 series edited in place; the chart title renamed in place', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await selectTopic(page, 'building/f2/ahu01', 'building/f2/ahu01/state');
    await page.click('[data-testid=add-field]');
    await page.selectOption('[data-testid=picker-also]', 'new');
    await page.click('[data-testid=picker-add]');
    await page.click('[data-testid=rail-charts]');
    await expect(page.locator('[data-testid=series-card]')).toHaveCount(2);
    await expect(page.locator('[data-testid=series-pane]')).not.toContainText('Edit series');
    const card = page.locator('[data-testid=series-card]').first();
    await card.locator('[data-testid=series-name]').fill('supply');
    await card.locator('[data-testid=series-name]').press('Enter');
    await expect(page.locator('[data-testid=chart-pane] .legend')).toContainText('supply · $.supply.temp');
    await card.locator('[data-testid=series-colour]').click();
    await page.locator('[data-testid=series-palette] [aria-label=Brick]').click();
    await card.locator('[data-testid=series-more]').click();
    await expect(card.locator('[data-testid=series-details]')).toContainText('Unit');
    await page.click('[data-testid=chart-title]');
    await page.fill('input[data-testid=chart-title]', 'AHU 01 · supply');
    await page.keyboard.press('Enter');
    await expect(page.locator('[data-testid=chart-title]')).toContainText('AHU 01 · supply');
    await page.click('[data-testid=chart-title]');
    await page.fill('input[data-testid=chart-title]', 'nope');
    await page.keyboard.press('Escape');
    await expect(page.locator('[data-testid=chart-title]')).toContainText('AHU 01 · supply');
    await expect(page.locator('[data-testid=series-card]')).toHaveCount(2);
  });

  test('V-11 typed host names, topics and client IDs keep their lower case', async ({ page }) => {
    await open(page, 'mock=demo&screen=connect');
    await page.click('[data-profile=lab]');
    const bad = await page.evaluate(() =>
      Array.from(document.querySelectorAll('input, textarea'))
        .filter((e) => !['checkbox', 'radio', 'file'].includes((e as HTMLInputElement).type))
        .filter((e) => e.getAttribute('autocapitalize') !== 'off' || e.getAttribute('autocorrect') !== 'off' || e.getAttribute('spellcheck') !== 'false').length,
    );
    expect(bad).toBe(0);
    await page.locator('#pf-cid').fill('');
    await page.locator('#pf-cid').pressSequentially('myclient');
    await expect(page.locator('#pf-cid')).toHaveValue('myclient');
  });
});

test.describe('Change set 9', () => {
  const now = (page: Page) => page.evaluate(() => Date.now());

  test('X-23 empty connection bar: no text, no button; the strip keeps its height', async ({ page }) => {
    for (const lang of ['en', 'fr', 'de']) {
      await page.goto(`/?mock=demo&screen=explore&lang=${lang}&theme=light`);
      await page.waitForSelector('#app-root');
      const bar = page.locator('[data-testid=cbar]');
      await expect(bar).toHaveAttribute('data-empty', 'true');
      expect((await bar.innerText()).trim()).toBe('');
      await expect(bar.locator('button, [role=button], span')).toHaveCount(0);
      expect((await bar.boundingBox())!.height).toBeGreaterThanOrEqual(33);
    }
    // a connection fills it, closing it empties it again
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await expect(page.locator('[data-testid=cbar-tab]')).toHaveCount(1);
    await page.click('[data-testid=cbar-close]');
    await expect(page.locator('[data-testid=cbar]')).toHaveAttribute('data-empty', 'true');
  });

  test('X-24 rates are one-second figures written "n msg/s", changing only when a second completes', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await expect(page.locator('[data-testid=cbar-tab] small')).toHaveText(/[\d.,\u202f]+ msg\/s/);
    await page.fill('[data-testid=search]', 'rate/probe');
    // 50 messages inside one second
    await page.evaluate(() => {
      const m = (window as any).__mqttpanelMock.engine;
      const s = Math.floor(Date.now() / 1000) * 1000;
      for (let i = 0; i < 50; i++) m.inject('plant', 'rate/probe/x', String(i), { at: s + i * 10 });
    });
    const rate = page.locator('[data-testid=tree-row][data-path="rate/probe/x"] [data-testid=tree-rate]');
    await expect(rate).toHaveText('50 msg/s', { timeout: 3000 });
    // sample the figure: it changes only right after a whole second, never in between
    const seen: { t: number; v: string }[] = [];
    const end = (await now(page)) + 2500;
    while ((await now(page)) < end) {
      seen.push({ t: await now(page), v: (await rate.textContent()) ?? '' });
      await page.waitForTimeout(40);
    }
    const changes = seen.filter((x, i) => i > 0 && x.v !== seen[i - 1].v);
    expect(changes.length).toBeGreaterThanOrEqual(1); // 50 → 0
    for (const c of changes) expect(c.t % 1000).toBeLessThan(300);
    expect(seen[seen.length - 1].v).toBe('0 msg/s');
  });

  test('X-25 right-click on a sparkline hides it (saved); "Show sparkline" and a new pick bring it back', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await page.fill('[data-testid=search]', 'building/f2/r205');
    const hum = page.locator('[data-testid=tree-row][data-path="building/f2/r205/humidity"]');
    await hum.locator('[data-testid=tree-spark]').click({ button: 'right' });
    await expect(page.locator('[data-testid=context-menu] .ctx-label')).toHaveText(['Hide sparkline']);
    await menuItem(page, 'Hide sparkline');
    await expect(hum.locator('[data-testid=tree-spark]')).toHaveCount(0);
    let p = ((await page.evaluate(() => (window as any).__mqttpanelMock.engine.listProfiles())) as any[]).find((x) => x.id === 'plant');
    expect(p.topics['building/f2/r205/humidity'].sparklineOff).toBe(true);
    await hum.click({ button: 'right' });
    await menuItem(page, 'Show sparkline');
    await expect(hum.locator('[data-testid=tree-spark]')).toHaveCount(1);
    // hide again, then a new "Use as sparkline" pick shows it
    const temp = page.locator('[data-testid=tree-row][data-path="building/f2/r205/temperature"]');
    await temp.click();
    await page.locator('[data-testid=payload-json] .k[data-path="$.value"]').click();
    await page.click('[data-testid=json-menu] .ctx-item[data-id=spark]');
    await temp.locator('[data-testid=tree-spark]').click({ button: 'right' });
    await menuItem(page, 'Hide sparkline');
    await expect(temp.locator('[data-testid=tree-spark]')).toHaveCount(0);
    await page.locator('[data-testid=payload-json] .k[data-path="$.value"]').click();
    await page.click('[data-testid=json-menu] .ctx-item[data-id=spark]');
    await expect(temp.locator('[data-testid=tree-spark]')).toHaveCount(1);
    p = ((await page.evaluate(() => (window as any).__mqttpanelMock.engine.listProfiles())) as any[]).find((x) => x.id === 'plant');
    expect(p.topics['building/f2/r205/temperature'].sparklineOff).toBeUndefined();
  });

  test('X-26 History rows expand; the field filter from the click menu or the button, with the chip; each cancels', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await selectTopic(page, 'building/f2/ahu01', 'building/f2/ahu01/state');
    await page.click('[data-testid=follow]');
    const exp = page.locator('[data-testid=history-expand]');
    await exp.nth(0).click();
    await exp.nth(2).click();
    await expect(page.locator('[data-testid=history-payload]')).toHaveCount(2);
    await expect(page.locator('[data-testid=history-payload] [data-testid=history-json]')).toHaveCount(2);
    await expect(page.locator('[data-testid=history-payload] .add, [data-testid=history-payload] .del')).toHaveCount(0);
    await expect(page.locator('[data-testid=detail] .callout')).toHaveCount(0); // the chevron does not select
    // the click menu entry
    await page.locator('[data-testid=payload-json] .k[data-path="$.supply.temp"]').click();
    await page.click('[data-testid=json-menu] .ctx-item[data-id=history]');
    await expect(page.locator('[data-testid=history-col]')).toHaveText('supply.temp');
    await expect(page.locator('[data-testid=history-filter-chip]')).toContainText('supply.temp');
    await expect(page.locator('[data-testid=history-filter]')).toHaveAttribute('aria-pressed', 'true');
    await expect(page.locator('[data-testid=history-value]').first()).toHaveText(/^-?[\d.]+$/);
    await page.locator('[data-testid=payload-json] .k[data-path="$.supply.temp"]').click();
    await expect(page.locator('[data-testid=json-menu] .ctx-item[data-id=history]')).toHaveText('Stop showing supply.temp in History');
    await page.click('[data-testid=json-menu] .ctx-item[data-id=history]');
    await expect(page.locator('[data-testid=history-filter-chip]')).toHaveCount(0);
    await expect(page.locator('[data-testid=history-col]')).toHaveText(/changed keys/i);
    // the filter button and its field list; the chip's ✕ cancels
    await page.click('[data-testid=history-filter]');
    await page.locator('[data-testid=history-filter-pop] [data-path="$.mode"]').click();
    await expect(page.locator('[data-testid=history-col]')).toHaveText('mode');
    await expect(page.locator('[data-testid=history-value]').first()).toHaveText(/\S/);
    await page.click('[data-testid=history-filter-clear]');
    await expect(page.locator('[data-testid=history-filter-chip]')).toHaveCount(0);
    // the button itself cancels an active filter; the filter is kept per topic while the app runs
    await page.click('[data-testid=history-filter]');
    await page.locator('[data-testid=history-filter-pop] [data-path="$.supply.fan"]').click();
    await selectTopic(page, 'building/f2/r204', 'building/f2/r204/temperature');
    await expect(page.locator('[data-testid=history-filter-chip]')).toHaveCount(0);
    await selectTopic(page, 'building/f2/ahu01', 'building/f2/ahu01/state');
    await expect(page.locator('[data-testid=history-filter-chip]')).toContainText('supply.fan');
    await page.click('[data-testid=history-filter]');
    await expect(page.locator('[data-testid=history-filter-chip]')).toHaveCount(0);
  });

  test('X-27 JSON / Tree: folding rows with diff colours, the click menu on a row, History in the same form, kept per connection', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant,local&screen=explore');
    await selectTopic(page, 'building/f2/ahu01', 'building/f2/ahu01/state');
    await page.click('[data-testid=follow]');
    await page.click('[data-testid=payload-form-tree]');
    const tree = page.locator('[data-testid=payload-tree]');
    await expect(tree.locator('[data-testid=jtree-row][data-path="$.supply.temp"] .k')).toHaveText('temp');
    await expect(tree.locator('[data-testid=jtree-row][data-kind=changed]').first()).toBeVisible();
    await expect(tree.locator('.add').first()).toBeVisible();
    await tree.locator('[data-testid=jtree-row][data-path="$.supply"] .tchev').click();
    await expect(tree.locator('[data-testid=jtree-row][data-path="$.supply.temp"]')).toHaveCount(0);
    await tree.locator('[data-testid=jtree-row][data-path="$.supply"] .tchev').click();
    await tree.locator('[data-testid=jtree-row][data-path="$.supply.temp"]').click();
    await expect(page.locator('[data-testid=json-menu] .ctx-item[data-id=spark]')).toHaveText('Use as sparkline');
    await page.keyboard.press('Escape');
    await page.locator('[data-testid=history-expand]').first().click();
    await expect(page.locator('[data-testid=history-payload] [data-testid=history-tree]')).toHaveCount(1);
    // kept for this connection, not for the other one
    await selectTopic(page, 'building/f2/r204', 'building/f2/r204/temperature');
    await expect(page.locator('[data-testid=payload-tree]')).toHaveCount(1);
    await page.locator('[data-testid=cbar-tab][data-conn=local]').click();
    await page.fill('[data-testid=search]', '');
    await expect(page.locator('[data-testid=payload-form-tree]')).toHaveCount(0);
  });

  test('X-28 tree row columns line up across rows in EN, FR and DE', async ({ page }) => {
    for (const lang of ['en', 'fr', 'de']) {
      await page.goto(`/?mock=demo&connect=plant&screen=explore&lang=${lang}&theme=light`);
      await page.waitForSelector('#app-root');
      await page.fill('[data-testid=search]', 'building/f2/r204');
      await page.waitForSelector('[data-testid=tree-row][data-path="building/f2/r204/temperature"] [data-testid=tree-spark]');
      const cols = await page.locator('[data-testid=tree-row]').evaluateAll((rows) =>
        rows.map((r) => Array.from(r.querySelectorAll(':scope > .v > *')).map((c) => Math.round((c as HTMLElement).getBoundingClientRect().right))),
      );
      expect(cols.length).toBeGreaterThan(3);
      for (const c of cols) expect(c).toEqual(cols[0]);
      // the figures fit their columns
      const clipped = await page.locator('[data-testid=tree-row] .v > *').evaluateAll((els) => els.filter((e) => e.scrollWidth > e.clientWidth + 1).length);
      expect(clipped).toBe(0);
    }
  });
});

test.describe('Change set 11', () => {
  test('C-31 every info button is the round ⓘ: an 18 px circle with an "i", accent on hover; light and dark', async ({ page }) => {
    for (const theme of ['light', 'dark']) {
      for (const q of ['mock=demo&screen=connect', 'mock=demo&screen=broker']) {
        await page.goto(`/?${q}&lang=en&theme=${theme}`);
        await page.waitForSelector('#app-root');
        if (q.includes('connect')) {
          await page.click('[data-profile=plant]');
          await page.click('[data-testid=fold-security]');
          await page.click('[data-testid=fold-advanced]');
        } else await page.click('[data-testid=fold-brokerAdvanced]');
        const infos = page.locator('button[data-testid^=info-]');
        const n = await infos.count();
        expect(n).toBeGreaterThan(4);
        for (let i = 0; i < n; i++) {
          const b = infos.nth(i);
          if (!(await b.isVisible())) continue;
          await expect(b).toHaveText('i');
          await expect(b).toHaveClass(/(^| )info-btn( |$)/);
          await expect(b).not.toHaveClass(/chip/);
          const box = (await b.boundingBox())!;
          expect(Math.round(box.width)).toBe(18);
          expect(Math.round(box.height)).toBe(18);
          expect(await b.evaluate((el) => getComputedStyle(el).borderRadius)).toBe('50%');
        }
        await expect(page.locator('.chip.info')).toHaveCount(0);
      }
      // hover takes the accent colour
      const first = page.locator('[data-testid=info-reachable]');
      const before = await first.evaluate((el) => getComputedStyle(el).borderColor);
      await first.hover();
      await expect.poll(() => first.evaluate((el) => getComputedStyle(el).borderColor)).not.toBe(before);
      await page.screenshot({ path: `test-results/c31-info-${theme}.png` });
    }
  });

  test('C-32 accepted formats after Browse; the password eye reveals the typed and the stored password while pressed; no wrap in EN, FR, DE', async ({ page }) => {
    for (const lang of ['en', 'fr', 'de'] as const) {
      await page.goto(`/?mock=demo&screen=connect&lang=${lang}&theme=light`);
      await page.waitForSelector('#app-root');
      await page.click('[data-profile=plant]');
      await page.click('[data-testid=fold-security]');
      await expect(page.locator('[data-testid=formats-caCert]')).toHaveText('PEM, CRT, DER');
      await expect(page.locator('[data-testid=formats-clientCert]')).toHaveText('PEM, CRT, DER, P12, PFX');
      await expect(page.locator('[data-testid=formats-clientKey]')).toHaveText('PEM, KEY');
      for (const f of ['caCert', 'clientCert', 'clientKey']) {
        const row = page.locator(`[data-testid=file-${f}]`);
        const h = (await row.boundingBox())!.height;
        expect(h).toBeLessThan(34);
        // the formats follow the Browse button
        const b = (await page.locator(`[data-testid=browse-${f}]`).boundingBox())!;
        const fm = (await page.locator(`[data-testid=formats-${f}]`).boundingBox())!;
        expect(fm.x).toBeGreaterThan(b.x + b.width - 1);
      }
    }
    await open(page, 'mock=demo&screen=connect');
    await page.click('[data-profile=plant]');
    const pw = page.locator('[data-testid=pf-pw]');
    const eye = page.locator('[data-testid=pw-eye]');
    await expect(pw).toHaveAttribute('type', 'password');
    await eye.dispatchEvent('pointerdown');
    await expect(pw).toHaveAttribute('type', 'text');
    await expect(pw).toHaveValue('site-2026-commissioning');
    await eye.dispatchEvent('pointerup');
    await expect(pw).toHaveAttribute('type', 'password');
    await expect(pw).toHaveValue('');
    await pw.fill('typed-now');
    await eye.dispatchEvent('pointerdown');
    await expect(pw).toHaveValue('typed-now');
    await expect(pw).toHaveAttribute('type', 'text');
    await eye.dispatchEvent('pointerup');
    await expect(pw).toHaveAttribute('type', 'password');
    // the copy icon stays reserved for the password the app generated
    await expect(page.locator('[data-testid=profile-form] [data-testid$=-copy]')).toHaveCount(0);
    await page.click('[data-profile=local]');
    await expect(page.locator('[data-testid=profile-pw-copy]')).toBeVisible();
  });

  test('C-33 generate key, download CSR, import signed certificate; a foreign certificate is refused; switching asks; Start over clears', async ({ page }) => {
    await open(page, 'mock=demo&screen=connect');
    await page.click('[data-profile=plant]');
    await page.click('[data-testid=fold-security]');
    await expect(page.locator('[data-testid=cert-way-import]')).toHaveAttribute('aria-checked', 'true');
    await expect(page.locator('[data-testid=fold-security-summary]')).toContainText('client certificate (imported)');
    // switching with imported material asks first
    await page.click('[data-testid=cert-way-generate]');
    await expect(page.locator('[data-testid=cert-way-confirm] .mh')).toHaveText('Discard the imported certificate and key?');
    await page.click('[data-testid=confirm-ok]');
    await expect(page.locator('[data-testid=gen-card]')).toBeVisible();
    await expect(page.locator('[data-testid=file-caCert]')).toBeVisible();
    await expect(page.locator('[data-testid=gen-csr]')).toBeDisabled();
    // RSA as a second choice, ECDSA by default
    await expect(page.locator('[data-testid=gen-alg] [aria-checked=true]')).toHaveText('ECDSA P-256');
    await page.click('[data-testid=gen-key]');
    await expect(page.locator('[data-testid=gen-key-line]')).toHaveText(/^Key generated · ECDSA P-256 · [0-9A-F:]+…[0-9A-F:]+$/);
    for (const id of ['certWay', 'genKey', 'genCsr', 'genCert']) await expect(page.locator(`[data-testid=info-${id}]`)).toBeVisible();
    await expect(page.locator('[data-testid=gen-cn]')).toHaveValue('mqttpanel-7f3a');
    await page.fill('[data-testid=gen-cn]', 'commissioning-laptop');
    await page.click('[data-testid=gen-csr]');
    await expect.poll(async () => (await saves(page)).map((s) => s.fileName)).toContain('commissioning-laptop.csr');
    // a dropped certificate for another key is refused with the plain sentence
    const box = (await page.locator('[data-testid=file-signedCert]').boundingBox())!;
    await page.evaluate(([x, y]) => (window as any).__mqttpanelMock.platform.simulateDrop(['/dropped/other-device.crt'], x, y), [box.x + 20, box.y + box.height / 2]);
    await expect(page.locator('[data-testid=file-error-signedCert]')).toHaveText('This certificate does not match the generated key.');
    await page.click('[data-testid=browse-signedCert]');
    await expect(page.locator('[data-testid=file-signedCert]')).toContainText('mqttpanel-7f3a.crt');
    await expect(page.locator('[data-testid=formats-signedCert]')).toHaveText('PEM, CRT, DER');
    await expect(page.locator('[data-testid=fold-security-summary]')).toContainText('client certificate (generated key)');
    // switching back asks about the generated key
    await page.click('[data-testid=cert-way-import]');
    await expect(page.locator('[data-testid=cert-way-confirm] .mh')).toHaveText('Discard the generated key?');
    await page.click('[data-testid=cert-way-confirm] .mf .btn >> nth=0');
    await expect(page.locator('[data-testid=gen-card]')).toBeVisible();
    // Start over clears after a confirmation
    await page.click('[data-testid=gen-start-over]');
    await expect(page.locator('[data-testid=start-over-confirm] .mh')).toHaveText('Discard the generated key and its certificate?');
    await page.click('[data-testid=confirm-ok]');
    await expect(page.locator('[data-testid=gen-key]')).toBeVisible();
    await expect(page.locator('[data-testid=file-signedCert]')).not.toContainText('.crt');
    // with nothing on the other side, switching is silent
    await page.click('[data-testid=cert-way-import]');
    await expect(page.locator('[data-testid=cert-way-confirm]')).toHaveCount(0);
    await expect(page.locator('[data-testid=file-clientKey]')).toBeVisible();
  });

  test('C-34 session expiry shows the unit only; its ⓘ explains 0 and empty; an empty field saves as 0', async ({ page }) => {
    await open(page, 'mock=demo&screen=connect');
    await page.click('[data-profile=plant]');
    await page.click('[data-testid=fold-advanced]');
    const row = page.locator('.f', { has: page.locator('#pf-se') });
    await expect(row).not.toContainText('0 = none');
    await expect(row.locator('.hint')).toHaveText('s');
    await page.click('[data-testid=info-sessionExpiry]');
    await expect(page.locator('[data-testid=bubble-text-sessionExpiry]')).toHaveText(
      'How long the broker keeps your session after a disconnect: subscriptions and queued messages. Empty or 0: the session ends with the connection.',
    );
    await page.keyboard.press('Escape');
    await page.fill('#pf-se', '300');
    await page.fill('#pf-se', '');
    await expect(page.locator('#pf-se')).toHaveValue('0');
    await expect
      .poll(async () => page.evaluate(async () => ((await (window as any).__mqttpanelMock.engine.listProfiles()) as { id: string; sessionExpiry: number }[]).find((p) => p.id === 'plant')!.sessionExpiry))
      .toBe(0);
  });

  test('B-27 Start is the accent primary button, Stop outlined in brick; the Status tile play/stop does the same', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    await expect(page.locator('[data-testid=broker-stop]')).toHaveClass(/danger/);
    await expect(page.locator('[data-testid=broker-stop]')).toHaveClass(/out/);
    await expect(page.locator('[data-testid=tile-start]')).toHaveCount(0);
    await page.click('[data-testid=tile-stop]');
    await expect(page.locator('[data-testid=broker-tiles] .stat').first()).toContainText('Stopped');
    const start = page.locator('[data-testid=broker-start]');
    await expect(start).toHaveClass(/pri/);
    const bg = await start.evaluate((el) => getComputedStyle(el).backgroundColor);
    const connectBg = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--sage').trim());
    expect(bg).not.toBe('rgba(0, 0, 0, 0)');
    expect(connectBg).not.toBe('');
    // the tile play button sits at the right end of the value line
    const tile = (await page.locator('[data-testid=broker-tiles] .stat').first().boundingBox())!;
    const play = (await page.locator('[data-testid=tile-start]').boundingBox())!;
    expect(play.x + play.width).toBeGreaterThan(tile.x + tile.width - 30);
    await page.click('[data-testid=tile-start]');
    await expect(page.locator('[data-testid=tile-stop]')).toBeVisible();
    await expect(page.locator('[data-testid=broker-stop]')).toBeVisible();
    // the header does the same
    await page.click('[data-testid=broker-stop]');
    await expect(page.locator('[data-testid=tile-start]')).toBeVisible();
  });

  test('B-29 "Export…" on the Server certificate line saves the served chain; Regenerate takes extra names', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    await page.click('[data-testid=fold-brokerAdvanced]');
    const line = page.locator('[data-testid=server-cert] button');
    await expect(line).toHaveText(['Import…', 'Regenerate…', 'Export…']);
    await page.click('[data-testid=export-server-cert]');
    await expect.poll(async () => (await saves(page)).map((s) => s.fileName)).toContain('server-chain.pem');
    await page.click('[data-testid=regenerate]');
    await expect(page.locator('[data-testid=regen-auto]')).toContainText('localhost, 127.0.0.1, ::1');
    await expect(page.locator('[data-testid=info-extraNames]')).toBeVisible();
    await page.fill('[data-testid=regen-extra]', 'broker.example.com, not_a_name');
    await page.click('[data-testid=regen-ok]');
    await expect(page.locator('[data-testid=regen-error]')).toContainText('not_a_name');
    await page.fill('[data-testid=regen-extra]', 'broker.example.com 203.0.113.7');
    await page.click('[data-testid=regen-ok]');
    await expect(page.locator('[data-testid=regen-dialog]')).toHaveCount(0);
    const san = await page.evaluate(async () => (await (window as any).__mqttpanelMock.engine.brokerCerts()).server.san as string[]);
    expect(san).toEqual(expect.arrayContaining(['localhost', '192.168.1.20', 'broker.example.com', '203.0.113.7']));
    await page.click('[data-testid=regenerate]');
    await expect(page.locator('[data-testid=regen-extra]')).toHaveValue('broker.example.com, 203.0.113.7');
  });
});

test.describe('Assistant (docs/MCP.md section 6)', () => {
  const hostFiles = (page: Page) => page.evaluate(() => (window as any).__mqttpanelMock.engine.hostFiles() as Record<string, string>);

  test('M-10 "Add to Claude" and "Add to ChatGPT" write the files of the (virtual) home; the snippet copies', async ({ page }) => {
    await open(page, 'mock=demo&screen=settings');
    await page.click('[data-testid=settings-nav-assistant]');
    const section = page.locator('[data-testid=settings-assistant]');
    await expect(section).toContainText('Over this interface an assistant can change everything in MQTT Panel');
    await expect(page.locator('[data-testid=assistant-claude-state]')).toHaveText('Not set up');
    await page.click('[data-testid=assistant-add-claude]');
    await expect(page.locator('[data-testid=assistant-confirm-files] li')).toHaveCount(2);
    await page.click('[data-testid=confirm-ok]');
    await expect(page.locator('[data-testid=assistant-claude-state]')).toHaveText('Set up in Claude Desktop, Claude Code');
    let files = await hostFiles(page);
    expect(JSON.parse(files['~/.claude.json']).numStartups).toBe(4);
    expect(JSON.parse(files['~/.claude.json']).mcpServers.mqttpanel).toEqual({ type: 'stdio', command: expect.stringContaining('MQTT Panel'), args: ['mcp'], env: {} });
    expect(JSON.parse(files['~/Library/Application Support/Claude/claude_desktop_config.json']).mcpServers.mqttpanel.args).toEqual(['mcp']);
    await page.click('[data-testid=assistant-add-chatgpt]');
    await expect(page.locator('[data-testid=assistant-confirm]')).toContainText('ChatGPT on the web cannot reach a program on this computer');
    await page.click('[data-testid=confirm-ok]');
    await expect(page.locator('[data-testid=assistant-chatgpt-state]')).toHaveText('Set up');
    files = await hostFiles(page);
    expect(files['~/.codex/config.toml']).toMatch(/^\[mcp_servers\.mqttpanel\]\ncommand = ".+"\nargs = \["mcp"\]\n$/);
    await page.click('[data-testid=assistant-copy]');
    expect(JSON.parse((await copied(page))!).mcpServers.mqttpanel.args).toEqual(['mcp']);
  });

  test('M-10 the acknowledgement: dialog on connect, Accept shows the date on the page', async ({ page }) => {
    await open(page, 'mock=demo&screen=connect');
    await page.evaluate(() => {
      const m = (window as any).__mqttpanelMock.engine;
      m.assistantEvent({ kind: 'session', connected: true, host: 'Claude Desktop' });
      m.assistantEvent({ kind: 'acknowledge' });
    });
    await expect(page.locator('[data-testid=assistant-ack]')).toContainText('Claude Desktop is connected to MQTT Panel.');
    await page.click('[data-testid=assistant-ack-accept]');
    await expect(page.locator('[data-testid=assistant-ack]')).toHaveCount(0);
    await page.evaluate(() => (window as any).__mqttpanelMock.engine.assistantEvent({ kind: 'navigate', screen: 'settings', page: 'assistant' }));
    await expect(page.locator('[data-testid=toast]', { hasText: 'The assistant moved the view.' })).toBeVisible();
    await expect(page.locator('[data-testid=assistant-accepted]')).toContainText('Accepted on');
    await expect(page.locator('[data-testid=assistant-session]')).toHaveText('Claude Desktop');
  });

  test('M-10 a password asked in the app goes to the app, not the chat', async ({ page }) => {
    await open(page, 'mock=demo&screen=connect');
    await page.evaluate(() => (window as any).__mqttpanelMock.engine.assistantEvent({ kind: 'request', id: 'r7', request: 'password', profileId: 'plant', profileName: 'Plant broker' }));
    await page.fill('[data-testid=assistant-password]', 'typed-in-the-app');
    await page.click('[data-testid=assistant-request-ok]');
    await expect(page.locator('[data-testid=assistant-request]')).toHaveCount(0);
    const answers = await page.evaluate(() => (window as any).__mqttpanelMock.engine.assistantAnswers());
    expect(answers).toEqual([{ requestId: 'r7', answer: { password: 'typed-in-the-app' } }]);
  });

  test('M-16 a new client certificate: the folder and the PKCS#12 passphrase are chosen in the app', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    await page.evaluate(() =>
      (window as any).__mqttpanelMock.engine.assistantEvent({ kind: 'request', id: 'r1', request: 'issueCert', profileId: '', profileName: 'sensor-12', format: 'pkcs12', withPassphrase: true }),
    );
    const dlg = page.locator('[data-testid=assistant-request]');
    await expect(dlg).toContainText('The assistant issues the client certificate sensor-12');
    await expect(page.locator('[data-testid=assistant-request-ok]')).toBeDisabled();
    await page.click('[data-testid=choose-assistant-place]');
    await expect(page.locator('[data-testid=file-assistant-place]')).toContainText('~/Documents');
    await page.fill('[data-testid=assistant-passphrase]', 'bundle-typed-here');
    await page.click('[data-testid=assistant-request-ok]');
    await expect(dlg).toHaveCount(0);
    // an export: the save place
    await page.evaluate(() =>
      (window as any).__mqttpanelMock.engine.assistantEvent({ kind: 'request', id: 'r2', request: 'saveFile', profileId: '', profileName: 'chart.csv', defaultName: 'chart.csv' }),
    );
    await page.click('[data-testid=choose-assistant-place]');
    await page.click('[data-testid=assistant-request-ok]');
    const answers = await page.evaluate(() => (window as any).__mqttpanelMock.engine.assistantAnswers());
    expect(answers).toEqual([
      { requestId: 'r1', answer: { path: '~/Documents', passphrase: 'bundle-typed-here' } },
      { requestId: 'r2', answer: { path: '~/Documents/chart.csv', passphrase: null } },
    ]);
  });

  test('M-17 charts made by the assistant show on the Charts screen; export_chart as PNG gets the drawn chart', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await page.evaluate(() =>
      (window as any).__mqttpanelMock.engine.assistantEvent({
        kind: 'charts',
        charts: [
          {
            id: 'ac1',
            name: 'Room 204',
            connId: 'plant',
            range: 'session',
            customFrom: null,
            customTo: null,
            paused: false,
            pausedAt: null,
            series: [{ id: 'as2', name: 'setpoint', topic: 'building/f2/r204/setpoint', path: '$.value', color: 'clay', style: 'step', axis: 'left', unit: '°C', enabled: true }],
          },
        ],
      }),
    );
    await page.evaluate(() => (window as any).__mqttpanelMock.engine.assistantEvent({ kind: 'request', id: 'r3', request: 'chartPng', profileId: '', profileName: 'Room 204', chartId: 'ac1' }));
    await expect(page.locator('[data-testid=chart-title]')).toContainText('Room 204');
    await expect(page.locator('[data-testid=assistant-request]')).toHaveCount(0);
    await expect.poll(async () => (await page.evaluate(() => (window as any).__mqttpanelMock.engine.assistantAnswers())).length).toBe(1);
    const [a] = await page.evaluate(() => (window as any).__mqttpanelMock.engine.assistantAnswers());
    expect(a.requestId).toBe('r3');
    expect(a.answer.data.startsWith('iVBORw0KGgo')).toBe(true);
    expect(a.answer.data.length).toBeGreaterThan(1000);
    // the window's charts go back to the app side
    const synced = await page.evaluate(() => (window as any).__mqttpanelMock.engine.assistantCharts());
    expect(synced.map((c: { id: string }) => c.id)).toEqual(['ac1']);
  });
});

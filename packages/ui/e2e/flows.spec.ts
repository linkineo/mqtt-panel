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
    await expect(page.locator('[data-testid=context-menu] .ctx-label')).toHaveText(['Another broker…', 'MQTT Panel broker']);
    await page.locator('[data-testid=context-menu] .ctx-item', { hasText: 'MQTT Panel broker' }).hover();
    await expect(page.locator('[data-testid=context-submenu] .ctx-label')).toHaveText(['Use an existing listener', 'Add a listener…']);
    await page.locator('[data-testid=context-menu] .ctx-item', { hasText: 'Another broker…' }).click();
    await expect(page.locator('[data-testid=profile-form] h5')).toHaveText('New connection');
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
    await expect(page.locator('[data-testid=context-menu] .ctx-label')).toHaveText(['Another broker…', 'MQTT Panel broker']);
    await page.keyboard.press('Escape');
    await page.click('[data-testid=welcome-broker]');
    await expect(page.locator('[data-testid=app-broker]')).toBeVisible();
    await expect(page.locator('[data-testid=appb-listener]').first()).toHaveAttribute('aria-checked', 'true');
  });

  test('C-13 "MQTT Panel broker" > "Add a listener…": no login row, then the steps run and the Explorer opens', async ({ page }) => {
    await open(page, 'mock=demo');
    await page.click('[data-testid=new-profile]');
    await page.locator('[data-testid=context-menu] .ctx-item', { hasText: 'MQTT Panel broker' }).hover();
    await page.locator('[data-testid=context-submenu] .ctx-item', { hasText: 'Add a listener…' }).click();
    await page.click('[data-testid=preset-mtls]');
    await page.click('[data-testid=addl-next]');
    const step2 = page.locator('[data-testid=add-listener-2]');
    await expect(step2).not.toContainText('Login');
    await expect(step2.locator('input[type=password]')).toHaveCount(0);
    await expect(page.locator('[data-testid=addl-after]')).toContainText('restarts the broker if needed and connects');
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
    // ✕ disconnects; the bar disappears with the last open connection
    await page.locator('[data-testid=cbar-tab][aria-selected=true] [data-testid=cbar-close]').click();
    await expect(tabs).toHaveCount(1);
    await expect(page.locator('[data-testid=cbar-tab][aria-selected=true]')).toContainText('Plant broker');
    await page.locator('[data-testid=cbar-close]').click();
    await expect(page.locator('[data-testid=cbar]')).toHaveCount(0);
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
    await expect(page.locator('[data-testid=cbar]')).toHaveCount(0);
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

  test('X-11 diff shows the changes and takes any history entry as baseline', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await selectTopic(page, 'building/f2/ahu01', 'building/f2/ahu01/state');
    await page.click('[data-testid=tab-diff]');
    await expect(page.locator('[data-testid=diff-changes]')).toBeVisible();
    await expect(page.locator('[data-testid=diff] .add, [data-testid=diff] .chg, [data-testid=diff] .del').first()).toBeVisible();
    const base = page.locator('[data-testid=detail] table.tbl tbody tr').nth(2).locator('button');
    await base.click();
    await expect(base).toHaveClass(/pri/);
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
    await page.click('[data-testid=tab-chart]');
    await page.click('[data-testid=add-field]');
    await page.click('[data-testid="picker-check-supply.temp"]');
    await page.click('[data-testid=picker-add]');
    await page.click('[data-testid=rail-charts]');
    await page.waitForSelector('[data-testid=series-card]');
  }

  test('D-18 clicking a key in the last message writes its path into the series', async ({ page }) => {
    await chartWithSeries(page);
    await page.locator('[data-testid=series-card] .t button').first().click();
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
    await page.click('[data-testid=tab-chart]');
    await page.click('[data-testid=add-field]');
    await page.click('[data-testid="picker-check-supply.temp"]');
    await page.click('[data-testid="picker-check-alarm.active"]');
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
    await expect(page.locator('[data-testid=restart-chip]')).toBeVisible();
    await page.click('[data-testid=restart-later]');
    await page.click('[data-testid=fold-brokerAdvanced]');
    await expect(page.locator('[data-testid=require-cert-seg] button.on')).toHaveText('Required');
  });

  test('B-12 "MQTT Panel broker" > "Use an existing listener" keeps one profile and one user after 3 runs', async ({ page }) => {
    await open(page, 'mock=empty');
    for (let i = 0; i < 3; i++) {
      await page.click('[data-testid=rail-connect]');
      await page.click(i === 0 ? '[data-testid=welcome-broker]' : '[data-testid=new-profile]');
      if (i > 0) {
        await page.locator('[data-testid=context-menu] .ctx-item', { hasText: 'MQTT Panel broker' }).hover();
        await page.locator('[data-testid=context-submenu] .ctx-item', { hasText: 'Use an existing listener' }).click();
      }
      await expect(page.locator('[data-testid=appb-listener][aria-checked=true]')).toHaveAttribute('data-listener', 'l-tls');
      await page.click('[data-testid=appb-connect]');
      if (i === 0) await expect(page.locator('[data-testid=appb-step-broker]')).toBeVisible();
      await expect(page.locator('[data-testid=explorer]')).toBeVisible({ timeout: 8000 });
    }
    await page.click('[data-testid=rail-connect]');
    await expect(page.locator('[data-testid=profile-card]', { hasText: 'Local broker' })).toHaveCount(1);
    await page.click('[data-testid=rail-broker]');
    await expect(page.locator('[data-testid=user-row]')).toHaveCount(1);
    await expect(page.locator('[data-testid=user-row] [data-testid=user-local-tag]')).toHaveText('Local broker');
    await expect(page.locator('[data-testid=connect-here]')).toHaveCount(0);
  });

  test('B-15 restart prompt: Later leaves a pulsing "Restart broker and apply changes"; users never prompt', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    await page.click('[data-testid=port-l-tls]');
    await page.fill('[data-testid=port-input-l-tls]', '8884');
    await page.keyboard.press('Enter');
    await expect(page.locator('[data-testid=restart-prompt] .mh')).toHaveText('Restart the broker to apply the change?');
    await expect(page.locator('[data-testid=restart-pending-list]')).toHaveText('Pending: TLS listener port 8883 → 8884.');
    await page.click('[data-testid=restart-later]');
    const btn = page.locator('[data-testid=broker-restart]');
    await expect(btn).toHaveText('Restart broker and apply changes');
    expect(await btn.evaluate((el) => getComputedStyle(el).animationName)).toBe('restart-pulse');
    await expect(page.locator('[data-testid=restart-chip]')).toHaveText('restart pending');
    await expect(page.locator('[data-testid=port-pending]')).toBeVisible();
    // the pending state survives navigation
    await page.click('[data-testid=rail-explore]');
    await page.click('[data-testid=rail-broker]');
    await expect(btn).toHaveText('Restart broker and apply changes');
    // reachability (broker-wide) and adding a listener prompt too
    await page.click('[data-testid=reach-localhost]');
    await expect(page.locator('[data-testid=restart-pending-list]')).toContainText('listeners reachable from this computer only');
    await page.click('[data-testid=restart-later]');
    await page.click('[data-testid=add-listener]');
    await page.click('[data-testid=addl-next]');
    await page.fill('[data-testid=addl-port]', '8887');
    await page.click('[data-testid=addl-add]');
    await expect(page.locator('[data-testid=restart-prompt]')).toBeVisible();
    await page.click('[data-testid=restart-later]');
    // users and Anonymous apply live and never prompt
    await page.click('[data-testid=add-user]');
    await page.fill('#cred-user', 'tester');
    await page.fill('#cred-pass', 'pw-tester');
    await page.click('[data-testid=cred-ok]');
    await expect(page.locator('[data-testid=user-row][data-user=tester]')).toBeVisible();
    await page.click('[data-testid=anonymous]');
    await page.click('[data-testid=confirm-ok]');
    await expect(page.locator('[data-testid=restart-prompt]')).toHaveCount(0);
    await page.click('[data-testid=anonymous]');
    await btn.click();
    await expect(btn).toHaveText('Restart');
    await expect(page.locator('[data-testid=restart-chip]')).toHaveCount(0);
    await expect(page.locator('[data-testid=port-pending]')).toHaveCount(0);
  });

  test('B-15 reduced motion: the pending Restart button is highlighted without pulsing', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await open(page, 'mock=demo&screen=broker');
    await page.click('[data-testid=port-l-tls]');
    await page.fill('[data-testid=port-input-l-tls]', '8884');
    await page.keyboard.press('Enter');
    await page.click('[data-testid=restart-now]');
    await expect(page.locator('[data-testid=broker-restart]')).toHaveText('Restart');
    await expect(page.locator('[data-testid=broker-restart]')).toBeEnabled();
    await page.click('[data-testid=port-l-tls]');
    await page.fill('[data-testid=port-input-l-tls]', '8885');
    await page.keyboard.press('Enter');
    await page.click('[data-testid=restart-later]');
    const btn = page.locator('[data-testid=broker-restart]');
    await expect(btn).toHaveClass(/pulse/);
    expect(await btn.evaluate((el) => getComputedStyle(el).animationName)).toBe('none');
    expect(await btn.evaluate((el) => getComputedStyle(el).boxShadow)).not.toBe('none');
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
    await expect(page.locator('[data-testid=restart-prompt]')).toBeVisible();
    await expect(page.locator('[data-testid=restart-pending-list]')).toHaveText('Pending: client certificate optional on encrypted listeners.');
    await page.click('[data-testid=restart-later]');
    await expect(page.locator('[data-testid=fold-brokerAdvanced-summary]')).toContainText('client certificates optional');
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
    await expect(page.locator('[data-testid=cbar]')).toHaveCount(0);
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
    await expect(page.locator('[data-testid=cbar]')).toHaveCount(0);
  });

  test('C-19 Security and Advanced are folds, closed at start, with a summary; the state is kept while the app runs', async ({ page }) => {
    await open(page, 'mock=demo&screen=connect');
    await page.click('[data-profile=plant]');
    const sec = page.locator('[data-testid=fold-security]');
    const adv = page.locator('[data-testid=fold-advanced]');
    await expect(sec).toHaveAttribute('aria-expanded', 'false');
    await expect(adv).toHaveAttribute('aria-expanded', 'false');
    await expect(page.locator('[data-testid=profile-form] .sec')).toHaveText(['Basics']);
    await expect(page.locator('[data-testid=fold-security-summary]')).toHaveText('verify on · CA · client certificate · verified');
    await expect(page.locator('[data-testid=fold-advanced-summary]')).toHaveText('keep-alive 60 s · 3 subscriptions · 1 schema');
    await expect(page.locator('[data-testid=info-caCert]')).toHaveCount(0);
    await sec.click();
    await expect(page.locator('[data-testid=info-caCert]')).toBeVisible();
    for (const id of ['verifyServer', 'caCert', 'clientCert', 'clientKey', 'sni', 'alpn']) await expect(page.locator(`[data-testid=info-${id}]`)).toBeVisible();
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
    await page.locator('[data-testid=context-menu] .ctx-item', { hasText: 'Another broker…' }).click();
    await expect(page.locator('[data-testid=fold-advanced-summary]')).toHaveCount(0);
    await page.click('[data-testid=scheme-mqtts]');
    await expect(page.locator('[data-testid=fold-security-summary]')).toHaveCount(0);
    // closed again after a restart of the app
    await open(page, 'mock=demo&screen=connect');
    await expect(page.locator('[data-testid=fold-security]')).toHaveAttribute('aria-expanded', 'false');
  });

  test('C-20 no parsed chips next to file fields or the password; no "mTLS"; cards show TLS, WSS or no TLS', async ({ page }) => {
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
    expect(new Set(chips)).toEqual(new Set(['TLS', 'no TLS', 'WSS']));
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

  test('B-21 reachability is one broker-wide choice with an ⓘ; switching asks to restart', async ({ page }) => {
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
    await page.click('[data-testid=reach-localhost]');
    await expect(page.locator('[data-testid=restart-pending-list]')).toHaveText('Pending: listeners reachable from this computer only.');
    await page.click('[data-testid=restart-later]');
    await expect(page.locator('[data-testid=reach-localhost]')).toHaveClass(/on/);
    await expect(page.locator('[data-testid=unencrypted]')).toHaveCount(0);
    await expect(page.locator('[data-testid=restart-chip]')).toHaveText('restart pending');
  });

  test('B-22 listener table: On, Kind, Port, Setup and ✕, which removes at once and asks to restart', async ({ page }) => {
    await open(page, 'mock=demo&screen=broker');
    await expect(page.locator('[data-testid=listeners] th')).toHaveText(['On', 'Kind', 'Port', 'Setup', '']);
    await expect(page.locator('[data-testid=listener-row] button', { hasText: '⋯' })).toHaveCount(0);
    const before = await page.locator('[data-testid=listener-row]').count();
    await expect(page.locator('[data-testid=remove-listener-l-ws]')).toHaveAttribute('aria-label', 'Remove the WebSocket listener on port 8083');
    await page.click('[data-testid=remove-listener-l-ws]');
    await expect(page.locator('[data-testid=confirm-ok]')).toHaveCount(0);
    await expect(page.locator('[data-testid=listener-row]')).toHaveCount(before - 1);
    await expect(page.locator('[data-testid=restart-pending-list]')).toHaveText('Pending: WebSocket listener on port 8083 removed.');
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
    await expect(certs.locator('.hd button')).toHaveText(['ⓘ', 'Import CSR…', 'Issue…']);
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
    await expect(adv.locator('[data-testid=server-cert] button')).toHaveText(['Import…', 'Regenerate…']);
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

  test('V-10 title bar holds the name and the mark; rail badges; "unverified" chip on tab and card', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant,local&screen=connect');
    await expect(page.locator('.tb')).toHaveText('MQTT Panel');
    await expect(page.locator('.tb .mark svg')).toBeVisible();
    await expect(page.locator('[data-testid=pill]')).toHaveCount(0);
    await expect(page.locator('[data-testid=rail-broker-badge]')).toHaveAttribute('data-state', 'running');
    await expect(page.locator('[data-testid=rail-connect-count]')).toHaveText('2');
    await expect(page.locator('[data-testid=rail-connect]')).toHaveAttribute('aria-label', 'Connect, 2 connections open');
    await expect(page.locator('[data-testid=cbar-unverified]')).toHaveCount(0);
    // the lab-only skip-verify setting: "unverified" on the tab and the card of the encrypted connection
    await page.click('[data-testid=rail-settings]');
    await page.click('[data-testid=skip-verify]');
    await page.click('[data-testid=rail-connect]');
    await expect(page.locator('[data-testid=cbar-tab][data-conn=plant] [data-testid=cbar-unverified]')).toHaveText('unverified');
    await expect(page.locator('[data-profile=plant] [data-testid=card-unverified]')).toHaveText('unverified');
    await expect(page.locator('[data-profile=lab] [data-testid=card-unverified]')).toHaveCount(0);
    // "anonymous" and "restart pending" live on the Broker screen only
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

  test('C-24 "MQTT Panel broker" in the menu, the dialog title and the card meta line, in EN, FR and DE', async ({ page }) => {
    for (const [lang, item, title, meta] of [
      ['en', 'MQTT Panel broker', 'Connect to the MQTT Panel broker', 'MQTT Panel broker'],
      ['fr', 'Broker MQTT Panel', 'Se connecter au broker MQTT Panel', 'Broker MQTT Panel'],
      ['de', 'MQTT Panel Broker', 'Mit dem MQTT Panel Broker verbinden', 'MQTT Panel Broker'],
    ]) {
      await page.goto(`/?mock=demo&screen=connect&lang=${lang}&theme=light`);
      await page.waitForSelector('#app-root');
      await expect(page.locator('[data-profile=local] .s').nth(1)).toContainText(meta);
      await page.click('[data-testid=new-profile]');
      await expect(page.locator('[data-testid=context-menu] .ctx-label').nth(1)).toHaveText(item);
      await page.locator('[data-testid=context-menu] .ctx-item').nth(1).hover();
      await page.locator('[data-testid=context-submenu] .ctx-item').first().click();
      await expect(page.locator('[data-testid=app-broker] .mh')).toHaveText(title);
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

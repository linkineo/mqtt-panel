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
    await expect(page.locator('[data-testid=context-menu] .ctx-label')).toHaveText(['Another broker…', "This app's broker"]);
    await page.locator('[data-testid=context-menu] .ctx-item', { hasText: "This app's broker" }).hover();
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
    await expect(page.locator('[data-testid=context-menu] .ctx-label')).toHaveText(['Another broker…', "This app's broker"]);
    await page.keyboard.press('Escape');
    await page.click('[data-testid=welcome-broker]');
    await expect(page.locator('[data-testid=app-broker]')).toBeVisible();
    await expect(page.locator('[data-testid=appb-listener]').first()).toHaveAttribute('aria-checked', 'true');
  });

  test('C-13 "This app\'s broker" > "Add a listener…": no login row, then the steps run and the Explorer opens', async ({ page }) => {
    await open(page, 'mock=demo');
    await page.click('[data-testid=new-profile]');
    await page.locator('[data-testid=context-menu] .ctx-item', { hasText: "This app's broker" }).hover();
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
    await expect(page.locator('[data-testid=pill]')).toContainText('Local broker');
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

  test('C-05 several connections open at once, the pill switches the active one', async ({ page }) => {
    await open(page, 'mock=demo&connect=plant&screen=explore');
    await expect(page.locator('[data-testid=pill]')).toContainText('Plant broker');
    await page.click('[data-testid=rail-connect]');
    await page.click('[data-profile=emqx]');
    await page.click('[data-testid=connect]');
    await expect(page.locator('[data-testid=pill]')).toContainText(/Cloud EMQX/, { timeout: 5000 });
    await expect(page.locator('[data-testid=explorer]')).toBeVisible();
    await page.waitForTimeout(300);
    await page.click('[data-testid=pill]');
    await menuItem(page, /Plant broker/);
    await expect(page.locator('[data-testid=pill]')).toContainText('Plant broker');
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
  test('B-02 Add listener with Mutual TLS requires client certificates on that listener only', async ({ page }) => {
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
    await expect(added.locator('[data-testid=listener-cert]')).toContainText(/required/);
    await expect(page.locator('[data-testid=restart-chip]')).toBeVisible();
  });

  test('B-12 "This app\'s broker" > "Use an existing listener" keeps one profile and one user after 3 runs', async ({ page }) => {
    await open(page, 'mock=empty');
    for (let i = 0; i < 3; i++) {
      await page.click('[data-testid=rail-connect]');
      await page.click(i === 0 ? '[data-testid=welcome-broker]' : '[data-testid=new-profile]');
      if (i > 0) {
        await page.locator('[data-testid=context-menu] .ctx-item', { hasText: "This app's broker" }).hover();
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
    await expect(page.locator('[data-testid=user-row]')).toContainText('created for the profile "Local broker"');
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
    await expect(page.locator('[data-testid=pill]')).toContainText('restart pending');
    await expect(page.locator('[data-testid=port-pending]')).toBeVisible();
    // the pending state survives navigation
    await page.click('[data-testid=rail-explore]');
    await page.click('[data-testid=rail-broker]');
    await expect(btn).toHaveText('Restart broker and apply changes');
    // reachability and adding a listener prompt too
    const ws = page.locator('[data-testid=listener-row][data-listener=l-ws]');
    await ws.locator('.seg button', { hasText: 'Network' }).click();
    await expect(page.locator('[data-testid=restart-pending-list]')).toContainText('WebSocket listener on port 8083 reachable from Network');
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
    expect((await style('[data-testid=pill]')).sel).toBe('none');
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

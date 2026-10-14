import type { Page } from '@playwright/test';

/** Every mockup screen and modal state, reachable with mock query parameters and clicks (V-01, V-02). */
export interface UiState {
  id: string;
  query: string;
  steps?: (page: Page) => Promise<void>;
}

async function selectTopic(page: Page, search: string, topic: string) {
  await page.fill('[data-testid=search]', search);
  await page.locator(`[data-testid=tree-row][data-path="${topic}"]`).click();
  await page.waitForSelector('[data-testid=detail]');
}

async function openAppBrokerDialog(page: Page) {
  await page.click('[data-testid=new-profile]');
  await page.waitForSelector('[data-testid=context-menu]');
  await page.locator('[data-testid=context-menu] .ctx-item').nth(1).hover();
  await page.waitForSelector('[data-testid=context-submenu]');
  await page.locator('[data-testid=context-submenu] .ctx-item').first().click();
  await page.waitForSelector('[data-testid=app-broker]');
}

/** Mockup 12c: the TLS port moves to 8884 and applies at once (change set 7). */
async function changeTlsPort(page: Page) {
  await page.click('[data-testid=port-l-tls]');
  await page.fill('[data-testid=port-input-l-tls]', '8884');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.querySelector('[data-testid=port-l-tls]')?.textContent === '8884');
}

export const STATES: UiState[] = [
  { id: '01-first-launch', query: 'mock=empty' },
  { id: '02-connections', query: 'mock=demo&connect=local&screen=connect' },
  {
    id: '02c-card-menu',
    query: 'mock=demo&connect=local,plant&reconnecting=plant&screen=connect',
    steps: async (p) => {
      await p.click('[data-profile=lab]');
      await p.locator('[data-profile=lab]').click({ button: 'right' });
      await p.waitForSelector('[data-testid=context-menu]');
    },
  },
  {
    id: '02c-delete-confirm',
    query: 'mock=demo&connect=local&screen=connect',
    steps: async (p) => {
      await p.click('[data-testid=delete-profile-btn]');
      await p.waitForSelector('[data-testid=delete-profile]');
    },
  },
  {
    id: '02-new-connection-menu',
    query: 'mock=demo',
    steps: async (p) => {
      await p.click('[data-testid=new-profile]');
      await p.waitForSelector('[data-testid=context-menu]');
      await p.locator('[data-testid=context-menu] .ctx-item').nth(1).hover();
      await p.waitForSelector('[data-testid=context-submenu]');
    },
  },
  {
    id: '02d-folds-open',
    query: 'mock=demo&connect=local&screen=connect',
    steps: async (p) => {
      await p.click('[data-testid=fold-security]');
      await p.click('[data-testid=fold-advanced]');
      await p.waitForSelector('[data-testid=info-caCert]');
    },
  },
  {
    // change set 6: "Skip unverified broker certificate" under Advanced asks "only this time" or "always"
    id: '02d-skip-prompt',
    query: 'mock=demo&connect=local&screen=connect',
    steps: async (p) => {
      await p.click('[data-testid=fold-advanced]');
      await p.click('[data-testid=skip-verify]');
      await p.waitForSelector('[data-testid=skip-dialog]');
    },
  },
  {
    id: '02-info-bubble',
    query: 'mock=demo',
    steps: async (p) => {
      await p.click('[data-testid=fold-security]');
      await p.click('[data-testid=info-caCert]');
      await p.waitForSelector('[data-testid=bubble-caCert]');
    },
  },
  {
    id: '02e-unencrypted',
    query: 'mock=demo&connect=local&screen=connect',
    steps: async (p) => {
      await p.click('[data-profile=lab]');
      await p.waitForSelector('.fold.off');
    },
  },
  { id: '02b-app-broker', query: 'mock=demo', steps: openAppBrokerDialog },
  {
    id: '02b-app-broker-steps',
    query: 'mock=demo&latency=8000',
    steps: async (p) => {
      // mockup 02b: the broker runs, so there is no broker step (nothing restarts, change set 7)
      await openAppBrokerDialog(p);
      await p.click('[data-testid=appb-connect]');
      await p.waitForSelector('[data-testid=appb-step-connect][data-state=run]');
    },
  },
  {
    id: '03-trust',
    query: 'mock=demo',
    steps: async (p) => {
      await p.click('[data-profile=lab]');
      await p.click('[data-testid=scheme-mqtts]');
      await p.waitForTimeout(500);
      await p.click('[data-testid=connect]');
      await p.waitForSelector('[data-testid=trust-dialog]');
    },
  },
  {
    id: '04-explorer-tree',
    query: 'mock=demo&connect=plant,local&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/r204', 'building/f2/r204/setpoint');
      await p.locator('[data-testid=tree-row][data-path="building/f2/r204/temperature"]').click({ button: 'right' });
      await p.waitForSelector('[data-testid=context-menu]');
    },
  },
  {
    id: '05-explorer-table',
    query: 'mock=demo&connect=plant,local&screen=explore',
    steps: async (p) => {
      await p.click('[data-testid=layout-table]');
      await p.click('[data-testid=filters-btn]');
      await p.waitForSelector('[data-testid=filters]');
    },
  },
  {
    id: '05b-columns',
    query: 'mock=demo&connect=plant,local&screen=explore',
    steps: async (p) => {
      await p.click('[data-testid=layout-table]');
      await p.click('[data-testid=columns-btn]');
      await p.waitForSelector('[data-testid=columns]');
    },
  },
  {
    id: '06-diff',
    query: 'mock=demo&connect=plant,local&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/ahu01', 'building/f2/ahu01/state');
      await p.click('[data-testid=tab-diff]');
      await p.waitForSelector('[data-testid=diff]');
    },
  },
  {
    id: '07-properties',
    query: 'mock=demo&connect=plant,local&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/ahu01/cmd', 'building/f2/ahu01/cmd');
      await p.click('[data-testid=tab-properties]');
      await p.waitForSelector('[data-testid=properties]');
    },
  },
  {
    id: '07b-history',
    query: 'mock=demo&connect=plant,local&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/r204', 'building/f2/r204/temperature');
      await p.click('[data-testid=tab-history]');
      await p.waitForSelector('[data-testid=history]');
    },
  },
  {
    id: '08-publish',
    query: 'mock=demo&connect=plant,local&screen=publish',
    steps: async (p) => {
      await p.locator('[data-testid=template]').nth(2).click();
      await p.click('[data-testid=props-toggle]');
      await p.click('[data-testid=send]');
      await p.waitForSelector('[data-testid=sent-detail]');
    },
  },
  {
    id: '08b-publish-311',
    query: 'mock=demo&connect=lab&screen=publish',
  },
  {
    id: '09-delete-retained-repeat',
    query: 'mock=demo&connect=plant,local&screen=publish',
    steps: async (p) => {
      await p.locator('[data-testid=template]').nth(3).click();
      await p.click('[data-testid=repeat-start]');
      await p.waitForSelector('[data-testid=repeat-callout]');
      // the mockup confirms deleting the retained setpoint while the simulation keeps running
      await p.fill('[data-testid=pub-topic]', 'building/f2/r204/setpoint');
      await p.keyboard.press('Escape');
      await p.click('[data-testid=delete-retained-btn]');
      await p.waitForSelector('[data-testid=delete-retained]');
    },
  },
  {
    id: '10-charts',
    query: 'mock=demo&connect=plant,local&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/ahu01', 'building/f2/ahu01/state');
      await p.click('[data-testid=tab-chart]');
      await p.click('[data-testid=add-field]');
      await p.click('[data-testid="picker-check-supply.temp"]');
      await p.click('[data-testid="picker-check-supply.fan"]');
      await p.click('[data-testid=picker-add]');
      await p.click('[data-testid=rail-charts]');
      await p.waitForSelector('[data-testid=series-card]');
      await p.locator('[data-testid=series-card] .t button').first().click();
      await p.waitForSelector('[data-testid=series-editor]');
      await p.waitForTimeout(1200);
    },
  },
  {
    id: '10b-field-picker',
    query: 'mock=demo&connect=plant,local&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/ahu01', 'building/f2/ahu01/state');
      await p.click('[data-testid=tab-chart]');
      await p.click('[data-testid=add-field]');
      await p.waitForSelector('[data-testid=field-picker]');
    },
  },
  {
    id: '11-export',
    query: 'mock=demo&connect=plant,local&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/r204', 'building/f2/r204/temperature');
      await p.click('[data-testid=export-btn]');
      await p.waitForSelector('[data-testid=export-dialog]');
      await p.waitForTimeout(300);
    },
  },
  {
    id: '12-broker',
    query: 'mock=demo&screen=broker',
    steps: async (p) => {
      await p.click('[data-testid=user-pw-mqtt-panel-eye]');
      await p.waitForFunction(() => document.querySelector('[data-testid=user-pw-mqtt-panel-value]')?.textContent !== '••••••••••••');
    },
  },
  {
    id: '12b-anonymous',
    query: 'mock=demo&screen=broker',
    steps: async (p) => {
      await p.click('[data-testid=anonymous]');
      await p.click('[data-testid=confirm-ok]');
      await p.waitForSelector('[data-testid=anon-warning]');
    },
  },
  {
    id: '12c-live-change',
    query: 'mock=demo&screen=broker',
    steps: async (p) => {
      // mockup 12c: the log expanded with its level filter, Advanced security open, a port change applied live
      await p.click('[data-testid=log-toggle]');
      await p.waitForSelector('[data-testid=broker-log]');
      await p.click('[data-testid=fold-brokerAdvanced]');
      await changeTlsPort(p);
      await p.waitForFunction(() => document.querySelector('[data-testid=broker-log]')?.textContent?.includes('listener TLS 8884 started'));
    },
  },
  {
    id: '12e-client-cert-optional',
    query: 'mock=demo&screen=broker',
    steps: async (p) => {
      await p.click('[data-testid=fold-brokerAdvanced]');
      await p.locator('[data-testid=require-cert-seg] button').first().click();
      await p.waitForFunction(() => document.querySelector('[data-testid=require-cert-seg] button')?.classList.contains('on'));
    },
  },
  {
    id: '12d-cert-export-menu',
    query: 'mock=demo&screen=broker',
    steps: async (p) => {
      // mockup 12: the ↓ button of a certificate row opens PEM / PKCS#12… (change set 7)
      await p.click('[data-cert=sensor-r205] [data-testid=cert-export]');
      await p.waitForSelector('[data-testid=context-menu]');
    },
  },
  {
    id: '12f-listener-error',
    query: 'mock=demo&screen=broker',
    steps: async (p) => {
      // a listener that cannot start shows its error on its row (change set 7)
      await p.evaluate(() => (window as any).__mqttpanelMock.engine.takePort(8084));
      await p.locator('[data-testid=listener-row][data-listener=l-wss]').getByRole('switch').click();
      await p.waitForSelector('[data-testid=listener-error-l-wss]');
    },
  },
  {
    id: '13-add-listener-1',
    query: 'mock=demo&screen=broker',
    steps: async (p) => {
      await p.click('[data-testid=add-listener]');
      await p.waitForSelector('[data-testid=add-listener-1]');
    },
  },
  {
    id: '14-add-listener-2',
    query: 'mock=demo&screen=broker',
    steps: async (p) => {
      await p.click('[data-testid=add-listener]');
      await p.click('[data-testid=addl-next]');
      await p.fill('[data-testid=addl-port]', '443');
      await p.waitForSelector('[data-testid=addl-port-error]');
    },
  },
  {
    id: '15-issue-cert',
    query: 'mock=demo&screen=broker',
    steps: async (p) => {
      await p.click('[data-testid=issue-cert]');
      await p.waitForSelector('[data-testid=issue-cert]');
      await p.keyboard.type('sensor-r206');
    },
  },
  { id: '16-settings', query: 'mock=demo&connect=plant,local&screen=settings' },
  {
    id: '17-settings-data',
    query: 'mock=demo&connect=plant,local&screen=settings',
    steps: async (p) => {
      // change set 6: no app-wide switch; "Plant broker" skips the check per connection, so its tab shows ⚠
      await p.click('[data-testid=rail-connect]');
      await p.click('[data-profile=plant]');
      await p.click('[data-testid=fold-advanced]');
      await p.click('[data-testid=skip-verify]');
      await p.click('[data-testid=skip-always-btn]');
      await p.click('[data-testid=fold-advanced]');
      await p.click('[data-testid=rail-settings]');
      await p.waitForSelector('[data-testid=cbar-tab][data-conn=plant] [data-testid=cbar-warn]');
      await p.click('[data-testid=settings-nav-data]');
    },
  },
  {
    id: '17b-settings-about',
    query: 'mock=demo&screen=settings',
    steps: async (p) => {
      await p.click('[data-testid=settings-nav-shortcuts]');
    },
  },
  {
    id: '18-palette',
    query: 'mock=demo&connect=plant,local&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/r204', 'building/f2/r204/setpoint');
      await p.keyboard.press('ControlOrMeta+K');
      // the mockup types "pub"; German names the Publish screen "Senden"
      await p.fill('[data-testid=palette-input]', p.url().includes('lang=de') ? 'send' : 'pub');
      await p.waitForSelector('[data-testid=palette-row]');
    },
  },
  { id: '19-no-connection', query: 'mock=demo&screen=explore' },
  { id: '20-charts-empty', query: 'mock=demo&connect=plant,local&screen=charts' },
];

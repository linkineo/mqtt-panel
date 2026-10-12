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

export const STATES: UiState[] = [
  { id: '01-first-launch', query: 'mock=empty' },
  { id: '02-connections', query: 'mock=demo' },
  {
    id: '02-info-bubble',
    query: 'mock=demo',
    steps: async (p) => {
      await p.click('[data-testid=info-caCert]');
      await p.waitForSelector('[data-testid=bubble-caCert]');
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
    query: 'mock=demo&connect=plant&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/r204', 'building/f2/r204/setpoint');
      await p.locator('[data-testid=tree-row][data-path="building/f2/r204/temperature"]').click({ button: 'right' });
      await p.waitForSelector('[data-testid=context-menu]');
    },
  },
  {
    id: '05-explorer-table',
    query: 'mock=demo&connect=plant&screen=explore',
    steps: async (p) => {
      await p.click('[data-testid=layout-table]');
      await p.click('[data-testid=filters-btn]');
      await p.waitForSelector('[data-testid=filters]');
    },
  },
  {
    id: '05b-columns',
    query: 'mock=demo&connect=plant&screen=explore',
    steps: async (p) => {
      await p.click('[data-testid=layout-table]');
      await p.click('[data-testid=columns-btn]');
      await p.waitForSelector('[data-testid=columns]');
    },
  },
  {
    id: '06-diff',
    query: 'mock=demo&connect=plant&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/ahu01', 'building/f2/ahu01/state');
      await p.click('[data-testid=tab-diff]');
      await p.waitForSelector('[data-testid=diff]');
    },
  },
  {
    id: '07-properties',
    query: 'mock=demo&connect=plant&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/ahu01/cmd', 'building/f2/ahu01/cmd');
      await p.click('[data-testid=tab-properties]');
      await p.waitForSelector('[data-testid=properties]');
    },
  },
  {
    id: '07b-history',
    query: 'mock=demo&connect=plant&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/r204', 'building/f2/r204/temperature');
      await p.click('[data-testid=tab-history]');
      await p.waitForSelector('[data-testid=history]');
    },
  },
  {
    id: '08-publish',
    query: 'mock=demo&connect=plant&screen=publish',
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
    query: 'mock=demo&connect=plant&screen=publish',
    steps: async (p) => {
      await p.locator('[data-testid=template]').nth(3).click();
      await p.click('[data-testid=repeat-start]');
      await p.waitForSelector('[data-testid=repeat-callout]');
      await p.click('[data-testid=delete-retained-btn]');
      await p.waitForSelector('[data-testid=delete-retained]');
    },
  },
  {
    id: '10-charts',
    query: 'mock=demo&connect=plant&screen=explore',
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
    query: 'mock=demo&connect=plant&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/ahu01', 'building/f2/ahu01/state');
      await p.click('[data-testid=tab-chart]');
      await p.click('[data-testid=add-field]');
      await p.waitForSelector('[data-testid=field-picker]');
    },
  },
  {
    id: '11-export',
    query: 'mock=demo&connect=plant&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/r204', 'building/f2/r204/temperature');
      await p.click('[data-testid=export-btn]');
      await p.waitForSelector('[data-testid=export-dialog]');
      await p.waitForTimeout(300);
    },
  },
  { id: '12-broker', query: 'mock=demo&screen=broker' },
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
    },
  },
  { id: '16-settings', query: 'mock=demo&connect=plant&screen=settings' },
  {
    id: '17-settings-data',
    query: 'mock=demo&connect=plant&screen=settings',
    steps: async (p) => {
      await p.click('[data-testid=skip-verify]');
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
    query: 'mock=demo&connect=plant&screen=explore',
    steps: async (p) => {
      await selectTopic(p, 'building/f2/r204', 'building/f2/r204/setpoint');
      await p.keyboard.press('ControlOrMeta+K');
      await p.fill('[data-testid=palette-input]', 'pub');
      await p.waitForSelector('[data-testid=palette-row]');
    },
  },
  { id: '19-no-connection', query: 'mock=demo&screen=explore' },
  { id: '20-charts-empty', query: 'mock=demo&connect=plant&screen=charts' },
];

import { beforeEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { createMockEngineClient, createMockPlatform, type MockEngineClient, type MockUpdateOffer } from '@mqttpanel/engine-client/mock';
import { startApp } from '../boot';
import { boot } from './util';
import { useApp } from '../store/app';
import { App } from '../App';
import { resources } from '../i18n';

// docs/UPDATER.md (26 Oct 2026): the direct macOS download checks mqtt-panel.com for updates; store builds do not
// (TESTPLAN U-01 to U-09).
const OFFER: MockUpdateOffer = { version: '1.0.2', date: '2026-11-02', notes: 'Release notes: https://github.com/linkineo/mqtt-panel/releases/tag/v1.0.2' };

async function bootDmg(opts: { lang?: 'en' | 'fr' | 'de'; offer?: MockUpdateOffer | null; lastCheck?: string } = {}): Promise<MockEngineClient> {
  const lang = opts.lang ?? 'en';
  const engine = createMockEngineClient({ scenario: 'demo', lang, live: false, latencyMs: 0, selfUpdate: true, updateOffer: opts.offer ?? null });
  await startApp(engine, createMockPlatform(), { lang });
  // the launch check runs in the background; wait until it is over
  await waitFor(() => expect(useApp.getState().update?.lastCheck ?? null).not.toBeNull());
  return engine;
}

beforeEach(() => useApp.setState({ toasts: [] }));

const toastAction = () => screen.queryByTestId('toast-open-updates');

describe('Self-update, store build (U-01)', () => {
  it('reports selfUpdate false, keeps the store sentence, no dot, no update paragraph in Privacy', async () => {
    const { engine } = await boot('demo');
    expect((await engine.appInfo()).selfUpdate).toBe(false);
    expect((await engine.updateStatus()).selfUpdate).toBe(false);
    await expect(engine.updateCheck(true)).rejects.toMatchObject({ key: 'update.storeBuild' });
    useApp.setState({ screen: 'settings' });
    render(<App />);
    expect(screen.getByTestId('updates-store')).toBeTruthy();
    expect(screen.queryByTestId('updates-self')).toBeNull();
    expect(screen.queryByTestId('privacy-updates')).toBeNull();
    expect(screen.queryByTestId('rail-settings-dot')).toBeNull();
    expect(useApp.getState().update).toBeNull();
  });
});

describe('Self-update, direct macOS download', () => {
  it('U-02 idle: the switch with its ⓘ, the last check, "Check now" says the version is the latest', async () => {
    const engine = await bootDmg();
    await waitFor(() => expect(useApp.getState().update?.lastCheck).not.toBeNull()); // the launch check ran
    expect(toastAction()).toBeNull();
    useApp.setState({ screen: 'settings' });
    render(<App />);
    expect(screen.queryByTestId('updates-store')).toBeNull();
    const sw = screen.getByTestId('updates-auto');
    expect(sw.getAttribute('aria-checked')).toBe('true');
    fireEvent.click(screen.getByTestId('info-updates.auto'));
    expect(screen.getByTestId('bubble-text-updates.auto').textContent).toMatch(/once a day.*version.*no identifier, no cookie/i);
    expect(screen.getByTestId('updates-last-check').textContent).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
    fireEvent.click(screen.getByTestId('updates-check'));
    await waitFor(() => expect(screen.getByTestId('updates-state').textContent).toBe('You have the latest version'));
    expect(screen.queryByTestId('rail-settings-dot')).toBeNull();
    expect(screen.getByTestId('privacy-updates').textContent).toContain('mqtt-panel.com');
    expect(screen.getByTestId('updates-release-notes')).toBeTruthy();
    // switching the automatic check off is saved and stops the launch check
    fireEvent.click(sw);
    await waitFor(() => expect(sw.getAttribute('aria-checked')).toBe('false'));
    expect((await engine.getSettings()).updates.auto).toBe(false);
    engine.setUpdateOffer(OFFER);
    const st = await engine.updateCheck(false);
    expect(st.state).not.toBe('available');
  });

  it('U-03 the launch check runs once a day only; a failed launch check stays silent', async () => {
    const engine = await bootDmg();
    await waitFor(() => expect(useApp.getState().update?.lastCheck).not.toBeNull());
    engine.setUpdateOffer(OFFER);
    expect((await engine.updateCheck(false)).state).toBe('idle'); // checked less than 24 h ago
    const fresh = await bootDmg({ offer: null });
    fresh.setUpdateUnreachable(true);
    const st = await fresh.updateCheck(true);
    expect(st.state).toBe('error');
    render(<App />);
    act(() => useApp.getState().openSettings('updates'));
    expect(screen.getByTestId('updates-error').textContent).toBe('Could not reach mqtt-panel.com.');
    expect(screen.queryAllByTestId('toast')).toHaveLength(0);
  });

  it('U-04 update found: toast with "Open Updates", dot on Settings, version, date, notes, install and Later', async () => {
    await bootDmg({ offer: OFFER });
    render(<App />);
    await waitFor(() => expect(toastAction()).not.toBeNull());
    expect(screen.getByTestId('toast').textContent).toContain('MQTT Panel 1.0.2 is available');
    expect(toastAction()!.textContent).toBe('Open Updates');
    expect(screen.getByTestId('rail-settings-dot')).toBeTruthy();
    expect(screen.getByTestId('rail-settings').getAttribute('aria-label')).toBe('Settings, update available');
    fireEvent.click(toastAction()!);
    expect(useApp.getState().screen).toBe('settings');
    expect(useApp.getState().settingsPage).toBe('updates');
    expect(screen.queryAllByTestId('toast')).toHaveLength(0);
    const box = screen.getByTestId('updates-available');
    expect(box.textContent).toContain('MQTT Panel 1.0.2 is available');
    expect(box.textContent).toContain('Published 2026-11-02');
    expect(screen.getByTestId('updates-notes').textContent).toBe(OFFER.notes);
    expect(screen.getByTestId('updates-install').textContent).toBe('Download and install');
    fireEvent.click(screen.getByTestId('updates-later'));
    expect(screen.queryByTestId('rail-settings-dot')).toBeNull();
    expect(screen.queryByTestId('updates-later')).toBeNull();
    expect(screen.getByTestId('updates-install')).toBeTruthy();
  });

  it('U-05 installing: progress bar, then "Restart now"; nothing installs without the click', async () => {
    const engine = await bootDmg({ offer: OFFER });
    await waitFor(() => expect(useApp.getState().update?.state).toBe('available'));
    expect(engine.updateRestarts()).toBe(0);
    render(<App />);
    act(() => useApp.getState().openSettings('updates'));
    // the downloading state as the app reports it
    act(() => useApp.setState({ update: { ...useApp.getState().update!, state: 'downloading', downloaded: 2_100_000, total: 4_200_000 } }));
    const bar = screen.getByTestId('updates-progress');
    expect(bar.getAttribute('aria-valuenow')).toBe('50');
    expect(screen.getByTestId('updates-available').textContent).toContain('Downloading… 50 %');
    expect(screen.queryByTestId('updates-install')).toBeNull();
    expect((screen.getByTestId('updates-check') as HTMLButtonElement).disabled).toBe(true);
    act(() => useApp.setState({ update: { ...useApp.getState().update!, state: 'available', downloaded: 0, total: null } }));
    fireEvent.click(screen.getByTestId('updates-install'));
    await waitFor(() => expect(screen.getByTestId('updates-ready')).toBeTruthy());
    expect(screen.getByTestId('updates-ready').textContent).toContain('MQTT Panel 1.0.2 is installed. Restart to use it.');
    expect(screen.getByTestId('rail-settings-dot')).toBeTruthy();
    fireEvent.click(screen.getByTestId('updates-restart'));
    await waitFor(() => expect(engine.updateRestarts()).toBe(1));
  });

  it('U-06 a bundle that cannot be replaced: "Move MQTT Panel to Applications first" instead of the install', async () => {
    const engine = await bootDmg({ offer: null });
    engine.setUpdateOffer(OFFER);
    engine.setUpdateCanInstall(false);
    useApp.setState({ update: await engine.updateCheck(true) });
    render(<App />);
    act(() => useApp.getState().openSettings('updates'));
    expect(screen.getByTestId('updates-move-first').textContent).toBe('Move MQTT Panel to Applications first');
    expect(screen.queryByTestId('updates-install')).toBeNull();
    await expect(engine.updateInstall()).rejects.toMatchObject({ key: 'update.moveFirst' });
  });

  it('U-07 French and German strings (« vous », "du")', async () => {
    await bootDmg({ lang: 'fr', offer: OFFER });
    render(<App />);
    await waitFor(() => expect(toastAction()).not.toBeNull());
    expect(screen.getByTestId('toast').textContent).toContain('MQTT Panel 1.0.2 est disponible');
    expect(toastAction()!.textContent).toBe('Ouvrir Mises à jour');
    fireEvent.click(toastAction()!);
    expect(screen.getByTestId('updates-install').textContent).toBe('Télécharger et installer');
    cleanup();
    await bootDmg({ lang: 'de', offer: OFFER });
    render(<App />);
    await waitFor(() => expect(toastAction()).not.toBeNull());
    expect(screen.getByTestId('toast').textContent).toContain('MQTT Panel 1.0.2 ist verfügbar');
    fireEvent.click(toastAction()!);
    expect(screen.getByTestId('updates-install').textContent).toBe('Laden und installieren');
    const de = (resources as unknown as Record<string, { ui: Record<string, string> }>).de.ui;
    expect(de['info.updates.auto']).toMatch(/\bdu\b/);
    expect(de['info.updates.auto']).not.toMatch(/\bSie\b/);
    const fr = (resources as unknown as Record<string, { ui: Record<string, string> }>).fr.ui;
    expect(fr['info.updates.auto']).toMatch(/\bvous\b/);
  });

  it('U-08 the window never overwrites the update settings with an old copy', async () => {
    const engine = await bootDmg();
    await waitFor(() => expect(useApp.getState().update?.lastCheck).not.toBeNull());
    const stale = { ...useApp.getState().settings, updates: { auto: false, lastCheck: null } };
    const saved = await engine.saveSettings(stale);
    expect(saved.updates.auto).toBe(true);
    expect(saved.updates.lastCheck).not.toBeNull();
  });
});

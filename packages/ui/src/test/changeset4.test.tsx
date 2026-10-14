import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { boot, connect } from './util';
import { engine } from '../services';
import { useApp } from '../store/app';
import { App } from '../App';
import { connectProfile, testProfile } from '../lib/actions';
import { ConnectionsScreen } from '../screens/connections/ConnectionsScreen';
import { ProfileForm } from '../screens/connections/ProfileForm';
import { resources } from '../i18n';

const card = (id: string) => screen.getAllByTestId('profile-card').find((c) => c.getAttribute('data-profile') === id)!;
const sync = async () => {
  useApp.setState({ connections: await engine().listConnections(), profiles: await engine().listProfiles(), broker: await engine().brokerStatus() });
};

describe('Change set 4: "Local broker" starts the broker (C-17)', () => {
  it('Connect starts a stopped broker, then connects', async () => {
    await boot('demo');
    await engine().brokerStop();
    await sync();
    expect(await connectProfile('local', false)).toBe(true);
    expect((await engine().brokerStatus()).state).toBe('running');
    expect((await engine().listConnections()).find((c) => c.id === 'local')!.state).toBe('connected');
    expect(useApp.getState().broker!.state).toBe('running');
  });

  it('Test does the same', async () => {
    await boot('demo');
    await engine().brokerStop();
    await testProfile('local');
    expect((await engine().brokerStatus()).state).toBe('running');
    expect((await engine().listProfiles()).find((p) => p.id === 'local')!.lastTest!.ok).toBe(true);
  });

  it('a broker that cannot start fails the connection with its reason', async () => {
    const { engine: mock } = await boot('demo');
    await engine().brokerStop();
    mock.failBrokerStart(true);
    expect(await connectProfile('local', false)).toBe(false);
    const toast = useApp.getState().toasts.at(-1)!;
    expect(toast.kind).toBe('err');
    expect(toast.msg!.key).toBe('broker.portInUseBy');
    expect((await engine().listConnections()).some((c) => c.id === 'local' && c.state === 'connected')).toBe(false);
  });

  it('other profiles never start the broker', async () => {
    await boot('demo');
    await engine().brokerStop();
    expect(await connectProfile('plant', false)).toBe(true);
    expect((await engine().brokerStatus()).state).toBe('stopped');
  });
});

describe('Change set 4: stop button on cards (C-18)', () => {
  it('connected and reconnecting cards carry it; it disconnects that connection only', async () => {
    const { engine: mock } = await boot('demo');
    await connect('local');
    await connect('plant');
    mock.setConnectionState('plant', 'reconnecting');
    await sync();
    render(<ConnectionsScreen />);
    const stop = within(card('local')).getByTestId('card-stop');
    expect(stop.getAttribute('aria-label')).toBe('Disconnect');
    expect(stop.getAttribute('title')).toBe('Disconnect');
    expect(within(card('plant')).getByTestId('card-stop')).toBeInTheDocument();
    expect(within(card('lab')).queryByTestId('card-stop')).toBeNull();
    const selected = useApp.getState().selectedProfileId;
    await act(async () => fireEvent.click(stop));
    await waitFor(() => expect(within(card('local')).queryByTestId('card-state')).toBeNull());
    expect(useApp.getState().selectedProfileId).toBe(selected);
    expect((await engine().brokerStatus()).state).toBe('running');
    expect((await engine().listConnections()).find((c) => c.id === 'plant')!.state).toBe('reconnecting');
    await act(async () => fireEvent.click(within(card('plant')).getByTestId('card-stop')));
    await waitFor(async () => expect((await engine().listConnections()).find((c) => c.id === 'plant')!.state).toBe('disconnected'));
  });
});

describe('Change set 4: form folds (C-19)', () => {
  it('Basics open, Security and Advanced closed, summaries as specified, no helper sentences', async () => {
    await boot('demo');
    render(<ProfileForm profileId="plant" />);
    expect(screen.getByTestId('fold-security').getAttribute('aria-expanded')).toBe('false');
    expect(screen.getByTestId('fold-advanced').getAttribute('aria-expanded')).toBe('false');
    expect(screen.getByTestId('fold-security-summary').textContent).toBe('CA · client certificate · verified');
    expect(screen.getByTestId('fold-advanced-summary').textContent).toBe('keep-alive 60 s · 3 subscriptions · 1 schema');
    expect(screen.queryByTestId('file-caCert')).toBeNull();
    await act(async () => fireEvent.click(screen.getByTestId('fold-security')));
    expect(screen.getByTestId('file-caCert')).toBeInTheDocument();
    expect(useApp.getState().folds.security).toBe(true);
    const form = screen.getByTestId('profile-form');
    for (const s of ['System roots plus the CA below.', 'Off. Any certificate is accepted.', 'Only if the broker serves several names.', 'Some cloud brokers require it.']) {
      expect(form.textContent).not.toContain(s);
    }
    for (const id of ['caCert', 'clientCert', 'clientKey', 'sni', 'alpn']) expect(screen.getByTestId(`info-${id}`)).toBeInTheDocument();
  });

  it('"skip verification" (change set 6) when the check is skipped; empty summaries at the defaults', async () => {
    await boot('demo');
    const p = await engine().newProfile();
    // mqtts: with mqtt the Security fold is off and carries its reason instead (change set 5)
    const saved = await engine().saveProfile({ ...p, name: 'Defaults', scheme: 'mqtts', port: 8883 });
    useApp.setState({ profiles: await engine().listProfiles() });
    const { unmount } = render(<ProfileForm profileId={saved.id} />);
    expect(screen.queryByTestId('fold-security-summary')).toBeNull();
    expect(screen.queryByTestId('fold-advanced-summary')).toBeNull();
    unmount();
    const off = await engine().saveProfile({ ...saved, keepAlive: 30, tls: { ...saved.tls, skipVerify: true } });
    useApp.setState({ profiles: await engine().listProfiles() });
    render(<ProfileForm profileId={off.id} />);
    expect(screen.queryByTestId('fold-security-summary')).toBeNull();
    expect(screen.getByTestId('fold-advanced-summary').textContent).toBe('keep-alive 30 s · skip verification');
    expect(screen.getByTestId('advanced-skip').className).toBe('warn');
  });
});

describe('Change set 4: no parsed chips, no "mTLS" (C-20)', () => {
  it('no chip next to CA, client certificate, client key or the password', async () => {
    await boot('demo');
    useApp.setState({ folds: { security: true, advanced: false, brokerAdvanced: false } });
    const { unmount } = render(<ProfileForm profileId="plant" />);
    for (const f of ['caCert', 'clientCert', 'clientKey']) {
      const field = screen.getByTestId(`file-${f}`);
      expect(field.querySelector('.chip')).toBeNull();
      expect(field.textContent).not.toMatch(/until|CN=|Keychain/);
    }
    expect(screen.getByTestId('profile-form').textContent).not.toContain('Keychain');
    unmount();
    render(<ProfileForm profileId="local" />);
    expect(screen.getByTestId('local-password').querySelector('.chip')).toBeNull();
  });

  it('"mTLS" and "Mutual TLS" appear in no catalog; the dropped keys are gone', () => {
    for (const lang of ['en', 'fr', 'de'] as const) {
      for (const cat of [resources[lang].ui, resources[lang].errors] as Record<string, string>[]) {
        for (const [k, v] of Object.entries(cat)) expect(v, `${lang} ${k}`).not.toMatch(/mTLS|mutual TLS|TLS mutuel/i);
      }
      const ui = resources[lang].ui as Record<string, string>;
      for (const k of ['file.keychain', 'file.until', 'conn.verifyHint', 'conn.sniHint', 'conn.alpnHint', 'broker.networkWarn', 'broker.logLive', 'broker.certsHelp', 'users.createdFor']) {
        expect(ui[k], `${lang} ${k}`).toBeUndefined();
      }
      expect(Object.keys(ui).filter((k) => k.startsWith('pill.')), lang).toEqual([]);
    }
    expect((resources.fr.ui as Record<string, string>)['preset.mtls']).toBe('Chiffré + certificats clients');
    expect((resources.de.ui as Record<string, string>)['preset.mtls']).toBe('Verschlüsselt + Client-Zertifikate');
  });

  it('cards read the protocol (change set 6), never "mTLS"', async () => {
    await boot('demo');
    render(<ConnectionsScreen />);
    expect(new Set(screen.getAllByTestId('card-chip').map((c) => c.textContent))).toEqual(new Set(['mqtts', 'mqtt', 'wss']));
  });
});

describe('Change set 4: broker-wide reachability (B-21, mock engine)', () => {
  it('one value for the broker; a change while running applies at once (change set 7)', async () => {
    await boot('demo');
    const cfg = await engine().brokerConfig();
    expect(cfg.bind).toBe('network');
    expect(cfg.listeners.every((l) => !('bind' in l))).toBe(true);
    const c = await engine().setBrokerBind('localhost');
    expect(c.bind).toBe('localhost');
    const st = await engine().brokerStatus();
    expect('restartNeeded' in st).toBe(false);
    expect(st.state).toBe('running');
  });
});

describe('Change set 4: title bar and rail (V-10)', () => {
  it('no pill; play badge while the broker runs (clay while starting); count bubble; ⚠ on the tab (change set 6)', async () => {
    await boot('demo');
    await connect('plant');
    await connect('local');
    await sync();
    render(<App />);
    expect(screen.queryByTestId('pill')).toBeNull();
    expect(document.querySelector('.tb')!.textContent).toBe('MQTT Panel');
    expect(screen.getByTestId('rail-broker-badge').getAttribute('data-state')).toBe('running');
    expect(screen.getByTestId('rail-broker-badge').className).toBe('badge');
    expect(screen.getByTestId('rail-connect-count').textContent).toBe('2');
    expect(screen.getByTestId('rail-connect').getAttribute('aria-label')).toBe('Connect, 2 connections open');
    act(() => useApp.setState({ broker: { ...useApp.getState().broker!, state: 'starting' } }));
    expect(screen.getByTestId('rail-broker-badge').className).toBe('badge starting');
    act(() => useApp.setState({ broker: { ...useApp.getState().broker!, state: 'stopped' } }));
    expect(screen.queryByTestId('rail-broker-badge')).toBeNull();
    expect(screen.queryAllByTestId('cbar-warn')).toHaveLength(0);
    await act(async () => {
      const plant = useApp.getState().profiles.find((p) => p.id === 'plant')!;
      await engine().saveProfile({ ...plant, tls: { ...plant.tls, skipVerify: true } });
      await sync();
    });
    const tabs = screen.getAllByTestId('cbar-tab');
    const warn = within(tabs.find((t) => t.getAttribute('data-conn') === 'plant')!).getByTestId('cbar-warn');
    expect(warn.textContent).toBe('⚠');
    expect(warn.getAttribute('title')).toContain('Certificate verification is off for this connection');
    // the anonymous flag is not in the title bar
    await act(async () => {
      await engine().setAnonymous(true);
      useApp.setState({ brokerConfig: await engine().brokerConfig() });
    });
    expect(document.querySelector('.tb')!.textContent).toBe('MQTT Panel');
  });
});

import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { boot, connect } from './util';
import { engine } from '../services';
import { useApp } from '../store/app';
import { ConnectionsScreen } from '../screens/connections/ConnectionsScreen';
import { ProfileForm } from '../screens/connections/ProfileForm';
import { resources } from '../i18n';

const card = (id: string) => screen.getAllByTestId('profile-card').find((c) => c.getAttribute('data-profile') === id)!;
const sync = async () => {
  useApp.setState({ connections: await engine().listConnections(), profiles: await engine().listProfiles(), broker: await engine().brokerStatus() });
};
const host = () => document.getElementById('pf-host') as HTMLInputElement;
const port = () => screen.getByTestId('pf-port') as HTMLInputElement;
const stored = async (id: string) => (await engine().listProfiles()).find((p) => p.id === id)!;
const labels = () => Array.from(screen.getByTestId('profile-form').querySelectorAll('.f > .lbl, .f > label')).map((l) => l.textContent);

describe('Change set 5: play button on idle cards (C-21)', () => {
  it('idle cards show ▶, open ones ■, never both; ▶ connects without changing the selection', async () => {
    await boot('demo');
    await connect('local');
    await sync();
    render(<ConnectionsScreen />);
    for (const id of ['plant', 'lab', 'emqx']) {
      const play = within(card(id)).getByTestId('card-play');
      expect(play.getAttribute('aria-label')).toBe('Connect');
      expect(play.getAttribute('title')).toBe('Connect');
      expect(within(card(id)).queryByTestId('card-stop')).toBeNull();
    }
    expect(within(card('local')).queryByTestId('card-play')).toBeNull();
    expect(within(card('local')).getByTestId('card-stop')).toBeInTheDocument();
    const selected = useApp.getState().selectedProfileId;
    await act(async () => fireEvent.click(within(card('lab')).getByTestId('card-play')));
    await waitFor(() => expect(within(card('lab')).getByTestId('card-stop')).toBeInTheDocument());
    expect(within(card('lab')).queryByTestId('card-play')).toBeNull();
    expect(useApp.getState().selectedProfileId).toBe(selected);
    expect(useApp.getState().screen).toBe('connect');
  });

  it('on "Local broker" it starts a stopped broker first', async () => {
    await boot('demo');
    await engine().brokerStop();
    await sync();
    render(<ConnectionsScreen />);
    await act(async () => fireEvent.click(within(card('local')).getByTestId('card-play')));
    await waitFor(() => expect(within(card('local')).getByTestId('card-stop')).toBeInTheDocument());
    expect((await engine().brokerStatus()).state).toBe('running');
  });
});

describe('Change set 5: form order and address prefiller (C-22)', () => {
  it('Basics rows in the specified order; no "Transport" row', async () => {
    await boot('demo');
    const { unmount } = render(<ProfileForm profileId="lab" />);
    expect(labels()).toEqual(['Name', 'Protocol', 'MQTT version', 'Host and port', 'Client ID', 'Username', 'Password']);
    expect(screen.getByTestId('profile-form').textContent).not.toContain('Transport');
    unmount();
    render(<ProfileForm profileId="emqx" />);
    expect(labels()).toEqual(['Name', 'Protocol', 'MQTT version', 'Host and port', 'WebSocket path', 'Client ID', 'Username', 'Password']);
  });

  it('the field shows the prefix and the port; choosing ws rewrites both', async () => {
    await boot('demo');
    render(<ProfileForm profileId="lab" />);
    expect(host().value).toBe('mqtt://10.20.4.17:1883');
    await act(async () => fireEvent.click(screen.getByTestId('scheme-ws')));
    expect(host().value).toBe('ws://10.20.4.17:8083');
    expect(port().value).toBe('8083');
    // a port that is not the default stays
    fireEvent.change(port(), { target: { value: '1999' } });
    expect(host().value).toBe('ws://10.20.4.17:1999');
    await act(async () => fireEvent.click(screen.getByTestId('scheme-mqtts')));
    expect(port().value).toBe('1999');
    expect(host().value).toBe('mqtts://10.20.4.17:1999');
  });

  it('typed prefix, ":port", pasted full address, IPv6, deleted prefix; the profile stores the host bare', async () => {
    await boot('demo');
    render(<ProfileForm profileId="emqx" />);
    const seg = (s: string) => screen.getByTestId(`scheme-${s}`).getAttribute('aria-checked');
    fireEvent.focus(host());
    fireEvent.change(host(), { target: { value: 'mqtt://demo.emqx.example:8084' } });
    expect(seg('mqtt')).toBe('true');
    fireEvent.change(host(), { target: { value: 'mqtt://demo.emqx.example:1884' } });
    expect(port().value).toBe('1884');
    fireEvent.blur(host());
    fireEvent.change(port(), { target: { value: '1885' } });
    expect(host().value).toBe('mqtt://demo.emqx.example:1885');
    // pasted full address
    fireEvent.focus(host());
    fireEvent.change(host(), { target: { value: 'wss://broker.example:8443/ws' } });
    fireEvent.blur(host());
    expect(seg('wss')).toBe('true');
    expect(port().value).toBe('8443');
    expect((document.getElementById('pf-ws') as HTMLInputElement).value).toBe('/ws');
    expect(host().value).toBe('wss://broker.example:8443');
    // IPv6 keeps its brackets
    fireEvent.focus(host());
    fireEvent.change(host(), { target: { value: 'tcp://[::1]:1883' } });
    fireEvent.blur(host());
    expect(seg('mqtt')).toBe('true');
    expect(host().value).toBe('mqtt://[::1]:1883');
    // a deleted prefix comes back on blur, the protocol unchanged
    await act(async () => fireEvent.click(screen.getByTestId('scheme-mqtts')));
    fireEvent.focus(host());
    fireEvent.change(host(), { target: { value: 'plant.example:8883' } });
    expect(host().value).toBe('plant.example:8883');
    fireEvent.blur(host());
    expect(seg('mqtts')).toBe('true');
    expect(host().value).toBe('mqtts://plant.example:8883');
    await waitFor(async () => expect((await stored('emqx')).host).toBe('plant.example'), { timeout: 2000 });
    const p = await stored('emqx');
    expect([p.scheme, p.port, p.wsPath]).toEqual(['mqtts', 8883, '/ws']);
  });
});

describe('Change set 5: Security off with mqtt and ws (C-23)', () => {
  it('greyed, not openable, with the reason; values kept and back with mqtts', async () => {
    await boot('demo');
    useApp.setState({ folds: { security: true, advanced: false, brokerAdvanced: false } });
    render(<ProfileForm profileId="plant" />);
    expect(screen.getByTestId('file-caCert')).toBeInTheDocument();
    await act(async () => fireEvent.click(screen.getByTestId('scheme-mqtt')));
    const fold = screen.getByTestId('fold-security');
    expect(fold.className).toBe('fold off');
    expect(fold.getAttribute('aria-disabled')).toBe('true');
    expect(fold.getAttribute('aria-expanded')).toBe('false');
    expect(screen.getByTestId('fold-security-summary').textContent).toBe('Not encrypted: choose mqtts or wss to use certificates.');
    expect(screen.queryByTestId('file-caCert')).toBeNull();
    await act(async () => fireEvent.click(fold));
    expect(screen.queryByTestId('file-caCert')).toBeNull();
    // the stored open state is kept for when the fold applies again
    expect(useApp.getState().folds.security).toBe(true);
    expect(screen.getByTestId('profile-form').textContent).not.toContain('Certificates are used only');
    await act(async () => fireEvent.click(screen.getByTestId('scheme-mqtts')));
    expect(screen.getByTestId('fold-security').className).toBe('fold');
    expect(screen.getByTestId('file-caCert').textContent).toContain('plant-root-ca.pem');
    expect(screen.getByTestId('fold-security-summary').textContent).toContain('verify on · CA · client certificate');
  });

  it('the reason in French and German', async () => {
    for (const [lang, text] of [
      ['fr', 'Non chiffré : choisissez mqtts ou wss pour utiliser des certificats.'],
      ['de', 'Nicht verschlüsselt: Wähle mqtts oder wss, um Zertifikate zu verwenden.'],
    ] as const) {
      await boot('demo', lang);
      const { unmount } = render(<ProfileForm profileId="lab" />);
      expect(screen.getByTestId('fold-security-summary').textContent).toBe(text);
      unmount();
    }
  });
});

describe('Change set 5: "MQTT Panel broker" (C-24)', () => {
  it('the new wording in EN, FR and DE; the old phrase appears in no catalog', () => {
    const ui = (l: 'en' | 'fr' | 'de') => resources[l].ui as Record<string, string>;
    expect([ui('en')['conn.newApp'], ui('fr')['conn.newApp'], ui('de')['conn.newApp']]).toEqual(['MQTT Panel broker', 'Broker MQTT Panel', 'MQTT Panel Broker']);
    expect([ui('en')['appb.title'], ui('fr')['appb.title'], ui('de')['appb.title']]).toEqual([
      'Connect to the MQTT Panel broker',
      'Se connecter au broker MQTT Panel',
      'Mit dem MQTT Panel Broker verbinden',
    ]);
    expect([ui('en')['conn.protocol'], ui('fr')['conn.protocol'], ui('de')['conn.protocol']]).toEqual(['Protocol', 'Protocole', 'Protokoll']);
    for (const lang of ['en', 'fr', 'de']) {
      const raw = readFileSync(join(__dirname, '..', 'i18n', lang, 'ui.json'), 'utf8');
      expect(raw, lang).not.toMatch(/this app's broker|this app’s broker|cette app\b|dieser App/i);
    }
  });

  it('the "Local broker" card meta line reads "MQTT Panel broker"', async () => {
    await boot('demo');
    render(<ConnectionsScreen />);
    expect(card('local').textContent).toContain('MQTT Panel broker · MQTT 5');
  });
});

describe('Change set 5: import hidden from the UI (C-25)', () => {
  it('no import line, link or welcome card; the strings are gone, the summary strings kept', async () => {
    await boot('demo');
    const { unmount } = render(<ConnectionsScreen />);
    expect(screen.queryByTestId('import')).toBeNull();
    expect(document.body.textContent).not.toMatch(/Import/);
    unmount();
    await boot('empty');
    render(<ConnectionsScreen />);
    const welcome = screen.getByTestId('welcome');
    expect(welcome.querySelectorAll('.big > .card')).toHaveLength(2);
    expect(welcome.textContent).not.toMatch(/Import/);
    for (const lang of ['en', 'fr', 'de'] as const) {
      const ui = resources[lang].ui as Record<string, string>;
      for (const k of ['conn.import', 'conn.importHint', 'conn.importLink', 'welcome.importTitle', 'welcome.importText', 'welcome.importButton', 'nativeMenu.import', 'conn.transport', 'conn.tlsUnused']) {
        expect(ui[k], `${lang} ${k}`).toBeUndefined();
      }
      expect(ui['import.title'], lang).toBeDefined();
    }
  });

  it('a file dropped on the list does nothing', async () => {
    const { platform } = await boot('demo');
    render(<ConnectionsScreen />);
    const before = (await engine().listProfiles()).length;
    const r = card('plant').getBoundingClientRect();
    await act(async () => platform.simulateDrop(['/dropped/connections.json'], r.left + 5, r.top + 5));
    expect((await engine().listProfiles()).length).toBe(before);
    expect(screen.queryByTestId('import-summary')).toBeNull();
  });
});

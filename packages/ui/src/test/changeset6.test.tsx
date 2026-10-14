import { describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { boot, connect } from './util';
import { engine } from '../services';
import { useApp } from '../store/app';
import { ConnectionsScreen } from '../screens/connections/ConnectionsScreen';
import { ProfileForm } from '../screens/connections/ProfileForm';
import { SettingsScreen } from '../screens/settings/SettingsScreen';
import { ConnectionBar, useOpenTabs } from '../components/ConnectionBar';
import { connectProfile, trustAndRetry, useTrust } from '../lib/actions';
import { hostPort, nameFromHost } from '../lib/address';
import { resources } from '../i18n';

const card = (id: string) => screen.getAllByTestId('profile-card').find((c) => c.getAttribute('data-profile') === id)!;
const sync = async () => {
  useApp.setState({ connections: await engine().listConnections(), profiles: await engine().listProfiles(), broker: await engine().brokerStatus() });
};
const stored = async (id: string) => (await engine().listProfiles()).find((p) => p.id === id)!;
const host = () => document.getElementById('pf-host') as HTMLInputElement;
const nameField = () => document.getElementById('pf-name') as HTMLInputElement;
const typeHost = async (text: string) => {
  await act(async () => {
    fireEvent.focus(host());
    fireEvent.change(host(), { target: { value: text } });
    fireEvent.blur(host());
  });
};
const ui = (l: 'en' | 'fr' | 'de') => resources[l].ui as Record<string, string>;
const raw = (l: string, f: string) => readFileSync(join(__dirname, '..', 'i18n', l, `${f}.json`), 'utf8');

function Bar() {
  return <ConnectionBar tabs={useOpenTabs()} />;
}

describe('Change set 6: helpers', () => {
  it('nameFromHost: first label of a name, IPs and localhost as they are', () => {
    expect(nameFromHost('myhost.com')).toBe('myhost');
    expect(nameFromHost('broker.plant.example')).toBe('broker');
    expect(nameFromHost('10.20.4.17')).toBe('10.20.4.17');
    expect(nameFromHost('::1')).toBe('::1');
    expect(nameFromHost('[::1]')).toBe('::1');
    expect(nameFromHost('fe80::1:2')).toBe('fe80::1:2');
    expect(nameFromHost('localhost')).toBe('localhost');
    expect(nameFromHost('  myhost  ')).toBe('myhost');
    expect(nameFromHost('')).toBe('');
  });

  it('hostPort: no scheme, IPv6 in brackets', () => {
    expect(hostPort({ host: 'broker.plant.example', port: 8883 })).toBe('broker.plant.example:8883');
    expect(hostPort({ host: '::1', port: 1883 })).toBe('[::1]:1883');
    expect(hostPort({ host: '10.20.4.17', port: 1883 })).toBe('10.20.4.17:1883');
  });
});

describe('Change set 6: wording (C-26)', () => {
  it('"New broker…", "Built-in broker", "Connect to the built-in broker", "Generate" in EN, FR and DE', () => {
    expect([ui('en')['conn.newOther'], ui('fr')['conn.newOther'], ui('de')['conn.newOther']]).toEqual(['New broker…', 'Nouveau broker…', 'Neuer Broker…']);
    expect([ui('en')['conn.newApp'], ui('fr')['conn.newApp'], ui('de')['conn.newApp']]).toEqual(['Built-in broker', 'Broker intégré', 'Eingebauter Broker']);
    expect([ui('en')['appb.title'], ui('fr')['appb.title'], ui('de')['appb.title']]).toEqual([
      'Connect to the built-in broker',
      'Connexion au broker intégré',
      'Mit dem eingebauten Broker verbinden',
    ]);
    expect([ui('en')['conn.generate'], ui('fr')['conn.generate'], ui('de')['conn.generate']]).toEqual(['Generate', 'Générer', 'Generieren']);
    expect(ui('fr')['conn.connect']).toBe('Connecter');
    expect(ui('fr')['palette.connectTo']).toBe('Connecter : {name}');
    expect(ui('fr')['welcome.brokerButton']).toBe('Démarrer et connecter');
  });

  it('the old words appear in no catalog (UI and errors, EN, FR, DE)', () => {
    for (const l of ['en', 'fr', 'de']) {
      for (const f of ['ui', 'errors']) {
        const text = raw(l, f);
        expect(text, `${l}/${f}`).not.toMatch(/MQTT Panel broker|Broker MQTT Panel|MQTT Panel Broker|Another broker|Un autre broker|Anderer Broker|Se connecter|Aléatoire|Zufällig/);
        expect(text, `${l}/${f}`).not.toMatch(/"Random"|use Random/);
      }
    }
  });

  it('the menu and the Client ID button use the new words', async () => {
    await boot('demo');
    render(<ConnectionsScreen />);
    await act(async () => fireEvent.click(screen.getByTestId('new-profile')));
    const menu = await screen.findByTestId('context-menu');
    expect(Array.from(menu.querySelectorAll('.ctx-label')).map((x) => x.textContent)).toEqual(['New broker…', 'Built-in broker']);
    await act(async () => fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' }));
    expect(screen.getByTestId('generate-cid').textContent).toBe('Generate');
  });
});

describe('Change set 6: cards (C-27)', () => {
  it('three lines: name with play or stop; the protocol chip; "host:port"; nothing else', async () => {
    await boot('demo');
    await connect('local');
    await sync();
    render(<ConnectionsScreen />);
    for (const [id, proto, addr, tone] of [
      ['plant', 'mqtts', 'broker.plant.example:8883', 'on'],
      ['local', 'mqtts', 'localhost:8883', 'on'],
      ['lab', 'mqtt', '10.20.4.17:1883', 'clay'],
      ['emqx', 'wss', 'demo.emqx.example:8084', 'on'],
    ] as const) {
      const c = card(id);
      expect(c.children, id).toHaveLength(3);
      const chip = within(c).getByTestId('card-chip');
      expect(chip.textContent).toBe(proto);
      expect(chip.className).toBe(`chip ${tone}`);
      expect(chip.parentElement!.textContent).toMatch(new RegExp(`^${proto}⚠?$`));
      const a = within(c).getByTestId('card-address');
      expect(a.textContent).toBe(addr);
      expect(a.className).toContain('mono');
      expect(c.textContent).not.toMatch(/MQTT 5|MQTT 3|subscription|used|ago|last week|Built-in/);
    }
  });

  it('⚠ in clay when the check is skipped, muted with a pinned fingerprint, with the tooltip as its name', async () => {
    await boot('demo');
    const plant = await stored('plant');
    await engine().saveProfile({ ...plant, tls: { ...plant.tls, trustedFingerprints: ['3F:9A'] } });
    await sync();
    render(<ConnectionsScreen />);
    const skip = within(card('emqx')).getByTestId('card-warn');
    expect(skip.className).toBe('warn-i');
    expect(skip.getAttribute('title')).toBe(
      'Certificate verification is off for this connection: any broker certificate is accepted, so an impostor could not be told apart.',
    );
    expect(skip.getAttribute('aria-label')).toBe(skip.getAttribute('title'));
    const pin = within(card('plant')).getByTestId('card-warn');
    expect(pin.className).toBe('warn-i pin');
    expect(pin.getAttribute('title')).toBe("You trusted this broker's certificate by its fingerprint; it could not be verified against a CA.");
    expect(within(card('lab')).queryByTestId('card-warn')).toBeNull();
    expect(within(card('local')).queryByTestId('card-warn')).toBeNull();
  });

  it('tooltips in French and German', async () => {
    expect(ui('fr')['conn.warnSkip']).toBe(
      'La vérification du certificat est désactivée pour cette connexion : n’importe quel certificat est accepté, un usurpateur ne serait pas détecté.',
    );
    expect(ui('de')['conn.warnSkip']).toBe(
      'Die Zertifikatsprüfung ist für diese Verbindung aus: Jedes Broker-Zertifikat wird akzeptiert, ein Angreifer wäre nicht zu erkennen.',
    );
    expect(ui('fr')['conn.warnPinned']).toBe('Vous avez accepté le certificat de ce broker par son empreinte ; il n’a pas pu être vérifié par une autorité.');
    expect(ui('de')['conn.warnPinned']).toBe('Du hast das Zertifikat dieses Brokers über seinen Fingerabdruck akzeptiert; es konnte nicht gegen eine CA geprüft werden.');
  });
});

describe('Change set 6: field widths', () => {
  it('fields carry one of the four widths; none stretches', async () => {
    await boot('demo');
    useApp.setState({ folds: { security: true, advanced: true, brokerAdvanced: false } });
    render(<ProfileForm profileId="plant" />);
    const w = (el: Element | null) => (el?.className.match(/\bw-(s|m|l|xl)\b/) ?? [])[1];
    expect(w(nameField())).toBe('xl');
    expect(w(host())).toBe('xl');
    expect(w(screen.getByTestId('pf-port'))).toBe('s');
    expect(w(document.getElementById('pf-cid'))).toBe('l');
    expect(w(document.getElementById('pf-user'))).toBe('l');
    expect(w(document.getElementById('pf-ka'))).toBe('s');
    expect(w(screen.getByLabelText('Server name (SNI)'))).toBe('l');
    expect(w(screen.getByLabelText('Protocol tag (ALPN)'))).toBe('m');
    for (const f of ['caCert', 'clientCert', 'clientKey']) expect(w(screen.getByTestId(`file-${f}`).querySelector('.input'))).toBe('xl');
    for (const input of Array.from(screen.getByTestId('profile-form').querySelectorAll('.input'))) {
      expect((input as HTMLElement).style.flex, input.outerHTML.slice(0, 60)).toBe('');
      expect(w(input), input.outerHTML.slice(0, 80)).toBeTruthy();
    }
  });
});

describe('Change set 6: auto name (C-28)', () => {
  it('a new profile follows the host until a name is typed; clearing restores the link', async () => {
    await boot('demo');
    render(<ConnectionsScreen />);
    await act(async () => fireEvent.click(screen.getByTestId('new-profile')));
    const menu = await screen.findByTestId('context-menu');
    await act(async () => fireEvent.click(within(menu).getByText('New broker…')));
    await waitFor(() => expect(screen.getByTestId('profile-form')).toBeInTheDocument());
    const id = useApp.getState().selectedProfileId!;
    expect(nameField().value).toBe('localhost');
    await typeHost('mqtt://myhost.com');
    expect(nameField().value).toBe('myhost');
    await typeHost('mqtt://broker.plant.example');
    expect(nameField().value).toBe('broker');
    await typeHost('mqtt://10.20.4.17');
    expect(nameField().value).toBe('10.20.4.17');
    await typeHost('mqtt://[::1]:1883');
    expect(nameField().value).toBe('::1');
    await typeHost('mqtt://localhost');
    expect(nameField().value).toBe('localhost');
    // typing a name breaks the link
    await act(async () => fireEvent.change(nameField(), { target: { value: 'Roof gateway' } }));
    await typeHost('mqtt://gw.roof.example');
    expect(nameField().value).toBe('Roof gateway');
    // clearing it restores the link: the next host change names it again, and leaving the field empty fills it
    await act(async () => fireEvent.change(nameField(), { target: { value: '' } }));
    await typeHost('mqtt://edge.roof.example');
    expect(nameField().value).toBe('edge');
    await act(async () => fireEvent.change(nameField(), { target: { value: '' } }));
    await act(async () => fireEvent.blur(nameField()));
    expect(nameField().value).toBe('edge');
    await waitFor(async () => expect((await stored(id)).name).toBe('edge'));
  });

  it('an existing profile is never renamed by a host change', async () => {
    await boot('demo');
    const lab = await stored('lab');
    await engine().saveProfile({ ...lab, name: '10.20.4.17' });
    await sync();
    render(<ProfileForm profileId="lab" />);
    await typeHost('mqtt://10.20.4.18');
    expect(nameField().value).toBe('10.20.4.17');
  });
});

describe('Change set 6: per-connection skip (C-29, T-20, S-05)', () => {
  it('Settings has no skip switch any more; the fingerprints stay', async () => {
    await boot('demo');
    useApp.getState().setSettingsPage('security');
    render(<SettingsScreen />);
    expect(screen.queryByTestId('skip-verify')).toBeNull();
    expect(document.body.textContent).not.toContain('Skip verification');
    expect(document.body.textContent).toContain('Trusted fingerprints');
    expect('skipVerify' in useApp.getState().settings).toBe(false);
  });

  it('the Security fold has no "Verify server" row; the Advanced switch opens the prompt; "Always" saves it', async () => {
    await boot('demo');
    useApp.setState({ folds: { security: true, advanced: true, brokerAdvanced: false } });
    render(<ProfileForm profileId="plant" />);
    expect(screen.getByTestId('profile-form').textContent).not.toContain('Verify server');
    const sw = screen.getByTestId('skip-verify');
    expect(sw.getAttribute('aria-label')).toBe('Skip unverified broker certificate');
    expect(screen.getByTestId('info-skipVerify')).toBeInTheDocument();
    await act(async () => fireEvent.click(sw));
    const dlg = screen.getByTestId('skip-dialog');
    expect(within(dlg).getByText('Skip the broker certificate check?')).toBeInTheDocument();
    expect(within(dlg).getByTestId('skip-risk').textContent).toBe(
      'Without verification, anyone between you and the broker could pose as it and read or change your messages.',
    );
    expect(Array.from(dlg.querySelectorAll('button')).map((b) => b.textContent)).toEqual(['Cancel', 'Only this time', 'Always for this connection']);
    // Cancel leaves everything as it was
    await act(async () => fireEvent.click(within(dlg).getByText('Cancel')));
    expect(screen.queryByTestId('skip-dialog')).toBeNull();
    expect(sw.getAttribute('aria-checked')).toBe('false');
    await act(async () => fireEvent.click(sw));
    await act(async () => fireEvent.click(screen.getByTestId('skip-always-btn')));
    expect(screen.getByTestId('skip-verify').getAttribute('aria-checked')).toBe('true');
    expect(screen.getByTestId('fold-advanced-summary').textContent).toContain('skip verification');
    await waitFor(async () => expect((await stored('plant')).tls.skipVerify).toBe(true));
    // turning it off needs no prompt
    await act(async () => fireEvent.click(screen.getByTestId('skip-verify')));
    expect(screen.queryByTestId('skip-dialog')).toBeNull();
    await waitFor(async () => expect((await stored('plant')).tls.skipVerify).toBe(false));
  });

  it('"Only this time" leaves the profile unchanged, shows "next connection only" and covers one attempt', async () => {
    const { engine: eng } = await boot('demo');
    const p = await engine().newProfile();
    const lab = await engine().saveProfile({ ...p, name: 'Lab TLS', host: '10.20.4.17', port: 8883, scheme: 'mqtts' });
    await sync();
    useApp.setState({ folds: { security: false, advanced: true, brokerAdvanced: false } });
    render(
      <>
        <ConnectionsScreen />
        <Bar />
      </>,
    );
    await act(async () => useApp.getState().selectProfile(lab.id));
    await act(async () => fireEvent.click(screen.getByTestId('skip-verify')));
    await act(async () => fireEvent.click(screen.getByTestId('skip-once-btn')));
    expect(screen.getByTestId('skip-verify').getAttribute('aria-checked')).toBe('false');
    expect(screen.getByTestId('skip-once').textContent).toBe('next connection only');
    expect(useApp.getState().skipOnce[lab.id]).toBe(true);
    const spy = vi.spyOn(eng, 'connectProfile');
    await act(async () => void (await connectProfile(lab.id, false)));
    expect(spy).toHaveBeenLastCalledWith(lab.id, true);
    expect(useTrust.getState().prompt).toBeNull();
    await act(async () => sync());
    expect(screen.queryByTestId('skip-once')).toBeNull();
    expect((await stored(lab.id)).tls.skipVerify).toBe(false);
    // the open connection skipped the check: ⚠ on the card and the tab
    expect(within(card(lab.id)).getByTestId('card-warn').className).toBe('warn-i');
    expect(screen.getByTestId('cbar-warn').getAttribute('data-kind')).toBe('skip');
    // the next attempt verifies again: the certificate prompt opens
    await act(async () => {
      await engine().closeConnection(lab.id);
      await sync();
    });
    await act(async () => void (await connectProfile(lab.id, false)));
    expect(spy).toHaveBeenLastCalledWith(lab.id, undefined);
    expect(useTrust.getState().prompt?.reason).toBe('selfSigned');
    act(() => useTrust.getState().set(null, null));
  });
});

describe('Change set 6: certificate prompt (C-30)', () => {
  const setup = async () => {
    await boot('demo');
    const p = await engine().newProfile();
    const lab = await engine().saveProfile({ ...p, name: 'Lab TLS', host: '10.20.4.17', port: 8883, scheme: 'mqtts' });
    await sync();
    await connectProfile(lab.id, false);
    expect(useTrust.getState().prompt?.profileId).toBe(lab.id);
    return lab.id;
  };

  it('"only this time" connects and saves nothing', async () => {
    const id = await setup();
    await act(async () => trustAndRetry('skipOnce'));
    const c = (await engine().listConnections()).find((x) => x.id === id)!;
    expect(c.state).toBe('connected');
    expect(c.skipVerify).toBe(true);
    const p = await stored(id);
    expect(p.tls.skipVerify).toBe(false);
    expect(p.tls.trustedFingerprints).toEqual([]);
  });

  it('"always for this connection" sets the profile switch', async () => {
    const id = await setup();
    await act(async () => trustAndRetry('skipAlways'));
    expect((await stored(id)).tls.skipVerify).toBe(true);
    expect((await engine().listConnections()).find((x) => x.id === id)!.skipVerify).toBe(true);
  });

  it('"Trust this certificate" pins the fingerprint; the connection is verified by it, not skipped', async () => {
    const id = await setup();
    await act(async () => trustAndRetry('certificate'));
    const p = await stored(id);
    expect(p.tls.trustedFingerprints).toHaveLength(1);
    expect(p.tls.skipVerify).toBe(false);
    expect((await engine().listConnections()).find((x) => x.id === id)!.skipVerify).toBe(false);
  });
});

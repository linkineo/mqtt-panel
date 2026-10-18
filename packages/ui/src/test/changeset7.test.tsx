import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { boot, connect } from './util';
import { engine } from '../services';
import { useApp } from '../store/app';
import { BrokerScreen } from '../screens/broker/BrokerScreen';
import { AddListenerDialog } from '../screens/broker/BrokerDialogs';
import { AppBrokerDialog } from '../screens/connections/AppBrokerDialog';
import en from '../i18n/en/ui.json';
import fr from '../i18n/fr/ui.json';
import de from '../i18n/de/ui.json';
import enErr from '../i18n/en/errors.json';
import frErr from '../i18n/fr/errors.json';
import deErr from '../i18n/de/errors.json';

async function brokerScreen() {
  useApp.setState({ brokerConfig: await engine().brokerConfig(), broker: await engine().brokerStatus(), screen: 'broker' });
  render(<BrokerScreen />);
  await waitFor(() => expect(screen.getAllByTestId('listener-row').length).toBeGreaterThan(0));
}

describe('Change set 7: no restart (B-25)', () => {
  it('the header holds Start and Stop only; a change applies at once, nothing prompts, nothing is pending', async () => {
    await boot('demo');
    await brokerScreen();
    const header = screen.getByTestId('broker').querySelector('.ph')!;
    expect(Array.from(header.querySelectorAll('button')).map((b) => b.textContent)).toEqual(['Stop']);
    expect(screen.queryByTestId('broker-restart')).toBeNull();
    expect(header.querySelector('.pulse')).toBeNull();
    await act(async () => fireEvent.click(screen.getByTestId('reach-localhost')));
    await waitFor(() => expect(useApp.getState().brokerConfig!.bind).toBe('localhost'));
    expect(screen.queryByTestId('restart-prompt')).toBeNull();
    expect(screen.queryByTestId('restart-chip')).toBeNull();
    expect(screen.queryByTestId('port-pending')).toBeNull();
    const st = await engine().brokerStatus();
    expect('restartNeeded' in st).toBe(false);
    expect(st.state).toBe('running');
    // a port change applies live: the log has the listener lines
    const tls = (await engine().brokerConfig()).listeners.find((l) => l.id === 'l-tls')!;
    await engine().updateListener({ ...tls, port: 8884 });
    const lines = (await engine().brokerLog('info', 0)).map((l) => l.text);
    expect(lines).toContain('listener TLS 8883 stopped');
    expect(lines.some((l) => /^listener TLS 8884 started · \d+ sessions? dropped: port changed$/.test(l))).toBe(true);
  });

  it('stopped: the header shows Start; a change is saved and used at the next start', async () => {
    await boot('demo');
    await engine().brokerStop();
    await brokerScreen();
    const header = screen.getByTestId('broker').querySelector('.ph')!;
    expect(Array.from(header.querySelectorAll('button')).map((b) => b.textContent)).toEqual(['Start']);
    await act(async () => fireEvent.click(screen.getByTestId('reach-localhost')));
    expect(screen.queryByTestId('restart-prompt')).toBeNull();
    expect((await engine().brokerConfig()).bind).toBe('localhost');
  });

  it('a listener that cannot start shows its error on its row, in brick, without wrapping; the others keep running', async () => {
    const { engine: mock } = await boot('demo');
    mock.takePort(8084);
    await brokerScreen();
    const row = screen.getAllByTestId('listener-row').find((r) => r.getAttribute('data-listener') === 'l-wss')!;
    await act(async () => fireEvent.click(within(row).getByRole('switch')));
    const err = await screen.findByTestId('listener-error-l-wss');
    expect(err.textContent).toBe('not started');
    expect(err.className).toBe('lerr');
    expect(err.getAttribute('title')).toBe('The WSS listener on port 8084 could not start: port already in use');
    const st = await engine().brokerStatus();
    expect(st.state).toBe('running');
    expect(st.listeners.filter((l) => l.id !== 'l-wss' && l.up).length).toBeGreaterThan(0);
    expect(st.listeners.find((l) => l.id === 'l-wss')!.up).toBe(false);
    expect(screen.queryByTestId('listener-error-l-tcp')).toBeNull();
    expect((await engine().brokerLog('error', 0)).map((l) => l.text)).toContain('listener WSS 8084 could not start: port already in use');
    // once the port is free again, Start tries the listener again
    mock.takePort(8084, false);
    await act(async () => {
      useApp.setState({ broker: await engine().brokerStart() });
    });
    await waitFor(() => expect(screen.queryByTestId('listener-error-l-wss')).toBeNull());
  });

  it('Add listener step 2 has no restart note', async () => {
    await boot('demo');
    const cfg = await engine().brokerConfig();
    render(<AddListenerDialog config={cfg} onClose={() => undefined} onConfig={() => undefined} />);
    await act(async () => fireEvent.click(screen.getByTestId('addl-next')));
    const note = await screen.findByTestId('addl-after');
    expect(note.textContent).not.toMatch(/restart/i);
  });

  it('the built-in broker dialog has no restart step: start only when stopped', async () => {
    await boot('demo');
    render(<AppBrokerDialog start="pick" onClose={() => undefined} />);
    await screen.findAllByTestId('appb-listener');
    const what = screen.getByTestId('appb-what').textContent!;
    expect(what).toContain('start the broker if it is stopped');
    expect(what).not.toMatch(/restart/i);
    await act(async () => fireEvent.click(screen.getByTestId('appb-connect')));
    await waitFor(() => expect(useApp.getState().screen).toBe('explore'), { timeout: 4000 });
  });

  it('no catalog mentions a broker restart; the ⓘ texts and the regenerate texts lost theirs', () => {
    const cats: [string, Record<string, string>][] = [
      ['en', { ...en, ...enErr }],
      ['fr', { ...fr, ...frErr }],
      ['de', { ...de, ...deErr }],
    ];
    for (const [lang, cat] of cats) {
      const hits = Object.entries(cat)
        .filter(([, v]) => /restart|redémarr|neu ?start|neustart|starte .* neu\b/i.test(v))
        .map(([k]) => k);
      // only the app's own restart, after a keychain refusal or a key file other users can read (change set 10)
      expect(hits, lang).toEqual(['keychain.refused', 'keychain.keyFile']);
      for (const k of ['broker.restart', 'broker.restartApply', 'broker.restartPending', 'broker.pendingChip', 'addl.afterPrompt', 'appb.s.restart', 'appb.s.restarted', 'rs.title']) {
        expect(cat[k], `${lang} ${k}`).toBeUndefined();
      }
      for (const k of ['broker.notStarted', 'certs.exportDots', 'certs.pem', 'certs.p12Dots', 'addl.afterFlow', 'appb.what', 'appb.whatAnon']) {
        expect(cat[k], `${lang} ${k}`).toBeTruthy();
      }
    }
    expect(en['certs.regenerated']).toBe('Server certificate regenerated.');
    expect(fr['addl.afterFlow']).toContain('démarre le broker s’il est arrêté');
    expect(de['appb.what']).toContain('den Broker starten, falls er gestoppt ist');
  });
});

describe('Change set 7: download menu on certificate rows (B-26)', () => {
  it('one ↓ button per row opens PEM / PKCS#12…; PEM exports at once, PKCS#12… asks for the passphrase', async () => {
    const { engine: mock } = await boot('demo');
    await brokerScreen();
    const rows = screen.getAllByTestId('issued-row');
    for (const r of rows) {
      expect(within(r).queryByText('PEM')).toBeNull();
      expect(within(r).queryByText('PKCS#12')).toBeNull();
      const btn = within(r).getByTestId('cert-export');
      expect(btn.textContent).toBe('↓');
      expect(btn.getAttribute('aria-label')).toBe('Export…');
      expect(btn.getAttribute('title')).toBe('Export…');
    }
    // the certificate of "Local broker" keeps its tag instead of ✕
    const desktop = rows.find((r) => r.getAttribute('data-cert') === 'mqttpanel-desktop')!;
    expect(within(desktop).getByTestId('cert-local-tag').textContent).toBe('Local broker');
    expect(within(desktop).queryByText('✕')).toBeNull();
    const edge = rows.find((r) => r.getAttribute('data-cert') === 'edge01')!;
    await act(async () => fireEvent.click(within(edge).getByTestId('cert-export')));
    const menu = await screen.findByTestId('context-menu');
    expect(Array.from(menu.querySelectorAll('.ctx-label')).map((x) => x.textContent)).toEqual(['PEM', 'PKCS#12…']);
    await act(async () => fireEvent.click(within(menu).getByText('PEM').closest('.ctx-item')!));
    await waitFor(() => expect(mock.savedFiles().map((f) => f.fileName)).toContain('edge01.pem'));
    await act(async () => fireEvent.click(within(edge).getByTestId('cert-export')));
    const menu2 = await screen.findByTestId('context-menu');
    await act(async () => fireEvent.click(within(menu2).getByText('PKCS#12…').closest('.ctx-item')!));
    expect(await screen.findByText('Export edge01 as PKCS#12')).toBeInTheDocument();
    expect(screen.getByTestId('p12-passphrase')).toBeInTheDocument();
  });

  it('greyed with Anonymous on', async () => {
    await boot('demo');
    await engine().setAnonymous(true);
    await brokerScreen();
    for (const b of screen.getAllByTestId('cert-export')) expect((b as HTMLButtonElement).disabled).toBe(true);
  });
});

describe('Change set 7: "Local broker" reconnects only when touched (C-16, mock engine)', () => {
  it('stays connected through unrelated changes and reconnects when its listener or login changes', async () => {
    await boot('demo');
    await connect('local');
    const state = async () => (await engine().listConnections()).find((c) => c.id === 'local')!;
    // every state the connection went through, from the engine's ticks
    const seen: string[] = [];
    const off = engine().onEvent((e) => {
      if (e.type === 'tick') for (const c of e.connections) if (c.id === 'local') seen.push(c.state);
    });
    // another listener's port and another user's password: untouched
    const tcp = (await engine().brokerConfig()).listeners.find((l) => l.id === 'l-tcp')!;
    await engine().updateListener({ ...tcp, port: 1884 });
    await engine().setUserPassword('edge01', 'another-one');
    expect((await state()).state).toBe('connected');
    expect(seen).not.toContain('reconnecting');
    // its own listener's port: reconnects at once on the new port
    const tls = (await engine().brokerConfig()).listeners.find((l) => l.id === 'l-tls')!;
    await engine().updateListener({ ...tls, port: 8885 });
    const after = await state();
    expect(after.state).toBe('connected');
    expect((await engine().listProfiles()).find((p) => p.id === 'local')!.port).toBe(8885);
    expect(seen).toContain('reconnecting');
    off();
  });
});

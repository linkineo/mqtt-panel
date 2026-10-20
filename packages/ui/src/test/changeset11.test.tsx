import { describe, expect, it } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { boot } from './util';
import { engine } from '../services';
import { useApp } from '../store/app';
import { ProfileForm } from '../screens/connections/ProfileForm';
import { BrokerScreen } from '../screens/broker/BrokerScreen';
import { RegenerateDialog } from '../screens/broker/BrokerDialogs';
import { shortFingerprint } from '../screens/connections/ClientCertWays';
import en from '../i18n/en/ui.json';
import fr from '../i18n/fr/ui.json';
import de from '../i18n/de/ui.json';

const root = join(__dirname, '..');
function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) return f === 'test' || f === 'i18n' ? [] : sources(p);
    return /\.(tsx?|css)$/.test(f) ? [p] : [];
  });
}

async function form(id = 'plant', folds = { security: true, advanced: false }) {
  const booted = await boot('demo');
  useApp.setState({ folds: { ...folds, brokerAdvanced: false } });
  render(<ProfileForm profileId={id} />);
  await screen.findByTestId('profile-form');
  return booted;
}

const fresh = async () => (await engine().listProfiles()).find((p) => p.id === 'plant')!;

describe('Change set 11: round ⓘ (C-31)', () => {
  it('every info button is the round component with an "i"; no pill-shaped info chip remains in the sources', async () => {
    for (const f of sources(root)) {
      const s = readFileSync(f, 'utf8');
      expect(s.includes('chip info'), f).toBe(false);
      expect(/\.chip\.info\b/.test(s), f).toBe(false);
    }
    await form();
    const infos = screen.getAllByRole('button', { name: /^About / });
    expect(infos.length).toBeGreaterThan(3);
    for (const b of infos) {
      expect(b.className.split(' ')).toContain('info-btn');
      expect(b.textContent).toBe('i');
    }
  });
});

describe('Change set 11: accepted formats and the password eye (C-32)', () => {
  it('writes the picker filters after Browse, in every language the same', async () => {
    await form();
    expect(screen.getByTestId('formats-caCert').textContent).toBe('PEM, CRT, DER');
    expect(screen.getByTestId('formats-clientCert').textContent).toBe('PEM, CRT, DER, P12, PFX');
    expect(screen.getByTestId('formats-clientKey').textContent).toBe('PEM, KEY');
    for (const cat of [fr, de] as Record<string, string>[]) {
      for (const k of ['caCert', 'clientCert', 'clientKey', 'signedCert']) expect(cat[`file.formats.${k}`]).toBe((en as Record<string, string>)[`file.formats.${k}`]);
    }
  });

  it('the picker filters in the native adapter match the written formats', () => {
    const files = readFileSync(join(root, '../../../apps/desktop/src-tauri/src/files.rs'), 'utf8');
    const filter = (kind: string) => {
      const m = files.match(new RegExp(`"${kind}" => \\("[^"]+", &\\[([^\\]]+)\\]`));
      return m![1].split(',').map((x) => x.trim().replace(/"/g, '').toUpperCase()).join(', ');
    };
    expect(filter('caCert')).toBe('PEM, CRT, DER');
    expect(filter('clientCert')).toBe('PEM, CRT, DER, P12, PFX');
    expect(filter('clientKey')).toBe('PEM, KEY');
    expect(filter('signedCert')).toBe('PEM, CRT, DER');
  });

  it('the eye reveals the stored password while pressed and the typed one before it is saved', async () => {
    await form('plant', { security: false, advanced: false });
    const input = screen.getByTestId('pf-pw') as HTMLInputElement;
    const eye = screen.getByTestId('pw-eye');
    expect(input.type).toBe('password');
    await act(async () => fireEvent.pointerDown(eye));
    await waitFor(() => expect(input.type).toBe('text'));
    expect(input.value).toBe('site-2026-commissioning');
    await act(async () => fireEvent.pointerUp(eye));
    expect(input.type).toBe('password');
    expect(input.value).toBe('');
    fireEvent.change(input, { target: { value: 'typed-now' } });
    await act(async () => fireEvent.keyDown(eye, { key: ' ' }));
    await waitFor(() => expect(input.type).toBe('text'));
    expect(input.value).toBe('typed-now');
    await act(async () => fireEvent.keyUp(eye, { key: ' ' }));
    expect(input.type).toBe('password');
    // no copy icon on a password the user typed
    expect(screen.queryByTestId('profile-pw-copy')).toBeNull();
  });
});

describe('Change set 11: client certificate, two ways (C-33)', () => {
  it('Import is the default with the known rows; the summary says "(imported)"', async () => {
    await form();
    const seg = screen.getByTestId('cert-way');
    expect(within(seg).getByTestId('cert-way-import').getAttribute('aria-checked')).toBe('true');
    expect(screen.getByTestId('file-clientCert')).toBeTruthy();
    expect(screen.getByTestId('file-clientKey')).toBeTruthy();
    expect(screen.getByTestId('file-caCert')).toBeTruthy();
    expect(screen.getByTestId('fold-security-summary').textContent).toContain('client certificate (imported)');
  });

  it('switching with material asks first; Cancel keeps it, Discard clears and shows the generate card with the CA row kept', async () => {
    await form();
    await act(async () => fireEvent.click(screen.getByTestId('cert-way-generate')));
    const dlg = await screen.findByTestId('cert-way-confirm');
    expect(dlg.textContent).toContain('Discard the imported certificate and key?');
    await act(async () => fireEvent.click(within(dlg).getByText('Cancel')));
    expect((await fresh()).tls.clientCert).not.toBeNull();
    await act(async () => fireEvent.click(screen.getByTestId('cert-way-generate')));
    await act(async () => fireEvent.click(within(await screen.findByTestId('cert-way-confirm')).getByTestId('confirm-ok')));
    await screen.findByTestId('gen-card');
    const p = await fresh();
    expect(p.tls.clientKeyGenerated).toBe(true);
    expect(p.tls.clientCert).toBeNull();
    expect(p.tls.clientKey).toBeNull();
    expect(p.tls.caCert).not.toBeNull();
    expect(screen.getByTestId('file-caCert')).toBeTruthy();
    expect(screen.queryByTestId('file-clientKey')).toBeNull();
    // nothing on the generate side yet: going back is silent
    await act(async () => fireEvent.click(screen.getByTestId('cert-way-import')));
    await waitFor(() => expect(screen.queryByTestId('gen-card')).toBeNull());
    expect(screen.queryByTestId('cert-way-confirm')).toBeNull();
  });

  it('generates a key, downloads the CSR with the client ID as common name, imports the signed certificate, starts over', async () => {
    const { engine: mock } = await form();
    await act(async () => engine().setClientCertWay('plant', true).then(async (p) => useApp.setState({ profiles: (await engine().listProfiles()).map((x) => (x.id === p.id ? p : x)) })));
    await screen.findByTestId('gen-card');
    expect((screen.getByTestId('gen-csr') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTestId('browse-signedCert') as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByTestId('formats-signedCert').textContent).toBe('PEM, CRT, DER');
    await act(async () => fireEvent.click(screen.getByTestId('gen-key')));
    const line = await screen.findByTestId('gen-key-line');
    expect(line.textContent).toMatch(/^Key generated · ECDSA P-256 · [0-9A-F]{2}(:[0-9A-F]{2}){3}…[0-9A-F]{2}:[0-9A-F]{2}$/);
    expect(line.getAttribute('title')).toMatch(/^([0-9A-F]{2}:){31}[0-9A-F]{2}$/);
    const cn = screen.getByTestId('gen-cn') as HTMLInputElement;
    expect(cn.value).toBe('mqttpanel-7f3a');
    fireEvent.change(cn, { target: { value: 'commissioning laptop' } });
    await act(async () => fireEvent.click(screen.getByTestId('gen-csr')));
    await waitFor(() => expect(mock.savedFiles().map((s) => s.fileName)).toContain('commissioning_laptop.csr'));
    // a certificate for another key is refused with the plain sentence
    await act(async () => {
      await engine().profileImportSignedCert('plant', '/x/other-device.crt').catch((e) => expect(e.key).toBe('file.notGeneratedKey'));
    });
    await act(async () => fireEvent.click(screen.getByTestId('browse-signedCert')));
    await waitFor(() => expect(screen.getByTestId('file-signedCert').textContent).toContain('mqttpanel-7f3a.crt'));
    expect(screen.getByTestId('fold-security-summary').textContent).toContain('client certificate (generated key)');
    await act(async () => fireEvent.click(screen.getByTestId('gen-start-over')));
    const dlg = await screen.findByTestId('start-over-confirm');
    await act(async () => fireEvent.click(within(dlg).getByTestId('confirm-ok')));
    await waitFor(() => expect(screen.queryByTestId('gen-key-line')).toBeNull());
    const p = await fresh();
    expect(p.tls.clientKeyGenerated).toBe(true);
    expect(p.tls.clientKey).toBeNull();
    expect(p.tls.clientCert).toBeNull();
  });

  it('shortens a fingerprint for the row', () => {
    const fp = Array.from({ length: 32 }, (_, i) => i.toString(16).padStart(2, '0').toUpperCase()).join(':');
    expect(shortFingerprint(fp)).toBe('00:01:02:03…1E:1F');
  });
});

describe('Change set 11: session expiry (C-34)', () => {
  it('shows only the unit, explains 0 and empty in its ⓘ, and saves an empty field as 0', async () => {
    await form('plant', { security: false, advanced: true });
    const input = screen.getByLabelText('Session expiry') as HTMLInputElement;
    const row = input.closest('.f') as HTMLElement;
    expect(row.textContent).not.toContain('0 = none');
    expect(within(row).getByText('s')).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByTestId('info-sessionExpiry')));
    expect(screen.getByTestId('bubble-text-sessionExpiry').textContent).toContain('Empty or 0: the session ends with the connection.');
    fireEvent.change(input, { target: { value: '120' } });
    fireEvent.change(input, { target: { value: '' } });
    await waitFor(async () => expect((await fresh()).sessionExpiry).toBe(0), { timeout: 2000 });
    for (const cat of [en, fr, de] as Record<string, string>[]) expect(cat['conn.sessionExpiryHint']).toBeUndefined();
  });
});

describe('Change set 11: Broker screen (B-27, B-29)', () => {
  it('Start is the accent primary button; the Status tile carries play/stop doing the same', async () => {
    await boot('empty');
    useApp.setState({ brokerConfig: await engine().brokerConfig(), broker: await engine().brokerStatus(), screen: 'broker' });
    render(<BrokerScreen />);
    const start = await screen.findByTestId('broker-start');
    expect(start.className.split(' ')).toContain('pri');
    await act(async () => fireEvent.click(screen.getByTestId('tile-start')));
    await waitFor(() => expect(screen.getByTestId('tile-stop')).toBeTruthy());
    const stop = screen.getByTestId('broker-stop');
    expect(stop.className.split(' ')).toEqual(expect.arrayContaining(['danger', 'out']));
    expect((await engine().brokerStatus()).state).toBe('running');
    await act(async () => fireEvent.click(screen.getByTestId('tile-stop')));
    await waitFor(() => expect(screen.getByTestId('tile-start')).toBeTruthy());
    expect((await engine().brokerStatus()).state).toBe('stopped');
  });

  it('Regenerate lists the automatic names read-only and keeps valid extra names; an invalid one is refused', async () => {
    await boot('demo');
    const certs = await engine().brokerCerts();
    expect(certs.autoSans).toContain('192.168.1.20');
    let got = certs;
    let closed = false;
    render(<RegenerateDialog certs={certs} onClose={() => (closed = true)} onCerts={(c) => (got = c)} />);
    expect(screen.getByTestId('regen-auto').textContent).toContain('localhost, 127.0.0.1, ::1');
    const extra = screen.getByTestId('regen-extra');
    fireEvent.change(extra, { target: { value: 'broker.example.com bad_name' } });
    await act(async () => fireEvent.click(screen.getByTestId('regen-ok')));
    expect((await screen.findByTestId('regen-error')).textContent).toContain('bad_name');
    expect(closed).toBe(false);
    fireEvent.change(extra, { target: { value: 'broker.example.com, 203.0.113.7' } });
    await act(async () => fireEvent.click(screen.getByTestId('regen-ok')));
    expect(closed).toBe(true);
    expect(got.server.san).toEqual(expect.arrayContaining(['localhost', '192.168.1.20', 'broker.example.com', '203.0.113.7']));
    expect((await engine().brokerConfig()).extraSans).toEqual(['broker.example.com', '203.0.113.7']);
  });

  it('"Export…" on the Server certificate line saves the served chain', async () => {
    const { engine: mock } = await boot('demo');
    useApp.setState({ brokerConfig: await engine().brokerConfig(), screen: 'broker', folds: { security: false, advanced: false, brokerAdvanced: true } });
    render(<BrokerScreen />);
    const btn = await screen.findByTestId('export-server-cert');
    await act(async () => fireEvent.click(btn));
    await waitFor(() => expect(mock.savedFiles().map((s) => s.fileName)).toContain('server-chain.pem'));
  });
});

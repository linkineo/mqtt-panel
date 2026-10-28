import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { boot } from './util';
import { engine } from '../services';
import { useApp } from '../store/app';
import { ProfileForm } from '../screens/connections/ProfileForm';
import { ConnectionsScreen } from '../screens/connections/ConnectionsScreen';
import { resources } from '../i18n';

const stored = async (id: string) => (await engine().listProfiles()).find((p) => p.id === id)!;
const card = (id: string) => screen.getAllByTestId('profile-card').find((c) => c.getAttribute('data-profile') === id)!;
const ui = (l: 'en' | 'fr' | 'de') => resources[l].ui as Record<string, string>;

async function form(id: string) {
  useApp.setState({ profiles: await engine().listProfiles(), folds: { security: true, advanced: false, brokerAdvanced: false } });
  render(<ProfileForm profileId={id} />);
  await screen.findByTestId('profile-form');
}

describe('Security fold (owner, 28 Oct 2026)', () => {
  it('two subheadings: broker certificate before the CA row, client certificate before the two ways', async () => {
    await boot('demo');
    await form('plant');
    const broker = screen.getByTestId('sub-broker-cert');
    const client = screen.getByTestId('sub-client-cert');
    expect(broker.textContent).toBe('Broker certificate');
    expect(client.textContent).toBe('Client certificate');
    expect(broker.className).toBe('sub');
    const follows = (a: Element, b: Element) => !!(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);
    expect(follows(broker, screen.getByTestId('file-caCert'))).toBe(true);
    expect(follows(screen.getByTestId('file-caCert'), client)).toBe(true);
    expect(follows(client, screen.getByTestId('cert-way'))).toBe(true);
    expect(ui('fr')['conn.sub.brokerCert']).toBe('Certificat du broker');
    expect(ui('de')['conn.sub.clientCert']).toBe('Client-Zertifikat');
  });

  it('a trusted certificate shows with its short fingerprint, an ⓘ and Forget; forgetting saves the profile', async () => {
    await boot('demo');
    const fp = '3F:9A:12:7C:00:11:22:33:44:55:D1:7E';
    const plant = await stored('plant');
    await engine().saveProfile({ ...plant, tls: { ...plant.tls, trustedFingerprints: [fp] } });
    await form('plant');
    expect(screen.getByTestId('fold-security-summary').textContent).toContain('trusted certificate');
    const chip = screen.getByTestId('trusted-fp');
    expect(chip.textContent).toContain('3F:9A:12:7C…D1:7E');
    expect(chip.getAttribute('title')).toBe(fp);
    expect(screen.getByTestId('info-trustedCert')).toBeInTheDocument();
    await act(async () => fireEvent.click(screen.getByTestId('forget-trusted')));
    expect(screen.queryByTestId('trusted-fp')).toBeNull();
    expect(screen.getByTestId('fold-security-summary').textContent).not.toContain('trusted certificate');
    await waitFor(async () => expect((await stored('plant')).tls.trustedFingerprints).toEqual([]), { timeout: 2000 });
  });

  it('no trusted row without a fingerprint', async () => {
    await boot('demo');
    await form('plant');
    expect(screen.queryByTestId('trusted-fp')).toBeNull();
  });

  it('a file that does not fit raises a toast, no text next to the field', async () => {
    await boot('demo');
    await form('plant');
    const error = { key: 'file.notACertificate', params: {}, text: 'This file is not a certificate.' };
    vi.spyOn(engine(), 'pickFile').mockResolvedValueOnce({ ok: false, path: 'x', fileName: 'x.txt', error });
    useApp.setState({ toasts: [] });
    await act(async () => fireEvent.click(screen.getByTestId('browse-caCert')));
    expect(useApp.getState().toasts.at(-1)).toMatchObject({ kind: 'err', msg: error });
    expect(screen.queryByTestId('file-error-caCert')).toBeNull();
    expect(screen.getByTestId('file-caCert').querySelector('.hint.err')).toBeNull();
    expect(screen.getByTestId('file-caCert').querySelector('.input.err')).not.toBeNull();
  });
});

describe('Skip always versus trusted fingerprint (owner, 28 Oct 2026)', () => {
  it('"always skip" adds a bold label next to the ⚠; a pinned certificate keeps the muted icon alone', async () => {
    await boot('demo');
    const plant = await stored('plant');
    await engine().saveProfile({ ...plant, tls: { ...plant.tls, trustedFingerprints: ['3F:9A'] } });
    useApp.setState({ profiles: await engine().listProfiles() });
    render(<ConnectionsScreen />);
    const label = within(card('emqx')).getByTestId('card-warn-label');
    expect(label.tagName).toBe('STRONG');
    expect(label.textContent).toBe('No verification');
    expect(within(card('plant')).getByTestId('card-warn').className).toBe('warn-i pin');
    expect(within(card('plant')).queryByTestId('card-warn-label')).toBeNull();
    expect(ui('fr')['conn.warnSkipLabel']).toBe('Sans vérification');
    expect(ui('de')['conn.warnSkipLabel']).toBe('Ohne Prüfung');
  });
});

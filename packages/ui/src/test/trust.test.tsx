import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { CertDetails, TrustPrompt } from '@mqttpanel/engine-client';
import { boot } from './util';
import { TrustDialogView } from '../components/TrustDialog';

const leaf: CertDetails = {
  subject: 'CN=mosquitto-lab',
  issuer: 'CN=mosquitto-lab',
  san: ['mosquitto-lab', '10.20.4.17'],
  notBefore: '2026-09-01',
  notAfter: '2027-09-01',
  fingerprint: '3F:9A',
  keyType: 'ECDSA P-256',
  selfSigned: true,
  isCa: false,
};

function prompt(p: Partial<TrustPrompt>): TrustPrompt {
  return {
    profileId: 'lab',
    reason: 'selfSigned',
    reasonText: { key: 'tls.reason.selfSigned', params: {}, text: 'Reason: the certificate is self-signed.' },
    leaf,
    chain: [leaf],
    caOffer: false,
    previousFingerprint: null,
    unsafe: false,
    ...p,
  };
}

describe('Trust dialog (T-04, T-06; change set 6: one list, one Connect button)', () => {
  it('lists pin (preselected), skip once and skip always for a self-signed certificate, with the risk sentence', async () => {
    await boot('empty');
    const onTrust = vi.fn();
    render(<TrustDialogView prompt={prompt({})} profileName="Lab Mosquitto" onCancel={() => undefined} onTrust={onTrust} />);
    expect(screen.getByText("This broker's certificate cannot be verified")).toBeInTheDocument();
    expect(screen.getByTestId('trust-risk').textContent).toBe(
      'Without verification, anyone between you and the broker could pose as it and read or change your messages.',
    );
    const group = screen.getByRole('radiogroup');
    const radios = Array.from(group.querySelectorAll('[role="radio"]'));
    expect(radios.map((r) => r.getAttribute('data-mode'))).toEqual(['certificate', 'skipOnce', 'skipAlways']);
    expect(screen.getByTestId('trust-certificate').getAttribute('aria-checked')).toBe('true');
    expect(screen.getByTestId('trust-certificate').textContent).toBe(
      'Trust this certificateThis certificate only, for the connection "Lab Mosquitto". If the broker presents a different one, you are asked again.',
    );
    expect(screen.getByTestId('trust-skipOnce').textContent).toBe('Skip verification only this timeThis attempt accepts any certificate. Nothing is saved.');
    expect(screen.getByTestId('trust-skipAlways').textContent).toBe(
      'Skip verification always for this connectionSaved in the connection: any certificate is accepted whenever it opens. The card shows ⚠.',
    );
    expect(screen.queryByTestId('trust-ca')).toBeNull();
    expect(screen.getByText(/self-signed\. Nobody vouches/)).toBeInTheDocument();
    expect(screen.getByTestId('trust-connect').textContent).toBe('Connect');
    fireEvent.click(screen.getByTestId('trust-connect'));
    expect(onTrust).toHaveBeenLastCalledWith('certificate');
    fireEvent.click(screen.getByTestId('trust-skipOnce'));
    expect(screen.getByTestId('trust-skipOnce').getAttribute('aria-checked')).toBe('true');
    fireEvent.click(screen.getByTestId('trust-connect'));
    expect(onTrust).toHaveBeenLastCalledWith('skipOnce');
    fireEvent.keyDown(group, { key: 'ArrowDown' });
    fireEvent.click(screen.getByTestId('trust-connect'));
    expect(onTrust).toHaveBeenLastCalledWith('skipAlways');
  });

  it('adds "Trust anything this CA signs" only when caOffer is true', async () => {
    await boot('empty');
    const onTrust = vi.fn();
    const ca = { ...leaf, subject: 'CN=Lab Private CA', isCa: true };
    render(
      <TrustDialogView
        prompt={prompt({ reason: 'unknownCa', caOffer: true, chain: [leaf, ca], reasonText: { key: 'tls.reason.unknownCa', params: { issuer: 'CN=Lab Private CA' }, text: '' } })}
        profileName="Lab"
        onCancel={() => undefined}
        onTrust={onTrust}
      />,
    );
    expect(screen.getByTestId('trust-ca').textContent).toContain('Trust anything this CA signs');
    expect(screen.getByTestId('trust-ca').textContent).toContain('Future certificates from this CA too');
    fireEvent.click(screen.getByTestId('trust-ca'));
    fireEvent.click(screen.getByTestId('trust-connect'));
    expect(onTrust).toHaveBeenCalledWith('ca');
  });

  it('marks expired certificates unsafe: danger tone on Connect', async () => {
    await boot('empty');
    render(<TrustDialogView prompt={prompt({ reason: 'expired', unsafe: true })} profileName="Lab" onCancel={() => undefined} onTrust={() => undefined} />);
    expect(screen.getByText('unsafe')).toBeInTheDocument();
    expect(screen.getByTestId('trust-connect').className).toBe('btn danger solid');
  });
});

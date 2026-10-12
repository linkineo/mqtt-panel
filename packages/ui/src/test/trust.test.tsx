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

describe('Trust dialog (T-04, T-06)', () => {
  it('offers one action for a self-signed certificate, with its scope line', async () => {
    await boot('empty');
    const onTrust = vi.fn();
    render(<TrustDialogView prompt={prompt({})} profileName="Lab Mosquitto" onCancel={() => undefined} onTrust={onTrust} />);
    expect(screen.getByText('This broker presents a certificate MQTT Panel cannot verify')).toBeInTheDocument();
    expect(screen.getByTestId('trust-cert').textContent).toBe('Trust this certificate');
    expect(screen.queryByTestId('trust-ca')).toBeNull();
    expect(screen.getByTestId('trust-scope-cert').textContent).toBe(
      'This certificate only, for the profile "Lab Mosquitto". If the broker presents a different one, you are asked again.',
    );
    expect(screen.getByText(/self-signed\. Nobody vouches/)).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('trust-cert'));
    expect(onTrust).toHaveBeenCalledWith('certificate');
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
    expect(screen.getByTestId('trust-ca').textContent).toBe('Trust anything this CA signs');
    expect(screen.getByTestId('trust-scope-ca').textContent).toContain('Future certificates from this CA too.');
    fireEvent.click(screen.getByTestId('trust-ca'));
    expect(onTrust).toHaveBeenCalledWith('ca');
  });

  it('marks expired certificates unsafe', async () => {
    await boot('empty');
    render(<TrustDialogView prompt={prompt({ reason: 'expired', unsafe: true })} profileName="Lab" onCancel={() => undefined} onTrust={() => undefined} />);
    expect(screen.getByText('unsafe')).toBeInTheDocument();
  });
});

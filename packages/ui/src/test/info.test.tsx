import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { resources } from '../i18n';
import { boot } from './util';
import { Info } from '../components/ui';
import { useApp } from '../store/app';

// Texts from docs/FEATURES.md (T-26); they must match exactly in English. Change set 4: the client certificate text
// drops "(mutual TLS)" (no "mTLS" or "Mutual TLS" anywhere, docs/DECISIONS.md, Open).
const FEATURES: Record<string, string> = {
  caCert:
    "The certificate of the authority that signed the broker's certificate. Needed when the broker uses a private or self-signed certificate; leave empty for brokers with a public certificate.",
  clientCert:
    'Identifies this computer to the broker, instead of or in addition to a password. Only needed if the broker requires client certificates.',
  clientKey: 'The private key belonging to the client certificate. Stored encrypted by MQTT Panel, never in the profile file.',
  skipVerify:
    'Accepts any certificate this broker presents, even an expired, self-signed or impostor one. Use it only on a network you trust, for a broker you cannot give a CA or a fingerprint for.',
  sni: 'The host name sent to the broker during the TLS handshake. Leave empty to use the host above; only change it if the broker serves several names on one address.',
  alpn: 'Tells the broker which protocol is wanted over TLS. Some cloud brokers require a specific value (for example mqtt); leave empty otherwise.',
  filters: 'Filters decide which topics this connection receives. # means everything; building/+/temperature means one level of wildcard.',
  passphrase:
    'Protects only the exported file (the PKCS#12 bundle or the PEM key). It is not the broker login and not a listener password. The device asks for it once, when the certificate is installed.',
  request:
    'Request and response is an MQTT 5 feature. Your message carries a response topic and a correlation ID. The device answers on that response topic and copies the correlation ID into its reply. MQTT Panel subscribes to the response topic for you, matches the reply by correlation ID and shows it next to the sent entry. Broker and device must both speak MQTT 5; on 3.1.1 this section is not available.',
};

const SECURITY = ['caCert', 'clientCert', 'clientKey', 'sni', 'alpn', 'filters', 'anonymous', 'onEncrypted', 'verifyAgainst', 'reachable', 'clientCerts', 'appbAdd', 'passphrase', 'certName', 'preset.plain', 'preset.encrypted', 'preset.passwords', 'preset.mtls', 'skipVerify', 'request'];

describe('Info bubbles (T-26)', () => {
  it('match the FEATURES text exactly in English', () => {
    const en = resources.en.ui as Record<string, string>;
    for (const [k, v] of Object.entries(FEATURES)) expect(en[`info.${k}`]).toBe(v);
  });

  it('exist in English, French and German for every security option', () => {
    for (const lang of ['en', 'fr', 'de'] as const) {
      const cat = resources[lang].ui as Record<string, string>;
      for (const k of SECURITY) {
        expect(cat[`info.${k}`], `${lang} info.${k}`).toBeTruthy();
        if (lang !== 'en') expect(cat[`info.${k}`], `${lang} info.${k} translated`).not.toBe((resources.en.ui as Record<string, string>)[`info.${k}`]);
      }
    }
  });

  it('opens the bubble with the text on click', async () => {
    await boot('empty');
    render(<Info id="sni" title="Server name (SNI)" />);
    await act(async () => fireEvent.click(screen.getByTestId('info-sni')));
    expect(screen.getByTestId('bubble-text-sni').textContent).toBe(FEATURES.sni);
  });

  // Change set 10: the ⓘ of the key fields adds where the master key lives, picked from `key_store_kind`.
  it('says where the master key lives, from the key store query (S-06)', async () => {
    const { engine } = await boot('empty');
    expect(await engine.keyStoreKind()).toBe('keychain');
    expect(useApp.getState().keyStore).toBe('keychain');
    const en = resources.en.ui as Record<string, string>;
    const { unmount } = render(<Info id="clientKey" title="Client key" keyStore />);
    await act(async () => fireEvent.click(screen.getByTestId('info-clientKey')));
    expect(screen.getByTestId('bubble-text-clientKey').textContent).toBe(`${FEATURES.clientKey} ${en['info.keyStore.keychain']}`);
    unmount();

    act(() => useApp.setState({ keyStore: 'file' }));
    render(<Info id="clientKey" title="Client key" keyStore />);
    await act(async () => fireEvent.click(screen.getByTestId('info-clientKey')));
    const text = screen.getByTestId('bubble-text-clientKey').textContent ?? '';
    expect(text).toBe(`${FEATURES.clientKey} ${en['info.keyStore.file']}`);
    expect(text).toContain('only your user account can read');

    for (const lang of ['en', 'fr', 'de'] as const) {
      const cat = resources[lang].ui as Record<string, string>;
      for (const k of ['file', 'keychain']) {
        expect(cat[`info.keyStore.${k}`], `${lang} info.keyStore.${k}`).toBeTruthy();
        if (lang !== 'en') expect(cat[`info.keyStore.${k}`]).not.toBe(en[`info.keyStore.${k}`]);
      }
    }
  });

  it('adds no key store sentence to other bubbles', async () => {
    await boot('empty');
    act(() => useApp.setState({ keyStore: 'file' }));
    render(<Info id="sni" title="Server name (SNI)" />);
    await act(async () => fireEvent.click(screen.getByTestId('info-sni')));
    expect(screen.getByTestId('bubble-text-sni').textContent).toBe(FEATURES.sni);
  });
});

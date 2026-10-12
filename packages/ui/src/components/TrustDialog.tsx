import { useTranslation } from 'react-i18next';
import type { TrustPrompt } from '@mqttpanel/engine-client';
import { useTrust, trustAndRetry } from '../lib/actions';
import { useApp } from '../store/app';
import { Modal, Msg } from './ui';

/** Mockup 03: a broker certificate that cannot be verified. One action for a self-signed certificate; the CA action only with caOffer. */
export function TrustDialogView({ prompt, profileName, onCancel, onTrust }: {
  prompt: TrustPrompt;
  profileName: string;
  onCancel(): void;
  onTrust(mode: 'certificate' | 'ca'): void;
}) {
  const { t } = useTranslation();
  const leaf = prompt.leaf;
  return (
    <Modal
      title={t('trust.title')}
      onClose={onCancel}
      testId="trust-dialog"
      footerStyle={{ flexDirection: 'column', alignItems: 'stretch', gap: 6 }}
      footer={
        <>
          <div className="row" style={{ justifyContent: 'flex-end' }}>
            {prompt.unsafe && <span className="chip brick">{t('trust.unsafe')}</span>}
            <span className="grow" />
            <button type="button" className="btn" onClick={onCancel}>
              {t('common.cancel')}
            </button>
            {prompt.caOffer && (
              <button type="button" className={`btn${prompt.unsafe ? ' danger' : ''}`} data-testid="trust-ca" onClick={() => onTrust('ca')}>
                {t('trust.trustCa')}
              </button>
            )}
            <button type="button" className={prompt.unsafe ? 'btn danger solid' : 'btn pri'} data-testid="trust-cert" onClick={() => onTrust('certificate')}>
              {t('trust.trustCert')}
            </button>
          </div>
          <div className="muted small" style={{ textAlign: 'right' }} data-testid="trust-scope-cert">
            <Msg msg={{ key: 'tls.trustScopeCert', params: { profile: profileName }, text: `This certificate only, for the profile "${profileName}".` }} />
          </div>
          {prompt.caOffer && (
            <div className="muted small" style={{ textAlign: 'right' }} data-testid="trust-scope-ca">
              {t('trust.trustCa')}: <Msg msg={{ key: 'tls.trustScopeCa', params: {}, text: 'Future certificates from this CA too.' }} />
            </div>
          )}
        </>
      }
    >
      <div className="warnstrip selectable">
        <Msg msg={prompt.reasonText} />
      </div>
      <dl className="kv">
        <dt>{t('trust.subject')}</dt>
        <dd>{leaf.subject}</dd>
        <dt>{t('trust.issuer')}</dt>
        <dd>
          {leaf.issuer}
          {leaf.selfSigned ? ` (${t('trust.selfSigned')})` : ''}
        </dd>
        <dt>{t('trust.san')}</dt>
        <dd>{leaf.san.join(', ')}</dd>
        <dt>{t('trust.valid')}</dt>
        <dd>
          {leaf.notBefore} → {leaf.notAfter}
        </dd>
        {prompt.previousFingerprint && (
          <>
            <dt>{t('trust.oldFingerprint')}</dt>
            <dd>{prompt.previousFingerprint}</dd>
          </>
        )}
        <dt>{prompt.previousFingerprint ? t('trust.newFingerprint') : t('trust.fingerprint')}</dt>
        <dd>{leaf.fingerprint}</dd>
        <dt>{t('trust.key')}</dt>
        <dd>{leaf.keyType}</dd>
        {prompt.caOffer && prompt.chain.length > 1 && (
          <>
            <dt>{t('trust.ca')}</dt>
            <dd>{prompt.chain[prompt.chain.length - 1].subject}</dd>
          </>
        )}
      </dl>
    </Modal>
  );
}

export function TrustDialog() {
  const prompt = useTrust((s) => s.prompt);
  const profiles = useApp((s) => s.profiles);
  if (!prompt) return null;
  const name = profiles.find((p) => p.id === prompt.profileId)?.name ?? '';
  return (
    <TrustDialogView
      prompt={prompt}
      profileName={name}
      onCancel={() => useTrust.getState().set(null, null)}
      onTrust={(m) => void trustAndRetry(m)}
    />
  );
}

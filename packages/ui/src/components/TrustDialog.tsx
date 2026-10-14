import { useState, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import type { TrustMode, TrustPrompt } from '@mqttpanel/engine-client';
import { useTrust, trustAndRetry } from '../lib/actions';
import { useApp } from '../store/app';
import { Modal, Msg } from './ui';

/**
 * Mockup 03 (change set 6): a broker certificate that cannot be verified. The reason, the certificate, the risk
 * sentence, then one list of choices for this connection only (pin the certificate, preselected; skip the check this
 * time; skip it always; trust the CA, only with caOffer) and one Connect button that applies the choice and retries.
 */
export function TrustDialogView({ prompt, profileName, onCancel, onTrust }: {
  prompt: TrustPrompt;
  profileName: string;
  onCancel(): void;
  onTrust(mode: TrustMode): void;
}) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<TrustMode>('certificate');
  const leaf = prompt.leaf;
  const choices: { mode: TrustMode; title: string; text: React.ReactNode }[] = [
    {
      mode: 'certificate',
      title: t('trust.trustCert'),
      text: <Msg msg={{ key: 'tls.trustScopeCert', params: { profile: profileName }, text: `This certificate only, for the connection "${profileName}".` }} />,
    },
    { mode: 'skipOnce', title: t('trust.skipOnce'), text: t('trust.skipOnceText') },
    { mode: 'skipAlways', title: t('trust.skipAlways'), text: t('trust.skipAlwaysText') },
  ];
  if (prompt.caOffer) {
    choices.push({ mode: 'ca', title: t('trust.trustCa'), text: <Msg msg={{ key: 'tls.trustScopeCa', params: {}, text: 'Future certificates from this CA too.' }} /> });
  }
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const i = choices.findIndex((c) => c.mode === mode);
    const next = choices[(i + (e.key === 'ArrowDown' ? 1 : choices.length - 1)) % choices.length];
    setMode(next.mode);
    (e.currentTarget.querySelector(`[data-mode="${next.mode}"]`) as HTMLElement | null)?.focus();
  };
  return (
    <Modal
      title={t('trust.title')}
      onClose={onCancel}
      testId="trust-dialog"
      footer={
        <>
          <button type="button" className="btn" onClick={onCancel}>
            {t('common.cancel')}
          </button>
          <span className="grow" />
          {prompt.unsafe && <span className="chip brick">{t('trust.unsafe')}</span>}
          <button type="button" className={prompt.unsafe ? 'btn danger solid' : 'btn pri'} data-testid="trust-connect" data-autofocus onClick={() => onTrust(mode)}>
            {t('conn.connect')}
          </button>
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
      <p className="risk" data-testid="trust-risk">
        {t('trust.risk')}
      </p>
      <div className="choice" role="radiogroup" aria-label={t('trust.choices')} onKeyDown={onKey} data-testid="trust-choices">
        {choices.map((c) => (
          <div
            key={c.mode}
            className={`card${mode === c.mode ? ' on' : ''}`}
            role="radio"
            aria-checked={mode === c.mode}
            tabIndex={mode === c.mode ? 0 : -1}
            data-testid={`trust-${c.mode}`}
            data-mode={c.mode}
            onClick={() => setMode(c.mode)}
          >
            <div className="radio">
              <i className={mode === c.mode ? 'on' : ''} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div className="t">
                  <span>{c.title}</span>
                </div>
                <div className="s">{c.text}</div>
              </div>
            </div>
          </div>
        ))}
      </div>
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
      key={prompt.profileId + prompt.leaf.fingerprint}
      prompt={prompt}
      profileName={name}
      onCancel={() => useTrust.getState().set(null, null)}
      onTrust={(m) => void trustAndRetry(m)}
    />
  );
}

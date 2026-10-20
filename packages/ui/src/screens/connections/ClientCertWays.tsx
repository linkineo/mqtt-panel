import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { Profile } from '@mqttpanel/engine-client';
import { engine } from '../../services';
import { reportSave, run } from '../../lib/actions';
import { ConfirmDialog, Info, Seg, NO_AUTO } from '../../components/ui';
import { FileField } from './FileField';

type Way = 'import' | 'generate';

/** "3F:9A:12:7C…D1:7E": a fingerprint short enough for one row; the whole value is in the tooltip. */
export function shortFingerprint(fp: string): string {
  const parts = fp.split(':');
  return parts.length > 8 ? `${parts.slice(0, 4).join(':')}…${parts.slice(-2).join(':')}` : fp;
}

/**
 * The client certificate in the Security fold (change set 11), under the CA certificate row: a segmented control with
 * two exclusive ways. "Import certificate and key" keeps the rows known so far; "Generate key and request a
 * certificate" shows one card with three steps: generate the key (kept in the secret store), download the CSR, import
 * the signed certificate. Switching while the other side holds material asks first; with nothing there it switches
 * silently.
 */
export function ClientCertWays({ p, flush, onProfile }: { p: Profile; flush(): Promise<Profile>; onProfile(p: Profile): void }) {
  const { t } = useTranslation();
  const way: Way = p.tls.clientKeyGenerated ? 'generate' : 'import';
  const [ask, setAsk] = useState<Way | null>(null);
  const [startOver, setStartOver] = useState(false);
  const hasMaterial = !!(p.tls.clientCert || p.tls.clientKey);

  const switchTo = async (next: Way) => {
    const saved = await flush();
    const np = await run(engine().setClientCertWay(saved.id, next === 'generate'));
    if (np) onProfile(np);
  };

  return (
    <>
      <div className="f">
        <span className="lbl" />
        <span className="row2">
          <Seg<Way>
            label={t('conn.certWay')}
            value={way}
            testId="cert-way"
            onChange={(next) => {
              if (next === way) return;
              if (hasMaterial) setAsk(next);
              else void switchTo(next);
            }}
            options={[
              { value: 'import', label: t('conn.certWay.import'), testId: 'cert-way-import' },
              { value: 'generate', label: t('conn.certWay.generate'), testId: 'cert-way-generate' },
            ]}
          />
          <Info id="certWay" title={t('conn.certWay')} />
        </span>
      </div>
      {way === 'import' ? (
        <>
          <div className="f">
            <span className="lbl">
              {t('conn.clientCert')} <Info id="clientCert" title={t('conn.clientCert')} />
            </span>
            <FileField field="clientCert" profile={p} label={t('conn.clientCert')} beforeChange={flush} onProfile={onProfile} />
          </div>
          <div className="f">
            <span className="lbl">
              {t('conn.clientKey')} <Info id="clientKey" title={t('conn.clientKey')} keyStore />
            </span>
            <FileField field="clientKey" profile={p} label={t('conn.clientKey')} beforeChange={flush} onProfile={onProfile} />
          </div>
        </>
      ) : (
        <GenerateCard p={p} flush={flush} onProfile={onProfile} onStartOver={() => setStartOver(true)} />
      )}
      {ask && (
        <ConfirmDialog
          title={way === 'import' ? t('conn.discardImported') : t('conn.discardGenerated')}
          confirmLabel={t('conn.discard')}
          danger
          testId="cert-way-confirm"
          onClose={() => setAsk(null)}
          onConfirm={() => void switchTo(ask)}
        />
      )}
      {startOver && (
        <ConfirmDialog
          title={t('gen.startOverTitle')}
          confirmLabel={t('conn.discard')}
          danger
          testId="start-over-confirm"
          onClose={() => setStartOver(false)}
          onConfirm={() => void switchTo('generate')}
        >
          <p className="small">{t('gen.startOverBody')}</p>
        </ConfirmDialog>
      )}
    </>
  );
}

function GenerateCard({ p, flush, onProfile, onStartOver }: { p: Profile; flush(): Promise<Profile>; onProfile(p: Profile): void; onStartOver(): void }) {
  const { t } = useTranslation();
  const [alg, setAlg] = useState<'ecdsa' | 'rsa'>('ecdsa');
  const [busy, setBusy] = useState(false);
  const [cn, setCn] = useState(p.clientId);
  // the common name follows the client ID until the user edits it
  const [cnEdited, setCnEdited] = useState(false);
  useEffect(() => {
    if (!cnEdited) setCn(p.clientId);
  }, [p.clientId, cnEdited]);
  const key = p.tls.clientKey?.summary?.kind === 'generated' ? p.tls.clientKey.summary : null;

  const generate = async () => {
    setBusy(true);
    try {
      const saved = await flush();
      const np = await run(engine().profileGenerateClientKey(saved.id, alg));
      if (np) onProfile(np);
    } finally {
      setBusy(false);
    }
  };
  const downloadCsr = async () => {
    const out = await run(engine().profileClientCsr(p.id, cn.trim()));
    if (out) reportSave(out, t);
  };

  return (
    <div className="gencard" data-testid="gen-card">
      <div className="f">
        <span className="lbl">
          {t('conn.clientKey')} <Info id="genKey" title={t('conn.clientKey')} keyStore />
        </span>
        <span className="row2">
          {key ? (
            <span className="small selectable" data-testid="gen-key-line" title={key.fingerprint ?? undefined}>
              {t('gen.generated', { alg: key.keyType ?? '', fp: shortFingerprint(key.fingerprint ?? '') })}
            </span>
          ) : (
            <>
              <Seg<'ecdsa' | 'rsa'>
                label={t('gen.keyType')}
                value={alg}
                testId="gen-alg"
                onChange={setAlg}
                options={[
                  { value: 'ecdsa', label: 'ECDSA P-256' },
                  { value: 'rsa', label: 'RSA 2048' },
                ]}
              />
              <button type="button" className="btn sm pri" disabled={busy} data-testid="gen-key" onClick={() => void generate()}>
                {t('gen.generate')}
              </button>
            </>
          )}
        </span>
      </div>
      <div className="f">
        <span className="lbl">
          {t('gen.request')} <Info id="genCsr" title={t('gen.request')} />
        </span>
        <span className="row2">
          <span className="muted">{t('gen.commonName')}</span>
          <input
            {...NO_AUTO}
            className="input mono w-m"
            aria-label={t('gen.commonName')}
            data-testid="gen-cn"
            value={cn}
            disabled={!key}
            onChange={(e) => {
              setCnEdited(true);
              setCn(e.target.value);
            }}
          />
          <button type="button" className="btn sm" disabled={!key || !cn.trim()} data-testid="gen-csr" onClick={() => void downloadCsr()}>
            {t('gen.downloadCsr')}
          </button>
        </span>
      </div>
      <div className="f">
        <span className="lbl">
          {t('conn.clientCert')} <Info id="genCert" title={t('conn.clientCert')} />
        </span>
        <FileField
          field="clientCert"
          kind="signedCert"
          testId="signedCert"
          profile={p}
          label={t('conn.clientCert')}
          browseLabel={t('gen.importSigned')}
          disabled={!key}
          beforeChange={flush}
          onProfile={onProfile}
          applyPath={(id, path) => engine().profileImportSignedCert(id, path)}
        />
      </div>
      {key && (
        <div className="f">
          <span className="lbl" />
          <span className="row2">
            <button type="button" className="btn sm danger" data-testid="gen-start-over" onClick={onStartOver}>
              {t('gen.startOver')}
            </button>
          </span>
        </div>
      )}
    </div>
  );
}

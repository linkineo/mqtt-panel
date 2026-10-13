import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { BrokerCerts, BrokerConfig, IssueCertRequest, Listener, ListenerKind, Preset, UserMessage } from '@mqttpanel/engine-client';
import { engine } from '../../services';
import { toMessage } from '../../i18n';
import { useApp } from '../../store/app';
import { reportSave, run } from '../../lib/actions';
import { Check, Info, Modal, Msg, Seg } from '../../components/ui';
import { SecretInput } from '../../components/Secret';
import { isoDateToMonth } from '../../lib/format';

export const DEFAULT_PORTS: Record<ListenerKind, number> = { tcp: 1883, tls: 8883, ws: 8083, wss: 8084 };
export const PRESETS: Preset[] = ['plain', 'encrypted', 'passwords', 'mtls'];

export function kindLabel(k: ListenerKind, t: (k: string) => string): string {
  return t(`kind.${k}`);
}

/**
 * Validation of a listener port: below 1024 gives the short message with the kind's default port as the
 * suggestion (step 2), everything else comes from the engine (range, duplicate, in use).
 */
export async function checkPort(port: number, kind: ListenerKind, listenerId: string | null, short: boolean): Promise<UserMessage | null> {
  if (short && Number.isInteger(port) && port >= 1 && port < 1024) {
    return { key: 'broker.portPrivilegedShort', params: { suggestion: DEFAULT_PORTS[kind] }, text: `Needs admin rights below 1024. Try ${DEFAULT_PORTS[kind]}.` };
  }
  return engine().validatePort(port, listenerId);
}

function untilDate(years: number): string {
  const d = new Date();
  d.setFullYear(d.getFullYear() + years);
  return d.toISOString().slice(0, 10);
}

/* ---------------------------------------------------------------- Add listener (mockups 13 and 14) */

/**
 * Add listener, steps 1 and 2. `context`: "broker" (Broker screen: the app then asks to restart) or "flow"
 * ("Connect to this app's broker": the flow restarts and connects by itself). `onAdded` receives the new listener;
 * without it the dialog closes itself.
 */
export function AddListenerDialog({
  config,
  certs,
  onClose,
  onConfig,
  onAdded,
  context = 'broker',
}: {
  config: BrokerConfig;
  certs: BrokerCerts | null;
  onClose(): void;
  onConfig(c: BrokerConfig): void;
  onAdded?(l: Listener): void;
  context?: 'broker' | 'flow';
}) {
  const { t } = useTranslation();
  const running = useApp((s) => s.broker?.state === 'running');
  const [step, setStep] = useState<1 | 2>(1);
  const [preset, setPreset] = useState<Preset>('mtls');
  const [kind, setKind] = useState<ListenerKind>('tls');
  const used = new Set(config.listeners.map((l) => l.port));
  const freePort = (k: ListenerKind) => {
    let p = DEFAULT_PORTS[k];
    while (used.has(p)) p++;
    return p;
  };
  const [port, setPort] = useState(String(freePort('tls')));
  const [bind, setBind] = useState<'network' | 'localhost'>('network');
  const [portError, setPortError] = useState<UserMessage | null>(null);
  const [busy, setBusy] = useState(false);
  const secure = kind === 'tls' || kind === 'wss';

  useEffect(() => {
    let live = true;
    const n = Number(port);
    void checkPort(Number.isFinite(n) && port !== '' ? n : NaN, kind, null, true).then((m) => live && setPortError(m));
    return () => {
      live = false;
    };
  }, [port, kind]);

  const choosePreset = (p: Preset) => {
    setPreset(p);
    const k: ListenerKind = p === 'plain' ? 'tcp' : 'tls';
    setKind(k);
    setPort(String(freePort(k)));
  };

  const add = async () => {
    setBusy(true);
    try {
      const before = new Set(config.listeners.map((l) => l.id));
      const c = await engine().addListener({ preset, kind, port: Number(port), bind });
      onConfig(c);
      const added = c.listeners.find((l) => !before.has(l.id));
      if (onAdded && added) onAdded(added);
      else onClose();
    } catch (e) {
      setPortError(toMessage(e));
    } finally {
      setBusy(false);
    }
  };

  if (step === 1) {
    return (
      <Modal
        title={
          <>
            {t('addl.title')} <span className="muted">· {t('addl.step1')}</span>
          </>
        }
        onClose={onClose}
        testId="add-listener-1"
        footer={
          <>
            <button type="button" className="btn" onClick={onClose}>
              {t('common.cancel')}
            </button>
            <span className="grow" />
            <button type="button" className="btn pri" data-testid="addl-next" onClick={() => setStep(2)}>
              {t('common.next')}
            </button>
          </>
        }
      >
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, minmax(0, 1fr))', gap: 8 }} role="radiogroup" aria-label={t('addl.setup')}>
          {PRESETS.map((p) => (
            <div key={p} className={`card${preset === p ? ' on' : ''}`} role="radio" aria-checked={preset === p} tabIndex={preset === p ? 0 : -1} data-testid={`preset-${p}`}
              onClick={() => choosePreset(p)}
              onKeyDown={(e) => {
                if (e.key === 'ArrowRight' || e.key === 'ArrowDown') choosePreset(PRESETS[(PRESETS.indexOf(p) + 1) % 4]);
                if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') choosePreset(PRESETS[(PRESETS.indexOf(p) + 3) % 4]);
              }}
            >
              <div className="radio">
                <i className={preset === p ? 'on' : ''} />
                <div style={{ minWidth: 0 }}>
                  <div className="t" style={{ justifyContent: 'flex-start' }}>
                    <span>{t(`preset.${p}`)}</span> <Info id={`preset.${p}`} title={t(`preset.${p}`)} tone={p === 'plain' ? 'clay' : undefined} />
                  </div>
                  <div className="small muted" style={{ marginTop: 2 }}>
                    {t(`preset.${p}.short`)}
                  </div>
                </div>
              </div>
            </div>
          ))}
        </div>
        <div className="callout" style={{ padding: '8px 12px' }} data-testid="preset-explain">
          <p className="small">
            <strong>{t(`preset.${preset}.whatTitle`)}</strong> {t(`preset.${preset}.what`)}
          </p>
        </div>
      </Modal>
    );
  }

  const certOk = certs?.server;
  return (
    <Modal
      title={
        <>
          {t('addl.title')} <span className="muted">· {t('addl.step2', { preset: t(`preset.${preset}`) })}</span>
        </>
      }
      onClose={onClose}
      testId="add-listener-2"
      footer={
        <>
          <button type="button" className="btn" onClick={() => setStep(1)}>
            {t('common.back')}
          </button>
          <span className="grow" />
          <button type="button" className="btn" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button type="button" className="btn pri" data-testid="addl-add" disabled={busy || !!portError} onClick={() => void add()}>
            {t('addl.add')}
          </button>
        </>
      }
    >
      <div className="form">
        <div className="f">
          <span className="lbl">{t('addl.kind')}</span>
          <Seg<ListenerKind>
            label={t('addl.kind')}
            value={kind}
            onChange={(k) => {
              setKind(k);
              if (Number(port) === DEFAULT_PORTS[kind] || !port) setPort(String(freePort(k)));
            }}
            options={(['tcp', 'tls', 'ws', 'wss'] as ListenerKind[]).map((k) => ({
              value: k,
              label: kindLabel(k, t),
              disabled: preset === 'plain' ? k === 'tls' || k === 'wss' : k === 'tcp' || k === 'ws',
            }))}
          />
        </div>
        <div className="f">
          <label htmlFor="addl-port">{t('addl.port')}</label>
          <span className="row2">
            <input id="addl-port" className={`input mono${portError ? ' err' : ''}`} style={{ width: 90 }} inputMode="numeric" value={port} data-testid="addl-port" onChange={(e) => setPort(e.target.value.replace(/\D/g, ''))} />
            {portError && (
              <span className="hint err" data-testid="addl-port-error">
                <Msg msg={portError} />
              </span>
            )}
          </span>
        </div>
        <div className="f">
          <span className="lbl">
            {t('broker.reachable')} <Info id="reachable" title={t('broker.reachable')} />
          </span>
          <span className="row2">
            <Seg
              label={t('broker.reachable')}
              value={bind}
              onChange={setBind}
              options={[
                { value: 'network', label: t('broker.network') },
                { value: 'localhost', label: t('broker.localhost') },
              ]}
            />
            <span className="hint">{bind === 'network' ? t('addl.networkHint') : t('addl.localhostHint')}</span>
          </span>
        </div>
        {secure && (
          // information only: the requirement is broker-wide (change set 2); Mutual TLS sets it to Required
          <div className="f" data-testid="addl-client-cert">
            <span className="lbl">{t('addl.clientCertificate')}</span>
            <span className="row2">
              <span className={`chip${preset === 'mtls' || config.requireClientCert ? ' on' : ''}`}>{preset === 'mtls' || config.requireClientCert ? t('broker.required') : t('broker.optional')}</span>
              <span className="hint">{t('addl.wideHint')}</span>
            </span>
          </div>
        )}
        {secure && (
          <div className="f">
            <span className="lbl">{t('addl.serverCert')}</span>
            <span className="row2">
              {certOk && <span className="chip on">{t('addl.serverChip', { name: certs!.server.subject.replace(/^CN=/, '').split(',')[0], date: isoDateToMonth(certs!.server.notAfter) })}</span>}
              <button type="button" className="btn sm" onClick={() => void run(engine().importBrokerCert())}>
                {t('addl.useAnother')}
              </button>
            </span>
          </div>
        )}
      </div>
      <div className="warnstrip" data-testid="addl-after">
        {context === 'flow' ? t('addl.afterFlow') : running ? t('addl.afterPrompt') : t('addl.afterStopped')} {t(`addl.after.${preset}`)}
      </div>
    </Modal>
  );
}

/* ---------------------------------------------------------------- Issue client certificate (mockup 15) */

function randomPassphrase(): string {
  const a = 'abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789';
  const buf = new Uint32Array(20);
  crypto.getRandomValues(buf);
  return Array.from(buf, (x) => a[x % a.length]).join('');
}

export function IssueCertDialog({ onClose, onIssued }: { onClose(): void; onIssued(): void }) {
  const { t } = useTranslation();
  const [req, setReq] = useState<IssueCertRequest>({ name: '', validYears: 2, keyType: 'ecdsa', format: 'pem', passphrase: '', saveCa: true });
  const [error, setError] = useState<UserMessage | null>(null);
  const [busy, setBusy] = useState(false);
  const upd = (p: Partial<IssueCertRequest>) => setReq({ ...req, ...p });
  return (
    <Modal
      title={t('issue.title')}
      width={560}
      onClose={onClose}
      testId="issue-cert"
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className="btn pri"
            disabled={busy || !req.name.trim()}
            data-testid="issue-ok"
            onClick={async () => {
              setBusy(true);
              try {
                const out = await engine().issueClientCert({ ...req, name: req.name.trim() });
                reportSave(out, t);
                if (out.kind !== 'cancelled') {
                  onIssued();
                  onClose();
                }
              } catch (e) {
                setError(toMessage(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            {t('issue.ok')}
          </button>
        </>
      }
    >
      <div className="form">
        <div className="f">
          <span className="lbl">
            {t('issue.name')} <Info id="certName" title={t('issue.name')} />
          </span>
          <span className="row2">
            <input className={`input mono${error ? ' err' : ''}`} style={{ flex: 1 }} value={req.name} placeholder="sensor-r206" aria-label={t('issue.name')} data-autofocus onChange={(e) => upd({ name: e.target.value })} />
            <span className="hint">{t('issue.nameHint')}</span>
          </span>
        </div>
        {error && (
          <div className="hint err">
            <Msg msg={error} />
          </div>
        )}
        <div className="f">
          <span className="lbl">{t('issue.validFor')}</span>
          <span className="row2">
            <Seg<number>
              label={t('issue.validFor')}
              value={req.validYears}
              onChange={(v) => upd({ validYears: v as 1 | 2 | 5 })}
              options={[
                { value: 1, label: t('issue.years', { count: 1 }) },
                { value: 2, label: t('issue.years', { count: 2 }) },
                { value: 5, label: t('issue.years', { count: 5 }) },
              ]}
            />
            <span className="hint">{t('issue.until', { date: untilDate(req.validYears) })}</span>
          </span>
        </div>
        <div className="f">
          <span className="lbl">{t('issue.key')}</span>
          <span className="row2">
            <Seg
              label={t('issue.key')}
              value={req.keyType}
              onChange={(keyType) => upd({ keyType })}
              options={[
                { value: 'ecdsa', label: 'ECDSA P-256' },
                { value: 'rsa', label: 'RSA 2048' },
              ]}
            />
            <span className="hint">{t('issue.rsaHint')}</span>
          </span>
        </div>
        <div className="f">
          <span className="lbl">{t('issue.exportAs')}</span>
          <Seg
            label={t('issue.exportAs')}
            value={req.format}
            onChange={(format) => upd({ format })}
            options={[
              { value: 'pem', label: t('issue.pem') },
              { value: 'pkcs12', label: t('issue.p12') },
            ]}
          />
        </div>
        <div className="f">
          <span className="lbl">
            {t('issue.passphrase')} <Info id="passphrase" title={t('issue.passphrase')} />
          </span>
          <span className="row2">
            <SecretInput value={req.passphrase} aria-label={t('issue.passphrase')} placeholder={req.format === 'pem' ? t('common.optional') : ''} testId="issue-passphrase" onValue={(passphrase) => upd({ passphrase })} />
            <button type="button" className="btn sm" onClick={() => upd({ passphrase: randomPassphrase() })}>
              {t('issue.generate')}
            </button>
            <span className="chip on">{t('issue.keychain')}</span>
          </span>
        </div>
        <div className="f">
          <span />
          <Check on={req.saveCa} label={t('issue.saveCa')} onChange={(saveCa) => upd({ saveCa })} />
        </div>
      </div>
      <div className="muted small">{t('issue.help')}</div>
    </Modal>
  );
}

/* ---------------------------------------------------------------- small dialogs */

export function CredentialsDialog(props: {
  title: string;
  withUser: boolean;
  initialUser?: string;
  confirmLabel: string;
  onConfirm(user: string, password: string): Promise<void>;
  onClose(): void;
  testId?: string;
}) {
  const { t } = useTranslation();
  const [user, setUser] = useState(props.initialUser ?? '');
  const [pass, setPass] = useState('');
  const [error, setError] = useState<UserMessage | null>(null);
  const ok = async () => {
    try {
      await props.onConfirm(user.trim(), pass);
      props.onClose();
    } catch (e) {
      setError(toMessage(e));
    }
  };
  return (
    <Modal
      title={props.title}
      width={440}
      onClose={props.onClose}
      testId={props.testId}
      footer={
        <>
          <button type="button" className="btn" onClick={props.onClose}>
            {t('common.cancel')}
          </button>
          <button type="button" className="btn pri" data-testid="cred-ok" disabled={props.withUser && !user.trim()} onClick={() => void ok()}>
            {props.confirmLabel}
          </button>
        </>
      }
    >
      <div className="form">
        {props.withUser && (
          <div className="f" style={{ gridTemplateColumns: '110px 1fr' }}>
            <label htmlFor="cred-user">{t('addl.username')}</label>
            <input id="cred-user" className="input mono" value={user} data-autofocus onChange={(e) => setUser(e.target.value)} />
          </div>
        )}
        <div className="f" style={{ gridTemplateColumns: '110px 1fr' }}>
          <label htmlFor="cred-pass">{t('addl.password')}</label>
          <SecretInput id="cred-pass" value={pass} testId="cred-pass" onValue={setPass} onKeyDown={(e) => e.key === 'Enter' && void ok()} />
        </div>
      </div>
      {error && (
        <div className="hint err">
          <Msg msg={error} />
        </div>
      )}
      <div className="muted small">{t('cred.keychain')}</div>
    </Modal>
  );
}

export function RegenerateDialog({ certs, onClose, onCerts }: { certs: BrokerCerts; onClose(): void; onCerts(c: BrokerCerts): void }) {
  const { t } = useTranslation();
  const [sans, setSans] = useState(certs.server.san.join(', '));
  const [keyType, setKeyType] = useState<'ecdsa' | 'rsa'>(certs.server.keyType.startsWith('RSA') ? 'rsa' : 'ecdsa');
  return (
    <Modal
      title={t('certs.regenTitle')}
      width={520}
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className="btn pri"
            onClick={async () => {
              const c = await run(engine().regenerateBrokerCerts(sans.split(/[\s,]+/).filter(Boolean), keyType));
              if (c) {
                onCerts(c);
                useApp.getState().toast({ kind: 'info', text: t('certs.regenerated') });
              }
              onClose();
            }}
          >
            {t('certs.regenerate')}
          </button>
        </>
      }
    >
      <div className="form">
        <div className="f" style={{ gridTemplateColumns: '110px 1fr' }}>
          <label htmlFor="regen-sans">{t('certs.sans')}</label>
          <input id="regen-sans" className="input mono" value={sans} onChange={(e) => setSans(e.target.value)} />
        </div>
        <div className="f" style={{ gridTemplateColumns: '110px 1fr' }}>
          <span className="lbl">{t('issue.key')}</span>
          <Seg
            label={t('issue.key')}
            value={keyType}
            onChange={setKeyType}
            options={[
              { value: 'ecdsa', label: 'ECDSA P-256' },
              { value: 'rsa', label: 'RSA 2048' },
            ]}
          />
        </div>
      </div>
      <div className="muted small">{t('certs.regenHelp')}</div>
    </Modal>
  );
}

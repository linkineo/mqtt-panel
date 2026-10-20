import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { BrokerBind, BrokerCerts, BrokerClient, BrokerConfig, BrokerUser, CertDetails, IssuedCert, Listener, LogLine, UserMessage } from '@mqttpanel/engine-client';
import { engine, platform } from '../../services';
import { renderMessage, toMessage } from '../../i18n';
import { useApp } from '../../store/app';
import { reportSave, run } from '../../lib/actions';
import { ConfirmDialog, Info, Modal, Msg, Popover, Seg, Switch, NO_AUTO } from '../../components/ui';
import { fmtAge, fmtNum, fmtRate, fmtTime, fmtUptime, isoDateToMonth } from '../../lib/format';
import { AddListenerDialog, CredentialsDialog, IssueCertDialog, RegenerateDialog, checkPort, kindLabel } from './BrokerDialogs';
import { SecretInput, SecretText } from '../../components/Secret';
import { FoldHeader, useFold } from '../../components/Fold';

/** Every change applies at once while the broker runs (change set 7): the status is read again right away, so a
 * listener that could not start shows its error without waiting for the next tick, and the profiles too, since
 * "Local broker" follows the broker (C-16). */
export async function refreshBrokerStatus(): Promise<void> {
  try {
    useApp.setState({ broker: await engine().brokerStatus() });
    await useApp.getState().refreshProfiles();
  } catch {
    // the next tick brings it
  }
}

function PortCell({ l, disabled, onConfig }: { l: Listener; disabled: boolean; onConfig(c: BrokerConfig): void }) {
  const { t } = useTranslation();
  const error = useApp((s) => s.broker?.listeners.find((x) => x.id === l.id)?.error ?? null);
  const [edit, setEdit] = useState(false);
  const [v, setV] = useState(String(l.port));
  const [err, setErr] = useState<UserMessage | null>(null);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!edit) return;
    let live = true;
    void checkPort(Number(v) || NaN, l.kind, l.id, false).then((m) => live && setErr(Number(v) === l.port ? null : m));
    return () => {
      live = false;
    };
  }, [v, edit, l.id, l.kind, l.port]);
  const save = async () => {
    if (err) return;
    if (Number(v) !== l.port) {
      const c = await run(engine().updateListener({ ...l, port: Number(v) }));
      if (c) onConfig(c);
    }
    setEdit(false);
  };
  if (!edit) {
    return (
      <span className="row" style={{ gap: 6 }}>
        <button
          type="button"
          className="btn link mono"
          style={{ fontWeight: 400 }}
          disabled={disabled}
          title={t('broker.editPort')}
          data-testid={`port-${l.id}`}
          onClick={() => setEdit(true)}
        >
          {l.port}
        </button>
        {error && (
          // A listener that could not start (port taken, missing material): a short brick word, the reason in the
          // tooltip, so the row never wraps (change set 7).
          <span className="lerr" title={renderMessage(t, error)} data-testid={`listener-error-${l.id}`}>
            {t('broker.notStarted')}
          </span>
        )}
      </span>
    );
  }
  return (
    <>
      <input {...NO_AUTO}
        ref={ref}
        autoFocus
        className={`input mono${err ? ' err' : ''}`}
        style={{ width: 76, height: 24 }}
        inputMode="numeric"
        value={v}
        aria-label={t('broker.port')}
        data-testid={`port-input-${l.id}`}
        onChange={(e) => setV(e.target.value.replace(/\D/g, ''))}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            // no implicit click on whatever takes the focus once the field closes
            e.preventDefault();
            void save();
          }
          if (e.key === 'Escape') {
            setV(String(l.port));
            setEdit(false);
          }
        }}
        onBlur={() => {
          if (!err) void save();
        }}
      />
      {err && (
        <Popover anchor={ref.current} onClose={() => { setV(String(l.port)); setEdit(false); }} label={t('broker.port')}>
          <span className="hint err" data-testid="port-error">
            <Msg msg={err} />
          </span>
        </Popover>
      )}
    </>
  );
}

/** A listener row (mockup 12, change set 4): On, Kind, Port, Setup and ✕, which removes the listener at once (no menu,
 * no confirmation; only its sessions drop, change set 7). Reachability is broker-wide, on the Listeners line. */
export function ListenerRow(props: { l: Listener; network: boolean; onConfig(c: BrokerConfig): void }) {
  const { t } = useTranslation();
  const { l, network } = props;
  const secure = l.kind === 'tls' || l.kind === 'wss';
  const params = { kind: kindLabel(l.kind, t), port: l.port };
  const upd = async (p: Partial<Listener>) => {
    const c = await run(engine().updateListener({ ...l, ...p }));
    if (c) props.onConfig(c);
  };
  const remove = async () => {
    const c = await run(engine().removeListener(l.id));
    if (c) props.onConfig(c);
  };
  return (
    <tr data-testid="listener-row" data-listener={l.id}>
      <td>
        <Switch on={l.enabled} label={t('broker.listenerOn', params)} onChange={(enabled) => void upd({ enabled })} />
      </td>
      <td>
        <span className="row" style={{ gap: 6 }}>
          {kindLabel(l.kind, t)}
          {!secure && network && <span className="chip clay" data-testid="unencrypted">{t('broker.unencrypted')}</span>}
        </span>
      </td>
      <td className="mono">
        <PortCell l={l} disabled={false} onConfig={props.onConfig} />
      </td>
      <td>{t(`preset.${l.preset}`)}</td>
      <td style={{ textAlign: 'right' }}>
        <button
          type="button"
          className="x danger"
          aria-label={t('broker.removeNamed', params)}
          title={t('broker.removeNamed', params)}
          data-testid={`remove-listener-${l.id}`}
          onClick={() => void remove()}
        >
          ✕
        </button>
      </td>
    </tr>
  );
}

const LEVELS: LogLine['level'][] = ['debug', 'info', 'warn', 'error'];

/**
 * The live broker log (B4). Collapsed by default to a title strip at the bottom of the right pane ("▸ Log", plus
 * "· n warnings" only when n > 0, change set 4); a click expands it to the full log with the level filter (mockup 12c).
 * Remembered in the settings.
 */
function LogView() {
  const { t } = useTranslation();
  const seq = useApp((s) => s.brokerLogSeq);
  const open = useApp((s) => s.settings.brokerLogOpen);
  const [level, setLevel] = useState<LogLine['level']>('info');
  const [lines, setLines] = useState<LogLine[]>([]);
  const [warnings, setWarnings] = useState(0);
  const after = useRef(0);
  const warnAfter = useRef(0);
  useEffect(() => {
    after.current = 0;
    setLines([]);
  }, [level, open]);
  useEffect(() => {
    let live = true;
    if (open) {
      void engine()
        .brokerLog(level, after.current)
        .then((l) => {
          if (!live || !l.length) return;
          after.current = l[l.length - 1].seq;
          setLines((x) => [...x, ...l].slice(-500));
        })
        .catch(() => undefined);
    }
    void engine()
      .brokerLog('warn', warnAfter.current)
      .then((l) => {
        if (!live || !l.length) return;
        warnAfter.current = l[l.length - 1].seq;
        setWarnings((n) => n + l.length);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [seq, level, open]);
  const toggle = () => void useApp.getState().saveSettings({ brokerLogOpen: !open });
  const title = (
    <button type="button" className="btn link logtitle" aria-expanded={open} data-testid="log-toggle" onClick={toggle}>
      <span className="muted" aria-hidden="true">
        {open ? '▾' : '▸'}
      </span>
      <strong>{t('broker.log')}</strong>
      {!open && warnings > 0 && (
        <span className="muted" data-testid="log-warnings">
          · {t('broker.logWarnings', { count: warnings, n: fmtNum(warnings) })}
        </span>
      )}
    </button>
  );
  if (!open) {
    return (
      <>
        <span className="grow" style={{ flex: 1 }} />
        <div className="ph logstrip" style={{ borderTop: '1px solid var(--line)' }} data-testid="broker-log-strip">
          {title}
        </div>
      </>
    );
  }
  return (
    <>
      <div className="ph" style={{ borderTop: '1px solid var(--line)' }}>
        {title}
        <Seg
          label={t('broker.logLevel')}
          value={level}
          onChange={setLevel}
          options={[
            { value: 'debug', label: t('log.debug') },
            { value: 'info', label: t('log.info') },
            { value: 'warn', label: t('log.warn') },
          ]}
        />
      </div>
      <div className="log selectable" style={{ padding: '8px 14px', overflow: 'auto', flex: 1, minHeight: 120 }} data-testid="broker-log">
        {[...lines].reverse().map((l) => (
          <div key={l.seq}>
            <b>{fmtTime(l.at)}</b> {LEVELS.indexOf(l.level) >= 2 && <span className={l.level === 'error' ? 'e' : 'w'}>{l.level === 'error' ? t('log.error') : t('log.warnWord')}</span>} {l.text}
          </div>
        ))}
      </div>
    </>
  );
}

export function BrokerScreen() {
  const { t } = useTranslation();
  const status = useApp((s) => s.broker);
  const config = useApp((s) => s.brokerConfig);
  const storeConfig = useApp((s) => s.setBrokerConfig);
  /** A new configuration after a change: shown at once, with the status and the profiles it may have changed. */
  const setConfig = (c: BrokerConfig) => {
    storeConfig(c);
    void refreshBrokerStatus();
  };
  const advancedOpen = useFold('brokerAdvanced');
  const [certs, setCertsState] = useState<BrokerCerts | null>(null);
  /** New broker certificates after a change (applied live): the profile "Local broker" may follow them. */
  const setCerts = (c: BrokerCerts, changed = true) => {
    setCertsState(c);
    if (changed) void refreshBrokerStatus();
  };
  const [issued, setIssued] = useState<IssuedCert[]>([]);
  const [clients, setClients] = useState<BrokerClient[]>([]);
  const [dialog, setDialog] = useState<
    | null
    | { kind: 'add' }
    | { kind: 'issue' }
    | { kind: 'addUser' }
    | { kind: 'userPass'; user: string }
    | { kind: 'anonOn' }
    | { kind: 'revoke'; name: string }
    | { kind: 'p12'; name: string }
    | { kind: 'regen' }
    | { kind: 'rotate' }
    | { kind: 'csr' }
  >(null);
  const now = Date.now();

  const loadAll = async () => {
    try {
      const [c, cfg, iss, cl] = await Promise.all([engine().brokerCerts(), engine().brokerConfig(), engine().issuedCerts(), engine().brokerClients()]);
      setCerts(c, false);
      storeConfig(cfg);
      setIssued(iss);
      setClients(cl);
    } catch (e) {
      useApp.getState().toast({ kind: 'err', msg: toMessage(e) });
    }
  };
  useEffect(() => {
    void loadAll();
    // The assistant changed the broker (certificates, users, listeners): read everything again.
    return useApp.getState().onAssistant((e) => {
      if (e.kind === 'changed' && e.what === 'broker') void loadAll();
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // clients update live (no Refresh button): on every change of the count or state, and once a second
  useEffect(() => {
    let live = true;
    const load = () =>
      void engine()
        .brokerClients()
        .then((c) => live && setClients(c))
        .catch(() => undefined);
    load();
    const timer = setInterval(load, 1000);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [status?.clients, status?.state]);

  if (!config || !status) return <div className="pane" />;
  const anonymous = config.anonymous;
  const running = status.state === 'running';
  const network = config.bind === 'network';

  const lifecycle = async (fn: () => Promise<typeof status>) => {
    const s = await run(fn());
    if (s) useApp.setState({ broker: s });
  };

  const userMenu = async (u: string, e: React.MouseEvent) => {
    e.preventDefault();
    const id = await platform().showContextMenu(
      [
        { id: 'pass', label: t('users.changePassword') },
        { id: 'remove', label: t('users.remove'), danger: true },
      ],
      e.clientX,
      e.clientY,
    );
    if (id === 'pass') setDialog({ kind: 'userPass', user: u });
    else if (id === 'remove') {
      const c = await run(engine().removeUser(u));
      if (c) setConfig(c);
    }
  };

  const exportCert = async (name: string, format: 'pem' | 'pkcs12', pass = '') => {
    const out = await run(engine().exportClientCert(name, format, pass));
    if (out) reportSave(out, t);
  };

  /** Reachability is one choice for the whole broker (change set 4); it applies at once, open sessions stay. */
  const setBind = async (bind: BrokerBind) => {
    if (bind === config.bind) return;
    const c = await run(engine().setBrokerBind(bind));
    if (c) setConfig(c);
  };

  /** The ↓ button of a certificate row: a menu with PEM (exports at once) and PKCS#12… (asks for a passphrase). */
  const exportMenu = async (name: string, anchor: HTMLElement) => {
    const r = anchor.getBoundingClientRect();
    const id = await platform().showContextMenu(
      [
        { id: 'pem', label: t('certs.pem') },
        { id: 'p12', label: t('certs.p12Dots') },
      ],
      r.left,
      r.bottom + 4,
    );
    if (id === 'pem') void exportCert(name, 'pem');
    else if (id === 'p12') setDialog({ kind: 'p12', name });
  };

  const listenersSection = (
    <>
      <div className="sec lsec" style={{ marginTop: 18 }} data-testid="listeners-line">
        {t('broker.listeners')}
        <span className="grow" />
        <Seg<BrokerBind>
          label={t('broker.reachable')}
          value={config.bind}
          testId="reach-seg"
          onChange={(b) => void setBind(b)}
          options={[
            { value: 'localhost', label: t('broker.thisComputer'), testId: 'reach-localhost' },
            { value: 'network', label: t('broker.wholeNetwork'), testId: 'reach-network' },
          ]}
        />
        <Info id="reachable" title={t('broker.reachable')} />
        <span style={{ width: 4 }} aria-hidden="true" />
        <button type="button" className="btn pri sm" data-testid="add-listener" onClick={() => setDialog({ kind: 'add' })}>
          {t('broker.addListener')}
        </button>
      </div>
      <table className="tbl compact" style={{ marginTop: 6 }} data-testid="listeners">
        <thead>
          <tr>
            <th>{t('broker.on')}</th>
            <th>{t('broker.kind')}</th>
            <th>{t('broker.port')}</th>
            <th>{t('broker.setup')}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {config.listeners.map((l) => (
            <ListenerRow key={l.id} l={l} network={network} onConfig={setConfig} />
          ))}
        </tbody>
      </table>
    </>
  );

  const usersPanel = (
    <div className="pnl" data-testid="users">
      <div className="hd">
        {t('broker.users')}
        <span className="grow" />
        <span style={{ fontWeight: 500, color: anonymous ? 'var(--clay)' : 'var(--ink-2)' }}>{t('broker.anonymous')}</span>
        <Switch
          on={anonymous}
          tone={anonymous ? 'clay' : undefined}
          label={t('broker.anonymous')}
          testId="anonymous"
          onChange={async (on) => {
            if (on) setDialog({ kind: 'anonOn' });
            else {
              const c = await run(engine().setAnonymous(false));
              if (c) setConfig(c);
            }
          }}
        />
        <Info id="anonymous" title={t('broker.anonymous')} />
        <span style={{ width: 6 }} aria-hidden="true" />
        <button type="button" className="btn sm" disabled={anonymous} data-testid="add-user" onClick={() => setDialog({ kind: 'addUser' })}>
          {t('users.add')}
        </button>
      </div>
      <UsersTable
        users={config.users}
        anonymous={anonymous}
        onChange={(u) => setDialog({ kind: 'userPass', user: u })}
        onRemove={async (u) => {
          const c = await run(engine().removeUser(u));
          if (c) setConfig(c);
        }}
        onMenu={(u, e) => void userMenu(u, e)}
      />
    </div>
  );

  const certsPanel = (
    <div className={`pnl${anonymous ? ' gated' : ''}`} data-testid="client-certs" aria-disabled={anonymous}>
      <div className="hd">
        {t('broker.clientCerts')} <Info id="clientCerts" title={t('broker.clientCerts')} />
        <span className="grow" />
        <button type="button" className="btn sm" disabled={anonymous} data-testid="import-csr" onClick={() => setDialog({ kind: 'csr' })}>
          {t('broker.importCsr')}
        </button>
        <button type="button" className="btn pri sm" disabled={anonymous} data-testid="issue-cert" onClick={() => setDialog({ kind: 'issue' })}>
          {t('broker.issue')}
        </button>
      </div>
      <table className="tbl compact">
        <thead>
          <tr>
            <th>{t('certs.name')}</th>
            <th>{t('certs.expires')}</th>
            <th>{t('certs.lastSeen')}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {issued.map((c) => (
            <tr key={c.name} data-testid="issued-row" data-cert={c.name}>
              <td className="mono">{c.name}</td>
              <td className="mono">{c.expiresAt}</td>
              <td>
                {c.superseded && !c.revoked && (
                  <span className="chip clay" title={t('certs.supersededHint')} style={{ marginRight: 4 }}>
                    {t('certs.reissue')}
                  </span>
                )}
                {c.revoked ? (
                  <span className="chip brick">{t('certs.revoked')}</span>
                ) : c.lastSeen === 'connected' ? (
                  <span className={`chip${anonymous ? '' : ' on'}`}>{t('certs.connected')}</span>
                ) : c.lastSeen === 'never' ? (
                  <span className="chip">{t('certs.never')}</span>
                ) : (
                  <span className="chip">{t('certs.ago', { age: fmtAge(now - c.lastSeen) })}</span>
                )}
              </td>
              <td>
                {!c.revoked && (
                  <span className="row" style={{ gap: 4, justifyContent: 'flex-end' }}>
                    <button
                      type="button"
                      className="btn sm icon"
                      disabled={anonymous}
                      aria-label={t('certs.exportDots')}
                      title={t('certs.exportDots')}
                      aria-haspopup="menu"
                      data-testid="cert-export"
                      onClick={(e) => void exportMenu(c.name, e.currentTarget)}
                    >
                      ↓
                    </button>
                    {c.system ? (
                      <span className="chip" data-testid="cert-local-tag">
                        {t('conn.localBrokerName')}
                      </span>
                    ) : (
                      <button
                        type="button"
                        className="x danger"
                        disabled={anonymous}
                        aria-label={t('certs.revokeNamed', { name: c.name })}
                        title={t('certs.revokeNamed', { name: c.name })}
                        data-testid={`revoke-${c.name}`}
                        onClick={() => setDialog({ kind: 'revoke', name: c.name })}
                      >
                        ✕
                      </button>
                    )}
                  </span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {issued.length === 0 && <div className="empty small">{t('certs.none')}</div>}
    </div>
  );

  const ca = certs?.ca;
  const server = certs?.server;
  const cn = (subject: string) => subject.replace(/^CN=/, '').split(',')[0];
  const advancedSummary = anonymous
    ? t('broker.clientCertsOff')
    : [
        t(config.requireClientCert ? 'broker.sum.required' : 'broker.sum.optional'),
        t(config.verifyAgainst === 'local' ? 'broker.sum.localCa' : 'broker.sum.importedCa'),
        ca ? t('broker.sum.ca', { name: cn(ca.subject), date: isoDateToMonth(ca.notAfter) }) : null,
        server ? t('broker.sum.server', { name: cn(server.subject), date: isoDateToMonth(server.notAfter) }) : null,
      ]
        .filter(Boolean)
        .join(' · ');
  const certLine = (c: CertDetails) => t('certs.line', { name: cn(c.subject), date: isoDateToMonth(c.notAfter), key: c.keyType });
  const certTitle = (c: CertDetails) => `${c.subject}\n${t('trust.issuer')}: ${c.issuer}\n${t('trust.san')}: ${c.san.join(', ') || '—'}\n${t('trust.fingerprint')}: ${c.fingerprint}`;

  /** "Advanced security" (change set 4): client-certificate requirement, Verify against and the broker certificates. */
  const advancedSection = (
    <>
      <div style={{ marginTop: 14 }}>
        <FoldHeader id="brokerAdvanced" title={t('broker.advancedSecurity')} summary={advancedSummary} disabled={anonymous} />
      </div>
      {advancedOpen && (
        <div className="form wide-label" style={{ marginTop: 4 }} data-testid="advanced-security">
          <div className={`f${anonymous ? ' gated' : ''}`} data-testid="require-cert">
            <span className="lbl">
              {t('broker.clientCerts')} <Info id="onEncrypted" title={t('broker.clientCerts')} />
            </span>
            <span className="row2">
              <Seg
                label={t('broker.clientCerts')}
                value={config.requireClientCert ? 'required' : 'optional'}
                disabled={anonymous}
                testId="require-cert-seg"
                onChange={async (v) => {
                  const c = await run(engine().setRequireClientCert(v === 'required'));
                  if (c) setConfig(c);
                }}
                options={[
                  { value: 'optional', label: t('broker.optionalCap') },
                  { value: 'required', label: t('broker.requiredCap') },
                ]}
              />
              <span className="muted small">{t('broker.onEncryptedMuted')}</span>
            </span>
          </div>
          <div className={`f${anonymous ? ' gated' : ''}`} data-testid="verify-against">
            <span className="lbl">
              {t('broker.verifyAgainst')} <Info id="verifyAgainst" title={t('broker.verifyAgainst')} />
            </span>
            <span className="row2">
              <Seg
                label={t('broker.verifyAgainst')}
                value={config.verifyAgainst}
                disabled={anonymous}
                onChange={async (v) => {
                  const c = await run(engine().setVerifyAgainst(v));
                  if (c) setConfig(c);
                }}
                options={[
                  { value: 'local', label: t('broker.localCa') },
                  { value: 'imported', label: t('broker.importedCa'), disabled: !certs?.importedCa },
                ]}
              />
              <button
                type="button"
                className="btn sm"
                disabled={anonymous}
                title={certs?.importedCa ? certTitle(certs.importedCa) : undefined}
                data-testid="import-client-ca"
                onClick={async () => {
                  const c = await run(engine().importClientCa());
                  if (c) setCerts(c);
                }}
              >
                {t('certs.importCa')}
              </button>
            </span>
          </div>
          {ca && (
            <div className="f" data-testid="local-ca">
              <span className="lbl">{t('certs.ca')}</span>
              <span className="row2">
                <span className="mono small ell" title={certTitle(ca)}>
                  {certLine(ca)}
                </span>
                <button
                  type="button"
                  className="btn sm"
                  data-testid="export-ca"
                  onClick={async () => {
                    const out = await run(engine().exportCa());
                    if (out) reportSave(out, t);
                  }}
                >
                  {t('certs.exportCa')}
                </button>
                <button type="button" className="btn sm" data-testid="rotate-ca" onClick={() => setDialog({ kind: 'rotate' })}>
                  {t('certs.rotateBtn')}
                </button>
              </span>
            </div>
          )}
          {server && (
            <div className="f" data-testid="server-cert">
              <span className="lbl">{t('certs.server')}</span>
              <span className="row2">
                <span className="mono small ell" title={certTitle(server)}>
                  {certLine(server)}
                </span>
                {certs?.imported && <span className="chip">{t('certs.importedChip')}</span>}
                <button
                  type="button"
                  className="btn sm"
                  data-testid="import-server-cert"
                  onClick={async () => {
                    const c = await run(engine().importBrokerCert());
                    if (c) setCerts(c);
                  }}
                >
                  {t('certs.import')}
                </button>
                <button type="button" className="btn sm" data-testid="regenerate" onClick={() => setDialog({ kind: 'regen' })}>
                  {t('certs.regenerateDots')}
                </button>
                {/* change set 11: the served chain as PEM, for a device that pins the server certificate */}
                <button
                  type="button"
                  className="btn sm"
                  data-testid="export-server-cert"
                  title={t('certs.exportServer')}
                  onClick={async () => {
                    const out = await run(engine().exportServerCert());
                    if (out) reportSave(out, t);
                  }}
                >
                  {t('certs.exportDots')}
                </button>
              </span>
            </div>
          )}
        </div>
      )}
    </>
  );

  const stoppable = running || status.state === 'starting';
  const start = () => void lifecycle(() => engine().brokerStart());
  const stop = () => void lifecycle(() => engine().brokerStop());

  return (
    <div className="cols" style={{ gridTemplateColumns: 'minmax(0,1fr) 350px' }}>
      <div className="pane" data-testid="broker">
        <div className="ph">
          <h5>{t('broker.title')}</h5>
          <div className="row" style={{ gap: 6 }}>
            {/* Start and Stop only: every change applies live, nothing restarts (change set 7). */}
            {/* change set 11: Start is the accent primary button like Connect; Stop stays outlined in brick */}
            {stoppable ? (
              <button type="button" className="btn danger out" data-testid="broker-stop" onClick={stop}>
                {t('broker.stop')}
              </button>
            ) : (
              <button type="button" className="btn pri" data-testid="broker-start" onClick={start}>
                {t('broker.start')}
              </button>
            )}
          </div>
        </div>
        <div className="pc">
          {anonymous && (
            <div className="warnstrip" style={{ marginBottom: 14 }} data-testid="anon-warning">
              <strong>{t('broker.anonWarnTitle')}</strong> {t('broker.anonWarn')}
            </div>
          )}
          {status.error && (
            <div className="callout err" style={{ marginBottom: 14 }}>
              <Msg msg={status.error} />
            </div>
          )}
          <div className="stats" data-testid="broker-tiles">
            <div className="stat">
              <div className="l">{t('broker.status')}</div>
              <div className="v">
                <span className={`dot${running ? '' : status.state === 'starting' ? ' warn' : status.state === 'failed' ? ' err' : ' off'}`} style={{ alignSelf: 'center' }} />
                {t(`bstate.${status.state}`)}
                {running && status.startedAt && (
                  <small data-testid="uptime" style={{ marginLeft: 4 }}>
                    {fmtUptime(now - status.startedAt)}
                  </small>
                )}
                {/* change set 11: the same two actions as the header, at the right end of the value line */}
                {stoppable ? (
                  <button type="button" className="tile-btn stop" data-testid="tile-stop" title={t('broker.tileStop')} aria-label={t('broker.tileStop')} onClick={stop}>
                    <svg viewBox="0 0 10 10" aria-hidden="true">
                      <rect x="1.5" y="1.5" width="7" height="7" rx="1" />
                    </svg>
                  </button>
                ) : (
                  <button type="button" className="tile-btn play" data-testid="tile-start" title={t('broker.tileStart')} aria-label={t('broker.tileStart')} onClick={start}>
                    <svg viewBox="0 0 10 10" aria-hidden="true">
                      <path d="M2.5 1.2 L9 5 L2.5 8.8 Z" />
                    </svg>
                  </button>
                )}
              </div>
            </div>
            <div className="stat in">
              <div className="l">↓ {t('broker.msgIn')}</div>
              <div className="v">
                {fmtRate(status.msgInRate)}
                <small>/s</small>
              </div>
            </div>
            <div className="stat out">
              <div className="l">↑ {t('broker.msgOut')}</div>
              <div className="v">
                {fmtRate(status.msgOutRate)}
                <small>/s</small>
              </div>
            </div>
            <div className="stat">
              <div className="l">{t('broker.retained')}</div>
              <div className="v">{fmtNum(status.retained)}</div>
            </div>
          </div>
          {listenersSection}
          {usersPanel}
          {certsPanel}
          {advancedSection}
        </div>
      </div>
      <div className="pane right" data-testid="broker-side">
        <div className="ph">
          <h5 data-testid="clients-title">{t('broker.clients')}</h5>
        </div>
        <div style={{ maxHeight: '45%', overflow: 'auto', flex: 'none' }}>
          <table className="tbl compact fixed" data-testid="clients">
            <colgroup>
              <col />
              <col style={{ width: 96 }} />
              <col style={{ width: 78 }} />
              <col style={{ width: 44 }} />
              <col style={{ width: 40 }} />
            </colgroup>
            <thead>
              <tr>
                <th>{t('clients.id')}</th>
                <th>{t('clients.from')}</th>
                <th>{t('clients.proto')}</th>
                <th>{t('clients.subs')}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {clients.map((c) => (
                <tr key={c.clientId}>
                  <td className="mono" title={c.certCn ? `CN=${c.certCn}` : (c.username ?? '')}>
                    {c.clientId}
                  </td>
                  <td className="mono" title={c.address}>{c.address === 'localhost' ? t('clients.localhost') : c.address}</td>
                  <td>
                    {c.protocol} · {c.transport}
                  </td>
                  <td>{c.subscriptions}</td>
                  <td>
                    <button
                      type="button"
                      className="btn sm icon"
                      aria-label={t('clients.disconnect', { id: c.clientId })}
                      title={t('clients.disconnect', { id: c.clientId })}
                      onClick={async () => {
                        await run(engine().disconnectClient(c.clientId));
                        setClients((await run(engine().brokerClients())) ?? []);
                      }}
                    >
                      ⏏
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {clients.length === 0 && <div className="empty small">{running ? t('clients.none') : t('clients.stopped')}</div>}
        </div>
        <LogView />
      </div>

      {dialog?.kind === 'add' && (
        <AddListenerDialog
          config={config}
          onClose={() => setDialog(null)}
          onConfig={setConfig}
          onAdded={() => setDialog(null)}
        />
      )}
      {dialog?.kind === 'issue' && <IssueCertDialog onClose={() => setDialog(null)} onIssued={async () => setIssued((await run(engine().issuedCerts())) ?? issued)} />}
      {dialog?.kind === 'addUser' && (
        <CredentialsDialog
          title={t('users.addTitle')}
          withUser
          confirmLabel={t('common.add')}
          onClose={() => setDialog(null)}
          onConfirm={async (u, p) => {
            const c = await engine().addUser(u, p);
            setConfig(c);
          }}
          testId="add-user"
        />
      )}
      {dialog?.kind === 'userPass' && (
        <CredentialsDialog
          title={t('users.passTitle', { user: dialog.user })}
          withUser={false}
          confirmLabel={t('common.save')}
          onClose={() => setDialog(null)}
          onConfirm={async (_u, p) => {
            const c = await engine().setUserPassword(dialog.user, p);
            setConfig(c);
          }}
        />
      )}
      {dialog?.kind === 'anonOn' && (
        <ConfirmDialog
          title={t('broker.anonConfirmTitle')}
          confirmLabel={t('broker.anonConfirm')}
          danger
          testId="anon-confirm"
          onClose={() => setDialog(null)}
          onConfirm={async () => {
            const c = await run(engine().setAnonymous(true));
            if (c) setConfig(c);
          }}
        >
          <div className="small">{t('broker.anonConfirmBody')}</div>
          <div className="muted small">{t('broker.anonConfirmKept')}</div>
        </ConfirmDialog>
      )}
      {dialog?.kind === 'revoke' && (
        <ConfirmDialog
          title={t('certs.revokeTitle', { name: dialog.name })}
          confirmLabel={t('certs.revoke')}
          danger
          onClose={() => setDialog(null)}
          onConfirm={async () => {
            const l = await run(engine().revokeClientCert(dialog.name));
            if (l) setIssued(l);
          }}
        >
          <div className="small">{t('certs.revokeBody')}</div>
        </ConfirmDialog>
      )}
      {dialog?.kind === 'rotate' && (
        <ConfirmDialog
          title={t('certs.rotateTitle')}
          confirmLabel={t('certs.rotate')}
          danger
          onClose={() => setDialog(null)}
          onConfirm={async () => {
            const c = await run(engine().rotateCa());
            if (c) {
              setCerts(c);
              setIssued((await run(engine().issuedCerts())) ?? issued);
            }
          }}
        >
          <div className="small">{t('certs.rotateBody')}</div>
        </ConfirmDialog>
      )}
      {dialog?.kind === 'regen' && certs && (
        <RegenerateDialog
          certs={certs}
          onClose={() => setDialog(null)}
          onCerts={(c) => setCerts(c)}
        />
      )}
      {dialog?.kind === 'p12' && <P12Dialog name={dialog.name} onClose={() => setDialog(null)} onExport={(p) => void exportCert(dialog.name, 'pkcs12', p)} />}
      {dialog?.kind === 'csr' && (
        <CsrDialog
          onClose={() => setDialog(null)}
          onDone={async () => setIssued((await run(engine().issuedCerts())) ?? issued)}
        />
      )}
    </div>
  );
}

function P12Dialog({ name, onClose, onExport }: { name: string; onClose(): void; onExport(p: string): void }) {
  const { t } = useTranslation();
  const [p, setP] = useState('');
  return (
    <Modal
      title={t('certs.p12Title', { name })}
      width={460}
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className="btn pri"
            disabled={!p}
            onClick={() => {
              onExport(p);
              onClose();
            }}
          >
            {t('certs.saveAs')}
          </button>
        </>
      }
    >
      <div className="form">
        <div className="f" style={{ gridTemplateColumns: '110px 1fr' }}>
          <span className="lbl">
            {t('issue.passphrase')} <Info id="passphrase" title={t('issue.passphrase')} />
          </span>
          <SecretInput value={p} aria-label={t('issue.passphrase')} data-autofocus testId="p12-passphrase" onValue={setP} />
        </div>
      </div>
    </Modal>
  );
}

function CsrDialog({ onClose, onDone }: { onClose(): void; onDone(): void }) {
  const { t } = useTranslation();
  const [name, setName] = useState('');
  const [years, setYears] = useState(2);
  return (
    <Modal
      title={t('csr.title')}
      width={480}
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
              const out = await run(engine().importCsr(name.trim() || null, years));
              if (out) {
                reportSave(out, t);
                onDone();
              }
              onClose();
            }}
          >
            {t('csr.choose')}
          </button>
        </>
      }
    >
      <div className="muted small">{t('csr.help')}</div>
      <div className="form">
        <div className="f" style={{ gridTemplateColumns: '110px 1fr' }}>
          <label htmlFor="csr-name">{t('issue.name')}</label>
          <input {...NO_AUTO} id="csr-name" className="input mono w-l" value={name} placeholder={t('csr.fromCsr')} onChange={(e) => setName(e.target.value)} />
        </div>
        <div className="f" style={{ gridTemplateColumns: '110px 1fr' }}>
          <span className="lbl">{t('issue.validFor')}</span>
          <Seg<number>
            label={t('issue.validFor')}
            value={years}
            onChange={setYears}
            options={[1, 2, 5].map((y) => ({ value: y, label: t('issue.years', { count: y }) }))}
          />
        </div>
      </div>
    </Modal>
  );
}

/** Users table (mockup 12, change set 4): username (with a muted "Local broker" tag for mqtt-panel), password with eye
 * and copy, ✎ change and ✕ remove. One set of users for every listener. */
function UsersTable(props: { users: BrokerUser[]; anonymous: boolean; onChange(u: string): void; onRemove(u: string): void; onMenu(u: string, e: React.MouseEvent): void }) {
  const { t } = useTranslation();
  const { anonymous } = props;
  return (
    <div className={anonymous ? 'gated' : undefined} aria-disabled={anonymous}>
      <table className="tbl compact" data-testid="users-table">
        <thead>
          <tr>
            <th>{t('users.username')}</th>
            <th>{t('users.password')}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {props.users.map((u) => (
            <tr key={u.username} data-testid="user-row" data-user={u.username} onContextMenu={anonymous ? undefined : (e) => props.onMenu(u.username, e)}>
              <td className="mono">
                <span className="row" style={{ gap: 6 }}>
                  {u.username}
                  {u.system && (
                    <span className="chip" data-testid="user-local-tag">
                      {t('conn.localBrokerName')}
                    </span>
                  )}
                </span>
              </td>
              <td>
                {u.hasPassword ? (
                  <SecretText load={() => engine().brokerUserPassword(u.username)} disabled={anonymous} testId={`user-pw-${u.username}`} />
                ) : (
                  <span className="muted small" title={t('users.notKeptHint')}>
                    {t('users.notKept')}
                  </span>
                )}
              </td>
              <td style={{ textAlign: 'right' }}>
                <button
                  type="button"
                  className="x"
                  disabled={anonymous}
                  aria-label={t('users.passTitle', { user: u.username })}
                  title={t('users.passTitle', { user: u.username })}
                  data-testid={`user-change-${u.username}`}
                  onClick={() => props.onChange(u.username)}
                >
                  ✎
                </button>
                <button
                  type="button"
                  className="x danger"
                  disabled={anonymous}
                  aria-label={t('users.removeNamed', { user: u.username })}
                  title={t('users.removeNamed', { user: u.username })}
                  data-testid={`user-remove-${u.username}`}
                  onClick={() => props.onRemove(u.username)}
                >
                  ✕
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {props.users.length === 0 && <div className="empty small">{t('users.none')}</div>}
    </div>
  );
}

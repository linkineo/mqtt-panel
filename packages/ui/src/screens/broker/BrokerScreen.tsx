import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { BrokerCerts, BrokerClient, BrokerConfig, BrokerUser, CertDetails, IssuedCert, Listener, LogLine, UserMessage } from '@mqttpanel/engine-client';
import { engine, platform } from '../../services';
import { toMessage } from '../../i18n';
import { useApp } from '../../store/app';
import { reportSave, run } from '../../lib/actions';
import { ConfirmDialog, Info, Modal, Msg, Popover, Seg, Switch } from '../../components/ui';
import { fmtAge, fmtNum, fmtRate, fmtTime, fmtUptime, isoDateToMonth } from '../../lib/format';
import { AddListenerDialog, CredentialsDialog, IssueCertDialog, RegenerateDialog, checkPort, kindLabel } from './BrokerDialogs';
import { SecretInput, SecretText } from '../../components/Secret';
import { noteRestartChange, restartBroker } from './restart';

function PortCell({ l, disabled, onConfig }: { l: Listener; disabled: boolean; onConfig(c: BrokerConfig): void }) {
  const { t } = useTranslation();
  const pending = useApp((s) => s.broker?.restartNeeded === true && s.pendingChanges.some((c) => c.field === 'port' && c.listenerId === l.id));
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
      if (c) {
        onConfig(c);
        void noteRestartChange({ key: 'rs.ch.port', params: { kind: kindLabel(l.kind, t), from: l.port, to: Number(v) }, listenerId: l.id, field: 'port' });
      }
    }
    setEdit(false);
  };
  if (!edit) {
    return (
      <span className="row" style={{ gap: 6 }}>
        <button
          type="button"
          className="btn link mono"
          style={{ fontWeight: 400, color: pending ? 'var(--clay)' : undefined }}
          disabled={disabled}
          title={t('broker.editPort')}
          data-testid={`port-${l.id}`}
          onClick={() => setEdit(true)}
        >
          {l.port}
        </button>
        {pending && (
          <span className="chip clay" data-testid="port-pending">
            {t('broker.pendingChip')}
          </span>
        )}
      </span>
    );
  }
  return (
    <>
      <input
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
            // no implicit click: the restart prompt may open and take the focus before the key is released
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

function CertKv({ c }: { c: CertDetails }) {
  const { t } = useTranslation();
  return (
    <dl className="kv" style={{ gridTemplateColumns: '150px 1fr' }}>
      <dt>{t('trust.subject')}</dt>
      <dd>{c.subject}</dd>
      <dt>{t('trust.issuer')}</dt>
      <dd>{c.issuer}</dd>
      <dt>{t('trust.san')}</dt>
      <dd>{c.san.join(', ') || '—'}</dd>
      <dt>{t('trust.valid')}</dt>
      <dd>
        {c.notBefore} → {c.notAfter}
      </dd>
      <dt>{t('trust.fingerprint')}</dt>
      <dd>{c.fingerprint}</dd>
      <dt>{t('trust.key')}</dt>
      <dd>{c.keyType}</dd>
    </dl>
  );
}

export function ListenerRow(props: { l: Listener; anonymous: boolean; requireCert: boolean; onConfig(c: BrokerConfig): void; onMenu(l: Listener, e: React.MouseEvent): void }) {
  const { t } = useTranslation();
  const { l, anonymous, requireCert } = props;
  const secure = l.kind === 'tls' || l.kind === 'wss';
  const upd = async (p: Partial<Listener>) => {
    const c = await run(engine().updateListener({ ...l, ...p }));
    if (!c) return;
    props.onConfig(c);
    const params = { kind: kindLabel(l.kind, t), port: l.port };
    if (p.enabled !== undefined) void noteRestartChange({ key: p.enabled ? 'rs.ch.on' : 'rs.ch.off', params, listenerId: l.id });
    else if (p.bind) void noteRestartChange({ key: 'rs.ch.bind', params: { ...params, where: t(p.bind === 'network' ? 'broker.network' : 'broker.localhost') }, listenerId: l.id });
  };
  return (
    <tr data-testid="listener-row" data-listener={l.id} onContextMenu={(e) => { e.preventDefault(); props.onMenu(l, e); }}>
      <td>
        <Switch on={l.enabled} label={t('broker.listenerOn', { kind: kindLabel(l.kind, t), port: l.port })} onChange={(enabled) => void upd({ enabled })} />
      </td>
      <td>
        <span className="row" style={{ gap: 6 }}>
          {kindLabel(l.kind, t)}
          {!secure && l.bind === 'network' && <span className="chip clay" data-testid="unencrypted">{t('broker.unencrypted')}</span>}
        </span>
      </td>
      <td>
        <span className="row" style={{ gap: 6 }}>
          <Seg
            label={t('broker.reachable')}
            value={l.bind}
            onChange={(bind) => void upd({ bind })}
            options={[
              { value: 'network', label: t('broker.network') },
              { value: 'localhost', label: t('broker.localhost') },
            ]}
          />
        </span>
      </td>
      <td className="mono">
        <PortCell l={l} disabled={false} onConfig={props.onConfig} />
      </td>
      <td>{t(`preset.${l.preset}`)}</td>
      <td data-testid="listener-cert" title={secure ? t('broker.wideSetting') : undefined}>
        {/* read-only: the requirement is one broker-wide setting, under Client certificates (change set 2) */}
        {!secure ? (
          <span className="muted">—</span>
        ) : requireCert ? (
          anonymous ? (
            <span className="chip clay">{t('broker.notChecked')}</span>
          ) : (
            <span className="chip on">{t('broker.required')}</span>
          )
        ) : (
          <span className="chip">{t('broker.optional')}</span>
        )}
      </td>
      <td>
        <button type="button" className="btn sm icon" aria-label={t('broker.listenerMenu')} title={t('broker.listenerMenu')} onClick={(e) => props.onMenu(l, e)}>
          ⋯
        </button>
      </td>
    </tr>
  );
}

const LEVELS: LogLine['level'][] = ['debug', 'info', 'warn', 'error'];

/**
 * The live broker log (B4). Collapsed by default to a title strip at the bottom of the right pane ("▸ Log · live ·
 * n warnings"); a click expands it to the full log with the level filter (mockup 12c). Remembered in the settings.
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
      <span className="muted">· {t('broker.logLive')}</span>
      {!open && warnings > 0 && <span className="muted">· {t('broker.logWarnings', { count: warnings, n: fmtNum(warnings) })}</span>}
    </button>
  );
  if (!open) {
    return (
      <>
        <span className="grow" style={{ flex: 1 }} />
        <div className="ph logstrip" style={{ borderTop: '1px solid var(--line)' }} data-testid="broker-log-strip">
          {title}
          <span className="muted small ell" style={{ minWidth: 0 }}>
            {t('broker.logExpand')}
          </span>
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
  const setConfig = useApp((s) => s.setBrokerConfig);
  const restartPrompt = useApp((s) => s.restartPrompt);
  const [certs, setCerts] = useState<BrokerCerts | null>(null);
  const [issued, setIssued] = useState<IssuedCert[]>([]);
  const [clients, setClients] = useState<BrokerClient[]>([]);
  const [certsOpen, setCertsOpen] = useState(false);
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
    | { kind: 'removeListener'; l: Listener }
    | { kind: 'csr' }
  >(null);
  const now = Date.now();

  const loadAll = async () => {
    try {
      const [c, cfg, iss, cl] = await Promise.all([engine().brokerCerts(), engine().brokerConfig(), engine().issuedCerts(), engine().brokerClients()]);
      setCerts(c);
      setConfig(cfg);
      setIssued(iss);
      setClients(cl);
    } catch (e) {
      useApp.getState().toast({ kind: 'err', msg: toMessage(e) });
    }
  };
  useEffect(() => {
    void loadAll();
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
  const anyNetwork = config.listeners.some((l) => l.enabled && l.bind === 'network');

  const lifecycle = async (fn: () => Promise<typeof status>) => {
    const s = await run(fn());
    if (s) useApp.setState({ broker: s });
  };

  const listenerMenu = async (l: Listener, e: React.MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const id = await platform().showContextMenu([{ id: 'remove', label: t('broker.removeListener'), danger: true }], r.right - 220, r.bottom + 4);
    if (id === 'remove') setDialog({ kind: 'removeListener', l });
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

  const listenersSection = (
    <>
      <div className="sec" style={{ marginTop: 18 }}>
        {t('broker.listeners')}
        {status.restartNeeded && <span className="chip clay" data-testid="restart-chip">{t('broker.restartPending')}</span>}
        <span className="grow" />
        <button type="button" className="btn pri sm" data-testid="add-listener" onClick={() => setDialog({ kind: 'add' })}>
          {t('broker.addListener')}
        </button>
      </div>
      <table className="tbl compact" style={{ marginTop: 6 }} data-testid="listeners">
        <thead>
          <tr>
            <th>{t('broker.on')}</th>
            <th>{t('broker.kind')}</th>
            <th>{t('broker.reachable')}</th>
            <th>{t('broker.port')}</th>
            <th>{t('broker.setup')}</th>
            <th>{t('broker.clientCert')}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {config.listeners.map((l) => (
            <ListenerRow key={l.id} l={l} anonymous={anonymous} requireCert={config.requireClientCert} onConfig={setConfig} onMenu={(x, e) => void listenerMenu(x, e)} />
          ))}
        </tbody>
      </table>
      {status.listeners
        .filter((x) => x.error)
        .map((x) => (
          <div key={x.id} className="hint err" style={{ marginTop: 4 }}>
            <Msg msg={x.error} />
          </div>
        ))}
      {anyNetwork && !anonymous && (
        <div className="callout warn" style={{ marginTop: 8, padding: '8px 12px' }} data-testid="network-warning">
          <p className="small">
            <strong>{t('broker.networkWarnTitle')}</strong> {t('broker.networkWarn')}
          </p>
        </div>
      )}
    </>
  );

  const clientCertsSection = (
    <div className={anonymous ? 'gated' : undefined} data-testid="client-certs" aria-disabled={anonymous}>
      <div className="sec" style={{ marginTop: 18 }}>
        {t('broker.clientCerts')} <span className="lower">· {anonymous ? t('broker.clientCertsOff') : t('broker.clientCertsSub')}</span>
      </div>
      <div className="form wide-label" style={{ marginTop: 6 }} data-testid="client-cert-policy">
        <div className="f" data-testid="require-cert">
          <span className="lbl">
            {t('broker.onEncrypted')} <Info id="onEncrypted" title={t('broker.onEncrypted')} />
          </span>
          <span className="row2">
            <Seg
              label={t('broker.onEncrypted')}
              value={config.requireClientCert ? 'required' : 'optional'}
              disabled={anonymous}
              testId="require-cert-seg"
              onChange={async (v) => {
                const c = await run(engine().setRequireClientCert(v === 'required'));
                if (c) {
                  setConfig(c);
                  void noteRestartChange({ key: v === 'required' ? 'rs.ch.certRequired' : 'rs.ch.certOptional', params: {} });
                }
              }}
              options={[
                { value: 'optional', label: t('broker.optionalCap') },
                { value: 'required', label: t('broker.requiredCap') },
              ]}
            />
          </span>
        </div>
        {config.requireClientCert && !anonymous && (
          // own line under the choice, so the row itself never wraps in French and German
          <div className="f">
            <span />
            <span className="hint" data-testid="require-cert-hint">
              {t('broker.requiredHint')}
            </span>
          </div>
        )}
        <div className="f" data-testid="verify-against">
          <span className="lbl">
            {t('broker.verifyAgainst')} <Info id="verifyAgainst" title={t('broker.verifyAgainst')} />
          </span>
          <Seg
            label={t('broker.verifyAgainst')}
            value={config.verifyAgainst}
            disabled={anonymous}
            onChange={async (v) => {
              const c = await run(engine().setVerifyAgainst(v));
              if (c) {
                setConfig(c);
                void noteRestartChange({ key: 'rs.ch.verify', params: {} });
              }
            }}
            options={[
              { value: 'local', label: t('broker.localCa') },
              { value: 'imported', label: t('broker.importedCa'), disabled: !certs?.importedCa },
            ]}
          />
        </div>
      </div>
      <div className="row" style={{ marginTop: 6 }}>
        <button type="button" className="btn pri" disabled={anonymous} data-testid="issue-cert" onClick={() => setDialog({ kind: 'issue' })}>
          {t('broker.issueCert')}
        </button>
        <button type="button" className="btn sm" disabled={anonymous} onClick={() => setDialog({ kind: 'csr' })}>
          {t('broker.importCsr')}
        </button>
      </div>
      {!anonymous && <p className="muted small" style={{ margin: '8px 0 0' }}>{t('broker.certsHelp')}</p>}
      <table className="tbl" style={{ marginTop: 8 }}>
        <thead>
          <tr>
            <th>{t('certs.name')}</th>
            <th>{t('certs.issued')}</th>
            <th>{t('certs.expires')}</th>
            <th>{t('certs.lastSeen')}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {issued.map((c) => (
            <tr key={c.name} data-testid="issued-row">
              <td className="mono">{c.name}</td>
              <td className="mono">{c.issuedAt}</td>
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
                {c.system ? (
                  <span className="muted small">{t('certs.usedBy')}</span>
                ) : c.revoked ? null : (
                  <span className="row" style={{ gap: 4 }}>
                    <button type="button" className="btn sm" disabled={anonymous} onClick={() => void exportCert(c.name, 'pem')}>
                      PEM
                    </button>
                    <button type="button" className="btn sm" disabled={anonymous} onClick={() => setDialog({ kind: 'p12', name: c.name })}>
                      PKCS#12
                    </button>
                    <button type="button" className={`btn sm${anonymous ? '' : ' danger'}`} disabled={anonymous} onClick={() => setDialog({ kind: 'revoke', name: c.name })}>
                      {t('certs.revoke')}
                    </button>
                  </span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );

  const ca = certs?.ca;
  const server = certs?.server;
  const sameMonth = ca && server && isoDateToMonth(ca.notAfter) === isoDateToMonth(server.notAfter);
  const brokerCertsSection = certs && ca && server && (
    <div style={{ marginTop: 14 }}>
      <div className="disclosure" data-testid="broker-certs">
        <button type="button" className="btn link row" style={{ gap: 8, minWidth: 0, flex: 1 }} aria-expanded={certsOpen} onClick={() => setCertsOpen(!certsOpen)}>
          <span className="muted">{certsOpen ? '▾' : '▸'}</span>
          <strong style={{ fontSize: 12 }}>{t('certs.brokerCerts')}</strong>
          <span className="muted small" style={{ fontWeight: 400, whiteSpace: 'normal', textAlign: 'left' }} data-testid="broker-certs-summary">
            {sameMonth
              ? t('certs.summarySame', { ca: ca.subject.replace(/^CN=/, '').split(',')[0], server: server.subject.replace(/^CN=/, '').split(',')[0], date: isoDateToMonth(server.notAfter), key: server.keyType })
              : t('certs.summary', { ca: ca.subject.replace(/^CN=/, '').split(',')[0], server: server.subject.replace(/^CN=/, '').split(',')[0], date: isoDateToMonth(server.notAfter), key: server.keyType })}
          </span>
        </button>
        <button type="button" className="btn sm" data-testid="export-ca" onClick={async () => {
          const out = await run(engine().exportCa());
          if (out) reportSave(out, t);
        }}>
          {t('certs.exportCa')}
        </button>
      </div>
      {certsOpen && (
        <div style={{ display: 'grid', gap: 10, padding: '10px 4px' }}>
          <div className="sec" style={{ marginTop: 0 }}>{t('certs.ca')}</div>
          <CertKv c={ca} />
          <div className="sec">{t('certs.server')} {certs.imported && <span className="chip">{t('certs.importedChip')}</span>}</div>
          <CertKv c={server} />
          <div className="row">
            <button type="button" className="btn sm" onClick={async () => {
              const c = await run(engine().importBrokerCert());
              if (c) {
                setCerts(c);
                void noteRestartChange({ key: 'rs.ch.certs', params: {} });
              }
            }}>
              {t('certs.import')}
            </button>
            <button type="button" className="btn sm" onClick={() => setDialog({ kind: 'regen' })}>
              {t('certs.regenerateDots')}
            </button>
            <button type="button" className="btn sm danger" onClick={() => setDialog({ kind: 'rotate' })}>
              {t('certs.rotate')}
            </button>
          </div>
          <div className="sec">{t('certs.importedCa')}</div>
          {certs.importedCa ? <CertKv c={certs.importedCa} /> : <div className="hint">{t('certs.noImportedCa')}</div>}
          <div>
            <button type="button" className="btn sm" onClick={async () => {
              const c = await run(engine().importClientCa());
              if (c) {
                setCerts(c);
                if (config.verifyAgainst === 'imported') void noteRestartChange({ key: 'rs.ch.certs', params: {} });
              }
            }}>
              {t('certs.importCa')}
            </button>
          </div>
        </div>
      )}
    </div>
  );

  const authSection = (
    <>
      <div className="sec" style={{ marginTop: 18 }}>
        {t('broker.auth')} <span className="lower">· {t('broker.authSub')}</span>
      </div>
      <div className="form" style={{ marginTop: 6 }} data-testid="auth">
        <div className="f">
          <span className="lbl">
            {t('broker.anonymous')} <Info id="anonymous" title={t('broker.anonymous')} />
          </span>
          <span className="row2">
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
            {anonymous ? <span className="small hint warn">{t('broker.anonOn')}</span> : <span className="hint">{t('broker.anonOff')}</span>}
          </span>
        </div>
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
      <div style={{ marginTop: 6 }} className={anonymous ? 'gated' : undefined}>
        <button type="button" className="btn sm" disabled={anonymous} data-testid="add-user" onClick={() => setDialog({ kind: 'addUser' })}>
          {t('users.add')}
        </button>
      </div>
    </>
  );

  return (
    <div className="cols" style={{ gridTemplateColumns: 'minmax(0,1fr) 350px' }}>
      <div className="pane" data-testid="broker">
        <div className="ph">
          <h5>{t('broker.title')}</h5>
          <div className="row" style={{ gap: 6 }}>
            {status.restartNeeded && running ? (
              <button type="button" className="btn pulse" data-testid="broker-restart" onClick={() => void run(restartBroker())}>
                {t('broker.restartApply')}
              </button>
            ) : (
              <button type="button" className="btn" disabled={!running} data-testid="broker-restart" onClick={() => void lifecycle(() => engine().brokerRestart())}>
                {t('broker.restart')}
              </button>
            )}
            {running || status.state === 'starting' ? (
              <button type="button" className="btn danger" data-testid="broker-stop" onClick={() => void lifecycle(() => engine().brokerStop())}>
                {t('broker.stop')}
              </button>
            ) : (
              <button type="button" className="btn" data-testid="broker-start" onClick={() => void lifecycle(() => engine().brokerStart())}>
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
          <div className="stats">
            <div className="stat">
              <div className="l row" style={{ justifyContent: 'space-between', gap: 6 }}>
                <span>{t('broker.status')}</span>
                {running && status.startedAt && <span className="mono" data-testid="uptime">{fmtUptime(now - status.startedAt)}</span>}
              </div>
              <div className="v">
                <span className={`dot${running ? '' : status.state === 'starting' ? ' warn' : status.state === 'failed' ? ' err' : ' off'}`} style={{ alignSelf: 'center' }} />
                {t(`bstate.${status.state}`)}
              </div>
            </div>
            <div className="stat">
              <div className="l">{t('broker.clients')}</div>
              <div className="v">
                {fmtNum(status.clients)}
                <small>{t('broker.max', { n: fmtNum(status.maxClients) })}</small>
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
          {authSection}
          {clientCertsSection}
          {brokerCertsSection}
        </div>
      </div>
      <div className="pane right" data-testid="broker-side">
        <div className="ph">
          <h5>{t('broker.clientsN', { n: fmtNum(clients.length) })}</h5>
          <span className="muted small">{t('broker.logLive')}</span>
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
          certs={certs}
          onClose={() => setDialog(null)}
          onConfig={setConfig}
          onAdded={(l) => {
            setDialog(null);
            void noteRestartChange({ key: 'rs.ch.added', params: { kind: kindLabel(l.kind, t), port: l.port }, listenerId: l.id });
          }}
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
              void noteRestartChange({ key: 'rs.ch.certs', params: {} });
            }
          }}
        >
          <div className="small">{t('certs.rotateBody')}</div>
        </ConfirmDialog>
      )}
      {dialog?.kind === 'removeListener' && (
        <ConfirmDialog
          title={t('broker.removeTitle', { kind: kindLabel(dialog.l.kind, t), port: dialog.l.port })}
          confirmLabel={t('broker.removeListener')}
          danger
          onClose={() => setDialog(null)}
          onConfirm={async () => {
            const c = await run(engine().removeListener(dialog.l.id));
            if (c) {
              setConfig(c);
              void noteRestartChange({ key: 'rs.ch.removed', params: { kind: kindLabel(dialog.l.kind, t), port: dialog.l.port } });
            }
          }}
        />
      )}
      {dialog?.kind === 'regen' && certs && (
        <RegenerateDialog
          certs={certs}
          onClose={() => setDialog(null)}
          onCerts={(c) => {
            setCerts(c);
            void noteRestartChange({ key: 'rs.ch.certs', params: {} });
          }}
        />
      )}
      {restartPrompt && <RestartPrompt />}
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
          <input id="csr-name" className="input mono" value={name} placeholder={t('csr.fromCsr')} onChange={(e) => setName(e.target.value)} />
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

/** Users table (mockup 12): username, password with eye and copy, note, Change… and Remove. Shared by every listener. */
function UsersTable(props: { users: BrokerUser[]; anonymous: boolean; onChange(u: string): void; onRemove(u: string): void; onMenu(u: string, e: React.MouseEvent): void }) {
  const { t } = useTranslation();
  const { anonymous } = props;
  return (
    <div className={anonymous ? 'gated' : undefined} aria-disabled={anonymous} data-testid="users">
      <table className="tbl compact" style={{ marginTop: 8 }} data-testid="users-table">
        <thead>
          <tr>
            <th>{t('users.username')}</th>
            <th>{t('users.password')}</th>
            <th />
            <th />
          </tr>
        </thead>
        <tbody>
          {props.users.map((u) => (
            <tr key={u.username} data-testid="user-row" data-user={u.username} onContextMenu={anonymous ? undefined : (e) => props.onMenu(u.username, e)}>
              <td className="mono">{u.username}</td>
              <td>
                {u.hasPassword ? (
                  <SecretText load={() => engine().brokerUserPassword(u.username)} disabled={anonymous} testId={`user-pw-${u.username}`} />
                ) : (
                  <span className="muted small" title={t('users.notKeptHint')}>
                    {t('users.notKept')}
                  </span>
                )}
              </td>
              <td className="muted small">{u.system ? t('users.createdFor') : ''}</td>
              <td>
                <span className="row" style={{ gap: 4, justifyContent: 'flex-end' }}>
                  <button type="button" className="btn sm" disabled={anonymous} onClick={() => props.onChange(u.username)}>
                    {t('users.change')}
                  </button>
                  <button type="button" className={`btn sm${anonymous ? '' : ' danger'}`} disabled={anonymous} aria-label={t('users.removeNamed', { user: u.username })} onClick={() => props.onRemove(u.username)}>
                    {t('users.removeBtn')}
                  </button>
                </span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {props.users.length === 0 && <div className="empty small">{t('users.none')}</div>}
    </div>
  );
}

/** "Restart the broker to apply the change?" (mockup 12c). */
function RestartPrompt() {
  const { t } = useTranslation();
  const pending = useApp((s) => s.pendingChanges);
  const close = () => useApp.setState({ restartPrompt: false });
  const list = pending.map((c) => t(c.key, c.params)).join('; ');
  return (
    <Modal
      title={t('rs.title')}
      width={520}
      onClose={close}
      testId="restart-prompt"
      footer={
        <>
          <button type="button" className="btn" data-testid="restart-later" onClick={close}>
            {t('rs.later')}
          </button>
          <span className="grow" />
          <button type="button" className="btn pri" data-testid="restart-now" onClick={() => void run(restartBroker())}>
            {t('rs.now')}
          </button>
        </>
      }
    >
      <p style={{ margin: 0 }}>{t('rs.body')}</p>
      {list && (
        <div className="muted small" data-testid="restart-pending-list">
          {t('rs.pending', { list })}
        </div>
      )}
    </Modal>
  );
}

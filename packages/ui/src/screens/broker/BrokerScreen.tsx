import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { BrokerCerts, BrokerClient, BrokerConfig, CertDetails, IssuedCert, Listener, LogLine, UserMessage } from '@mqttpanel/engine-client';
import { engine, platform } from '../../services';
import { toMessage } from '../../i18n';
import { useApp } from '../../store/app';
import { handleConnectOutcome, reportSave, run } from '../../lib/actions';
import { ConfirmDialog, Info, Modal, Msg, Popover, Seg, Switch } from '../../components/ui';
import { fmtAge, fmtNum, fmtRate, fmtTime, fmtUptime, isoDateToMonth } from '../../lib/format';
import { AddListenerDialog, CredentialsDialog, IssueCertDialog, RegenerateDialog, checkPort, kindLabel } from './BrokerDialogs';

function PortCell({ l, disabled, onConfig }: { l: Listener; disabled: boolean; onConfig(c: BrokerConfig): void }) {
  const { t } = useTranslation();
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
      <button type="button" className="btn link mono" style={{ fontWeight: 400 }} disabled={disabled} title={t('broker.editPort')} data-testid={`port-${l.id}`} onClick={() => setEdit(true)}>
        {l.port}
      </button>
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
          if (e.key === 'Enter') void save();
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

export function ListenerRow(props: { l: Listener; anonymous: boolean; onConfig(c: BrokerConfig): void; onLogin(l: Listener): void; onMenu(l: Listener, e: React.MouseEvent): void }) {
  const { t } = useTranslation();
  const { l, anonymous } = props;
  const secure = l.kind === 'tls' || l.kind === 'wss';
  const upd = async (p: Partial<Listener>) => {
    const c = await run(engine().updateListener({ ...l, ...p }));
    if (c) props.onConfig(c);
  };
  return (
    <tr data-testid="listener-row" data-listener={l.id}>
      <td>
        <Switch on={l.enabled} label={t('broker.listenerOn', { kind: kindLabel(l.kind, t), port: l.port })} onChange={(enabled) => void upd({ enabled })} />
      </td>
      <td>{kindLabel(l.kind, t)}</td>
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
          {!secure && l.bind === 'network' && <span className="chip clay">{t('broker.unencrypted')}</span>}
        </span>
      </td>
      <td className="mono">
        <PortCell l={l} disabled={false} onConfig={props.onConfig} />
      </td>
      <td>{t(`preset.${l.preset}`)}</td>
      <td className={anonymous ? 'gated' : undefined} data-testid="listener-login">
        {anonymous ? (
          l.login ? (
            <span>
              <span className="mono">{l.login.username}</span> · ••••
            </span>
          ) : (
            '—'
          )
        ) : l.login ? (
          <span className="row" style={{ gap: 6 }}>
            <span>
              <span className="mono">{l.login.username}</span> · ••••
            </span>
            <button type="button" className="btn sm" onClick={() => props.onLogin(l)}>
              {t('broker.edit')}
            </button>
          </span>
        ) : (
          <button type="button" className="btn sm" onClick={() => props.onLogin(l)}>
            {t('broker.set')}
          </button>
        )}
      </td>
      <td data-testid="listener-cert">
        {!secure ? (
          <span className="muted">—</span>
        ) : l.requireClientCert ? (
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
        <button type="button" className="btn sm" aria-label={t('broker.listenerMenu')} onClick={(e) => props.onMenu(l, e)}>
          ⋯
        </button>
      </td>
    </tr>
  );
}

const LEVELS: LogLine['level'][] = ['debug', 'info', 'warn', 'error'];

function LogView() {
  const { t } = useTranslation();
  const seq = useApp((s) => s.brokerLogSeq);
  const [level, setLevel] = useState<LogLine['level']>('info');
  const [lines, setLines] = useState<LogLine[]>([]);
  const after = useRef(0);
  useEffect(() => {
    after.current = 0;
    setLines([]);
  }, [level]);
  useEffect(() => {
    let live = true;
    void engine()
      .brokerLog(level, after.current)
      .then((l) => {
        if (!live || !l.length) return;
        after.current = l[l.length - 1].seq;
        setLines((x) => [...x, ...l].slice(-500));
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [seq, level]);
  return (
    <>
      <div className="ph" style={{ borderTop: '1px solid var(--line)' }}>
        <h5>{t('broker.log')}</h5>
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
  const [certs, setCerts] = useState<BrokerCerts | null>(null);
  const [issued, setIssued] = useState<IssuedCert[]>([]);
  const [clients, setClients] = useState<BrokerClient[]>([]);
  const [certsOpen, setCertsOpen] = useState(false);
  const [dialog, setDialog] = useState<
    | null
    | { kind: 'add' }
    | { kind: 'issue' }
    | { kind: 'login'; l: Listener }
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
  // refresh clients when the count changes
  useEffect(() => {
    void engine()
      .brokerClients()
      .then(setClients)
      .catch(() => undefined);
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
    const id = await platform().showContextMenu(
      [
        { id: 'connect', label: t('broker.connectThis'), enabled: running && l.enabled },
        { id: 's', label: '', separator: true },
        { id: 'remove', label: t('broker.removeListener'), danger: true },
      ],
      r.right - 220,
      r.bottom + 4,
    );
    if (id === 'connect') {
      const out = await run(engine().connectExplorerHere(l.id));
      if (out) await handleConnectOutcome('', out);
    } else if (id === 'remove') setDialog({ kind: 'removeListener', l });
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
        {status.restartNeeded && <span className="chip clay" data-testid="restart-chip">{t('broker.restartToApply')}</span>}
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
            <th>{t('broker.login')}</th>
            <th>{t('broker.clientCert')}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {config.listeners.map((l) => (
            <ListenerRow key={l.id} l={l} anonymous={anonymous} onConfig={setConfig} onLogin={(x) => setDialog({ kind: 'login', l: x })} onMenu={(x, e) => void listenerMenu(x, e)} />
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
      <div className="row" style={{ marginTop: 6 }}>
        <button type="button" className="btn pri" disabled={anonymous} data-testid="issue-cert" onClick={() => setDialog({ kind: 'issue' })}>
          {t('broker.issueCert')}
        </button>
        <button type="button" className="btn sm" disabled={anonymous} onClick={() => setDialog({ kind: 'csr' })}>
          {t('broker.importCsr')}
        </button>
        {!anonymous && <span className="hint">{t('broker.certsHelp')}</span>}
      </div>
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
          <span className="hint" style={{ fontWeight: 400 }}>
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
              if (c) setCerts(c);
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
              if (c) setCerts(c);
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
        {t('broker.auth')}
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
        <div className={`f${anonymous ? ' gated' : ''}`} data-testid="users">
          <span className="lbl">{t('broker.users')}</span>
          <span className="row2 wrap">
            {config.users.map((u) => (
              <span key={u.username} className="chip mono" onContextMenu={anonymous ? undefined : (e) => void userMenu(u.username, e)}>
                <button type="button" className="x" style={{ opacity: 1 }} disabled={anonymous} title={t('users.changePassword')} onClick={() => setDialog({ kind: 'userPass', user: u.username })}>
                  {u.username}
                </button>
                {!anonymous && (
                  <button
                    type="button"
                    className="x"
                    aria-label={t('users.removeNamed', { user: u.username })}
                    onClick={async () => {
                      const c = await run(engine().removeUser(u.username));
                      if (c) setConfig(c);
                    }}
                  >
                    ✕
                  </button>
                )}
              </span>
            ))}
            <button type="button" className="btn sm" disabled={anonymous} onClick={() => setDialog({ kind: 'addUser' })}>
              {t('users.add')}
            </button>
          </span>
        </div>
        <div className={`f${anonymous ? ' gated' : ''}`} data-testid="verify-against">
          <span className="lbl">
            {t('broker.clientCerts')} <Info id="requireClientCert" title={t('broker.clientCerts')} />
          </span>
          <span className="row2">
            <span className="muted">{t('broker.verifyAgainst')}</span>
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
          </span>
        </div>
      </div>
    </>
  );

  return (
    <div className="cols" style={{ gridTemplateColumns: 'minmax(0,1fr) 380px' }}>
      <div className="pane" data-testid="broker">
        <div className="ph">
          <h5>{t('broker.title')}</h5>
          <div className="row" style={{ gap: 6 }}>
            <button type="button" className="btn" disabled={!running} onClick={() => void lifecycle(() => engine().brokerRestart())}>
              {t('broker.restart')}
            </button>
            {running || status.state === 'starting' ? (
              <button type="button" className="btn danger" data-testid="broker-stop" onClick={() => void lifecycle(() => engine().brokerStop())}>
                {t('broker.stop')}
              </button>
            ) : (
              <button type="button" className="btn" data-testid="broker-start" onClick={() => void lifecycle(() => engine().brokerStart())}>
                {t('broker.start')}
              </button>
            )}
            <button
              type="button"
              className="btn pri"
              disabled={!running}
              data-testid="connect-here"
              onClick={async () => {
                const out = await run(engine().connectExplorerHere(null));
                if (out) await handleConnectOutcome('', out);
              }}
            >
              {t('broker.connectHere')}
            </button>
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
              <div className="l">{t('broker.status')}</div>
              <div className="v">
                <span className={`dot${running ? '' : status.state === 'starting' ? ' warn' : status.state === 'failed' ? ' err' : ' off'}`} style={{ alignSelf: 'center' }} />
                {t(`bstate.${status.state}`)}
                {running && status.startedAt && <small>{fmtUptime(now - status.startedAt)}</small>}
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
          {anonymous ? (
            <>
              {authSection}
              {clientCertsSection}
              {brokerCertsSection}
            </>
          ) : (
            <>
              {clientCertsSection}
              {brokerCertsSection}
              {authSection}
            </>
          )}
        </div>
      </div>
      <div className="pane right" data-testid="broker-side">
        <div className="ph">
          <h5>{t('broker.clientsN', { n: fmtNum(clients.length) })}</h5>
          <button type="button" className="btn sm" onClick={async () => setClients((await run(engine().brokerClients())) ?? clients)}>
            {t('broker.refresh')}
          </button>
        </div>
        <div style={{ maxHeight: '45%', overflow: 'auto', flex: 'none' }}>
          <table className="tbl compact" data-testid="clients">
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
                  <td className="mono">{c.address === 'localhost' ? t('clients.localhost') : c.address}</td>
                  <td>
                    {c.protocol} · {c.transport}
                  </td>
                  <td>{c.subscriptions}</td>
                  <td>
                    <button
                      type="button"
                      className="btn sm"
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

      {dialog?.kind === 'add' && <AddListenerDialog config={config} certs={certs} onClose={() => setDialog(null)} onConfig={setConfig} />}
      {dialog?.kind === 'issue' && <IssueCertDialog onClose={() => setDialog(null)} onIssued={async () => setIssued((await run(engine().issuedCerts())) ?? issued)} />}
      {dialog?.kind === 'login' && (
        <CredentialsDialog
          title={t('broker.loginTitle', { kind: kindLabel(dialog.l.kind, t), port: dialog.l.port })}
          withUser
          initialUser={dialog.l.login?.username ?? ''}
          confirmLabel={t('common.save')}
          onClose={() => setDialog(null)}
          onConfirm={async (u, p) => {
            const c = await engine().setListenerLogin(dialog.l.id, u, p || null);
            setConfig(c);
          }}
        />
      )}
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
            if (c) setCerts(c);
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
            if (c) setConfig(c);
          }}
        />
      )}
      {dialog?.kind === 'regen' && certs && <RegenerateDialog certs={certs} onClose={() => setDialog(null)} onCerts={setCerts} />}
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
          <input className="input" type="password" value={p} aria-label={t('issue.passphrase')} data-autofocus onChange={(e) => setP(e.target.value)} />
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

import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { Profile, Qos, Scheme, Subscription } from '@mqttpanel/engine-client';
import { engine } from '../../services';
import { useApp } from '../../store/app';
import { connectProfile, testProfile, disconnect, saveProfile, replaceProfile, useBusy, run } from '../../lib/actions';
import { Info, Msg, Modal, Popover, Seg, Switch } from '../../components/ui';
import { FileField } from './FileField';
import { DeleteProfileDialog } from './DeleteProfileDialog';
import { SecretText } from '../../components/Secret';
import { FoldHeader, useFold } from '../../components/Fold';
import { fmtAge, fmtBytes } from '../../lib/format';

const DEFAULT_PORT: Record<Scheme, number> = { mqtt: 1883, mqtts: 8883, ws: 8083, wss: 8084 };

function SubscriptionAdd({ onAdd }: { onAdd(s: Subscription): void }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState('');
  const [qos, setQos] = useState<Qos>(0);
  const ref = useRef<HTMLButtonElement>(null);
  const add = () => {
    if (!filter.trim()) return;
    onAdd({ filter: filter.trim(), qos });
    setFilter('');
    setOpen(false);
  };
  return (
    <>
      <button ref={ref} type="button" className="btn sm" onClick={() => setOpen(!open)} data-testid="sub-add">
        {t('conn.addSub')}
      </button>
      {open && (
        <Popover anchor={ref.current} onClose={() => setOpen(false)} label={t('conn.addSubTitle')}>
          <div className="row">
            <input
              className="input mono"
              style={{ width: 220 }}
              placeholder="building/+/temperature"
              value={filter}
              data-testid="sub-filter"
              aria-label={t('conn.filter')}
              autoFocus
              onChange={(e) => setFilter(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && add()}
            />
            <Seg<Qos> label="QoS" value={qos} onChange={setQos} options={([0, 1, 2] as Qos[]).map((q) => ({ value: q, label: `QoS ${q}` }))} />
            <button type="button" className="btn pri sm" onClick={add} disabled={!filter.trim()} data-testid="sub-add-ok">
              {t('common.add')}
            </button>
          </div>
        </Popover>
      )}
    </>
  );
}

function ProtoDialog({ profile, fileName, onClose, onProfile }: { profile: Profile; fileName: string; onClose(): void; onProfile(p: Profile): void }) {
  const { t } = useTranslation();
  const schema = profile.protoSchemas.find((s) => s.fileName === fileName);
  const [filter, setFilter] = useState(schema?.topicFilter ?? '#');
  const [type, setType] = useState(schema?.messageType ?? '');
  const path = useRef<string>(fileName);
  if (!schema) return null;
  return (
    <Modal
      title={t('proto.title', { file: fileName })}
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
              const p = await run(engine().addProtoSchema(profile.id, path.current, filter, type || null));
              if (p) onProfile(p);
              onClose();
            }}
          >
            {t('common.save')}
          </button>
        </>
      }
    >
      <div className="form">
        <div className="f" style={{ gridTemplateColumns: '120px 1fr' }}>
          <label>{t('proto.filter')}</label>
          <input className="input mono" value={filter} onChange={(e) => setFilter(e.target.value)} />
        </div>
        <div className="f" style={{ gridTemplateColumns: '120px 1fr' }}>
          <label>{t('proto.type')}</label>
          <select className="input mono" value={type} onChange={(e) => setType(e.target.value)}>
            {schema.messageTypes.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
        </div>
      </div>
    </Modal>
  );
}

export function ProfileForm({ profileId }: { profileId: string }) {
  const { t } = useTranslation();
  const stored = useApp((s) => s.profiles.find((p) => p.id === profileId) ?? null);
  const conn = useApp((s) => s.connections.find((c) => c.id === profileId) ?? null);
  const busy = useBusy();
  const securityOpen = useFold('security');
  const advancedOpen = useFold('advanced');
  const [p, setP] = useState<Profile | null>(stored);
  const [pw, setPw] = useState('');
  const [protoEdit, setProtoEdit] = useState<{ fileName: string; path: string } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const draft = useRef<Profile | null>(stored);

  // take the stored profile when switching profiles or when the engine changed it
  useEffect(() => {
    if (!stored) return;
    if (!draft.current || draft.current.id !== stored.id || !timer.current) {
      draft.current = stored;
      setP(stored);
    }
  }, [stored]);
  useEffect(() => setPw(''), [profileId]);

  const flush = useCallback(async (): Promise<Profile> => {
    if (timer.current) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    const cur = draft.current!;
    return saveProfile(cur);
  }, []);
  useEffect(
    () => () => {
      if (timer.current) {
        clearTimeout(timer.current);
        timer.current = null;
        if (draft.current) void saveProfile(draft.current);
      }
    },
    [profileId],
  );

  const update = (patch: Partial<Profile>) => {
    const next = { ...draft.current!, ...patch };
    draft.current = next;
    setP(next);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      timer.current = null;
      void saveProfile(draft.current!);
    }, 400);
  };
  const updateTls = (patch: Partial<Profile['tls']>) => update({ tls: { ...draft.current!.tls, ...patch } });
  const fromEngine = (np: Profile) => {
    draft.current = np;
    setP(np);
    replaceProfile(np);
  };

  if (!p) return null;
  const tls = p.scheme === 'mqtts' || p.scheme === 'wss';
  const connected = conn && (conn.state === 'connected' || conn.state === 'connecting' || conn.state === 'reconnecting');
  const lastTest = p.lastTest;

  const browseProto = async () => {
    const saved = await flush();
    const ins = await run(engine().pickFile('proto'));
    if (!ins) return;
    if (!ins.ok) {
      useApp.getState().toast({ kind: 'err', msg: ins.error });
      return;
    }
    const np = await run(engine().addProtoSchema(saved.id, ins.path, '#', null));
    if (np) {
      fromEngine(np);
      setProtoEdit({ fileName: ins.fileName, path: ins.path });
    }
  };

  const subs = p.subscriptions;

  /** "verify on · CA · client certificate · verified" or "verify off"; empty while everything is at its default. */
  const securitySummary = () => {
    const parts: React.ReactNode[] = [];
    const set = !!(p.tls.caCert || p.tls.clientCert);
    if (!p.tls.verifyServer) parts.push(t('conn.sum.verifyOff'));
    else if (set) parts.push(t('conn.sum.verifyOn'));
    if (p.tls.caCert) parts.push(t('conn.sum.ca'));
    if (p.tls.clientCert) parts.push(t('conn.sum.clientCert'));
    if (lastTest && lastTest.ok && lastTest.verified !== null) {
      parts.push(
        <span key="v" className={lastTest.verified ? 'ok' : 'warn'} data-testid="security-verified">
          {lastTest.verified ? t('conn.verified') : t('conn.unverified')}
        </span>,
      );
    }
    return parts.length ? parts.flatMap((x, i) => (i ? [' · ', x] : [x])) : null;
  };

  /** "keep-alive 60 s · 3 subscriptions · 1 schema"; empty while everything is at its default. */
  const advancedSummary = () => {
    const parts: string[] = [];
    const defaultSubs = subs.length === 1 && subs[0].filter === '#' && subs[0].qos === 0;
    if (!defaultSubs) parts.push(subs.length ? t('conn.subCount', { count: subs.length }) : t('conn.sum.noSubs'));
    if (p.protoSchemas.length) parts.push(t('conn.sum.schemas', { count: p.protoSchemas.length }));
    if (p.sessionExpiry > 0) parts.push(t('conn.sum.sessionExpiry', { n: p.sessionExpiry }));
    if (!p.cleanStart) parts.push(t('conn.sum.noCleanStart'));
    if (p.lastWill?.topic) parts.push(t('conn.sum.lastWill'));
    // keep-alive leads the line whenever something is shown, or when it differs from the default
    if (parts.length || p.keepAlive !== 60) parts.unshift(t('conn.sum.keepAlive', { n: p.keepAlive }));
    return parts.join(' · ');
  };

  const testLine = () => {
    if (!lastTest) return null;
    if (!lastTest.ok) return <Msg msg={lastTest.error} />;
    const parts: string[] = [t('test.connectedIn', { ms: lastTest.latencyMs ?? 0 })];
    if (lastTest.serverCert) {
      const c = lastTest.serverCert;
      const cn = c.subject.split(',')[0];
      const iss = c.issuer.replace(/^CN=/, '').split(',')[0];
      parts.push(t('test.serverCert', { subject: cn, issuer: iss, date: c.notAfter }));
    }
    if (lastTest.negotiatedVersion) {
      parts.push(
        lastTest.maxPacketSize
          ? t('test.negotiatedMax', { version: lastTest.negotiatedVersion, size: fmtBytes(lastTest.maxPacketSize) })
          : t('test.negotiated', { version: lastTest.negotiatedVersion }),
      );
    }
    return parts.join(' ');
  };

  return (
    <div className="pane" data-testid="profile-form">
      <div className="ph">
        <h5>{p.name || t('conn.untitled')}</h5>
        <div className="row">
          {/* change set 2: Delete outlined in brick, normal size, then Test and Connect; Duplicate is in the card menu */}
          <button type="button" className="btn danger out" data-testid="delete-profile-btn" onClick={() => setConfirmDelete(true)}>
            {t('conn.deleteBtn')}
          </button>
          <span style={{ width: 10 }} aria-hidden="true" />
          <button type="button" className="btn" data-testid="try-connect" disabled={busy.testing === p.id} onClick={async () => {
            await flush();
            await testProfile(p.id);
          }}>
            {busy.testing === p.id ? t('conn.testing') : t('conn.tryConnect')}
          </button>
          {connected ? (
            <button type="button" className="btn" data-testid="disconnect" onClick={() => void disconnect(p.id)}>
              {t('conn.disconnect')}
            </button>
          ) : (
            <button type="button" className="btn pri" data-testid="connect" disabled={busy.connecting === p.id} onClick={async () => {
              await flush();
              await connectProfile(p.id);
            }}>
              {busy.connecting === p.id ? t('conn.connecting') : t('conn.connect')}
            </button>
          )}
        </div>
      </div>
      <div className="pc">
        <div className="form">
          <div className="sec">{t('conn.basics')}</div>
          <div className="f">
            <label htmlFor="pf-name">{t('conn.name')}</label>
            <input id="pf-name" className="input" value={p.name} placeholder={t('conn.untitled')} onChange={(e) => update({ name: e.target.value })} />
          </div>
          <div className="f">
            <label htmlFor="pf-host">{t('conn.hostPort')}</label>
            <span className="row2">
              <input id="pf-host" className="input mono" style={{ flex: 1 }} value={p.host} onChange={(e) => update({ host: e.target.value.trim() })} />
              <input
                className="input mono"
                style={{ width: 80 }}
                aria-label={t('conn.port')}
                inputMode="numeric"
                value={p.port}
                onChange={(e) => update({ port: Number(e.target.value.replace(/\D/g, '')) || 0 })}
              />
            </span>
          </div>
          <div className="f">
            <span className="lbl">{t('conn.transport')}</span>
            <span className="row2">
              <Seg
                label={t('conn.transport')}
                value={p.scheme}
                testId="scheme"
                onChange={(scheme) => update({ scheme, port: p.port === DEFAULT_PORT[p.scheme] ? DEFAULT_PORT[scheme] : p.port })}
                options={(['mqtt', 'mqtts', 'ws', 'wss'] as Scheme[]).map((s) => ({ value: s, label: s, testId: `scheme-${s}` }))}
              />
              <Seg
                label={t('conn.version')}
                value={p.mqttVersion}
                onChange={(mqttVersion) => update({ mqttVersion })}
                options={[
                  { value: '3.1.1', label: '3.1.1' },
                  { value: '5', label: '5' },
                  { value: 'auto', label: 'auto' },
                ]}
              />
            </span>
          </div>
          {(p.scheme === 'ws' || p.scheme === 'wss') && (
            <div className="f">
              <label htmlFor="pf-ws">{t('conn.wsPath')}</label>
              <input id="pf-ws" className="input mono" style={{ width: 160 }} value={p.wsPath} onChange={(e) => update({ wsPath: e.target.value })} />
            </div>
          )}
          <div className="f">
            <label htmlFor="pf-cid">{t('conn.clientId')}</label>
            <span className="row2">
              <input id="pf-cid" className="input mono" style={{ flex: 1 }} value={p.clientId} onChange={(e) => update({ clientId: e.target.value })} />
              <button type="button" className="btn sm" onClick={() => update({ clientId: 'mqttpanel-' + Math.random().toString(16).slice(2, 6) })}>
                {t('conn.random')}
              </button>
            </span>
          </div>
          <div className="f">
            <label htmlFor="pf-user">{t('conn.username')}</label>
            <input id="pf-user" className="input" value={p.username} onChange={(e) => update({ username: e.target.value })} />
          </div>
          <div className="f">
            <label htmlFor="pf-pw">{t('conn.password')}</label>
            {p.isLocalBroker && p.hasPassword ? (
              <span className="row2" data-testid="local-password">
                <SecretText load={() => engine().profilePassword(p.id)} testId="profile-pw" />
              </span>
            ) : (
              <span className="row2">
                <input
                  id="pf-pw"
                  className="input"
                  style={{ flex: 1 }}
                  type="password"
                  autoComplete="off"
                  value={pw}
                  placeholder={p.hasPassword ? '••••••••••••' : t('conn.passwordNone')}
                  onChange={(e) => setPw(e.target.value)}
                  onBlur={async () => {
                    if (!pw) return;
                    const saved = await flush();
                    const np = await run(engine().setPassword(saved.id, pw));
                    setPw('');
                    if (np) fromEngine(np);
                  }}
                />
                {p.hasPassword && (
                  <button type="button" className="iconbtn" aria-label={t('conn.removePassword')} onClick={async () => {
                    const np = await run(engine().setPassword(p.id, ''));
                    if (np) fromEngine(np);
                  }}>
                    ✕
                  </button>
                )}
              </span>
            )}
          </div>

          {/* change set 4: Security and Advanced are folds, closed at app start; rows carry labels, controls and ⓘ only */}
          <FoldHeader id="security" title={t('conn.security')} summary={securitySummary()} />
          {securityOpen && (
            <>
              <div className="f">
                <span className="lbl">
                  {t('conn.verifyServer')} <Info id="verifyServer" title={t('conn.verifyServer')} />
                </span>
                <span className="row2">
                  <Switch on={p.tls.verifyServer} label={t('conn.verifyServer')} onChange={(v) => updateTls({ verifyServer: v })} />
                </span>
              </div>
              <div className="f">
                <span className="lbl">
                  {t('conn.caCert')} <Info id="caCert" title={t('conn.caCert')} />
                </span>
                <FileField field="caCert" profile={p} label={t('conn.caCert')} beforeChange={flush} onProfile={fromEngine} />
              </div>
              <div className="f">
                <span className="lbl">
                  {t('conn.clientCert')} <Info id="clientCert" title={t('conn.clientCert')} />
                </span>
                <FileField field="clientCert" profile={p} label={t('conn.clientCert')} beforeChange={flush} onProfile={fromEngine} />
              </div>
              <div className="f">
                <span className="lbl">
                  {t('conn.clientKey')} <Info id="clientKey" title={t('conn.clientKey')} />
                </span>
                <FileField field="clientKey" profile={p} label={t('conn.clientKey')} beforeChange={flush} onProfile={fromEngine} />
              </div>
              <div className="f">
                <span className="lbl">
                  {t('conn.sni')} <Info id="sni" title={t('conn.sni')} />
                </span>
                <span className="row2">
                  <input className="input mono" style={{ width: 220 }} aria-label={t('conn.sni')} placeholder={t('common.optional')} value={p.tls.sni} onChange={(e) => updateTls({ sni: e.target.value.trim() })} />
                </span>
              </div>
              <div className="f">
                <span className="lbl">
                  {t('conn.alpn')} <Info id="alpn" title={t('conn.alpn')} />
                </span>
                <span className="row2">
                  <input className="input mono" style={{ width: 140 }} aria-label={t('conn.alpn')} placeholder={t('common.optional')} value={p.tls.alpn} onChange={(e) => updateTls({ alpn: e.target.value.trim() })} />
                </span>
              </div>
              {!tls && (p.tls.caCert || p.tls.clientCert) && <div className="hint warn">{t('conn.tlsUnused')}</div>}
            </>
          )}

          <FoldHeader id="advanced" title={t('conn.advanced')} summary={advancedSummary()} />
          {advancedOpen && (
            <>
              <div className="sub">{t('conn.session')}</div>
              <div className="f">
                <label htmlFor="pf-ka">{t('conn.keepAlive')}</label>
                <span className="row2">
                  <input id="pf-ka" className="input mono" style={{ width: 80 }} inputMode="numeric" value={p.keepAlive} onChange={(e) => update({ keepAlive: Number(e.target.value.replace(/\D/g, '')) || 0 })} />
                  <span className="hint">s</span>
                  <span className="muted">{t('conn.cleanStart')}</span>
                  <Switch on={p.cleanStart} label={t('conn.cleanStart')} onChange={(cleanStart) => update({ cleanStart })} />
                </span>
              </div>
              <div className="f">
                <label htmlFor="pf-se">{t('conn.sessionExpiry')}</label>
                <span className="row2">
                  <input id="pf-se" className="input mono" style={{ width: 100 }} inputMode="numeric" value={p.sessionExpiry} onChange={(e) => update({ sessionExpiry: Number(e.target.value.replace(/\D/g, '')) || 0 })} />
                  <span className="hint">{t('conn.sessionExpiryHint')}</span>
                </span>
              </div>
              <div className="f">
                <span className="lbl">{t('conn.lastWill')}</span>
                <span className="row2">
                  <input
                    className="input mono"
                    style={{ flex: 1 }}
                    aria-label={t('conn.lastWillTopic')}
                    placeholder={t('conn.lastWillTopic')}
                    value={p.lastWill?.topic ?? ''}
                    onChange={(e) =>
                      update({ lastWill: e.target.value || p.lastWill?.payload ? { qos: 0, retain: false, payload: '', ...p.lastWill, topic: e.target.value } : null })
                    }
                  />
                  <input
                    className="input mono"
                    style={{ flex: 1 }}
                    aria-label={t('conn.lastWillPayload')}
                    placeholder={t('conn.lastWillPayload')}
                    value={p.lastWill?.payload ?? ''}
                    onChange={(e) => update({ lastWill: { qos: 0, retain: false, topic: '', ...p.lastWill, payload: e.target.value } })}
                  />
                </span>
              </div>

              <div className="sub">{t('conn.subscriptions')}</div>
              <div className="f">
                <span className="lbl">
                  {t('conn.filters')} <Info id="filters" title={t('conn.filters')} />
                </span>
                <span className="row2 wrap" data-testid="subscriptions">
                  {subs.map((s, i) => (
                    <span key={s.filter + i} className="row" style={{ gap: 6, flex: 'none' }}>
                      <span className="chip mono" data-testid="sub-chip">
                        {s.filter} · QoS {s.qos}
                        <button
                          type="button"
                          className="x"
                          aria-label={t('conn.removeSub', { filter: s.filter })}
                          data-testid="sub-remove"
                          onClick={() => update({ subscriptions: subs.filter((_, j) => j !== i) })}
                        >
                          ✕
                        </button>
                      </span>
                      {s.filter === '#' && <span className="hint">{t('conn.everything')}</span>}
                    </span>
                  ))}
                  <SubscriptionAdd onAdd={(s) => update({ subscriptions: [...subs.filter((x) => x.filter !== s.filter), s] })} />
                </span>
              </div>

              <div className="sub">{t('conn.decoders')}</div>
              <div className="f">
                <span className="lbl">{t('conn.protobuf')}</span>
                <span className="row2 wrap">
                  {p.protoSchemas.map((s) => (
                    <span key={s.fileName} className="chip mono">
                      <button type="button" className="x" style={{ opacity: 1 }} onClick={() => setProtoEdit({ fileName: s.fileName, path: s.fileName })}>
                        {s.fileName} → {s.topicFilter} · {s.messageType.split('.').pop()}
                      </button>
                      <button
                        type="button"
                        className="x"
                        aria-label={t('conn.removeProto', { file: s.fileName })}
                        onClick={async () => {
                          const np = await run(engine().removeProtoSchema(p.id, s.fileName));
                          if (np) fromEngine(np);
                        }}
                      >
                        ✕
                      </button>
                    </span>
                  ))}
                  <button type="button" className="btn sm" onClick={() => void browseProto()}>
                    {t('common.browse')}
                  </button>
                </span>
              </div>
            </>
          )}
          {/* shown with the folds closed too: a profile without subscriptions receives nothing */}
          {subs.length === 0 && (
            <div className="warnstrip" data-testid="no-subs">
              <Msg msg={{ key: 'profile.noSubscriptions', params: {}, text: 'No subscriptions: you will see nothing until you add one.' }} />
            </div>
          )}
          {conn?.subscriptionErrors.map((se) => (
            <div key={se.filter} className="hint err">
              <Msg msg={se.error} />
            </div>
          ))}
        </div>
        {lastTest && (
          <div className={`callout${lastTest.ok ? '' : ' err'}`} style={{ marginTop: 16 }} data-testid="last-test">
            <div className="eyebrow">{t('test.last', { age: fmtAge(Date.now() - lastTest.at) })}</div>
            <p className="small selectable" style={{ marginTop: 4 }}>
              {testLine()}
            </p>
          </div>
        )}
      </div>
      {protoEdit && (
        <ProtoDialog profile={p} fileName={protoEdit.fileName} onClose={() => setProtoEdit(null)} onProfile={fromEngine} />
      )}
      {confirmDelete && (
        <DeleteProfileDialog
          profile={p}
          onClose={() => setConfirmDelete(false)}
          beforeDelete={() => {
            if (timer.current) {
              clearTimeout(timer.current);
              timer.current = null;
            }
          }}
        />
      )}
    </div>
  );
}

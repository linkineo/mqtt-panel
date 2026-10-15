import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { Profile, Qos, Scheme, Subscription } from '@mqttpanel/engine-client';
import { engine } from '../../services';
import { useApp } from '../../store/app';
import { connectProfile, testProfile, disconnect, saveProfile, replaceProfile, setSkipOnce, useBusy, run } from '../../lib/actions';
import { Info, Msg, Modal, Popover, Seg, Switch, NO_AUTO } from '../../components/ui';
import { FileField } from './FileField';
import { DeleteProfileDialog } from './DeleteProfileDialog';
import { SecretText } from '../../components/Secret';
import { FoldHeader, useFold } from '../../components/Fold';
import { fmtAge, fmtBytes } from '../../lib/format';
import { applyAddressText, applyScheme, formatAddress, isWebSocket, nameFromHost, type AddressFields } from '../../lib/address';

const fieldsOf = (p: Profile): AddressFields => ({ scheme: p.scheme, host: p.host, port: p.port, wsPath: p.wsPath });
const sameFields = (a: AddressFields, b: AddressFields) => a.scheme === b.scheme && a.host === b.host && a.port === b.port && a.wsPath === b.wsPath;

/**
 * "Host and port" with the address prefiller (change set 5): the field shows "<scheme>://<host>:<port>" and is a view
 * over the profile's scheme, host, port and WebSocket path (parsing and formatting in lib/address.ts). While focused it
 * keeps the user's text and the profile follows what it reads; on blur, or when the protocol or the port change from
 * outside, the text is normalised (a deleted prefix comes back).
 */
function AddressField({ p, onChange }: { p: Profile; onChange(patch: Partial<Profile>): void }) {
  const { t } = useTranslation();
  const [text, setText] = useState<string | null>(null);
  const cur = fieldsOf(p);
  // a change that did not come from this text (protocol segment, port box, engine): rewrite the text
  useEffect(() => {
    if (text !== null && !sameFields(applyAddressText(cur, text), cur)) setText(formatAddress(p));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [p.scheme, p.host, p.port, p.wsPath]);
  return (
    <span className="row2">
      <input {...NO_AUTO}
        id="pf-host"
        className="input mono w-xl"
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        value={text ?? formatAddress(p)}
        onFocus={() => setText(formatAddress(p))}
        onBlur={() => setText(null)}
        onChange={(e) => {
          setText(e.target.value);
          const next = applyAddressText(cur, e.target.value);
          if (!sameFields(next, cur)) onChange(next);
        }}
      />
      <input {...NO_AUTO}
        className="input mono w-s"
        aria-label={t('conn.port')}
        data-testid="pf-port"
        inputMode="numeric"
        value={p.port}
        onChange={(e) => onChange({ port: Number(e.target.value.replace(/\D/g, '').slice(0, 5)) || 0 })}
      />
    </span>
  );
}

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
            <input {...NO_AUTO}
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
          <input {...NO_AUTO} className="input mono w-l" value={filter} onChange={(e) => setFilter(e.target.value)} />
        </div>
        <div className="f" style={{ gridTemplateColumns: '120px 1fr' }}>
          <label>{t('proto.type')}</label>
          <select className="input mono w-l" value={type} onChange={(e) => setType(e.target.value)}>
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

/**
 * "Skip the broker certificate check?" (change set 6), opened by turning on "Skip unverified broker certificate":
 * the risk sentence, then Cancel, "Only this time" (the next attempt only, nothing saved) and "Always for this
 * connection" (saved in the profile).
 */
export function SkipVerifyDialog({ onCancel, onOnce, onAlways }: { onCancel(): void; onOnce(): void; onAlways(): void }) {
  const { t } = useTranslation();
  return (
    <Modal
      title={t('skip.title')}
      width={520}
      onClose={onCancel}
      testId="skip-dialog"
      footer={
        <>
          <button type="button" className="btn" onClick={onCancel}>
            {t('common.cancel')}
          </button>
          <span className="grow" />
          <button type="button" className="btn" data-testid="skip-once-btn" onClick={onOnce}>
            {t('skip.once')}
          </button>
          <button type="button" className="btn clay" data-testid="skip-always-btn" onClick={onAlways}>
            {t('skip.always')}
          </button>
        </>
      }
    >
      <p className="risk" data-testid="skip-risk">
        {t('trust.risk')}
      </p>
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
  const [skipPrompt, setSkipPrompt] = useState(false);
  const skipOnce = useApp((s) => !!s.skipOnce[profileId]);
  const autoNamed = useApp((s) => !!s.autoNamed[profileId]);
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
    const cur = draft.current!;
    // change set 6: a profile made with "New broker…" takes its name from the host while the name is empty or still
    // the one derived from the previous host; a typed name breaks the link, clearing it restores the link
    if (autoNamed && patch.host !== undefined && patch.name === undefined && patch.host !== cur.host && (cur.name === '' || cur.name === nameFromHost(cur.host))) {
      patch = { ...patch, name: nameFromHost(patch.host) };
    }
    const next = { ...cur, ...patch };
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

  /** "CA · client certificate · verified"; empty while everything is at its default (change set 6: no verify part). */
  const securitySummary = () => {
    const parts: React.ReactNode[] = [];
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

  /** "keep-alive 60 s · 3 subscriptions · 1 schema", plus "skip verification" in clay; empty while everything is at
   * its default. */
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
    if (!p.tls.skipVerify) return parts.join(' · ');
    const skip = (
      <span key="skip" className="warn" data-testid="advanced-skip">
        {t('conn.sum.skipVerify')}
      </span>
    );
    return parts.length ? [parts.join(' · '), ' · ', skip] : [skip];
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
            <input {...NO_AUTO}
              id="pf-name"
              className="input w-xl"
              value={p.name}
              placeholder={t('conn.untitled')}
              onChange={(e) => update({ name: e.target.value })}
              onBlur={() => {
                if (autoNamed && !draft.current!.name.trim()) update({ name: nameFromHost(draft.current!.host) });
              }}
            />
          </div>
          {/* change set 5: Protocol and MQTT version on their own rows, before the address */}
          <div className="f">
            <span className="lbl">{t('conn.protocol')}</span>
            <span className="row2">
              <Seg
                label={t('conn.protocol')}
                value={p.scheme}
                testId="scheme"
                onChange={(scheme) => update(applyScheme(fieldsOf(draft.current!), scheme))}
                options={(['mqtt', 'mqtts', 'ws', 'wss'] as Scheme[]).map((s) => ({ value: s, label: s, testId: `scheme-${s}` }))}
              />
            </span>
          </div>
          <div className="f">
            <span className="lbl">{t('conn.version')}</span>
            <span className="row2">
              <Seg
                label={t('conn.version')}
                value={p.mqttVersion}
                testId="mqtt-version"
                onChange={(mqttVersion) => update({ mqttVersion })}
                options={[
                  { value: '3.1.1', label: '3.1.1' },
                  { value: '5', label: '5' },
                  { value: 'auto', label: 'auto' },
                ]}
              />
            </span>
          </div>
          <div className="f">
            <label htmlFor="pf-host">{t('conn.hostPort')}</label>
            <AddressField p={p} onChange={update} />
          </div>
          {isWebSocket(p.scheme) && (
            <div className="f">
              <label htmlFor="pf-ws">{t('conn.wsPath')}</label>
              <input {...NO_AUTO} id="pf-ws" className="input mono w-m" value={p.wsPath} onChange={(e) => update({ wsPath: e.target.value })} />
            </div>
          )}
          <div className="f">
            <label htmlFor="pf-cid">{t('conn.clientId')}</label>
            <span className="row2">
              <input {...NO_AUTO} id="pf-cid" className="input mono w-l" value={p.clientId} onChange={(e) => update({ clientId: e.target.value })} />
              <button type="button" className="btn sm" data-testid="generate-cid" onClick={() => update({ clientId: 'mqttpanel-' + Math.random().toString(16).slice(2, 6) })}>
                {t('conn.generate')}
              </button>
            </span>
          </div>
          <div className="f">
            <label htmlFor="pf-user">{t('conn.username')}</label>
            <input {...NO_AUTO} id="pf-user" className="input w-l" value={p.username} onChange={(e) => update({ username: e.target.value })} />
          </div>
          <div className="f">
            <label htmlFor="pf-pw">{t('conn.password')}</label>
            {p.isLocalBroker && p.hasPassword ? (
              <span className="row2" data-testid="local-password">
                <SecretText load={() => engine().profilePassword(p.id)} testId="profile-pw" />
              </span>
            ) : (
              <span className="row2">
                <input {...NO_AUTO}
                  id="pf-pw"
                  className="input w-l"
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
          {/* change set 5: with mqtt or ws the Security fold does not apply; its values stay in the profile */}
          <FoldHeader id="security" title={t('conn.security')} summary={tls ? securitySummary() : t('conn.securityOff')} off={!tls} />
          {securityOpen && tls && (
            <>
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
                  <input {...NO_AUTO} className="input mono w-l" aria-label={t('conn.sni')} placeholder={t('common.optional')} value={p.tls.sni} onChange={(e) => updateTls({ sni: e.target.value.trim() })} />
                </span>
              </div>
              <div className="f">
                <span className="lbl">
                  {t('conn.alpn')} <Info id="alpn" title={t('conn.alpn')} />
                </span>
                <span className="row2">
                  <input {...NO_AUTO} className="input mono w-m" aria-label={t('conn.alpn')} placeholder={t('common.optional')} value={p.tls.alpn} onChange={(e) => updateTls({ alpn: e.target.value.trim() })} />
                </span>
              </div>
            </>
          )}

          <FoldHeader id="advanced" title={t('conn.advanced')} summary={advancedSummary()} />
          {advancedOpen && (
            <>
              <div className="sub">{t('conn.session')}</div>
              <div className="f">
                <label htmlFor="pf-ka">{t('conn.keepAlive')}</label>
                <span className="row2">
                  <input {...NO_AUTO} id="pf-ka" className="input mono w-s" inputMode="numeric" value={p.keepAlive} onChange={(e) => update({ keepAlive: Number(e.target.value.replace(/\D/g, '')) || 0 })} />
                  <span className="hint">s</span>
                  <span className="muted">{t('conn.cleanStart')}</span>
                  <Switch on={p.cleanStart} label={t('conn.cleanStart')} onChange={(cleanStart) => update({ cleanStart })} />
                </span>
              </div>
              <div className="f">
                <label htmlFor="pf-se">{t('conn.sessionExpiry')}</label>
                <span className="row2">
                  <input {...NO_AUTO} id="pf-se" className="input mono w-s" inputMode="numeric" value={p.sessionExpiry} onChange={(e) => update({ sessionExpiry: Number(e.target.value.replace(/\D/g, '')) || 0 })} />
                  <span className="hint">{t('conn.sessionExpiryHint')}</span>
                </span>
              </div>
              {/* change set 6: per-connection skip of certificate verification; turning it on asks "only this time" or "always" */}
              {/* the label is longer than the label column in every language: it follows the switch, like "Clean start" */}
              <div className="f">
                <span className="lbl" />
                <span className="row2">
                  <Switch
                    on={p.tls.skipVerify}
                    tone="clay"
                    label={t('conn.skipVerify')}
                    testId="skip-verify"
                    onChange={(v) => {
                      if (v) setSkipPrompt(true);
                      else {
                        updateTls({ skipVerify: false });
                        setSkipOnce(p.id, false);
                      }
                    }}
                  />
                  <span className="muted">{t('conn.skipVerify')}</span>
                  <Info id="skipVerify" title={t('conn.skipVerify')} />
                  {skipOnce && !p.tls.skipVerify && (
                    <span className="muted" data-testid="skip-once">
                      {t('conn.skipOnceNote')}
                    </span>
                  )}
                </span>
              </div>
              <div className="f">
                <span className="lbl">{t('conn.lastWill')}</span>
                <span className="row2">
                  <input {...NO_AUTO}
                    className="input mono w-l"
                    aria-label={t('conn.lastWillTopic')}
                    placeholder={t('conn.lastWillTopic')}
                    value={p.lastWill?.topic ?? ''}
                    onChange={(e) =>
                      update({ lastWill: e.target.value || p.lastWill?.payload ? { qos: 0, retain: false, payload: '', ...p.lastWill, topic: e.target.value } : null })
                    }
                  />
                  <input {...NO_AUTO}
                    className="input mono w-l"
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
      {skipPrompt && (
        <SkipVerifyDialog
          onCancel={() => setSkipPrompt(false)}
          onOnce={() => {
            setSkipPrompt(false);
            setSkipOnce(p.id, true);
          }}
          onAlways={() => {
            setSkipPrompt(false);
            setSkipOnce(p.id, false);
            updateTls({ skipVerify: true });
          }}
        />
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

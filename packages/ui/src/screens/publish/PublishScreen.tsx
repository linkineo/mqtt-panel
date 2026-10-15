import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { PayloadEncoding, PublishRequest, Qos, SentEntry, Template, UserMessage } from '@mqttpanel/engine-client';
import { engine, platform } from '../../services';
import { useApp, useActiveConnection } from '../../store/app';
import { usePublish, buildRequest, composerFromRequest, findPlaceholders, fillPlaceholders, propsSet, templateFolders, EMPTY_COMPOSER } from '../../store/publish';
import { reportSave, run } from '../../lib/actions';
import { cliCommand } from '../../lib/topicActions';
import { shortcut } from '../../lib/keys';
import { Info, Kbd, Modal, Msg, PromptDialog, ConfirmDialog, Seg, Switch, NO_AUTO } from '../../components/ui';
import { PayloadEditor } from '../../components/PayloadEditor';
import { NoConnection } from '../../components/NoConnection';
import { DeleteRetainedDialog } from '../../components/DeleteRetainedDialog';
import { fmtNum, fmtTime, shortTopic } from '../../lib/format';
import { renderMessage } from '../../i18n';

const NO_SENT: SentEntry[] = [];

export function PublishScreen() {
  const conn = useActiveConnection();
  if (!conn) return <NoConnection />;
  return <Publish key={conn.id} connId={conn.id} />;
}

export function TopicInput({ connId, value, onChange }: { connId: string; value: string; onChange(v: string): void }) {
  const { t } = useTranslation();
  const [list, setList] = useState<string[]>([]);
  const [open, setOpen] = useState(false);
  const [i, setI] = useState(0);
  useEffect(() => {
    if (!open) return;
    const h = setTimeout(() => {
      void engine()
        .knownTopics(connId, value, 12)
        .then((l) => setList(l.filter((x) => x !== value)))
        .catch(() => setList([]));
    }, 80);
    return () => clearTimeout(h);
  }, [connId, value, open]);
  return (
    <div style={{ position: 'relative', minWidth: 0 }}>
      <input {...NO_AUTO}
        className="input mono"
        style={{ width: '100%' }}
        value={value}
        placeholder="building/f2/r204/setpoint"
        aria-label={t('publish.topic')}
        aria-autocomplete="list"
        aria-expanded={open && list.length > 0}
        data-testid="pub-topic"
        onChange={(e) => {
          onChange(e.target.value);
          setOpen(true);
          setI(0);
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => setTimeout(() => setOpen(false), 120)}
        onKeyDown={(e) => {
          if (!open || !list.length) return;
          if (e.key === 'ArrowDown') {
            e.preventDefault();
            setI((i + 1) % list.length);
          } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            setI((i - 1 + list.length) % list.length);
          } else if (e.key === 'Enter' || e.key === 'Tab') {
            if (list[i]) {
              e.preventDefault();
              onChange(list[i]);
              setOpen(false);
            }
          } else if (e.key === 'Escape') setOpen(false);
        }}
      />
      {open && list.length > 0 && (
        <div className="autocomplete" role="listbox">
          {list.map((x, j) => (
            <div
              key={x}
              role="option"
              aria-selected={j === i}
              className={j === i ? 'on' : undefined}
              onMouseDown={(e) => {
                e.preventDefault();
                onChange(x);
                setOpen(false);
              }}
            >
              {x}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export function statusChip(e: SentEntry, t: (k: string, o?: Record<string, unknown>) => string, msg: (m: UserMessage | null) => string): { label: string; tone: string } {
  switch (e.status) {
    case 'pending':
      return { label: t('sent.pending'), tone: '' };
    case 'sent':
      return { label: t('sent.sent'), tone: '' };
    case 'acked':
      if (e.reply) return { label: t('sent.ackedReplied'), tone: 'on' };
      if (e.replyTimedOut) return { label: t('sent.ackedNoReply'), tone: 'clay' };
      return { label: t('sent.acked'), tone: 'on' };
    case 'failed':
      return { label: t('sent.failed', { reason: msg(e.reason) }), tone: 'brick' };
    case 'repeat':
      return { label: t('sent.repeat', { n: fmtNum(e.repeatCount ?? 0) }), tone: 'clay' };
    case 'deletedRetained':
      return { label: t('sent.deleted'), tone: '' };
  }
}

function entryRequest(e: SentEntry): PublishRequest {
  return {
    topic: e.topic,
    payload: e.payloadText,
    format: e.format,
    qos: e.qos,
    retain: e.retain,
    props: null,
    request: e.request ? { responseTopic: e.request.responseTopic, correlationData: '', waitS: 10 } : null,
  };
}

function Publish({ connId }: { connId: string }) {
  const { t } = useTranslation();
  const info = useApp((s) => s.appInfo);
  const conn = useActiveConnection()!;
  const profile = useApp((s) => s.profiles.find((p) => p.id === connId) ?? null);
  const prefill = useApp((s) => s.publishPrefill);
  const repeat = useApp((s) => s.repeats.find((r) => r.connectionId === connId) ?? null);
  const c = usePublish((s) => s.composer);
  const set = usePublish((s) => s.setComposer);
  const sent = usePublish((s) => s.sent[connId] ?? NO_SENT);
  const selectedId = usePublish((s) => s.selectedSent);
  const [templates, setTemplates] = useState<Template[]>([]);
  const [validation, setValidation] = useState<{ ok: true; size: number } | { ok: false; error: UserMessage } | null>(null);
  const [saveTpl, setSaveTpl] = useState<PublishRequest | null>(null);
  const [saveTplRepeat, setSaveTplRepeat] = useState<number | null>(null);
  const [renameTpl, setRenameTpl] = useState<{ tpl: Template; field: 'name' | 'folder' } | null>(null);
  const [deleteTpl, setDeleteTpl] = useState<Template | null>(null);
  const [delRet, setDelRet] = useState(false);
  const [placeholders, setPlaceholders] = useState<{ names: string[]; values: Record<string, string>; then: (v: Record<string, string>) => void } | null>(null);
  const [confirmClearLog, setConfirmClearLog] = useState(false);
  const mqtt5 = conn.negotiatedVersion === '5';
  const now = Date.now();

  const loadTemplates = async () => {
    if (!profile) return;
    const l = await run(engine().listTemplates(profile.id));
    if (l) setTemplates(l);
  };
  useEffect(() => {
    void loadTemplates();
    void engine()
      .sentLog(connId)
      .then((l) => usePublish.getState().setSent(connId, l))
      .catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connId]);
  useEffect(
    () =>
      useApp.getState().onSent((updates) => {
        updates.forEach((u) => usePublish.getState().upsertSent(u.connectionId, u.entry));
      }),
    [],
  );

  // "Publish here" from the Explorer
  useEffect(() => {
    if (!prefill) return;
    usePublish.getState().setComposer({ ...EMPTY_COMPOSER, topic: prefill.topic, payload: prefill.payload, format: prefill.format });
    useApp.setState({ publishPrefill: null });
  }, [prefill]);

  // validation line (P-08)
  useEffect(() => {
    const h = setTimeout(() => {
      void engine()
        .encodePayload(c.format, c.payload)
        .then(setValidation)
        .catch(() => setValidation(null));
    }, 120);
    return () => clearTimeout(h);
  }, [c.format, c.payload]);

  const withPlaceholders = (req: PublishRequest, then: (r: PublishRequest) => void) => {
    const names = findPlaceholders(req.topic, req.payload);
    if (!names.length) return then(req);
    setPlaceholders({
      names,
      values: Object.fromEntries(names.map((n) => [n, ''])),
      then: (v) => then({ ...req, topic: fillPlaceholders(req.topic, v), payload: fillPlaceholders(req.payload, v) }),
    });
  };

  const doSend = async (req: PublishRequest, clear = false) => {
    if (!req.topic) {
      useApp.getState().toast({ kind: 'warn', text: t('publish.needTopic') });
      return;
    }
    const e = await run(engine().publish(connId, req));
    if (!e) return;
    usePublish.getState().upsertSent(connId, e);
    usePublish.getState().select(e.id);
    if (e.status === 'failed') useApp.getState().toast({ kind: 'err', msg: e.reason });
    if (clear) usePublish.getState().clear();
  };
  const send = (clear = false) => withPlaceholders(buildRequest(usePublish.getState().composer, mqtt5), (r) => void doSend(r, clear));

  const startRepeat = () => {
    const s = Number(c.repeatS);
    if (!(s > 0)) return;
    withPlaceholders(buildRequest(c, mqtt5), async (r) => {
      if (!r.topic) return;
      await run(engine().startRepeat(connId, r, s, c.repeatCount ? Number(c.repeatCount) || null : null));
    });
  };

  const folders = useMemo(() => {
    const m = new Map<string, Template[]>();
    templates.forEach((x) => m.set(x.folder || '', [...(m.get(x.folder || '') ?? []), x]));
    return Array.from(m.entries());
  }, [templates]);

  const tplMenu = async (tpl: Template, e: React.MouseEvent) => {
    e.preventDefault();
    const id = await platform().showContextMenu(
      [
        { id: 'load', label: t('tpl.load') },
        { id: 'rename', label: t('common.rename') },
        { id: 'folder', label: t('tpl.moveFolder') },
        { id: 'dup', label: t('tpl.duplicate') },
        { id: 's', label: '', separator: true },
        { id: 'delete', label: t('tpl.delete'), danger: true },
      ],
      e.clientX,
      e.clientY,
    );
    if (!profile || !id) return;
    if (id === 'load') set(composerFromRequest(tpl.request, tpl.repeatS));
    else if (id === 'rename') setRenameTpl({ tpl, field: 'name' });
    else if (id === 'folder') setRenameTpl({ tpl, field: 'folder' });
    else if (id === 'dup') {
      await run(engine().saveTemplate(profile.id, { ...tpl, id: '', name: tpl.name + t('tpl.copySuffix') }));
      await loadTemplates();
    } else if (id === 'delete') setDeleteTpl(tpl);
  };

  const selected = sent.find((x) => x.id === selectedId) ?? null;
  const msgText = (m: UserMessage | null) => renderMessage(t, m);
  const repeatRunning = repeat?.running;

  return (
    <div className="cols" style={{ gridTemplateColumns: '232px minmax(0,1fr) 326px' }}>
      {/* templates */}
      <div className="pane side" data-testid="templates">
        <div className="ph">
          <h5>{t('tpl.title')}</h5>
          <span className="muted small nowrap">{t('tpl.onRequest')}</span>
        </div>
        <div className="pc">
          {folders.map(([folder, list], fi) => (
            <div key={folder} style={{ marginTop: fi ? 14 : 0 }}>
              {folder && <div className="sec" style={{ marginTop: 0 }}>{folder}</div>}
              <div className="list" style={{ marginTop: 6 }}>
                {list.map((tpl) => {
                  const running = repeatRunning && repeat?.topic === tpl.request.topic;
                  return (
                    <button
                      key={tpl.id}
                      type="button"
                      className={`card${running ? ' on' : ''}`}
                      data-testid="template"
                      onClick={() => set(composerFromRequest(tpl.request, tpl.repeatS))}
                      onContextMenu={(e) => void tplMenu(tpl, e)}
                    >
                      <div className="t">
                        <span>{tpl.name}</span>
                        {running ? <span className="chip clay">{t('tpl.running')}</span> : tpl.repeatS ? <span className="chip clay">{t('tpl.repeat', { s: tpl.repeatS })}</span> : null}
                      </div>
                      <div className="s mono">{tpl.request.topic}</div>
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
          <div className="muted small" style={{ marginTop: 14 }}>
            {t('tpl.note')}
          </div>
        </div>
      </div>

      {/* composer */}
      <div className="pane" data-testid="composer">
        <div className="ph">
          <h5>
            {t('publish.compose')} <span className="muted" style={{ fontWeight: 500 }}>· {t('publish.oneOff')}</span>
          </h5>
          <div className="row" style={{ gap: 4 }}>
            <button type="button" className="btn sm" onClick={() => usePublish.getState().clear()}>
              {t('publish.clear')}
            </button>
            <button
              type="button"
              className="btn sm"
              data-testid="save-template"
              onClick={() => {
                setSaveTplRepeat(Number(c.repeatS) || null);
                setSaveTpl(buildRequest(c, mqtt5));
              }}
            >
              {t('publish.saveTemplate')}
            </button>
            <button
              type="button"
              className="btn sm"
              onClick={async () => {
                await platform().copyText(cliCommand(profile, c.topic, c.payload, c.qos, c.retain));
                useApp.getState().toast({ kind: 'info', text: t('common.copied') });
              }}
            >
              {t('publish.copyCli')}
            </button>
            {/* "Send ⌘↵" top right in the composer header, like Connect on the Connections screen (owner, 13 Oct 2026) */}
            <button type="button" className="btn pri" data-testid="send" onClick={() => send()}>
              {t('publish.send')} <Kbd>{shortcut(info, 'mod+enter')}</Kbd>
            </button>
          </div>
        </div>
        <div className="pc">
          <div className="form narrow pub">
            <div className="f">
              <span className="lbl">{t('publish.topic')}</span>
              <TopicInput connId={connId} value={c.topic} onChange={(topic) => set({ topic })} />
            </div>
            <div className="f">
              <span className="lbl">{t('publish.options')}</span>
              <span className="row2 wrap" style={{ rowGap: 6 }}>
                <span className="muted">QoS</span>
                <Seg<Qos> label="QoS" value={c.qos} onChange={(qos) => set({ qos })} options={([0, 1, 2] as Qos[]).map((q) => ({ value: q, label: String(q), testId: `pub-qos-${q}` }))} />
                <span className="muted">{t('detail.retain')}</span>
                <Switch on={c.retain} label={t('detail.retain')} onChange={(retain) => set({ retain })} testId="pub-retain" />
                <span className="muted">{t('publish.format')}</span>
                <Seg<PayloadEncoding>
                  label={t('publish.format')}
                  value={c.format}
                  onChange={(format) => set({ format })}
                  options={(['json', 'text', 'hex', 'base64'] as PayloadEncoding[]).map((f) => ({ value: f, label: t(`encoding.${f}`), testId: `pub-format-${f}` }))}
                />
              </span>
            </div>
          </div>
          <div style={{ marginTop: 12 }}>
            <PayloadEditor value={c.payload} format={c.format} label={t('publish.payload')} onChange={(payload) => set({ payload })} onSend={() => send()} placeholder={c.format === 'json' ? '{"value": 1}' : ''} />
          </div>
          <div className={`small ${validation && !validation.ok ? 'hint err' : 'muted'}`} style={{ marginTop: 6 }} data-testid="validation">
            {validation === null
              ? ' '
              : c.payload === ''
                ? t('publish.empty')
                : validation.ok
                ? c.format === 'json'
                  ? t('publish.validJson', { n: fmtNum(validation.size) })
                  : t('publish.bytes', { n: fmtNum(validation.size) })
                : msgText(validation.error)}
          </div>

          {mqtt5 ? (
            <div data-testid="request-section">
              <div className="sec" style={{ marginTop: 14 }}>
                {t('publish.request')} <span className="chip on">MQTT 5</span> <Info id="request" title={t('publish.request')} />
              </div>
              <div className="form narrow pub" style={{ marginTop: 8 }}>
                <div className="f">
                  <label htmlFor="pub-resp">{t('publish.responseTopic')}</label>
                  <span className="row2">
                    <input {...NO_AUTO} id="pub-resp" className="input mono" style={{ flex: 1 }} value={c.responseTopic} placeholder={t('common.optional')} onChange={(e) => set({ responseTopic: e.target.value })} />
                    <span className="hint">{t('publish.subscribedAuto')}</span>
                  </span>
                </div>
                <div className="f">
                  <label htmlFor="pub-corr">{t('publish.correlation')}</label>
                  <span className="row2">
                    <input {...NO_AUTO} id="pub-corr" className="input mono" style={{ width: 140 }} value={c.correlation} placeholder={t('publish.auto')} onChange={(e) => set({ correlation: e.target.value })} />
                    <span className="hint">{t('publish.waitUpTo')}</span>
                    <input {...NO_AUTO} className="input mono" style={{ width: 60 }} inputMode="numeric" aria-label={t('publish.waitUpTo')} value={c.waitS} onChange={(e) => set({ waitS: Number(e.target.value.replace(/\D/g, '')) || 0 })} />
                    <span className="hint">s</span>
                  </span>
                </div>
              </div>
              <button type="button" className="sec" style={{ marginTop: 14, background: 'none', border: 0, padding: 0 }} aria-expanded={c.propsOpen} onClick={() => set({ propsOpen: !c.propsOpen })} data-testid="props-toggle">
                {c.propsOpen ? '▾' : '▸'} {t('publish.props')} <span className="lower">· {t('publish.propsSet', { n: propsSet(c) })}</span>
              </button>
              {c.propsOpen && (
                <div className="form narrow pub" style={{ marginTop: 8 }}>
                  <div className="f">
                    <label htmlFor="pub-ct">{t('props.contentType')}</label>
                    <input {...NO_AUTO} id="pub-ct" className="input mono" value={c.contentType} placeholder="application/json" onChange={(e) => set({ contentType: e.target.value })} />
                  </div>
                  <div className="f">
                    <span className="lbl">{t('props.payloadFormat')}</span>
                    <Seg<string>
                      label={t('props.payloadFormat')}
                      value={c.payloadFormat === null ? 'none' : String(c.payloadFormat)}
                      onChange={(v) => set({ payloadFormat: v === 'none' ? null : Number(v) })}
                      options={[
                        { value: 'none', label: t('publish.unset') },
                        { value: '0', label: t('publish.pfBytes') },
                        { value: '1', label: 'UTF-8' },
                      ]}
                    />
                  </div>
                  <div className="f">
                    <span className="lbl">{t('props.userProps')}</span>
                    <span style={{ display: 'grid', gap: 6 }}>
                      {[...c.userProps, ['', ''] as [string, string]].map(([k, v], i) => (
                        <span key={i} className="row2">
                          <input {...NO_AUTO}
                            className="input mono"
                            style={{ width: 120 }}
                            value={k}
                            placeholder={t('publish.key')}
                            aria-label={t('publish.key')}
                            onChange={(e) => {
                              const n = [...c.userProps];
                              n[i] = [e.target.value, n[i]?.[1] ?? ''];
                              set({ userProps: n.filter(([a, b], j) => a || b || j < n.length - 1) });
                            }}
                          />
                          <input {...NO_AUTO}
                            className="input mono"
                            style={{ flex: 1 }}
                            value={v}
                            placeholder={t('publish.value')}
                            aria-label={t('publish.value')}
                            onChange={(e) => {
                              const n = [...c.userProps];
                              n[i] = [n[i]?.[0] ?? '', e.target.value];
                              set({ userProps: n });
                            }}
                          />
                          {i < c.userProps.length ? (
                            <button type="button" className="btn sm" aria-label={t('common.remove')} onClick={() => set({ userProps: c.userProps.filter((_, j) => j !== i) })}>
                              ✕
                            </button>
                          ) : (
                            <span className="btn sm" aria-hidden="true" style={{ visibility: 'hidden' }}>
                              ✕
                            </span>
                          )}
                        </span>
                      ))}
                    </span>
                  </div>
                  <div className="f">
                    <label htmlFor="pub-exp">{t('publish.expiry')}</label>
                    <span className="row2">
                      <input {...NO_AUTO} id="pub-exp" className="input mono" style={{ width: 100 }} inputMode="numeric" value={c.expiry} placeholder={t('publish.never')} onChange={(e) => set({ expiry: e.target.value.replace(/\D/g, '') })} />
                      <span className="hint">s</span>
                    </span>
                  </div>
                </div>
              )}
            </div>
          ) : (
            <div className="hint" style={{ marginTop: 14 }} data-testid="request-unavailable">
              <Msg msg={{ key: 'request.needsMqtt5', params: {}, text: 'Request and response needs MQTT 5; this broker negotiated 3.1.1.' }} />
            </div>
          )}

          <div className="row" style={{ marginTop: 18, flexWrap: 'wrap', rowGap: 8 }}>
            <button type="button" className="btn" data-testid="send-clear" onClick={() => send(true)}>
              {t('publish.sendClear')}
            </button>
            <span className="muted nowrap">{t('publish.repeatEvery')}</span>
            <input {...NO_AUTO} className="input mono" style={{ width: 52 }} inputMode="numeric" placeholder="—" aria-label={t('publish.repeatEvery')} value={c.repeatS} onChange={(e) => set({ repeatS: e.target.value.replace(/[^\d.]/g, '') })} data-testid="repeat-s" />
            <span className="muted">s</span>
            {c.repeatS && !repeatRunning && (
              <>
                <input {...NO_AUTO} className="input mono" style={{ width: 52 }} inputMode="numeric" placeholder="∞" aria-label={t('publish.repeatCount')} title={t('publish.repeatCount')} value={c.repeatCount} onChange={(e) => set({ repeatCount: e.target.value.replace(/\D/g, '') })} />
                <span className="muted">×</span>
                <button type="button" className="btn sm" data-testid="repeat-start" onClick={startRepeat}>
                  {t('publish.start')}
                </button>
              </>
            )}
            <span className="grow" />
            <button type="button" className="btn danger" data-testid="delete-retained-btn" disabled={!c.topic.trim()} onClick={() => setDelRet(true)}>
              {t('menu.deleteRetained')}
            </button>
          </div>
          {repeat && (repeat.running || repeat.stoppedReason) && (
            <div className={`callout${repeat.running ? '' : ' warn'}`} style={{ marginTop: 14, display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 10 }} data-testid="repeat-callout">
              {repeat.running ? (
                <>
                  <span className="dot" />
                  <strong className="nowrap">{t('repeat.every', { s: repeat.intervalS })}</strong>
                  <span className="muted small nowrap">
                    {t('repeat.status', {
                      n: fmtNum(repeat.sent),
                      next: repeat.nextAt ? Math.max(0, Math.ceil((repeat.nextAt - now) / 1000)) : 0,
                    })}
                  </span>
                  <span className="grow" />
                  <button type="button" className="btn danger" data-testid="repeat-stop" onClick={() => void run(engine().stopRepeat(connId))}>
                    {t('repeat.stop')}
                  </button>
                </>
              ) : (
                <Msg msg={repeat.stoppedReason} />
              )}
            </div>
          )}
        </div>
      </div>

      {/* sent log */}
      <div className="pane right" data-testid="sent-log">
        <div className="ph">
          <h5>{t('sent.title', { n: fmtNum(sent.length) })}</h5>
          <div className="row" style={{ gap: 6 }}>
            <button
              type="button"
              className="btn sm"
              onClick={async () => {
                const out = await run(engine().exportSentLog(connId));
                if (out) reportSave(out, t);
              }}
            >
              {t('sent.export')}
            </button>
            <button type="button" className="btn sm" onClick={() => setConfirmClearLog(true)}>
              {t('publish.clear')}
            </button>
          </div>
        </div>
        <div className="pc" style={{ padding: 0, display: 'flex', flexDirection: 'column' }}>
          <div style={{ flex: 1, overflow: 'auto', minHeight: 0 }}>
            <table className="tbl fixed">
              <colgroup>
                <col style={{ width: 74 }} />
                <col />
                <col style={{ width: 38 }} />
                <col style={{ width: 140 }} />
              </colgroup>
              <thead>
                <tr>
                  <th>{t('col.time')}</th>
                  <th>{t('col.topic')}</th>
                  <th>QoS</th>
                  <th>{t('sent.status')}</th>
                </tr>
              </thead>
              <tbody>
                {sent.map((e) => {
                  const chip = statusChip(e, t, msgText);
                  return (
                    <tr key={e.id} className={`click${e.id === selectedId ? ' sel' : ''}`} data-testid="sent-row" onClick={() => usePublish.getState().select(e.id)}>
                      <td className="mono">{fmtTime(e.at)}</td>
                      <td className="mono" title={e.topic}>
                        {shortTopic(e.topic)}
                      </td>
                      <td>
                        {e.qos}
                        {e.retain ? ' R' : ''}
                      </td>
                      <td>
                        <span className={`chip ${chip.tone}`} title={chip.label} style={{ maxWidth: '100%', overflow: 'hidden', textOverflow: 'ellipsis', display: 'inline-block' }} data-testid="sent-status" data-overflow-ok>
                          {chip.label}
                        </span>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
            {sent.length === 0 && <div className="empty small">{t('sent.empty')}</div>}
          </div>
          {selected && (
            <div style={{ padding: '10px 14px', borderTop: '1px solid var(--line)' }} data-testid="sent-detail">
              <div className="sec" style={{ marginTop: 0 }}>
                {t('sent.selected', { time: fmtTime(selected.at) })}
              </div>
              <div className="json wrap" style={{ marginTop: 6, fontSize: 11, maxHeight: 90 }}>
                {selected.payloadText || ' '}
              </div>
              {selected.reply && (
                <div className="reply selectable" style={{ marginTop: 8 }}>
                  <strong>{t('sent.replyIn', { ms: selected.reply.rttMs })}</strong> · {t('sent.correlation', { c: selected.reply.correlationData })}
                  <br />
                  <span className="mono">{selected.reply.payloadText}</span>
                </div>
              )}
              {selected.replyTimedOut && (
                <div className="hint warn" style={{ marginTop: 8 }}>
                  <Msg msg={{ key: 'request.timeout', params: { seconds: 10 }, text: 'No reply.' }} />
                </div>
              )}
              <div className="row" style={{ gap: 6, marginTop: 8, flexWrap: 'wrap' }}>
                <button type="button" className="btn sm" data-testid="resend" onClick={() => void doSend(entryRequest(selected))}>
                  {t('sent.resend')}
                </button>
                <button type="button" className="btn sm" onClick={() => set(composerFromRequest(entryRequest(selected)))}>
                  {t('sent.load')}
                </button>
                <button type="button" className="btn sm" data-testid="sent-save-template" onClick={() => {
                  setSaveTplRepeat(null);
                  setSaveTpl(entryRequest(selected));
                }}>
                  {t('publish.saveTemplate')}
                </button>
                <button
                  type="button"
                  className="btn sm"
                  onClick={async () => {
                    await platform().copyText(cliCommand(profile, selected.topic, selected.payloadText, selected.qos, selected.retain));
                    useApp.getState().toast({ kind: 'info', text: t('common.copied') });
                  }}
                >
                  {t('publish.copyCli')}
                </button>
              </div>
            </div>
          )}
        </div>
      </div>

      {saveTpl && profile && (
        <PromptDialog
          title={t('tpl.saveTitle')}
          label={t('tpl.name')}
          confirmLabel={t('common.save')}
          extra={{ label: t('tpl.folder'), initial: templateFolders(templates)[0] ?? '', suggestions: templateFolders(templates) }}
          onConfirm={() => undefined}
          onConfirmExtra={async (name, folder) => {
            await run(engine().saveTemplate(profile.id, { id: '', name, folder, request: saveTpl, repeatS: saveTplRepeat }));
            await loadTemplates();
            useApp.getState().toast({ kind: 'info', text: t('tpl.saved', { name }) });
          }}
          onClose={() => setSaveTpl(null)}
          testId="save-template-dialog"
        />
      )}
      {renameTpl && profile && (
        <PromptDialog
          title={renameTpl.field === 'name' ? t('tpl.renameTitle') : t('tpl.folderTitle')}
          label={renameTpl.field === 'name' ? t('tpl.name') : t('tpl.folder')}
          initial={renameTpl.tpl[renameTpl.field]}
          confirmLabel={t('common.save')}
          onConfirm={async (v) => {
            await run(engine().saveTemplate(profile.id, { ...renameTpl.tpl, [renameTpl.field]: v }));
            await loadTemplates();
          }}
          onClose={() => setRenameTpl(null)}
        />
      )}
      {deleteTpl && profile && (
        <ConfirmDialog
          title={t('tpl.deleteTitle', { name: deleteTpl.name })}
          confirmLabel={t('common.delete')}
          danger
          onClose={() => setDeleteTpl(null)}
          onConfirm={async () => {
            await run(engine().deleteTemplate(profile.id, deleteTpl.id));
            await loadTemplates();
          }}
        />
      )}
      {confirmClearLog && (
        <ConfirmDialog
          title={t('sent.clearTitle')}
          confirmLabel={t('publish.clear')}
          danger
          onClose={() => setConfirmClearLog(false)}
          onConfirm={async () => {
            await run(engine().clearSentLog(connId));
            usePublish.getState().setSent(connId, []);
          }}
        />
      )}
      {delRet && <DeleteRetainedDialog connId={connId} topic={c.topic.trim()} onClose={() => setDelRet(false)} />}
      {placeholders && (
        <Modal
          title={t('placeholders.title')}
          width={460}
          onClose={() => setPlaceholders(null)}
          testId="placeholders"
          footer={
            <>
              <button type="button" className="btn" onClick={() => setPlaceholders(null)}>
                {t('common.cancel')}
              </button>
              <button
                type="button"
                className="btn pri"
                data-testid="placeholders-ok"
                onClick={() => {
                  placeholders.then(placeholders.values);
                  setPlaceholders(null);
                }}
              >
                {t('publish.send')}
              </button>
            </>
          }
        >
          <div className="muted small">{t('placeholders.help')}</div>
          <div className="form">
            {placeholders.names.map((n, i) => (
              <div key={n} className="f" style={{ gridTemplateColumns: '120px 1fr' }}>
                <label htmlFor={`ph-${n}`} className="mono">{`{${n}}`}</label>
                <input {...NO_AUTO}
                  id={`ph-${n}`}
                  className="input mono"
                  data-autofocus={i === 0 ? true : undefined}
                  value={placeholders.values[n]}
                  onChange={(e) => setPlaceholders({ ...placeholders, values: { ...placeholders.values, [n]: e.target.value } })}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      placeholders.then(placeholders.values);
                      setPlaceholders(null);
                    }
                  }}
                />
              </div>
            ))}
          </div>
        </Modal>
      )}
    </div>
  );
}

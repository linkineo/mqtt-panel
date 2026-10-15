import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type {
  DiffResult,
  HistoryEntry,
  MessageView,
  NegotiatedVersion,
  MqttVersionPref,
  PayloadEncoding,
  PayloadFormat,
  Qos,
  SentEntry,
  SeriesData,
  TopicDetail,
} from '@mqttpanel/engine-client';
import { engine, platform } from '../../services';
import { useApp } from '../../store/app';
import { rangeMs } from '../../store/charts';
import { useExplorer, type DetailTab } from '../../store/explorer';
import { run } from '../../lib/actions';
import { addToChart, copyCli, copyPayload, copyTopic, clearHistory, payloadFormat, payloadText, publishHere, type TopicDialog } from '../../lib/topicActions';
import { JsonView, DiffView, FieldMenu, copyField, pathLabel, type FieldActions, type FieldClick } from '../../components/JsonView';
import { RailIcon } from '../../components/icons';
import { setChartFields, setSparklinePath, useTopicPrefs } from '../../lib/topicPrefs';
import { UChart } from '../../components/UChart';
import { Msg, PromptDialog, Seg, Switch, NO_AUTO } from '../../components/ui';
import { fmtBytes, fmtNum, fmtTime } from '../../lib/format';

const FORMATS: PayloadFormat[] = ['auto', 'json', 'text', 'hex', 'base64', 'image', 'cbor', 'msgpack', 'sparkplug', 'protobuf'];
const PAGE = 64 * 1024;

function useThrottled(value: number, ms: number): number {
  const [v, setV] = useState(value);
  const last = useRef(0);
  useEffect(() => {
    const wait = Math.max(0, ms - (Date.now() - last.current));
    const h = setTimeout(() => {
      last.current = Date.now();
      setV(value);
    }, wait);
    return () => clearTimeout(h);
  }, [value, ms]);
  return v;
}

export function countProps(m: MessageView | null): number {
  const p = m?.properties;
  if (!p) return 0;
  return (
    (p.contentType !== null ? 1 : 0) +
    (p.payloadFormat !== null ? 1 : 0) +
    (p.responseTopic !== null ? 1 : 0) +
    (p.correlationData !== null ? 1 : 0) +
    (p.messageExpiry !== null ? 1 : 0) +
    (p.userProperties.length ? 1 : 0) +
    (p.subscriptionIds.length ? 1 : 0) +
    (p.topicAlias !== null ? 1 : 0)
  );
}

/** Properties tab (mockup 07). The MQTT 5 block is absent, not empty, on 3.1.1 (X-13). */
export function PropertiesView(props: {
  message: MessageView;
  negotiated: NegotiatedVersion | null;
  requested: MqttVersionPref;
  reply?: SentEntry | null;
}) {
  const { t } = useTranslation();
  const { message: m } = props;
  const p = m.properties;
  const yn = (b: boolean) => (b ? t('common.yes') : t('common.no'));
  return (
    <div data-testid="properties">
      <dl className="kv" style={{ gridTemplateColumns: '170px 1fr' }}>
        <dt>{t('props.protocol')}</dt>
        <dd>
          MQTT {props.negotiated ?? '?'}{' '}
          <span className="muted">{t('props.negotiated', { asked: props.requested })}</span>
        </dd>
        <dt>{t('props.received')}</dt>
        <dd>{fmtTime(m.receivedAt, true)}</dd>
        <dt>QoS</dt>
        <dd>{m.qos}</dd>
        <dt>{t('props.retained')}</dt>
        <dd>{yn(m.retain)}</dd>
        <dt>{t('props.duplicate')}</dt>
        <dd>{yn(m.dup)}</dd>
        <dt>{t('props.size')}</dt>
        <dd>
          {fmtBytes(m.size)}
          {m.truncated ? ` (${t('props.truncated')})` : ''}
        </dd>
      </dl>
      {p && (
        <div data-testid="mqtt5-props">
          <div className="sec" style={{ marginTop: 14 }}>
            {t('props.mqtt5')}
          </div>
          <dl className="kv" style={{ gridTemplateColumns: '170px 1fr', marginTop: 6 }}>
            {p.contentType !== null && (
              <>
                <dt>{t('props.contentType')}</dt>
                <dd>{p.contentType}</dd>
              </>
            )}
            {p.payloadFormat !== null && (
              <>
                <dt>{t('props.payloadFormat')}</dt>
                <dd>
                  {p.payloadFormat} ({p.payloadFormat === 1 ? 'UTF-8' : t('props.bytes')})
                </dd>
              </>
            )}
            {p.responseTopic !== null && (
              <>
                <dt>{t('props.responseTopic')}</dt>
                <dd>{p.responseTopic}</dd>
              </>
            )}
            {p.correlationData !== null && (
              <>
                <dt>{t('props.correlation')}</dt>
                <dd>{p.correlationData}</dd>
              </>
            )}
            {p.messageExpiry !== null && (
              <>
                <dt>{t('props.expiry')}</dt>
                <dd>{p.messageExpiry} s</dd>
              </>
            )}
            {p.userProperties.length > 0 && (
              <>
                <dt>{t('props.userProps')}</dt>
                <dd>
                  {p.userProperties.map(([k, v], i) => (
                    <div key={i}>
                      {k} = {v}
                    </div>
                  ))}
                </dd>
              </>
            )}
            {p.subscriptionIds.length > 0 && (
              <>
                <dt>{t('props.subIds')}</dt>
                <dd>{p.subscriptionIds.join(', ')}</dd>
              </>
            )}
            {p.topicAlias !== null && (
              <>
                <dt>{t('props.topicAlias')}</dt>
                <dd>{p.topicAlias}</dd>
              </>
            )}
            {countProps(m) === 0 && (
              <>
                <dt className="muted">{t('props.noneSet')}</dt>
                <dd />
              </>
            )}
          </dl>
        </div>
      )}
      {props.reply?.reply && (
        <div className="reply selectable" style={{ marginTop: 14 }} data-testid="reply-box">
          <strong>{t('props.replyReceived')}</strong>{' '}
          {t('props.replyDetail', { topic: props.reply.request?.responseTopic ?? '', ms: props.reply.reply.rttMs, correlation: props.reply.reply.correlationData })}{' '}
          <span className="mono">{props.reply.reply.payloadText}</span>
        </div>
      )}
    </div>
  );
}

function PayloadBody({ m, onFormatAnyway, actions }: { m: MessageView; onFormatAnyway(): void; actions: FieldActions }) {
  const { t } = useTranslation();
  const d = m.decoded;
  const [page, setPage] = useState(0);
  useEffect(() => setPage(0), [m.seq]);
  if (d.paged) {
    const text = d.text ?? d.hex ?? '';
    const pages = Math.max(1, Math.ceil(text.length / PAGE));
    return (
      <div data-testid="paged">
        <div className="row" style={{ marginBottom: 8 }}>
          <span className="hint">{t('payload.paged', { size: fmtBytes(m.size) })}</span>
          <span className="grow" />
          <button type="button" className="btn sm" onClick={onFormatAnyway} data-testid="format-anyway">
            {t('payload.formatAnyway')}
          </button>
        </div>
        <div className="json wrap" style={{ maxHeight: 360 }}>
          {text.slice(page * PAGE, (page + 1) * PAGE)}
        </div>
        <div className="row" style={{ marginTop: 6 }}>
          <button type="button" className="btn sm" disabled={page === 0} onClick={() => setPage(page - 1)}>
            ‹
          </button>
          <span className="hint">{t('payload.page', { page: page + 1, pages })}</span>
          <button type="button" className="btn sm" disabled={page >= pages - 1} onClick={() => setPage(page + 1)}>
            ›
          </button>
        </div>
      </div>
    );
  }
  return (
    <>
      {d.error && (
        <div className="hint err" style={{ marginBottom: 6, whiteSpace: 'normal' }}>
          <Msg msg={d.error} />
        </div>
      )}
      {d.json !== null && ['json', 'cbor', 'msgpack', 'sparkplug', 'protobuf'].includes(d.format) ? (
        <JsonView value={d.json} actions={actions} testId="payload-json" />
      ) : d.format === 'image' && d.imageDataUrl ? (
        <img src={d.imageDataUrl} alt={t('payload.image')} style={{ maxWidth: '100%', borderRadius: 8 }} draggable={false} />
      ) : d.format === 'hex' || (d.error && d.hex) ? (
        <div className="json">{d.hex}</div>
      ) : d.format === 'base64' ? (
        <div className="json wrap">{d.base64}</div>
      ) : (
        <div className="json wrap" data-testid="payload-text">
          {d.text ?? ''}
        </div>
      )}
    </>
  );
}

/** A JSON-ish payload: the diff is drawn key by key; anything else falls back to the line diff or the plain view. */
function diffable(m: MessageView, format: PayloadFormat): boolean {
  return format === 'auto' && !m.decoded.paged && !m.decoded.error && (m.decoded.json !== null || m.decoded.text !== null) && m.decoded.format !== 'image';
}

export function Detail(props: { connId: string; topic: string; openDialog(d: TopicDialog): void }) {
  const { t } = useTranslation();
  const { connId, topic } = props;
  const conn = useApp((s) => s.connections.find((c) => c.id === connId) ?? null);
  const profile = useApp((s) => s.profiles.find((p) => p.id === connId) ?? null);
  const seq = useApp((s) => s.seqs[connId] ?? 0);
  const mac = useApp((s) => s.appInfo.platform === 'macos' || (s.appInfo.platform === 'web' && /Mac/.test(navigator.platform)));
  const st = useExplorer((s) => s.get(connId));
  const patch = useExplorer((s) => s.patch);
  const prefs = useTopicPrefs(connId, topic);
  const tseq = useThrottled(seq, 250);
  const slowSeq = useThrottled(seq, 1000);

  const [detail, setDetail] = useState<TopicDetail | null>(null);
  const [frozen, setFrozen] = useState<TopicDetail | null>(null);
  const [format, setFormat] = useState<PayloadFormat>('auto');
  const [forced, setForced] = useState<MessageView | null>(null);
  const [anyway, setAnyway] = useState(false);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [diff, setDiff] = useState<DiffResult | null>(null);
  const [chartData, setChartData] = useState<Record<string, SeriesData>>({});
  const [sent, setSent] = useState<SentEntry[]>([]);
  const [qp, setQp] = useState<{ payload: string; format: PayloadEncoding; qos: Qos; retain: boolean }>({ payload: '', format: 'text', qos: 0, retain: false });
  const [raising, setRaising] = useState(false);
  const [menu, setMenu] = useState<FieldClick | null>(null);

  const tab = st.tab;
  const follow = st.follow;
  const viewSeq = st.viewSeq;
  const diffOn = st.diff;
  const fields = prefs.chartFields ?? [];

  // latest detail
  useEffect(() => {
    let live = true;
    void engine()
      .topicDetail(connId, topic)
      .then((d) => live && setDetail(d))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [connId, topic, tseq]);

  // reset per topic
  useEffect(() => {
    setFrozen(null);
    setFormat('auto');
    setForced(null);
    setAnyway(false);
    setDiff(null);
    setChartData({});
    setMenu(null);
  }, [connId, topic]);

  // quick publish follows the last message's format
  const qpTopic = useRef('');
  useEffect(() => {
    if (!detail || qpTopic.current === connId + topic) return;
    qpTopic.current = connId + topic;
    setQp({ payload: payloadText(detail.last), format: payloadFormat(detail.last), qos: detail.last?.qos ?? 0, retain: detail.last?.retain ?? false });
  }, [detail, connId, topic]);

  // pause freezes the pane
  useEffect(() => {
    if (!follow && detail && !frozen) setFrozen(detail);
    if (follow) setFrozen(null);
  }, [follow, detail, frozen]);
  const shown = follow ? detail : (frozen ?? detail);
  const missed = !follow && frozen && detail ? Math.max(0, detail.messages - frozen.messages) : 0;
  const shownSeq = viewSeq ?? shown?.last?.seq ?? null;

  // a forced format, a history entry or "format anyway"
  useEffect(() => {
    if (shownSeq === null || (format === 'auto' && viewSeq === null && !anyway)) {
      setForced(null);
      return;
    }
    let live = true;
    void engine()
      .message(connId, topic, shownSeq, format, anyway)
      .then((m) => live && setForced(m))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [connId, topic, shownSeq, format, anyway, viewSeq]);
  const message = forced ?? shown?.last ?? null;

  // history (the History section under the payload, and the diff baseline)
  useEffect(() => {
    if (!follow && history.length) return;
    let live = true;
    void engine()
      .history(connId, topic)
      .then((h) => live && setHistory(h))
      .catch(() => undefined);
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connId, topic, follow ? tseq : 0]);

  // "Show diff to previous" (change set 8): the message shown against the one received just before it
  const diffB = shownSeq;
  const baseEntry = diffB === null ? null : (history.find((h) => h.seq < diffB) ?? null);
  const diffA = baseEntry?.seq ?? null;
  const canDiff = !!message && diffable(message, format);
  useEffect(() => {
    if (tab !== 'payload' || !diffOn || !canDiff || diffA === null || diffB === null) {
      setDiff(null);
      return;
    }
    let live = true;
    void engine()
      .diff(connId, topic, diffA, diffB)
      .then((d) => live && setDiff(d))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [tab, diffOn, canDiff, connId, topic, diffA, diffB]);

  // the topic's own chart (change set 8): the fields saved in the profile, last 15 minutes
  const fieldsKey = fields.map((f) => f.path).join('|');
  useEffect(() => {
    if (tab !== 'payload' || !fields.length) return;
    let live = true;
    void Promise.all(fields.map((f) => engine().series(connId, topic, f.path, Date.now() - (rangeMs('15m') ?? 0)).catch(() => null))).then((list) => {
      if (!live) return;
      const out: Record<string, SeriesData> = {};
      list.forEach((d, i) => d && (out[fields[i].path] = d));
      setChartData(out);
    });
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab, connId, topic, fieldsKey, slowSeq]);

  // sent log, for the reply box in Properties
  useEffect(() => {
    if (tab !== 'properties') return;
    void engine()
      .sentLog(connId)
      .then(setSent)
      .catch(() => undefined);
  }, [tab, connId, slowSeq]);

  const openPicker = (path: string | null) => addToChart(connId, topic, path);
  const actions: FieldActions = { open: setMenu };
  const pinned = st.pinned.includes(topic);
  const nProps = countProps(message);
  const negotiated = conn?.negotiatedVersion ?? null;

  if (!shown) return <div className="pane right" />;

  const setTab = (tab: DetailTab) => patch(connId, { tab });
  const tabs: { id: DetailTab; label: string }[] = [
    { id: 'payload', label: t('detail.payload') },
    { id: 'properties', label: negotiated === '5' ? t('detail.propertiesN', { n: nProps }) : t('detail.properties') },
  ];

  const moreMenu = async (e: React.MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const id = await platform().showContextMenu(
      [
        { id: 'copyTopic', label: t('menu.copyTopic'), accelerator: mac ? '⌘C' : 'Ctrl+C' },
        { id: 'copyPayload', label: t('menu.copyPayload') },
        { id: 'copyCli', label: t('menu.copyCli') },
        { id: 'chart', label: t('menu.addToChart'), accelerator: 'C' },
        { id: 'export', label: t('menu.export') },
        { id: 's', label: '', separator: true },
        { id: 'delRet', label: t('menu.deleteRetained'), danger: true },
        { id: 'clear', label: t('menu.clearHistory') },
      ],
      r.left,
      r.bottom + 4,
    );
    if (id === 'copyTopic') void copyTopic(topic);
    else if (id === 'copyPayload') void copyPayload(connId, topic);
    else if (id === 'copyCli') void copyCli(connId, topic);
    else if (id === 'chart') openPicker(null);
    else if (id === 'export') props.openDialog({ kind: 'export', topic, scope: 'topic' });
    else if (id === 'delRet') props.openDialog({ kind: 'deleteRetained', topic });
    else if (id === 'clear') void clearHistory(connId, topic);
  };

  const formatMenu = async (e: React.MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const id = await platform().showContextMenu(
      FORMATS.map((f) => ({ id: f, label: (f === format ? '✓ ' : '') + t(`format.${f}`) })),
      r.left,
      r.bottom + 4,
    );
    if (id) setFormat(id as PayloadFormat);
  };

  const send = async () => {
    const e = await run(engine().publish(connId, { topic, payload: qp.payload, format: qp.format, qos: qp.qos, retain: qp.retain, props: null, request: null }));
    if (e?.status === 'failed') useApp.getState().toast({ kind: 'err', msg: e.reason });
    else if (e) useApp.getState().toast({ kind: 'info', text: t('detail.sent', { topic }) });
  };

  const useSparkline = async (path: string) => {
    await setSparklinePath(connId, topic, path);
    useApp.getState().toast({ kind: 'info', text: t('field.sparklineSet', { field: pathLabel(path) }) });
  };

  const replyFor = message?.properties?.correlationData
    ? (sent.find((s) => s.reply && (s.request?.correlationData === message.properties!.correlationData || s.reply.correlationData === message.properties!.correlationData)) ?? null)
    : null;

  const structured = message?.decoded.json !== null && typeof message?.decoded.json === 'object';
  const showDiff = diffOn && canDiff && diff !== null && diffA !== null;

  return (
    <div className="pane right" style={{ flex: 1 }} data-testid="detail">
      <div className="ph">
        <h5 className="mono selectable" style={{ fontWeight: 600 }} title={topic} data-overflow-ok>
          {topic}
        </h5>
        <div className="row" style={{ gap: 6 }}>
          <button
            type="button"
            className={`btn sm ico${pinned ? ' on' : ''}`}
            data-testid="pin-btn"
            aria-pressed={pinned}
            aria-label={pinned ? t('detail.pinnedTip') : t('detail.pin')}
            title={pinned ? t('detail.pinnedTip') : t('detail.pin')}
            onClick={() => void useExplorer.getState().togglePin(connId, topic)}
          >
            {pinned ? '★' : '☆'}
          </button>
          <button type="button" className="btn sm ico" data-testid="publish-here" aria-label={t('menu.publishHere')} title={t('menu.publishHere')} onClick={() => void publishHere(connId, topic)}>
            <RailIcon name="publish" />
          </button>
          <button type="button" className="btn sm ico" aria-label={t('detail.more')} title={t('detail.more')} onClick={(e) => void moreMenu(e)}>
            ⋯
          </button>
        </div>
      </div>
      {!shown.exists || !shown.last ? (
        <div className="empty small">{t('detail.noMessage')}</div>
      ) : (
        <>
          <div style={{ padding: '8px 14px' }} className="meta">
            <span>
              {t('detail.received')} <b>{fmtTime(shown.last.receivedAt, true)}</b>
            </span>
            <span>
              QoS <b>{shown.last.qos}</b>
            </span>
            {shown.last.retain && (
              <span>
                <b>{t('detail.retained')}</b>
              </span>
            )}
            <span>
              {t('detail.size')} <b>{fmtBytes(shown.last.size)}</b>
            </span>
            <span className="row" style={{ gap: 6 }}>
              {t('detail.follow')}
              <Switch on={follow} label={t('detail.follow')} testId="follow" onChange={(v) => patch(connId, { follow: v })} />
            </span>
            {!follow && <span className="chip clay" data-testid="missed">{t('detail.missed', { count: missed, n: fmtNum(missed) })}</span>}
          </div>
          {/* change set 8: one boxed tool with thin separators; no text label */}
          <div className="qp" role="group" aria-label={t('detail.quickPublish')} title={t('detail.quickPublish')} data-testid="quick-publish">
            <input {...NO_AUTO}
              className="input mono"
              style={{ flex: 1, minWidth: 60 }}
              value={qp.payload}
              aria-label={t('detail.quickPayload')}
              data-testid="qp-payload"
              onChange={(e) => setQp({ ...qp, payload: e.target.value })}
              onKeyDown={(e) => e.key === 'Enter' && void send()}
            />
            <i className="sep" aria-hidden="true" />
            <button
              type="button"
              className="chip"
              title={t('detail.quickFormat')}
              data-testid="qp-format"
              onClick={() => setQp({ ...qp, format: qp.format === 'json' ? 'text' : qp.format === 'text' ? 'hex' : qp.format === 'hex' ? 'base64' : 'json' })}
            >
              {t(`encoding.${qp.format}`)}
            </button>
            <i className="sep" aria-hidden="true" />
            <Seg<Qos> label="QoS" value={qp.qos} testId="qp-qos" onChange={(qos) => setQp({ ...qp, qos })} options={([0, 1, 2] as Qos[]).map((q) => ({ value: q, label: String(q), testId: `qp-qos-${q}` }))} />
            <i className="sep" aria-hidden="true" />
            <span className="muted small">{t('detail.retain')}</span>
            <Switch on={qp.retain} label={t('detail.retain')} testId="qp-retain" onChange={(retain) => setQp({ ...qp, retain })} />
            <i className="sep" aria-hidden="true" />
            <button type="button" className="btn pri sm" data-testid="qp-send" onClick={() => void send()}>
              {t('detail.send')}
            </button>
          </div>
          <div className="dtabs" role="tablist">
            {tabs.map((x) => (
              <button key={x.id} type="button" role="tab" aria-selected={tab === x.id} className={tab === x.id ? 'on' : undefined} data-testid={`tab-${x.id}`} onClick={() => setTab(x.id)}>
                {x.label}
              </button>
            ))}
          </div>
          <div className="pc">
            {viewSeq !== null && (
              <div className="callout" style={{ marginBottom: 8, padding: '4px 10px' }}>
                <span className="row">
                  <span>{t('detail.showingOld', { time: message ? fmtTime(message.receivedAt, true) : '' })}</span>
                  <span className="grow" />
                  <button type="button" className="btn sm" onClick={() => patch(connId, { viewSeq: null })}>
                    {t('detail.backToLatest')}
                  </button>
                </span>
              </div>
            )}
            {tab === 'payload' && message && (
              <>
                <div className="row" style={{ marginBottom: 8, flexWrap: 'wrap', rowGap: 6 }} data-testid="diff-row">
                  <span className="muted small nowrap">{t('payload.diffToggle')}</span>
                  <Switch on={diffOn} label={t('payload.diffToggle')} testId="diff-toggle" onChange={(v) => patch(connId, { diff: v })} />
                  {diffOn && baseEntry && canDiff && (
                    <span className="muted small nowrap" data-testid="diff-baseline">
                      {t('payload.vs', { time: fmtTime(baseEntry.receivedAt) })}
                    </span>
                  )}
                  <span className="grow" />
                  {showDiff && diff.changes > 0 && <span className="chip" data-testid="diff-changes">{t('diff.changes', { count: diff.changes })}</span>}
                  <button type="button" className="btn sm" data-testid="format-menu" title={t('detail.quickFormat')} onClick={(e) => void formatMenu(e)}>
                    {format === 'auto' ? t('payload.auto', { detected: t(`format.${message.decoded.detected}`) }) : t(`format.${format}`)} ▾
                  </button>
                  <button type="button" className="btn sm" onClick={() => void copyPayload(connId, topic)}>
                    {t('payload.copy')}
                  </button>
                </div>
                {showDiff && diff.mode === 'json' ? (
                  <DiffView
                    lines={diff.lines}
                    highlight
                    actions={actions}
                    testId="payload-json"
                    leafValue={(path) => {
                      const l = diff.lines.find((x) => x.path === path);
                      try {
                        return l?.newValue ? JSON.parse(l.newValue) : null;
                      } catch {
                        return l?.newValue;
                      }
                    }}
                  />
                ) : showDiff && diff.mode === 'text' ? (
                  <div className="json wrap" data-testid="payload-text">
                    {diff.textLines.map((l, i) => (
                      <span key={i} className={`line${l.kind === 'added' ? ' add' : ''}${l.kind === 'removed' ? ' del' : ''}`} data-kind={l.kind}>
                        {l.text}
                      </span>
                    ))}
                  </div>
                ) : (
                  <PayloadBody m={message} actions={actions} onFormatAnyway={() => setAnyway(true)} />
                )}
                <div className="sec" style={{ marginTop: 14, flexWrap: 'wrap', rowGap: 4 }} data-testid="topic-chart-head">
                  {t('payload.chart')}
                  {fields.map((f) => (
                    <span key={f.path} className="chip on mono" data-testid="chart-field" data-path={f.path}>
                      {pathLabel(f.path)}
                      <button
                        type="button"
                        className="x"
                        aria-label={t('payload.removeField', { field: pathLabel(f.path) })}
                        title={t('payload.removeField', { field: pathLabel(f.path) })}
                        onClick={() => void setChartFields(connId, topic, fields.filter((x) => x.path !== f.path))}
                      >
                        ✕
                      </button>
                    </span>
                  ))}
                  <button type="button" className="btn sm lower" data-testid="add-field" onClick={() => openPicker(null)}>
                    {t('payload.addField')}
                  </button>
                </div>
                {fields.length > 0 && (
                  <>
                    <div style={{ marginTop: 6 }}>
                      <UChart
                        height={130}
                        compact
                        testId="topic-chart"
                        label={t('payload.chartLabel', { fields: fields.map((f) => pathLabel(f.path)).join(', ') })}
                        series={fields.map((f) => ({
                          label: pathLabel(f.path),
                          color: f.colour,
                          points: chartData[f.path]?.points ?? [],
                          style: f.style,
                          axis: f.axis,
                          unit: f.unit,
                          boolean: chartData[f.path]?.castRule === 'boolean',
                        }))}
                        xRange={[Date.now() - (rangeMs('15m') ?? 0), Date.now()]}
                      />
                    </div>
                    <div className="legend" style={{ marginTop: 6 }}>
                      {fields.map((f) => (
                        <span key={f.path}>
                          <i style={{ background: `var(--c-${f.colour})` }} />
                          {[pathLabel(f.path), f.unit, f.axis === 'left' ? t('series.left') : t('series.right')].filter(Boolean).join(' · ')}
                        </span>
                      ))}
                    </div>
                  </>
                )}
                <div className="sec" style={{ marginTop: 14 }}>
                  {t('payload.history', { n: fmtNum(history.length) })}
                </div>
                <table className="tbl fixed" data-testid="history">
                  <colgroup>
                    <col style={{ width: 112 }} />
                    <col />
                    <col style={{ width: 48 }} />
                    <col style={{ width: 72 }} />
                  </colgroup>
                  <thead>
                    <tr>
                      <th>{t('col.time')}</th>
                      <th>{structured ? t('diff.changedKeys') : t('col.value')}</th>
                      <th>QoS</th>
                      <th>{t('col.size')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {history.map((h) => {
                      const v = structured ? h.changedKeys.join(', ') : h.preview;
                      return (
                        <tr key={h.seq} className={`click${h.seq === shownSeq ? ' sel' : ''}`} data-testid="history-row" data-seq={h.seq} onClick={() => patch(connId, { viewSeq: h.seq === shown.last?.seq && follow ? null : h.seq })}>
                          <td className="mono">{fmtTime(h.receivedAt, true)}</td>
                          <td className="mono ell" title={v}>
                            {v}
                          </td>
                          <td>{h.qos}</td>
                          <td>{fmtBytes(h.size)}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
                <div className="muted small" style={{ marginTop: 6 }} data-testid="buffer-line">
                  {t('history.buffer', { count: shown.bufferSize, n: fmtNum(shown.bufferSize) })} ·{' '}
                  <button type="button" className="ulink" data-testid="buffer-raise" onClick={() => setRaising(true)}>
                    {t('history.raiseMore')}
                  </button>
                </div>
              </>
            )}
            {tab === 'properties' && message && (
              <PropertiesView message={message} negotiated={negotiated} requested={profile?.mqttVersion ?? 'auto'} reply={replyFor} />
            )}
          </div>
        </>
      )}
      {menu && (
        <FieldMenu
          at={menu}
          sparklinePath={prefs.sparklinePath ?? null}
          onClose={() => setMenu(null)}
          onSparkline={(p) => void useSparkline(p)}
          onChart={(p) => openPicker(p)}
          onCopy={copyField}
        />
      )}
      {raising && (
        <PromptDialog
          title={t('history.raiseTitle')}
          label={t('history.raiseTo')}
          initial={String(Math.min(10000, shown.bufferSize * 5))}
          confirmLabel={t('common.save')}
          onClose={() => setRaising(false)}
          onConfirm={async (v) => {
            const n = Math.min(10000, Math.max(10, Number(v.replace(/\D/g, '')) || Math.min(10000, shown.bufferSize * 5)));
            await run(engine().setTopicBuffer(connId, topic, n));
            const d = await engine().topicDetail(connId, topic);
            setDetail(d);
            setFrozen((f) => (f ? { ...f, bufferSize: d.bufferSize } : f));
          }}
        />
      )}
    </div>
  );
}

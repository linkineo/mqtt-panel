import { useEffect, useMemo, useRef, useState } from 'react';
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
import { useCharts, rangeMs } from '../../store/charts';
import { useExplorer, type DetailTab } from '../../store/explorer';
import { run } from '../../lib/actions';
import { addToChart, copyCli, copyPayload, copyTopic, clearHistory, payloadFormat, payloadText, publishHere, type TopicDialog } from '../../lib/topicActions';
import { JsonView, DiffView, fieldActions, pathLabel } from '../../components/JsonView';
import { UChart, type UChartSeries } from '../../components/UChart';
import { Msg, Seg, Switch } from '../../components/ui';
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

function PayloadBody({ m, onFormatAnyway, actions }: { m: MessageView; onFormatAnyway(): void; actions: ReturnType<typeof fieldActions> }) {
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

export function Detail(props: { connId: string; topic: string; openDialog(d: TopicDialog): void }) {
  const { t } = useTranslation();
  const { connId, topic } = props;
  const conn = useApp((s) => s.connections.find((c) => c.id === connId) ?? null);
  const profile = useApp((s) => s.profiles.find((p) => p.id === connId) ?? null);
  const seq = useApp((s) => s.seqs[connId] ?? 0);
  const mac = useApp((s) => s.appInfo.platform === 'macos' || (s.appInfo.platform === 'web' && /Mac/.test(navigator.platform)));
  const st = useExplorer((s) => s.get(connId));
  const patch = useExplorer((s) => s.patch);
  const charts = useCharts((s) => s.charts);
  const tseq = useThrottled(seq, 250);
  const slowSeq = useThrottled(seq, 1000);

  const [detail, setDetail] = useState<TopicDetail | null>(null);
  const [frozen, setFrozen] = useState<TopicDetail | null>(null);
  const [format, setFormat] = useState<PayloadFormat>('auto');
  const [forced, setForced] = useState<MessageView | null>(null);
  const [anyway, setAnyway] = useState(false);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [baseline, setBaseline] = useState<number | null>(null);
  const [diff, setDiff] = useState<DiffResult | null>(null);
  const [highlight, setHighlight] = useState(true);
  const [mini, setMini] = useState<SeriesData | null>(null);
  const [chartData, setChartData] = useState<Record<string, SeriesData>>({});
  const [sent, setSent] = useState<SentEntry[]>([]);
  const [qp, setQp] = useState<{ payload: string; format: PayloadEncoding; qos: Qos; retain: boolean }>({ payload: '', format: 'text', qos: 0, retain: false });
  const [bufferInput, setBufferInput] = useState('');

  const tab = st.tab;
  const follow = st.follow;
  const viewSeq = st.viewSeq;

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
    setBaseline(null);
    setDiff(null);
    setMini(null);
    setChartData({});
    setBufferInput('');
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

  // history (payload Recent, diff, history tabs)
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

  // diff
  const diffB = shownSeq;
  const diffA = baseline ?? history.find((h) => diffB !== null && h.seq < diffB)?.seq ?? null;
  useEffect(() => {
    if (tab !== 'diff' || diffA === null || diffB === null) {
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
  }, [tab, connId, topic, diffA, diffB]);

  // mini chart of the default field
  const defaultField = shown?.defaultField ?? null;
  useEffect(() => {
    if (tab !== 'payload' || !defaultField) {
      setMini(null);
      return;
    }
    let live = true;
    void engine()
      .series(connId, topic, defaultField, Date.now() - 3600_000)
      .then((s) => live && setMini(s))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [tab, connId, topic, defaultField, slowSeq]);

  // charted fields for this topic (Chart tab)
  const charted = useMemo(() => {
    const seen = new Map<string, { path: string; color: UChartSeries['color']; unit: string; axis: 'left' | 'right'; style: UChartSeries['style'] }>();
    charts
      .filter((c) => c.connId === connId)
      .forEach((c) => c.series.filter((s) => s.topic === topic).forEach((s) => !seen.has(s.path) && seen.set(s.path, s)));
    return Array.from(seen.values());
  }, [charts, connId, topic]);
  const chartedKey = charted.map((c) => c.path).join('|');
  useEffect(() => {
    if (tab !== 'chart' || !charted.length) return;
    let live = true;
    void Promise.all(charted.map((c) => engine().series(connId, topic, c.path, Date.now() - 15 * 60_000))).then((list) => {
      if (!live) return;
      const out: Record<string, SeriesData> = {};
      list.forEach((s, i) => (out[charted[i].path] = s));
      setChartData(out);
    });
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab, connId, topic, chartedKey, slowSeq]);

  // sent log, for the reply box in Properties
  useEffect(() => {
    if (tab !== 'properties') return;
    void engine()
      .sentLog(connId)
      .then(setSent)
      .catch(() => undefined);
  }, [tab, connId, slowSeq]);

  const openPicker = (path: string | null) => addToChart(connId, topic, path);
  const actions = fieldActions((path) => openPicker(path));
  const pinned = st.pinned.includes(topic);
  const nProps = countProps(message);
  const negotiated = conn?.negotiatedVersion ?? null;

  if (!shown) return <div className="pane right" />;

  const setTab = (tab: DetailTab) => patch(connId, { tab });
  const tabs: { id: DetailTab; label: string }[] = [
    { id: 'payload', label: t('detail.payload') },
    { id: 'diff', label: t('detail.diff') },
    { id: 'history', label: t('detail.history', { n: fmtNum(shown.historyCount) }) },
    { id: 'chart', label: t('detail.chart') },
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

  const miniSeries: UChartSeries[] = mini && mini.points.length
    ? [{ label: pathLabel(mini.path), color: 'sage', points: mini.points, style: mini.step ? 'step' : 'line', axis: 'left', unit: '', boolean: mini.castRule === 'boolean' }]
    : [];

  const replyFor = message?.properties?.correlationData
    ? (sent.find((s) => s.reply && (s.request?.correlationData === message.properties!.correlationData || s.reply.correlationData === message.properties!.correlationData)) ?? null)
    : null;

  return (
    <div className="pane right" style={{ flex: 1 }} data-testid="detail">
      <div className="ph">
        <h5 className="mono selectable" style={{ fontWeight: 600 }} title={topic}>
          {topic}
        </h5>
        <div className="row" style={{ gap: 6 }}>
          <button type="button" className="btn sm" data-testid="pin-btn" aria-pressed={pinned} onClick={() => void useExplorer.getState().togglePin(connId, topic)}>
            {pinned ? t('detail.pinned') : t('detail.pin')}
          </button>
          <button type="button" className="btn sm" onClick={() => void publishHere(connId, topic)}>
            {t('menu.publishHere')}
          </button>
          <button type="button" className="btn sm" aria-label={t('detail.more')} onClick={(e) => void moreMenu(e)}>
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
          <div className="row" style={{ gap: 6, padding: '0 14px 8px' }}>
            <span className="muted small nowrap">{t('detail.quickPublish')}</span>
            <input
              className="input mono"
              style={{ flex: 1, minWidth: 100 }}
              value={qp.payload}
              aria-label={t('detail.quickPayload')}
              data-testid="qp-payload"
              onChange={(e) => setQp({ ...qp, payload: e.target.value })}
              onKeyDown={(e) => e.key === 'Enter' && void send()}
            />
            <button
              type="button"
              className="btn sm"
              title={t('detail.quickFormat')}
              data-testid="qp-format"
              onClick={() => setQp({ ...qp, format: qp.format === 'json' ? 'text' : qp.format === 'text' ? 'hex' : qp.format === 'hex' ? 'base64' : 'json' })}
            >
              {t(`encoding.${qp.format}`)}
            </button>
            <Seg<Qos> label="QoS" value={qp.qos} testId="qp-qos" onChange={(qos) => setQp({ ...qp, qos })} options={([0, 1, 2] as Qos[]).map((q) => ({ value: q, label: String(q), testId: `qp-qos-${q}` }))} />
            <span className="muted small">{t('detail.retain')}</span>
            <Switch on={qp.retain} label={t('detail.retain')} testId="qp-retain" onChange={(retain) => setQp({ ...qp, retain })} />
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
                <div className="row" style={{ marginBottom: 8 }}>
                  <span className="muted small">{t('payload.format')}</span>
                  <button type="button" className="btn sm" data-testid="format-menu" onClick={(e) => void formatMenu(e)}>
                    {format === 'auto' ? t('payload.auto', { detected: t(`format.${message.decoded.detected}`) }) : t(`format.${format}`)} ▾
                  </button>
                  <span className="grow" />
                  <button type="button" className="btn sm" onClick={() => void copyPayload(connId, topic)}>
                    {t('payload.copy')}
                  </button>
                </div>
                <PayloadBody m={message} actions={actions} onFormatAnyway={() => setAnyway(true)} />
                {miniSeries.length > 0 && (
                  <>
                    <div className="sec" style={{ marginTop: 14 }}>
                      {t('payload.chartOf', { field: pathLabel(defaultField ?? '$') })}
                    </div>
                    <div style={{ marginTop: 6 }}>
                      <UChart series={miniSeries} height={120} compact testId="mini-chart" />
                    </div>
                  </>
                )}
                {history.length > 0 && (
                  <>
                    <div className="sec" style={{ marginTop: 14 }}>
                      {t('payload.recent')}
                    </div>
                    <table className="tbl">
                      <thead>
                        <tr>
                          <th>{t('col.time')}</th>
                          <th>{t('col.value')}</th>
                          <th>QoS</th>
                          <th>{t('col.size')}</th>
                        </tr>
                      </thead>
                      <tbody>
                        {history.slice(0, 4).map((h) => (
                          <tr key={h.seq} className={`click${h.seq === shownSeq ? ' sel' : ''}`} onClick={() => patch(connId, { viewSeq: h.seq })}>
                            <td className="mono">{fmtTime(h.receivedAt, true)}</td>
                            <td className="mono">{h.preview}</td>
                            <td>{h.qos}</td>
                            <td>{fmtBytes(h.size)}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </>
                )}
              </>
            )}
            {tab === 'diff' && (
              <>
                <div className="row" style={{ flexWrap: 'nowrap' }}>
                  <span className="muted small">{t('diff.compare')}</span>
                  <span className="seg">
                    <span className="on">{t('diff.current', { time: message ? fmtTime(message.receivedAt) : '' })}</span>
                  </span>
                  <span className="muted small">{t('diff.with')}</span>
                  <span className="seg">
                    <button type="button" className={baseline === null ? 'on' : undefined} onClick={() => setBaseline(null)}>
                      {t('diff.previous', { time: fmtTime(history.find((h) => h.seq === (diffA ?? -1))?.receivedAt ?? Date.now()) })}
                    </button>
                    <button
                      type="button"
                      className={baseline !== null ? 'on' : undefined}
                      data-testid="diff-pick"
                      onClick={async (e) => {
                        const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
                        const id = await platform().showContextMenu(
                          history.slice(0, 30).map((h) => ({ id: String(h.seq), label: `${fmtTime(h.receivedAt, true)} · ${h.preview.slice(0, 30)}` })),
                          r.left,
                          r.bottom + 4,
                        );
                        if (id) setBaseline(Number(id));
                      }}
                    >
                      {t('diff.pick')}
                    </button>
                  </span>
                  <span className="grow" />
                  {diff && <span className="chip">{t('diff.changes', { count: diff.changes })}</span>}
                  <button type="button" className="btn sm" onClick={() => setHighlight(!highlight)}>
                    {highlight ? t('diff.highlightOff') : t('diff.highlightOn')}
                  </button>
                </div>
                <div style={{ marginTop: 10 }}>
                  {!diff ? (
                    <div className="hint">{t('diff.nothing')}</div>
                  ) : diff.mode === 'json' ? (
                    <DiffView
                      lines={diff.lines}
                      highlight={highlight}
                      actions={actions}
                      leafValue={(path) => {
                        const l = diff.lines.find((x) => x.path === path);
                        try {
                          return l?.newValue ? JSON.parse(l.newValue) : null;
                        } catch {
                          return l?.newValue;
                        }
                      }}
                    />
                  ) : (
                    <div className="json" data-testid="diff">
                      {diff.textLines.map((l, i) => (
                        <span key={i} className={`line${highlight && l.kind === 'added' ? ' add' : ''}${highlight && l.kind === 'removed' ? ' del' : ''}`}>
                          {l.kind === 'added' ? '+ ' : l.kind === 'removed' ? '- ' : '  '}
                          {l.text}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
                <div className="sec" style={{ marginTop: 14 }}>
                  {t('detail.historyHead')}
                </div>
                <table className="tbl">
                  <thead>
                    <tr>
                      <th>{t('col.time')}</th>
                      <th>{t('diff.changedKeys')}</th>
                      <th>{t('col.size')}</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {history.slice(0, 50).map((h) => (
                      <tr key={h.seq} className={h.seq === diffB ? 'sel' : undefined}>
                        <td className="mono">{fmtTime(h.receivedAt, true)}</td>
                        <td className="mono">{h.changedKeys.join(', ')}</td>
                        <td>{fmtBytes(h.size)}</td>
                        <td>
                          <button type="button" className={`btn sm${baseline === h.seq ? ' pri' : ''}`} onClick={() => setBaseline(h.seq)}>
                            {t('diff.baseline')}
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </>
            )}
            {tab === 'history' && (
              <>
                <div className="row" style={{ marginBottom: 8 }}>
                  <span className="hint">{t('history.buffer', { count: shown.bufferSize, n: fmtNum(shown.bufferSize) })}</span>
                  <span className="grow" />
                  <input
                    className="input mono"
                    style={{ width: 80 }}
                    inputMode="numeric"
                    placeholder={String(shown.bufferSize * 5 > 10000 ? 10000 : shown.bufferSize * 5)}
                    value={bufferInput}
                    aria-label={t('history.raiseTo')}
                    onChange={(e) => setBufferInput(e.target.value.replace(/\D/g, ''))}
                  />
                  <button
                    type="button"
                    className="btn sm"
                    onClick={async () => {
                      const n = Math.min(10000, Math.max(10, Number(bufferInput) || Math.min(10000, shown.bufferSize * 5)));
                      await run(engine().setTopicBuffer(connId, topic, n));
                      setBufferInput('');
                      setDetail(await engine().topicDetail(connId, topic));
                    }}
                  >
                    {t('history.raise')}
                  </button>
                </div>
                <table className="tbl" data-testid="history">
                  <thead>
                    <tr>
                      <th>{t('col.time')}</th>
                      <th>{t('col.size')}</th>
                      <th>QoS</th>
                      <th>{t('col.retained')}</th>
                      <th>{t('col.value')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {history.map((h) => (
                      <tr
                        key={h.seq}
                        className={`click${h.seq === shownSeq ? ' sel' : ''}`}
                        onClick={() => patch(connId, { viewSeq: h.seq, tab: 'payload' })}
                      >
                        <td className="mono">{fmtTime(h.receivedAt, true)}</td>
                        <td>{fmtBytes(h.size)}</td>
                        <td>{h.qos}</td>
                        <td>{h.retain ? 'R' : ''}</td>
                        <td className="mono">{h.preview}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </>
            )}
            {tab === 'chart' && (
              <>
                <div className="row" style={{ flexWrap: 'wrap' }}>
                  <span className="muted small">{t('chartTab.fields')}</span>
                  {charted.map((c) => (
                    <span key={c.path} className="chip on mono">
                      {pathLabel(c.path)}
                      <button type="button" className="x" aria-label={t('chartTab.remove', { field: pathLabel(c.path) })} onClick={() => useCharts.getState().removeTopicField(connId, topic, c.path)}>
                        ✕
                      </button>
                    </span>
                  ))}
                  <button type="button" className="btn sm" data-testid="add-field" onClick={() => openPicker(null)}>
                    {t('chartTab.addField')}
                  </button>
                </div>
                {charted.length === 0 ? (
                  <div className="hint" style={{ marginTop: 10 }}>
                    {t('chartTab.empty')}
                  </div>
                ) : (
                  <>
                    <div style={{ marginTop: 10 }}>
                      <UChart
                        height={150}
                        compact
                        series={charted.map((c) => ({
                          label: pathLabel(c.path),
                          color: c.color,
                          points: chartData[c.path]?.points ?? [],
                          style: c.style,
                          axis: c.axis,
                          unit: c.unit,
                          boolean: chartData[c.path]?.castRule === 'boolean',
                        }))}
                        xRange={[Date.now() - (rangeMs('15m') ?? 0), Date.now()]}
                      />
                    </div>
                    <div className="legend" style={{ marginTop: 6 }}>
                      {charted.map((c) => (
                        <span key={c.path}>
                          <i style={{ background: `var(--c-${c.color})` }} />
                          {[pathLabel(c.path), c.unit, c.axis === 'left' ? t('series.left') : t('series.right')].filter(Boolean).join(' · ')}
                        </span>
                      ))}
                    </div>
                  </>
                )}
              </>
            )}
            {tab === 'properties' && message && (
              <PropertiesView message={message} negotiated={negotiated} requested={profile?.mqttVersion ?? 'auto'} reply={replyFor} />
            )}
          </div>
        </>
      )}
    </div>
  );
}

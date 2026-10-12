import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { ExportEstimate, ExportOptions } from '@mqttpanel/engine-client';
import { engine } from '../../services';
import { reportSave, run } from '../../lib/actions';
import { Check, Modal, Seg } from '../../components/ui';
import { fmtBytes, fmtHm, fmtNum } from '../../lib/format';

type Range = '15m' | '1h' | 'session' | 'custom';

function toLocalInput(ms: number): string {
  const d = new Date(ms - new Date().getTimezoneOffset() * 60000);
  return d.toISOString().slice(0, 16);
}

/** Mockup 11: scope, time range, format, size estimate, then the native save dialog. */
export function ExportDialog(props: {
  connId: string;
  topic: string | null;
  initialScope: 'topic' | 'subtree' | 'visible';
  visibleTopics: string[];
  onClose(): void;
}) {
  const { t } = useTranslation();
  const { connId, topic } = props;
  const isLeafScope = !!topic;
  const [scope, setScope] = useState<'topic' | 'subtree' | 'visible'>(props.initialScope);
  const [range, setRange] = useState<Range>('1h');
  const [from, setFrom] = useState(() => toLocalInput(Date.now() - 3600_000));
  const [to, setTo] = useState(() => toLocalInput(Date.now()));
  const [format, setFormat] = useState<ExportOptions['format']>('csv');
  const [props5, setProps5] = useState(true);
  const [b64, setB64] = useState(false);
  const [est, setEst] = useState<ExportEstimate | null>(null);
  const [subCount, setSubCount] = useState<number | null>(null);
  const now = Date.now();
  const prefix = topic ?? '';

  useEffect(() => {
    if (!topic) return;
    void engine()
      .knownTopics(connId, prefix, 100000)
      .then((l) => setSubCount(l.filter((x) => x === prefix || x.startsWith(prefix + '/')).length))
      .catch(() => undefined);
  }, [connId, prefix, topic]);

  const options = (): ExportOptions => {
    const sc: ExportOptions['scope'] =
      scope === 'topic' && topic ? { kind: 'topic', topic } : scope === 'subtree' && topic ? { kind: 'subtree', prefix } : { kind: 'topics', topics: props.visibleTopics };
    let fromMs: number | null = null;
    let toMs: number | null = null;
    if (range === '15m') fromMs = now - 15 * 60_000;
    if (range === '1h') fromMs = now - 3600_000;
    if (range === 'custom') {
      fromMs = new Date(from).getTime() || null;
      toMs = new Date(to).getTime() || null;
    }
    return { scope: sc, fromMs, toMs, format, includeProperties: props5, rawBase64: b64 };
  };
  const key = JSON.stringify({ scope, range, from, to, format, props5, b64 });
  useEffect(() => {
    const h = setTimeout(() => {
      void engine()
        .exportEstimate(connId, options())
        .then(setEst)
        .catch(() => setEst(null));
    }, 150);
    return () => clearTimeout(h);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, connId]);

  const radio = (v: typeof scope, label: React.ReactNode, disabled = false) => (
    <button type="button" role="radio" aria-checked={scope === v} className="radio" disabled={disabled} style={disabled ? { opacity: 0.45 } : undefined} onClick={() => setScope(v)} data-testid={`export-scope-${v}`}>
      <i className={scope === v ? 'on' : ''} />
      <span className="nowrap">{label}</span>
    </button>
  );

  return (
    <Modal
      title={t('export.title')}
      width={600}
      onClose={props.onClose}
      testId="export-dialog"
      footer={
        <>
          <button type="button" className="btn" onClick={props.onClose}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className="btn pri"
            data-testid="export-save"
            onClick={async () => {
              const out = await run(engine().exportData(connId, options()));
              if (out) reportSave(out, t);
              if (out?.kind !== 'cancelled') props.onClose();
            }}
          >
            {t('export.saveAs')}
          </button>
        </>
      }
    >
      <div className="sec">{t('export.what')}</div>
      <div role="radiogroup" style={{ display: 'grid', gap: 6 }}>
        {radio(
          'topic',
          <>
            {t('export.selectedTopic')} · <span className="mono">{topic ?? '—'}</span>
          </>,
          !isLeafScope,
        )}
        {radio(
          'subtree',
          <>
            {t('export.subtree')} · <span className="mono">{topic ? `${topic}/#` : '—'}</span>
            {subCount !== null ? ` · ${t('export.topics', { count: subCount, n: fmtNum(subCount) })}` : ''}
          </>,
          !isLeafScope,
        )}
        {radio('visible', <>{t('export.visible')} · {t('export.topics', { count: props.visibleTopics.length, n: fmtNum(props.visibleTopics.length) })}</>, props.visibleTopics.length === 0)}
      </div>
      <div className="sec" style={{ marginTop: 6 }}>
        {t('export.range')}
      </div>
      <div className="row">
        <Seg
          label={t('export.range')}
          value={range}
          onChange={setRange}
          options={[
            { value: '15m', label: t('range.last15') },
            { value: '1h', label: t('range.lastHour') },
            { value: 'session', label: t('range.session') },
            { value: 'custom', label: t('range.custom') },
          ]}
        />
        {range === 'custom' ? (
          <>
            <input className="input" type="datetime-local" value={from} aria-label={t('export.from')} onChange={(e) => setFrom(e.target.value)} style={{ width: 170 }} />
            <input className="input" type="datetime-local" value={to} aria-label={t('export.to')} onChange={(e) => setTo(e.target.value)} style={{ width: 170 }} />
          </>
        ) : (
          <span className="hint">
            {range === 'session' ? t('export.wholeSession') : t('export.fromTo', { from: fmtHm(range === '15m' ? now - 15 * 60_000 : now - 3600_000), to: fmtHm(now) })}
          </span>
        )}
      </div>
      <div className="sec" style={{ marginTop: 6 }}>
        {t('export.format')}
      </div>
      <div className="row">
        <Seg
          label={t('export.format')}
          value={format}
          onChange={setFormat}
          options={[
            { value: 'csv', label: 'CSV' },
            { value: 'json', label: 'JSON' },
            { value: 'ndjson', label: 'NDJSON' },
          ]}
        />
        <span className="hint" title={t(`export.desc.${format}`)}>
          {t(`export.desc.${format}`)}
        </span>
      </div>
      <div className="row" style={{ gap: 16, marginTop: 4 }}>
        <Check on={props5} onChange={setProps5} label={t('export.includeProps')} />
        <Check on={b64} onChange={setB64} label={t('export.rawBase64')} />
      </div>
      <div className="callout" style={{ marginTop: 4, padding: '8px 12px' }} data-testid="export-estimate">
        <p className="small">
          <strong>{est ? t('export.estimate', { count: est.messages, n: fmtNum(est.messages), size: fmtBytes(est.bytes) }) : '…'}</strong> {t('export.estimateNote')}
        </p>
      </div>
    </Modal>
  );
}

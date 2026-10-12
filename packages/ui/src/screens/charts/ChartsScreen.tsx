import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type uPlot from 'uplot';
import type { MemoryEstimate, SeriesData } from '@mqttpanel/engine-client';
import { engine, platform } from '../../services';
import { useApp, useActiveConnection } from '../../store/app';
import { COLORS, rangeMs, useCharts, type Chart, type ChartRange, type ChartSeries } from '../../store/charts';
import { reportSave, run } from '../../lib/actions';
import { addToChart } from '../../lib/topicActions';
import { NoConnection } from '../../components/NoConnection';
import { UChart } from '../../components/UChart';
import { JsonView, pathLabel } from '../../components/JsonView';
import { Popover, PromptDialog, Seg, Switch } from '../../components/ui';
import { TopicInput } from '../publish/PublishScreen';
import { castLabel } from '../explorer/FieldPicker';
import { fmtBytes, fmtNum } from '../../lib/format';

export function ChartsScreen() {
  const conn = useActiveConnection();
  if (!conn) return <NoConnection />;
  return <Charts connId={conn.id} />;
}

function sinceFor(c: Chart, now: number): number | null {
  if (c.range === 'custom') return c.customFrom;
  const r = rangeMs(c.range);
  return r === null ? null : now - r;
}

function ruleLabel(s: SeriesData | undefined, t: (k: string) => string): string {
  if (!s) return '';
  if (s.castRule === 'mixed') return t('cast.mixed');
  if (s.castRule === 'none') return t('cast.none');
  return castLabel({ cast: s.castRule, plottable: true }, t);
}

function AddSeries({ connId, chartId }: { connId: string; chartId: string | null }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [topic, setTopic] = useState('');
  const ref = useRef<HTMLButtonElement>(null);
  const go = () => {
    if (!topic.trim()) return;
    setOpen(false);
    useCharts.getState().openPicker({ connId, topic: topic.trim(), preselect: null, chartId });
    setTopic('');
  };
  return (
    <>
      <button ref={ref} type="button" className="btn sm" data-testid="series-add" onClick={() => setOpen(!open)}>
        {t('charts.add')}
      </button>
      {open && (
        <Popover anchor={ref.current} onClose={() => setOpen(false)} align="end" label={t('charts.addTitle')}>
          <div style={{ display: 'grid', gap: 8, width: 360 }}>
            <div className="small muted">{t('charts.addHelp')}</div>
            <TopicInput connId={connId} value={topic} onChange={setTopic} />
            <div className="row" style={{ justifyContent: 'flex-end' }}>
              <button type="button" className="btn pri sm" disabled={!topic.trim()} onClick={go}>
                {t('charts.chooseFields')}
              </button>
            </div>
          </div>
        </Popover>
      )}
    </>
  );
}

function SeriesEditor({ chart, s, data, connId, onDone }: { chart: Chart; s: ChartSeries; data: SeriesData | undefined; connId: string; onDone(): void }) {
  const { t } = useTranslation();
  const upd = (p: Partial<ChartSeries>) => useCharts.getState().updateSeries(chart.id, s.id, p);
  const [sample, setSample] = useState<unknown>(undefined);
  const [path, setPath] = useState(s.path);
  useEffect(() => setPath(s.path), [s.path]);
  useEffect(() => {
    let live = true;
    void engine()
      .topicDetail(connId, s.topic)
      .then((d) => {
        if (!live) return;
        const dec = d.last?.decoded;
        setSample(dec ? (dec.json ?? dec.text ?? undefined) : undefined);
      })
      .catch(() => setSample(undefined));
    return () => {
      live = false;
    };
  }, [connId, s.topic]);
  const lbl = { gridTemplateColumns: '60px minmax(0,1fr)' };
  return (
    <div className="card on" data-testid="series-editor">
      <div className="t">
        <span className="row" style={{ gap: 8 }}>
          <i className="cdot" style={{ background: `var(--c-${s.color})` }} />
          {t('charts.editing')}
        </span>
        <button type="button" className="btn sm" onClick={onDone}>
          {t('common.done')}
        </button>
      </div>
      <div className="form" style={{ marginTop: 8, gap: 6 }}>
        <div className="f" style={lbl}>
          <label htmlFor={`sn-${s.id}`}>{t('series.name')}</label>
          <input id={`sn-${s.id}`} className="input" value={s.name} onChange={(e) => upd({ name: e.target.value })} />
        </div>
        <div className="f" style={lbl}>
          <span className="lbl">{t('series.topic')}</span>
          <TopicInput connId={connId} value={s.topic} onChange={(topic) => upd({ topic })} />
        </div>
        <div className="f" style={lbl}>
          <label htmlFor={`sp-${s.id}`}>{t('series.field')}</label>
          <span className="row2">
            <input
              id={`sp-${s.id}`}
              className="input mono"
              style={{ flex: 1 }}
              value={path}
              data-testid="series-path"
              onChange={(e) => setPath(e.target.value)}
              onBlur={() => upd({ path: path.startsWith('$') ? path : '$.' + path })}
              onKeyDown={(e) => e.key === 'Enter' && upd({ path: path.startsWith('$') ? path : '$.' + path })}
            />
            {data && <span className="chip">{ruleLabel(data, t)}</span>}
          </span>
        </div>
        {sample !== undefined && (
          <>
            <div className="f" style={lbl}>
              <span />
              <span className="hint">{t('series.pickHint')}</span>
            </div>
            <div style={{ marginLeft: 0 }}>
              <JsonView value={sample} small onPick={(p) => upd({ path: p })} pickedPath={s.path} testId="series-sample" />
            </div>
          </>
        )}
        <div className="f" style={lbl}>
          <span className="lbl">{t('series.colour')}</span>
          <span className="row2" role="radiogroup" aria-label={t('series.colour')}>
            {COLORS.map((c) => (
              <button
                key={c}
                type="button"
                role="radio"
                aria-checked={s.color === c}
                aria-label={t(`colour.${c}`)}
                className={`swatch${s.color === c ? ' on' : ''}`}
                style={{ background: `var(--c-${c})`, color: `var(--c-${c})` }}
                onClick={() => upd({ color: c })}
              />
            ))}
          </span>
        </div>
        <div className="f" style={lbl}>
          <span className="lbl">{t('series.style')}</span>
          <span className="row2">
            <Seg
              label={t('series.style')}
              value={s.style}
              onChange={(style) => upd({ style })}
              options={[
                { value: 'line', label: t('series.line') },
                { value: 'step', label: t('series.step') },
                { value: 'points', label: t('series.points') },
              ]}
            />
          </span>
        </div>
        <div className="f" style={lbl}>
          <span className="lbl">{t('series.axis')}</span>
          <Seg
            label={t('series.axis')}
            value={s.axis}
            onChange={(axis) => upd({ axis })}
            options={[
              { value: 'left', label: t('series.leftAxis') },
              { value: 'right', label: t('series.rightAxis') },
            ]}
          />
        </div>
        <div className="f" style={lbl}>
          <label htmlFor={`su-${s.id}`}>{t('series.unit')}</label>
          <span className="row2">
            <input id={`su-${s.id}`} className="input mono" style={{ width: 60 }} value={s.unit} onChange={(e) => upd({ unit: e.target.value })} />
            <span className="hint">{t('series.unitHint')}</span>
          </span>
        </div>
      </div>
      <div className="row" style={{ justifyContent: 'space-between', marginTop: 8 }}>
        <span className="hint">{t('series.pointsCount', { n: fmtNum(data?.count ?? 0), skipped: fmtNum(data?.skipped ?? 0) })}</span>
        <button type="button" className="btn sm danger" onClick={() => useCharts.getState().removeSeries(chart.id, s.id)}>
          {t('series.remove')}
        </button>
      </div>
    </div>
  );
}

function Charts({ connId }: { connId: string }) {
  const { t } = useTranslation();
  const all = useCharts((s) => s.charts);
  const activeId = useCharts((s) => s.activeId);
  const charts = useMemo(() => all.filter((c) => c.connId === connId), [all, connId]);
  const chart = charts.find((c) => c.id === activeId) ?? charts[0] ?? null;
  const seq = useApp((s) => s.seqs[connId] ?? 0);
  const [data, setData] = useState<Record<string, SeriesData>>({});
  const [editing, setEditing] = useState<string | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [mem, setMem] = useState<MemoryEstimate | null>(null);
  const [now, setNow] = useState(Date.now());
  const plot = useRef<uPlot | null>(null);
  const lastFetch = useRef(0);

  // refresh series about once a second, unless paused (D-20: the engine keeps recording)
  useEffect(() => {
    if (!chart || chart.paused) return;
    if (Date.now() - lastFetch.current < 900) return;
    lastFetch.current = Date.now();
    const n = Date.now();
    setNow(n);
    const since = sinceFor(chart, n);
    void Promise.all(chart.series.map((s) => engine().series(connId, s.topic, s.path, since).catch(() => null))).then((list) => {
      const out: Record<string, SeriesData> = {};
      list.forEach((d, i) => d && (out[chart.series[i].id] = d));
      setData(out);
    });
    void engine()
      .memory()
      .then(setMem)
      .catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seq, chart?.id, chart?.paused, chart?.range, chart?.series.map((s) => s.topic + s.path).join('|')]);

  if (!chart) {
    return (
      <div className="cols" style={{ gridTemplateColumns: '1fr' }}>
        <div className="pane" data-testid="charts-empty">
          <div className="empty" style={{ height: '100%' }}>
            <h4>{t('charts.emptyTitle')}</h4>
            <span className="small" style={{ maxWidth: '56ch' }}>
              {t('charts.emptyText')}
            </span>
            <AddSeries connId={connId} chartId={null} />
          </div>
        </div>
      </div>
    );
  }

  const upd = (p: Partial<Chart>) => useCharts.getState().updateChart(chart.id, p);
  const since = sinceFor(chart, chart.paused && chart.pausedAt ? chart.pausedAt : now);
  const end = chart.range === 'custom' && chart.customTo ? chart.customTo : chart.paused && chart.pausedAt ? chart.pausedAt : now;
  const xRange: [number, number] | null = since !== null ? [since, end] : null;
  const enabled = chart.series.filter((s) => s.enabled);
  const points = Object.values(data).reduce((a, d) => a + d.points.length, 0);

  const exportMenu = async (e: React.MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const id = await platform().showContextMenu(
      [
        { id: 'csv', label: t('charts.exportCsv') },
        { id: 'png', label: t('charts.exportPng') },
      ],
      r.left,
      r.bottom + 4,
    );
    if (id === 'csv') {
      const out = await run(engine().exportChartCsv(connId, chart.series.map((s) => ({ name: s.name, topic: s.topic, path: s.path })), since));
      if (out) reportSave(out, t);
    } else if (id === 'png') {
      const canvas = plot.current?.ctx.canvas;
      if (!canvas) return;
      // paint the theme background under the transparent canvas
      const c2 = document.createElement('canvas');
      c2.width = canvas.width;
      c2.height = canvas.height;
      const g = c2.getContext('2d')!;
      g.fillStyle = getComputedStyle(document.documentElement).getPropertyValue('--surface').trim() || '#fff';
      g.fillRect(0, 0, c2.width, c2.height);
      g.drawImage(canvas, 0, 0);
      const b64 = c2.toDataURL('image/png').split(',')[1];
      const out = await run(engine().savePng(`${chart.name || 'chart'}.png`, b64));
      if (out) reportSave(out, t);
    }
  };

  const moreMenu = async (e: React.MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const id = await platform().showContextMenu(
      [
        { id: 'rename', label: t('common.rename') },
        { id: 'dup', label: t('charts.duplicate') },
        { id: 'remove', label: t('charts.remove'), danger: true },
      ],
      r.right - 200,
      r.bottom + 4,
    );
    if (id === 'rename') setRenaming(true);
    else if (id === 'dup') useCharts.getState().duplicateChart(chart.id, t('charts.copySuffix'));
    else if (id === 'remove') useCharts.getState().removeChart(chart.id);
  };

  return (
    <div className="cols" style={{ gridTemplateColumns: 'minmax(0,1fr) 300px' }}>
      <div className="pane" data-testid="chart-pane">
        <div className="ph">
          <h5>
            <button type="button" className="btn link" style={{ fontSize: 13 }} title={t('common.rename')} onClick={() => setRenaming(true)} data-testid="chart-name">
              {chart.name}
            </button>
          </h5>
          <div className="row" style={{ gap: 6 }}>
            <Seg<ChartRange>
              label={t('charts.range')}
              value={chart.range}
              onChange={(range) => upd({ range, customFrom: range === 'custom' ? (chart.customFrom ?? Date.now() - 3600_000) : chart.customFrom, customTo: null })}
              options={[
                { value: '5m', label: t('range.last5') },
                { value: '15m', label: '15 min' },
                { value: '1h', label: '1 h' },
                { value: 'session', label: t('range.session') },
                { value: 'custom', label: t('range.custom') },
              ]}
            />
            <button type="button" className={`btn sm${chart.paused ? ' pri' : ''}`} data-testid="chart-pause" aria-pressed={chart.paused} onClick={() => upd({ paused: !chart.paused, pausedAt: chart.paused ? null : Date.now() })}>
              {chart.paused ? t('charts.resume') : t('charts.pause')}
            </button>
            <button type="button" className="btn sm" onClick={(e) => void exportMenu(e)} data-testid="chart-export">
              {t('explore.export')}
            </button>
            <button type="button" className="btn sm" aria-label={t('detail.more')} onClick={(e) => void moreMenu(e)}>
              ⋯
            </button>
          </div>
        </div>
        <div className="pc">
          {chart.range === 'custom' && (
            <div className="row" style={{ marginBottom: 8 }}>
              <span className="hint">{t('export.from')}</span>
              <input
                className="input"
                type="datetime-local"
                style={{ width: 180 }}
                aria-label={t('export.from')}
                value={chart.customFrom ? new Date(chart.customFrom - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 16) : ''}
                onChange={(e) => upd({ customFrom: new Date(e.target.value).getTime() || null })}
              />
              <span className="hint">{t('export.to')}</span>
              <input
                className="input"
                type="datetime-local"
                style={{ width: 180 }}
                aria-label={t('export.to')}
                value={chart.customTo ? new Date(chart.customTo - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 16) : ''}
                onChange={(e) => upd({ customTo: new Date(e.target.value).getTime() || null })}
              />
            </div>
          )}
          {chart.paused && <div className="hint warn" style={{ marginBottom: 6 }}>{t('charts.pausedNote')}</div>}
          {enabled.length === 0 ? (
            <div className="empty small" style={{ height: 300 }}>
              {t('charts.noSeries')}
            </div>
          ) : (
            <UChart
              height={300}
              testId="chart"
              onReady={(u) => (plot.current = u)}
              xRange={xRange}
              series={enabled.map((s) => ({
                label: s.name,
                color: s.color,
                points: data[s.id]?.points ?? [],
                style: s.style,
                axis: s.axis,
                unit: s.unit,
                boolean: data[s.id]?.castRule === 'boolean',
              }))}
            />
          )}
          <div className="legend" style={{ marginTop: 8 }}>
            {enabled.map((s) => (
              <span key={s.id}>
                <i style={{ background: `var(--c-${s.color})` }} />
                {s.name} · {s.path}
                {s.style === 'step' ? ` (${t('series.step').toLowerCase()})` : ''}
                {s.axis === 'right' ? ` · ${t('series.rightAxis').toLowerCase()}` : ''}
              </span>
            ))}
          </div>
          {charts.length > 1 && (
            <>
              <div className="sec" style={{ marginTop: 18 }}>
                {t('charts.others')}
              </div>
              <div className="list" style={{ marginTop: 6, gridTemplateColumns: '1fr 1fr', display: 'grid', gap: 8 }}>
                {charts
                  .filter((c) => c.id !== chart.id)
                  .map((c) => (
                    <button key={c.id} type="button" className="card" onClick={() => useCharts.getState().setActive(c.id)}>
                      <div className="t">
                        <span>{c.name}</span>
                      </div>
                      <div className="s">
                        {t('charts.seriesCount', { count: c.series.length })} · {c.range === 'session' ? t('range.session').toLowerCase() : c.range === 'custom' ? t('range.custom').toLowerCase() : c.range.replace('m', ' min').replace('1h', '1 h')}
                      </div>
                    </button>
                  ))}
              </div>
            </>
          )}
        </div>
      </div>
      <div className="pane right" data-testid="series-pane">
        <div className="ph">
          <h5>{t('charts.series')}</h5>
          <AddSeries connId={connId} chartId={chart.id} />
        </div>
        <div className="pc">
          <div className="list">
            {chart.series.map((s) =>
              editing === s.id ? (
                <SeriesEditor key={s.id} chart={chart} s={s} data={data[s.id]} connId={connId} onDone={() => setEditing(null)} />
              ) : (
                <div key={s.id} className="card" data-testid="series-card">
                  <div className="t">
                    <button type="button" className="btn link row" style={{ gap: 8, minWidth: 0 }} onClick={() => setEditing(s.id)} title={t('charts.edit')}>
                      <i className="cdot" style={{ background: `var(--c-${s.color})` }} />
                      <span className="ell">{s.name}</span>
                    </button>
                    <Switch on={s.enabled} label={t('charts.show', { name: s.name })} onChange={(enabled) => useCharts.getState().updateSeries(chart.id, s.id, { enabled })} />
                  </div>
                  <div className="s mono">{s.topic}</div>
                  <div className="s">
                    {t('charts.summary', {
                      path: s.path,
                      rule: ruleLabel(data[s.id], t),
                      style: t(`series.${s.style}`).toLowerCase(),
                      axis: s.axis === 'left' ? t('series.left') : t('series.right'),
                      n: fmtNum(data[s.id]?.count ?? 0),
                    })}
                    {data[s.id]?.skipped ? ` · ${t('charts.skipped', { n: fmtNum(data[s.id].skipped) })}` : ''}
                  </div>
                </div>
              ),
            )}
          </div>
          {chart.series.length === 0 && (
            <div className="hint" style={{ whiteSpace: 'normal' }}>
              {t('charts.addFirst')}
            </div>
          )}
          <div className="sec" style={{ marginTop: 16 }}>
            {t('charts.memory')}
          </div>
          <div className="muted small" style={{ marginTop: 4 }}>
            {t('charts.memoryNote', {
              points: fmtNum(points),
              size: fmtBytes(points * 16),
              used: fmtBytes(mem?.bytes ?? 0),
              budget: fmtBytes(mem?.budgetBytes ?? 0),
            })}
          </div>
          <div style={{ marginTop: 12 }}>
            <button type="button" className="btn sm" onClick={() => useCharts.getState().createChart(connId, t('charts.newName', { n: charts.length + 1 }))}>
              {t('charts.newChart')}
            </button>
          </div>
        </div>
      </div>
      {renaming && (
        <PromptDialog
          title={t('charts.renameTitle')}
          label={t('views.name')}
          initial={chart.name}
          confirmLabel={t('common.save')}
          onClose={() => setRenaming(false)}
          onConfirm={(name) => upd({ name })}
        />
      )}
    </div>
  );
}

export { addToChart, pathLabel };

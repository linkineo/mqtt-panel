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
import { Info, Popover, Seg, Switch, NO_AUTO } from '../../components/ui';
import { TopicInput } from '../publish/PublishScreen';
import { fmtBytes, fmtNum } from '../../lib/format';
import { chartPngBase64 } from '../../lib/chartImage';

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

function AddSeries({ connId, chartId }: { connId: string; chartId: string | null }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [topic, setTopic] = useState('');
  const ref = useRef<HTMLButtonElement>(null);
  const go = () => {
    if (!topic.trim()) return;
    setOpen(false);
    useCharts.getState().openPicker({ connId, topic: topic.trim(), preselect: null, chartId, mode: 'chart' });
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

/** Saves after a short pause while typing (like the profile form). */
function useDebounced(save: (v: string) => void, value: string, ms = 400): [string, (v: string) => void, () => void] {
  const [v, setV] = useState(value);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pending = useRef<string | null>(null);
  useEffect(() => {
    if (pending.current === null) setV(value);
  }, [value]);
  const flush = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    if (pending.current !== null) save(pending.current);
    pending.current = null;
  };
  const change = (x: string) => {
    setV(x);
    pending.current = x;
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(flush, ms);
  };
  useEffect(() => () => {
    if (timer.current) clearTimeout(timer.current);
  }, []);
  return [v, change, flush];
}

const normPath = (p: string) => (p.trim() === '' ? '$' : p.trim().startsWith('$') ? p.trim() : '$.' + p.trim().replace(/^\./, ''));

/** The cast rule shown on a card: the one chosen, else the one the engine detected. */
export function castOfSeries(s: ChartSeries, d: SeriesData | undefined): 'number' | 'parsedNumber' | 'boolean' {
  if (s.cast) return s.cast;
  return d?.castRule === 'parsedNumber' || d?.castRule === 'boolean' ? d.castRule : 'number';
}

/**
 * A series card, edited in place (owner, change set 8; mockup 10): switch, colour dot (palette popover), name, field
 * path with the cast rule, and a chevron that opens style, axis, unit, "pick from the last message" and Remove.
 * No edit mode: every change applies as it is made.
 */
function SeriesCard({ chart, s, data, connId }: { chart: Chart; s: ChartSeries; data: SeriesData | undefined; connId: string }) {
  const { t } = useTranslation();
  const upd = (p: Partial<ChartSeries>) => useCharts.getState().updateSeries(chart.id, s.id, p);
  const [open, setOpen] = useState(false);
  const [palette, setPalette] = useState(false);
  const dot = useRef<HTMLButtonElement>(null);
  const [sample, setSample] = useState<unknown>(undefined);
  const [name, setName, flushName] = useDebounced((name) => upd({ name }), s.name);
  const [path, setPath, flushPath] = useDebounced((p) => upd({ path: normPath(p) }), s.path);
  const [unit, setUnit, flushUnit] = useDebounced((unit) => upd({ unit }), s.unit);
  useEffect(() => {
    if (!open) return;
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
  }, [open, connId, s.topic]);
  const cast = castOfSeries(s, data);
  return (
    <div className={`card series${open ? ' on' : ''}`} data-testid="series-card" data-series={s.id}>
      <div className="t">
        <span className="row" style={{ gap: 8, minWidth: 0, flex: 1 }}>
          <button
            ref={dot}
            type="button"
            className="cdot btnless"
            style={{ background: `var(--c-${s.color})` }}
            aria-label={`${t('series.colour')}: ${t(`colour.${s.color}`)}`}
            title={t('series.colour')}
            data-testid="series-colour"
            onClick={() => setPalette(!palette)}
          />
          <input {...NO_AUTO}
            className="input sname"
            value={name}
            aria-label={t('series.name')}
            data-testid="series-name"
            onChange={(e) => setName(e.target.value)}
            onBlur={flushName}
            onKeyDown={(e) => e.key === 'Enter' && flushName()}
          />
        </span>
        <span className="row" style={{ gap: 6 }}>
          <Switch on={s.enabled} label={t('charts.show', { name: s.name })} onChange={(enabled) => upd({ enabled })} />
          <button
            type="button"
            className="iconbtn chev"
            aria-expanded={open}
            aria-label={t('series.more')}
            title={t('series.more')}
            data-testid="series-more"
            onClick={() => setOpen(!open)}
          >
            {open ? '▾' : '▸'}
          </button>
        </span>
      </div>
      <div className="s mono" title={s.topic}>
        <span className="ell">{s.topic}</span>
      </div>
      <div className="s" style={{ gap: 6 }}>
        <input {...NO_AUTO}
          className="input mono"
          style={{ width: 150 }}
          value={path}
          aria-label={t('series.field')}
          data-testid="series-path"
          onChange={(e) => setPath(e.target.value)}
          onBlur={flushPath}
          onKeyDown={(e) => e.key === 'Enter' && flushPath()}
        />
        <Seg
          label={t('series.cast')}
          value={cast}
          testId="series-cast"
          onChange={(c) => upd({ cast: c, style: c === 'boolean' ? 'step' : s.style })}
          options={[
            { value: 'number', label: t('series.castNumber') },
            { value: 'parsedNumber', label: t('series.castParsed') },
            { value: 'boolean', label: t('series.castBool') },
          ]}
        />
      </div>
      {palette && (
        <Popover anchor={dot.current} onClose={() => setPalette(false)} label={t('series.colour')} testId="series-palette">
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
                onClick={() => {
                  upd({ color: c });
                  setPalette(false);
                }}
              />
            ))}
          </span>
        </Popover>
      )}
      {open && (
        <div className="form" style={{ marginTop: 8, gap: 6 }} data-testid="series-details">
          {sample !== undefined && (
            <>
              <p className="muted small" style={{ margin: 0 }}>
                {t('series.pickHint')}
              </p>
              <JsonView
                value={sample}
                small
                onPick={(p) => {
                  setPath(p);
                  upd({ path: p });
                }}
                pickedPath={s.path}
                testId="series-sample"
              />
            </>
          )}
          <div className="f" style={{ gridTemplateColumns: '60px minmax(0,1fr)' }}>
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
          <div className="f" style={{ gridTemplateColumns: '60px minmax(0,1fr)' }}>
            <span className="lbl">{t('series.axis')}</span>
            <span className="row2">
              <Seg
                label={t('series.axis')}
                value={s.axis}
                onChange={(axis) => upd({ axis })}
                options={[
                  { value: 'left', label: t('series.leftAxis') },
                  { value: 'right', label: t('series.rightAxis') },
                ]}
              />
            </span>
          </div>
          <div className="f" style={{ gridTemplateColumns: '60px minmax(0,1fr)' }}>
            <label htmlFor={`su-${s.id}`}>{t('series.unit')}</label>
            <span className="row2">
              <input {...NO_AUTO}
                id={`su-${s.id}`}
                className="input mono"
                style={{ width: 60 }}
                value={unit}
                onChange={(e) => setUnit(e.target.value)}
                onBlur={flushUnit}
                onKeyDown={(e) => e.key === 'Enter' && flushUnit()}
              />
              <Info id="seriesUnit" title={t('series.unit')} />
            </span>
          </div>
          <div className="row" style={{ justifyContent: 'space-between' }}>
            <span className="hint">
              {t('series.pointsCount', { n: fmtNum(data?.count ?? 0), skipped: fmtNum(data?.skipped ?? 0) })}
            </span>
            <button type="button" className="btn sm danger" data-testid="series-remove" onClick={() => useCharts.getState().removeSeries(chart.id, s.id)}>
              {t('series.remove')}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/** The chart title, renamed in place (change set 8): click, type, Enter or focus loss saves, Escape cancels. */
function ChartTitle({ name, onRename, editing, setEditing }: { name: string; onRename(n: string): void; editing: boolean; setEditing(v: boolean): void }) {
  const { t } = useTranslation();
  const [v, setV] = useState(name);
  const cancelled = useRef(false);
  useEffect(() => {
    if (editing) {
      setV(name);
      cancelled.current = false;
    }
  }, [editing, name]);
  if (!editing) {
    return (
      <h5 className="ell" style={{ minWidth: 0 }}>
        <button type="button" className="btn link titlebtn" title={t('charts.rename')} data-testid="chart-title" onClick={() => setEditing(true)}>
          <span className="ell">{name}</span> <span className="muted" style={{ fontWeight: 400 }} aria-hidden="true">✎</span>
        </button>
      </h5>
    );
  }
  const save = () => {
    if (cancelled.current) return;
    const n = v.trim();
    if (n && n !== name) onRename(n);
    setEditing(false);
  };
  return (
    <input {...NO_AUTO}
      className="input titlein"
      autoFocus
      value={v}
      aria-label={t('charts.renameTitle')}
      data-testid="chart-title"
      onChange={(e) => setV(e.target.value)}
      onFocus={(e) => e.currentTarget.select()}
      onBlur={save}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          save();
        } else if (e.key === 'Escape') {
          e.preventDefault();
          e.stopPropagation();
          cancelled.current = true;
          setEditing(false);
        }
      }}
    />
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
      const b64 = chartPngBase64(canvas);
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
    <div className="cols" style={{ gridTemplateColumns: 'minmax(0,1fr) 400px' }}>
      <div className="pane" data-testid="chart-pane">
        <div className="ph">
          <ChartTitle name={chart.name} editing={renaming} setEditing={setRenaming} onRename={(name) => upd({ name })} />
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
              <input {...NO_AUTO}
                className="input"
                type="datetime-local"
                style={{ width: 180 }}
                aria-label={t('export.from')}
                value={chart.customFrom ? new Date(chart.customFrom - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 16) : ''}
                onChange={(e) => upd({ customFrom: new Date(e.target.value).getTime() || null })}
              />
              <span className="hint">{t('export.to')}</span>
              <input {...NO_AUTO}
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
              series={enabled.map((s) => {
                const bool = castOfSeries(s, data[s.id]) === 'boolean';
                const pts = data[s.id]?.points ?? [];
                return {
                  label: s.name,
                  color: s.color,
                  points: bool && s.cast === 'boolean' ? pts.map(([x, y]) => [x, y !== 0 ? 1 : 0] as [number, number]) : pts,
                  style: s.style,
                  axis: s.axis,
                  unit: s.unit,
                  boolean: bool,
                };
              })}
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
            {chart.series.map((s) => (
              <SeriesCard key={s.id} chart={chart} s={s} data={data[s.id]} connId={connId} />
            ))}
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
    </div>
  );
}

export { addToChart, pathLabel };

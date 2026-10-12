import { useEffect, useRef, useState } from 'react';
import uPlot from 'uplot';
import { useApp } from '../store/app';
import { fmtHm, fmtTime } from '../lib/format';
import type { ChartColor } from '../store/charts';

export interface UChartSeries {
  label: string;
  color: ChartColor;
  points: [number, number][];
  style: 'line' | 'step' | 'points';
  axis: 'left' | 'right';
  unit: string;
  boolean?: boolean;
}

export function cssColor(c: ChartColor): string {
  if (typeof document === 'undefined') return '#5E7A6E';
  const v = getComputedStyle(document.documentElement).getPropertyValue(`--c-${c}`).trim();
  return v || '#5E7A6E';
}
function cssVar(name: string, fallback: string): string {
  const v = typeof document !== 'undefined' ? getComputedStyle(document.documentElement).getPropertyValue(name).trim() : '';
  return v || fallback;
}

/** True where a canvas is not available (jsdom in unit tests). */
function noCanvas(): boolean {
  if (typeof navigator !== 'undefined' && /jsdom/i.test(navigator.userAgent)) return true;
  return typeof HTMLCanvasElement === 'undefined';
}

/** Merges per-series points onto one time axis for uPlot (nulls where a series has no point). */
export function alignSeries(series: UChartSeries[]): uPlot.AlignedData {
  const ts = new Set<number>();
  series.forEach((s) => s.points.forEach((p) => ts.add(p[0])));
  const xs = Array.from(ts).sort((a, b) => a - b);
  const index = new Map(xs.map((x, i) => [x, i]));
  const cols = series.map((s) => {
    const col: (number | null)[] = new Array(xs.length).fill(null);
    s.points.forEach(([x, y]) => (col[index.get(x)!] = y));
    return col;
  });
  return [xs, ...cols] as uPlot.AlignedData;
}

function fmtVal(v: number, s: UChartSeries): string {
  if (s.boolean) return `${v} (${v ? 'true' : 'false'})`;
  const txt = Math.abs(v) >= 100 ? v.toFixed(1) : v.toFixed(2);
  return s.unit ? `${txt} ${s.unit}` : txt;
}

/** A uPlot time-series chart themed with the palette: up to two y axes, line, step or points, hover readout (H2). */
export function UChart(props: {
  series: UChartSeries[];
  height: number;
  xRange?: [number, number] | null;
  compact?: boolean;
  onReady?(u: uPlot | null): void;
  testId?: string;
}) {
  const box = useRef<HTMLDivElement>(null);
  const plot = useRef<uPlot | null>(null);
  const [width, setWidth] = useState(0);
  const [readout, setReadout] = useState<{ left: number; top: number; text: string[]; time: number } | null>(null);
  const theme = useApp((s) => `${s.settings.theme}|${s.settings.accent}`);
  const [schemeTick, setSchemeTick] = useState(0);
  const seriesKey = props.series.map((s) => `${s.label}|${s.color}|${s.style}|${s.axis}|${s.unit}`).join(';');
  const latest = useRef(props.series);
  latest.current = props.series;

  useEffect(() => {
    if (!window.matchMedia) return;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const f = () => setSchemeTick((x) => x + 1);
    mq.addEventListener?.('change', f);
    return () => mq.removeEventListener?.('change', f);
  }, []);

  useEffect(() => {
    if (!box.current || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver((e) => setWidth(Math.floor(e[0].contentRect.width)));
    ro.observe(box.current);
    setWidth(box.current.clientWidth);
    return () => ro.disconnect();
  }, []);

  // (re)create on structure, size or theme change
  useEffect(() => {
    if (noCanvas() || !box.current || width < 50) return;
    const ink2 = cssVar('--c-ink2', '#6B6760');
    const line = cssVar('--line', 'rgba(29,29,27,.12)');
    const line2 = cssVar('--line-2', 'rgba(29,29,27,.06)');
    const hasRight = props.series.some((s) => s.axis === 'right');
    const leftUnit = props.series.find((s) => s.axis === 'left' && s.unit)?.unit ?? '';
    const rightUnit = props.series.find((s) => s.axis === 'right' && s.unit)?.unit ?? '';
    const font = `10px ${cssVar('--sans', 'sans-serif')}`;
    const axis = (side: 1 | 3, scale: string, label: string): uPlot.Axis => ({
      scale,
      side,
      stroke: ink2,
      font,
      labelFont: font,
      label: props.compact ? undefined : label || undefined,
      labelSize: label && !props.compact ? 14 : 0,
      size: props.compact ? 34 : 44,
      grid: { stroke: line2, width: 1 },
      ticks: { stroke: line, width: 1, size: 4 },
    });
    const opts: uPlot.Options = {
      width,
      height: props.height,
      ms: 1e-3 as 1e-3,
      legend: { show: false },
      cursor: { drag: { x: false, y: false }, points: { size: 7 } },
      scales: { x: { time: true }, y: { auto: true }, y2: { auto: true } },
      axes: [
        {
          stroke: ink2,
          font,
          grid: { stroke: line2, width: 1 },
          ticks: { stroke: line, width: 1, size: 4 },
          size: 28,
          values: (_u, vals) => vals.map((v) => fmtHm(v)),
        },
        axis(3, 'y', leftUnit),
        ...(hasRight ? [axis(1, 'y2', rightUnit)] : []),
      ],
      series: [
        {},
        ...props.series.map(
          (s): uPlot.Series => ({
            label: s.label,
            scale: s.axis === 'right' ? 'y2' : 'y',
            stroke: cssColor(s.color),
            width: 1.6,
            spanGaps: true,
            paths: s.style === 'step' ? uPlot.paths.stepped!({ align: 1 }) : s.style === 'points' ? () => null : undefined,
            points: { show: s.style === 'points' ? true : undefined, size: 4, fill: cssColor(s.color) },
          }),
        ),
      ],
      hooks: {
        setCursor: [
          (u) => {
            const idx = u.cursor.idx;
            if (idx === null || idx === undefined || u.cursor.left === undefined || u.cursor.left < 0) {
              setReadout(null);
              return;
            }
            const time = u.data[0][idx] as number;
            const text = latest.current.map((s, i) => {
              // last known value at or before the cursor time
              let v: number | null | undefined = null;
              for (let j = idx; j >= 0; j--) {
                const y = u.data[i + 1][j];
                if (y !== null && y !== undefined) {
                  v = y as number;
                  break;
                }
              }
              return `${s.label.padEnd(12)} ${v === null || v === undefined ? '—' : fmtVal(v, s)}`;
            });
            const left = Math.min((u.cursor.left ?? 0) + 60, width - 200);
            setReadout({ left, top: 8, text, time });
          },
        ],
      },
    };
    const u = new uPlot(opts, alignSeries(latest.current), box.current);
    plot.current = u;
    props.onReady?.(u);
    return () => {
      u.destroy();
      plot.current = null;
      props.onReady?.(null);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seriesKey, width, props.height, theme, schemeTick, props.compact]);

  // data updates
  useEffect(() => {
    const u = plot.current;
    if (!u) return;
    u.setData(alignSeries(props.series), !props.xRange);
    if (props.xRange) u.setScale('x', { min: props.xRange[0], max: props.xRange[1] });
  }, [props.series, props.xRange]);

  return (
    <div className="chartbox" ref={box} style={{ height: props.height }} data-testid={props.testId} onMouseLeave={() => setReadout(null)}>
      {readout && (
        <div className="readout" style={{ left: readout.left, top: readout.top }} data-testid="chart-readout">
          <div className="t">{fmtTime(readout.time)}</div>
          {readout.text.join('\n')}
        </div>
      )}
    </div>
  );
}

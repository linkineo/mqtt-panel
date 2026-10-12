import { create } from 'zustand';

export type ChartColor = 'sage' | 'clay' | 'slate' | 'brick' | 'ink2';
export const COLORS: ChartColor[] = ['sage', 'clay', 'slate', 'brick', 'ink2'];
export type ChartRange = '5m' | '15m' | '1h' | 'session' | 'custom';

export interface ChartSeries {
  id: string;
  name: string;
  topic: string;
  path: string;
  color: ChartColor;
  style: 'line' | 'step' | 'points';
  axis: 'left' | 'right';
  unit: string;
  enabled: boolean;
}

export interface Chart {
  id: string;
  name: string;
  connId: string;
  range: ChartRange;
  customFrom: number | null;
  customTo: number | null;
  paused: boolean;
  pausedAt: number | null;
  series: ChartSeries[];
}

export interface PickerRequest {
  connId: string;
  topic: string;
  preselect: string | null;
  /** Chart the picker adds to by default. */
  chartId: string | null;
}

interface ChartsState {
  charts: Chart[];
  activeId: string | null;
  picker: PickerRequest | null;
  openPicker(r: PickerRequest): void;
  closePicker(): void;
  setActive(id: string): void;
  createChart(connId: string, name: string): string;
  addSeries(chartId: string, items: { topic: string; path: string; name: string; step: boolean; unit?: string }[]): void;
  updateChart(id: string, p: Partial<Chart>): void;
  updateSeries(chartId: string, seriesId: string, p: Partial<ChartSeries>): void;
  removeSeries(chartId: string, seriesId: string): void;
  removeTopicField(connId: string, topic: string, path: string): void;
  duplicateChart(id: string, copySuffix: string): void;
  removeChart(id: string): void;
}

let nextId = 1;
const uid = (p: string) => `${p}${nextId++}`;

export function rangeMs(r: ChartRange): number | null {
  if (r === '5m') return 5 * 60_000;
  if (r === '15m') return 15 * 60_000;
  if (r === '1h') return 60 * 60_000;
  return null;
}

export const useCharts = create<ChartsState>((set, get) => ({
  charts: [],
  activeId: null,
  picker: null,
  openPicker: (picker) => set({ picker }),
  closePicker: () => set({ picker: null }),
  setActive: (activeId) => set({ activeId }),
  createChart: (connId, name) => {
    const id = uid('c');
    const chart: Chart = { id, name, connId, range: '1h', customFrom: null, customTo: null, paused: false, pausedAt: null, series: [] };
    set({ charts: [...get().charts, chart], activeId: get().activeId ?? id });
    return id;
  },
  addSeries: (chartId, items) =>
    set({
      charts: get().charts.map((c) => {
        if (c.id !== chartId) return c;
        const used = new Set(c.series.map((s) => s.color));
        const series = [...c.series];
        for (const it of items) {
          if (series.some((s) => s.topic === it.topic && s.path === it.path)) continue;
          const color = COLORS.find((x) => !used.has(x)) ?? COLORS[series.length % COLORS.length];
          used.add(color);
          series.push({
            id: uid('s'),
            name: it.name,
            topic: it.topic,
            path: it.path,
            color,
            style: it.step ? 'step' : 'line',
            axis: 'left',
            unit: it.unit ?? '',
            enabled: true,
          });
        }
        return { ...c, series };
      }),
    }),
  updateChart: (id, p) => set({ charts: get().charts.map((c) => (c.id === id ? { ...c, ...p } : c)) }),
  updateSeries: (chartId, seriesId, p) =>
    set({
      charts: get().charts.map((c) =>
        c.id === chartId ? { ...c, series: c.series.map((s) => (s.id === seriesId ? { ...s, ...p } : s)) } : c,
      ),
    }),
  removeSeries: (chartId, seriesId) =>
    set({ charts: get().charts.map((c) => (c.id === chartId ? { ...c, series: c.series.filter((s) => s.id !== seriesId) } : c)) }),
  removeTopicField: (connId, topic, path) =>
    set({
      charts: get().charts.map((c) =>
        c.connId === connId ? { ...c, series: c.series.filter((s) => !(s.topic === topic && s.path === path)) } : c,
      ),
    }),
  duplicateChart: (id, copySuffix) => {
    const c = get().charts.find((x) => x.id === id);
    if (!c) return;
    const copy: Chart = { ...c, id: uid('c'), name: c.name + copySuffix, series: c.series.map((s) => ({ ...s, id: uid('s') })) };
    set({ charts: [...get().charts, copy], activeId: copy.id });
  },
  removeChart: (id) => {
    const charts = get().charts.filter((c) => c.id !== id);
    set({ charts, activeId: get().activeId === id ? (charts[0]?.id ?? null) : get().activeId });
  },
}));

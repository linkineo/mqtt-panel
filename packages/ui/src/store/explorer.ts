import { create } from 'zustand';
import type { Filters, SavedView, TableColumn, TableSort } from '@mqttpanel/engine-client';
import { engine } from '../services';
import { useApp } from './app';
import { saveProfile } from '../lib/actions';

export const ALL_COLUMNS: TableColumn[] = ['pin', 'topic', 'value', 'spark', 'updated', 'rate', 'messages', 'qos', 'retained', 'size'];
export const EMPTY_FILTERS: Filters = { retainedOnly: false, changedWithinMin: null, payloadContains: '', qos: null, hasValue: false };

/** Detail pane tabs (change set 8): Payload (with the diff switch, the topic chart and History) and Properties. */
export type DetailTab = 'payload' | 'properties';

export interface ExplorerConn {
  layout: 'tree' | 'table';
  viewName: string;
  search: string;
  filters: Filters;
  columns: TableColumn[];
  sort: TableSort;
  pinned: string[];
  selected: string | null;
  follow: boolean;
  tab: DetailTab;
  /** Message seq shown instead of the latest one (picked from History). */
  viewSeq: number | null;
  /** "Show diff to previous" above the payload (change set 8), on by default. */
  diff: boolean;
  /** Change set 9: the payload view's "JSON / Tree" segment, also used by the rows expanded in History. */
  payloadForm: PayloadForm;
  /** Change set 9: the History field filter per topic (a field path), kept while the app runs. */
  historyFilter: Record<string, string>;
  initialised: boolean;
}

export type PayloadForm = 'json' | 'tree';

export type ExplorerCommand = 'focusSearch' | 'nextChanged' | 'togglePause' | 'pinSelected' | 'publishSelected' | 'chartSelected' | 'exportSelected' | 'clearHistory';

interface ExplorerState {
  byConn: Record<string, ExplorerConn>;
  command: { name: ExplorerCommand; nonce: number } | null;
  get(connId: string): ExplorerConn;
  patch(connId: string, p: Partial<ExplorerConn>): void;
  init(connId: string): Promise<void>;
  select(connId: string, topic: string | null): void;
  togglePin(connId: string, topic: string): Promise<void>;
  applyView(connId: string, view: SavedView): Promise<void>;
  saveView(connId: string, name: string, replace?: string): Promise<void>;
  deleteView(connId: string, name: string): Promise<void>;
  run(name: ExplorerCommand): void;
  setHistoryFilter(connId: string, topic: string, path: string | null): void;
}

export function defaultConn(): ExplorerConn {
  return {
    layout: 'tree',
    viewName: 'Default',
    search: '',
    filters: { ...EMPTY_FILTERS },
    columns: [...ALL_COLUMNS],
    sort: { column: 'topic', dir: 'asc' },
    pinned: [],
    selected: null,
    follow: true,
    tab: 'payload',
    viewSeq: null,
    diff: true,
    payloadForm: 'json',
    historyFilter: {},
    initialised: false,
  };
}

export function filterCount(f: Filters): number {
  return (f.retainedOnly ? 1 : 0) + (f.changedWithinMin !== null ? 1 : 0) + (f.payloadContains ? 1 : 0) + (f.qos !== null ? 1 : 0) + (f.hasValue ? 1 : 0);
}

function profileOf(connId: string) {
  return useApp.getState().profiles.find((p) => p.id === connId) ?? null;
}

const defaults = new Map<string, ExplorerConn>();

export const useExplorer = create<ExplorerState>((set, get) => ({
  byConn: {},
  command: null,
  get: (connId) => {
    const c = get().byConn[connId];
    if (c) return c;
    let d = defaults.get(connId);
    if (!d) {
      d = defaultConn();
      defaults.set(connId, d);
    }
    return d;
  },
  patch: (connId, p) => set({ byConn: { ...get().byConn, [connId]: { ...get().get(connId), ...p } } }),
  init: async (connId) => {
    if (get().byConn[connId]?.initialised) return;
    const profile = profileOf(connId);
    const view = profile?.views.find((v) => v.name === 'Default') ?? profile?.views[0];
    get().patch(connId, { initialised: true });
    if (view) await get().applyView(connId, view);
  },
  select: (connId, topic) => get().patch(connId, { selected: topic, viewSeq: null }),
  togglePin: async (connId, topic) => {
    const st = get().get(connId);
    const pinned = st.pinned.includes(topic) ? st.pinned.filter((x) => x !== topic) : [...st.pinned, topic];
    get().patch(connId, { pinned });
    await engine().setPinned(connId, pinned);
    // pins belong to the current view and are saved with the profile
    const profile = profileOf(connId);
    if (profile) {
      const views = profile.views.map((v) => (v.name === st.viewName ? { ...v, pinned } : v));
      await saveProfile({ ...profile, views });
    }
  },
  applyView: async (connId, view) => {
    get().patch(connId, {
      viewName: view.name,
      layout: view.layout,
      search: view.search,
      filters: { ...EMPTY_FILTERS, ...view.filters },
      columns: view.columns.length ? view.columns : [...ALL_COLUMNS],
      sort: view.sort,
      pinned: view.pinned,
    });
    try {
      await engine().setExpandedSet(connId, view.expanded);
      await engine().setPinned(connId, view.pinned);
    } catch {
      /* connection closed */
    }
  },
  saveView: async (connId, name, replace) => {
    const profile = profileOf(connId);
    if (!profile) return;
    const st = get().get(connId);
    const expanded = await engine().getExpanded(connId);
    const view: SavedView = {
      name,
      layout: st.layout,
      search: st.search,
      filters: st.filters,
      expanded,
      pinned: st.pinned,
      columns: st.columns,
      sort: st.sort,
    };
    const target = replace ?? name;
    const exists = profile.views.some((v) => v.name === target);
    const views = exists ? profile.views.map((v) => (v.name === target ? view : v)) : [...profile.views, view];
    await saveProfile({ ...profile, views });
    get().patch(connId, { viewName: name });
  },
  deleteView: async (connId, name) => {
    const profile = profileOf(connId);
    if (!profile || name === 'Default') return;
    await saveProfile({ ...profile, views: profile.views.filter((v) => v.name !== name) });
    const def = profile.views.find((v) => v.name === 'Default');
    if (def) await get().applyView(connId, def);
  },
  run: (name) => set({ command: { name, nonce: Date.now() + Math.random() } }),
  setHistoryFilter: (connId, topic, path) => {
    const historyFilter = { ...get().get(connId).historyFilter };
    if (path) historyFilter[topic] = path;
    else delete historyFilter[topic];
    get().patch(connId, { historyFilter });
  },
}));

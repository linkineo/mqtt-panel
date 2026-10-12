import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import type { Filters, Qos, TableColumn, TopicDetail } from '@mqttpanel/engine-client';
import { engine, platform } from '../../services';
import { isMac, useApp } from '../../store/app';
import { ALL_COLUMNS, EMPTY_FILTERS, filterCount, useExplorer } from '../../store/explorer';
import { addToChart, copyTopic, clearHistory, publishHere, topicMenu, type TopicDialog } from '../../lib/topicActions';
import { modHeld, typingTarget } from '../../lib/keys';
import { Check, Kbd, Popover, PromptDialog, ConfirmDialog, Seg } from '../../components/ui';
import { NoConnection } from '../../components/NoConnection';
import { DeleteRetainedDialog, DeleteSubtreeDialog } from '../../components/DeleteRetainedDialog';
import { fmtNum, shortTopic } from '../../lib/format';
import { Tree } from './Tree';
import { Table, colLabel } from './Table';
import { Detail } from './Detail';
import { ExportDialog } from './ExportDialog';

function FiltersPopover({ filters, onChange, anchor, onClose }: { filters: Filters; onChange(f: Filters): void; anchor: HTMLElement | null; onClose(): void }) {
  const { t } = useTranslation();
  const [mins, setMins] = useState(String(filters.changedWithinMin ?? 10));
  return (
    <Popover anchor={anchor} onClose={onClose} testId="filters" label={t('explore.filters')}>
      <div style={{ display: 'grid', gap: 8, minWidth: 300 }}>
        <Check on={filters.retainedOnly} label={t('filters.retained')} onChange={(v) => onChange({ ...filters, retainedOnly: v })} />
        <div className="row">
          <Check
            on={filters.changedWithinMin !== null}
            label={t('filters.changed')}
            onChange={(v) => onChange({ ...filters, changedWithinMin: v ? Math.max(1, Number(mins) || 10) : null })}
          />
          <input
            className="input mono"
            style={{ width: 56 }}
            value={mins}
            aria-label={t('filters.minutes')}
            onChange={(e) => {
              const v = e.target.value.replace(/\D/g, '');
              setMins(v);
              if (filters.changedWithinMin !== null && Number(v) > 0) onChange({ ...filters, changedWithinMin: Number(v) });
            }}
          />
          <span className="hint">min</span>
        </div>
        <div className="row">
          <span className="small nowrap">{t('filters.contains')}</span>
          <input className="input mono" style={{ flex: 1 }} value={filters.payloadContains} aria-label={t('filters.contains')} onChange={(e) => onChange({ ...filters, payloadContains: e.target.value })} />
        </div>
        <div className="row">
          <span className="small nowrap">QoS</span>
          <Seg<string>
            label="QoS"
            value={filters.qos === null ? 'any' : String(filters.qos)}
            onChange={(v) => onChange({ ...filters, qos: v === 'any' ? null : (Number(v) as Qos) })}
            options={[
              { value: 'any', label: t('filters.any') },
              { value: '0', label: '0' },
              { value: '1', label: '1' },
              { value: '2', label: '2' },
            ]}
          />
        </div>
        <Check on={filters.hasValue} label={t('filters.hasValue')} onChange={(v) => onChange({ ...filters, hasValue: v })} />
        <div className="row" style={{ justifyContent: 'flex-end' }}>
          <button type="button" className="btn sm" onClick={() => onChange({ ...EMPTY_FILTERS })}>
            {t('filters.clear')}
          </button>
        </div>
      </div>
    </Popover>
  );
}

function ColumnsPopover({ columns, onChange, anchor, onClose }: { columns: TableColumn[]; onChange(c: TableColumn[]): void; anchor: HTMLElement | null; onClose(): void }) {
  const { t } = useTranslation();
  const order = [...columns, ...ALL_COLUMNS.filter((c) => !columns.includes(c))];
  const move = (c: TableColumn, d: -1 | 1) => {
    const i = columns.indexOf(c);
    const j = i + d;
    if (i < 0 || j < 0 || j >= columns.length) return;
    const n = [...columns];
    [n[i], n[j]] = [n[j], n[i]];
    onChange(n);
  };
  return (
    <Popover anchor={anchor} onClose={onClose} align="end" testId="columns" label={t('explore.columns')}>
      <div style={{ display: 'grid', gap: 4, minWidth: 220 }}>
        {order.map((c) => (
          <div key={c} className="row">
            <Check
              on={columns.includes(c)}
              disabled={c === 'topic'}
              label={c === 'pin' ? t('col.pinned') : colLabel(c, t)}
              onChange={(v) => onChange(v ? [...columns, c] : columns.filter((x) => x !== c))}
            />
            <span className="grow" />
            <button type="button" className="iconbtn" aria-label={t('explore.moveLeft', { col: colLabel(c, t) })} disabled={!columns.includes(c)} onClick={() => move(c, -1)}>
              ↑
            </button>
            <button type="button" className="iconbtn" aria-label={t('explore.moveRight', { col: colLabel(c, t) })} disabled={!columns.includes(c)} onClick={() => move(c, 1)}>
              ↓
            </button>
          </div>
        ))}
      </div>
    </Popover>
  );
}

function PinnedRow({ connId, pinned, onSelect }: { connId: string; pinned: string[]; onSelect(topic: string): void }) {
  const { t } = useTranslation();
  const seq = useApp((s) => s.seqs[connId] ?? 0);
  const selected = useExplorer((s) => s.get(connId).selected);
  const [values, setValues] = useState<Record<string, string>>({});
  const last = useRef(0);
  useEffect(() => {
    if (!pinned.length) return;
    if (Date.now() - last.current < 1000) return;
    last.current = Date.now();
    void Promise.all(pinned.map((p) => engine().topicDetail(connId, p).catch(() => null as TopicDetail | null))).then((ds) => {
      const out: Record<string, string> = {};
      ds.forEach((d, i) => {
        if (!d?.last) return;
        const j = d.last.decoded.json as Record<string, unknown> | null;
        const v = j && typeof j === 'object' && 'value' in j ? String(j.value) : (d.last.decoded.text ?? '');
        out[pinned[i]] = v.length > 16 ? v.slice(0, 16) + '…' : v;
      });
      setValues(out);
    });
  }, [connId, pinned, seq]);
  return (
    <div className="row" style={{ gap: 6, padding: '8px 14px', borderBottom: '1px solid var(--line)', flexWrap: 'nowrap', overflow: 'hidden' }} data-testid="pinned-row">
      <span className="muted small">{t('explore.pinnedLabel')}</span>
      {pinned.map((p) => (
        <button key={p} type="button" className="chip on mono" title={p} onClick={() => onSelect(p)}>
          {shortTopic(p)} {values[p] ?? ''}
        </button>
      ))}
      <button
        type="button"
        className="chip mono"
        disabled={!selected || pinned.includes(selected)}
        onClick={() => selected && void useExplorer.getState().togglePin(connId, selected)}
        data-testid="add-pin"
      >
        {t('explore.addPin')}
      </button>
    </div>
  );
}

export function ExplorerScreen() {
  const connId = useApp((s) => s.activeConnId);
  const conn = useApp((s) => s.connections.find((c) => c.id === s.activeConnId) ?? null);
  if (!connId || !conn) return <NoConnection />;
  return <Explorer key={connId} connId={connId} />;
}

function Explorer({ connId }: { connId: string }) {
  const { t } = useTranslation();
  const info = useApp((s) => s.appInfo);
  const profile = useApp((s) => s.profiles.find((p) => p.id === connId) ?? null);
  const st = useExplorer((s) => s.get(connId));
  const patch = useExplorer((s) => s.patch);
  const command = useExplorer((s) => s.command);
  const [search, setSearch] = useState(st.search);
  const [pop, setPop] = useState<'filters' | 'columns' | null>(null);
  const [dialog, setDialog] = useState<TopicDialog | null>(null);
  const [prompt, setPrompt] = useState<'saveAs' | 'rename' | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [version, setVersion] = useState(0);
  const [counts, setCounts] = useState({ total: 0, inView: 0, visible: [] as string[] });
  const [detailW, setDetailW] = useState(520);
  const searchRef = useRef<HTMLInputElement>(null);
  const filtersBtn = useRef<HTMLButtonElement>(null);
  const columnsBtn = useRef<HTMLButtonElement>(null);
  const revealRef = useRef<((topic: string) => void) | null>(null);
  const mac = isMac(info);
  const onCounts = useCallback((total: number, inView: number, visible: string[]) => setCounts({ total, inView, visible }), []);

  useEffect(() => {
    void useExplorer.getState().init(connId).then(() => setVersion((v) => v + 1));
  }, [connId]);
  useEffect(() => setSearch(st.search), [st.search]);
  useEffect(() => {
    const h = setTimeout(() => search !== st.search && patch(connId, { search }), 150);
    return () => clearTimeout(h);
  }, [search, st.search, connId, patch]);

  const query = { search: st.search, filters: st.filters };
  const select = useCallback((path: string) => useExplorer.getState().select(connId, path), [connId]);
  const reveal = (topic: string) => {
    select(topic);
    if (useExplorer.getState().get(connId).layout === 'tree') revealRef.current?.(topic);
  };

  const runCommand = useCallback(
    async (name: string) => {
      const cur = useExplorer.getState().get(connId);
      const sel = cur.selected;
      if (name === 'focusSearch') searchRef.current?.focus();
      else if (name === 'togglePause') patch(connId, { follow: !cur.follow });
      else if (name === 'nextChanged') {
        const next = await engine().nextChanged(connId, sel, 60_000);
        if (next) reveal(next);
      } else if (name === 'pinSelected' && sel) void useExplorer.getState().togglePin(connId, sel);
      else if (name === 'publishSelected' && sel) void publishHere(connId, sel);
      else if (name === 'chartSelected' && sel) addToChart(connId, sel);
      else if (name === 'exportSelected') setDialog({ kind: 'export', topic: sel ?? '', scope: sel ? 'subtree' : 'visible' });
      else if (name === 'clearHistory') void clearHistory(connId, null);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [connId],
  );
  useEffect(() => {
    if (command) void runCommand(command.name);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [command?.nonce]);

  // "/" focuses search from anywhere on this screen
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key === '/' && !typingTarget(e) && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        searchRef.current?.focus();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  /** Single-key shortcuts on the focused tree or table (S2). */
  const onListKey = (e: KeyboardEvent, row: { path: string } | null): boolean => {
    const k = e.key;
    if (modHeld(info, e) && k.toLowerCase() === 'c' && row) {
      e.preventDefault();
      void copyTopic(row.path);
      return true;
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return false;
    if (k === 'P' && e.shiftKey && row) {
      e.preventDefault();
      void publishHere(connId, row.path);
      return true;
    }
    if (k.toLowerCase() === 'p' && row) {
      e.preventDefault();
      void useExplorer.getState().togglePin(connId, row.path);
      return true;
    }
    if (k.toLowerCase() === 'c' && row) {
      e.preventDefault();
      addToChart(connId, row.path);
      return true;
    }
    if (k.toLowerCase() === 'j') {
      e.preventDefault();
      void runCommand('nextChanged');
      return true;
    }
    if (k === ' ') {
      e.preventDefault();
      patch(connId, { follow: !useExplorer.getState().get(connId).follow });
      return true;
    }
    return false;
  };

  const onMenu = (path: string, isTopic: boolean, x: number, y: number) => {
    select(path);
    void topicMenu(connId, path, isTopic, x, y, mac, setDialog);
  };

  const viewMenu = async (e: React.MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const views = profile?.views ?? [];
    const id = await platform().showContextMenu(
      [
        ...views.map((v) => ({ id: `view:${v.name}`, label: (v.name === st.viewName ? '✓ ' : '') + (v.name === 'Default' ? t('explore.defaultView') : v.name) })),
        { id: 's', label: '', separator: true },
        { id: 'save', label: t('views.save') },
        { id: 'saveAs', label: t('views.saveAs') },
        { id: 'rename', label: t('views.rename'), enabled: st.viewName !== 'Default' },
        { id: 'delete', label: t('views.delete'), enabled: st.viewName !== 'Default', danger: true },
      ],
      r.left,
      r.bottom + 4,
    );
    if (!id) return;
    if (id.startsWith('view:')) {
      const v = views.find((x) => x.name === id.slice(5));
      if (v) {
        await useExplorer.getState().applyView(connId, v);
        setVersion((x) => x + 1);
      }
    } else if (id === 'save') {
      await useExplorer.getState().saveView(connId, st.viewName);
      useApp.getState().toast({ kind: 'info', text: t('views.saved') });
    } else if (id === 'saveAs') setPrompt('saveAs');
    else if (id === 'rename') setPrompt('rename');
    else if (id === 'delete') setConfirmDelete(true);
  };

  const expandMenu = async (e: React.MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const id = await platform().showContextMenu(
      [
        { id: 'node', label: t('expand.node'), enabled: !!st.selected },
        { id: 'd1', label: t('expand.depth', { n: 1 }) },
        { id: 'd2', label: t('expand.depth', { n: 2 }) },
        { id: 'd3', label: t('expand.depth', { n: 3 }) },
        { id: 'changed', label: t('expand.changed') },
        { id: 's', label: '', separator: true },
        { id: 'collapse', label: t('expand.collapseAll') },
      ],
      r.left,
      r.bottom + 4,
    );
    if (!id) return;
    if (id === 'node' && st.selected) await engine().setExpanded(connId, st.selected, true);
    else if (id.startsWith('d')) await engine().expandToDepth(connId, Number(id.slice(1)));
    else if (id === 'changed') await engine().expandChanged(connId, 60_000);
    else if (id === 'collapse') await engine().collapseAll(connId);
    setVersion((v) => v + 1);
  };

  const fc = filterCount(st.filters);
  const viewLabel = st.viewName === 'Default' ? t('explore.defaultView') : st.viewName;

  const startResize = (e: React.PointerEvent) => {
    const x0 = e.clientX;
    const w0 = detailW;
    const move = (ev: PointerEvent) => setDetailW(Math.max(440, Math.min(560, w0 - (ev.clientX - x0))));
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  return (
    <div className="cols" style={{ gridTemplateColumns: st.selected ? `minmax(0,1fr) 0 ${detailW}px` : 'minmax(0,1fr)' }}>
      <div className="pane" data-testid="explorer">
        <div className="ph">
          <label className="search">
            <input
              ref={searchRef}
              className="mono"
              value={search}
              placeholder={t('explore.search')}
              aria-label={t('explore.search')}
              data-testid="search"
              onChange={(e) => setSearch(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Escape') {
                  setSearch('');
                  patch(connId, { search: '' });
                }
              }}
            />
            <Kbd>/</Kbd>
          </label>
          <Seg
            label={t('explore.layout')}
            value={st.layout}
            onChange={(layout) => {
              patch(connId, { layout });
              if (layout === 'tree' && st.selected) setTimeout(() => revealRef.current?.(st.selected!), 50);
            }}
            options={[
              { value: 'tree', label: t('explore.tree'), testId: 'layout-tree' },
              { value: 'table', label: t('explore.table'), testId: 'layout-table' },
            ]}
          />
          <button type="button" className="seg" style={{ padding: 0, background: 'none' }} onClick={(e) => void viewMenu(e)} aria-label={t('views.menu')} data-testid="views">
            <span className="on" style={{ maxWidth: 160, overflow: 'hidden', textOverflow: 'ellipsis' }}>
              {viewLabel}
            </span>
            <span>▾</span>
          </button>
          <button ref={filtersBtn} type="button" className="btn sm" data-testid="filters-btn" onClick={() => setPop(pop === 'filters' ? null : 'filters')}>
            {fc ? t('explore.filtersN', { n: fc }) : t('explore.filters')}
          </button>
          {st.layout === 'tree' ? (
            <button type="button" className="btn sm" onClick={(e) => void expandMenu(e)} data-testid="expand-menu">
              {t('explore.expand')}
            </button>
          ) : (
            <button ref={columnsBtn} type="button" className="btn sm" data-testid="columns-btn" onClick={() => setPop(pop === 'columns' ? null : 'columns')}>
              {t('explore.columns')}
            </button>
          )}
          <button type="button" className="btn sm" data-testid="export-btn" onClick={() => setDialog({ kind: 'export', topic: st.selected ?? '', scope: st.layout === 'table' ? 'visible' : st.selected ? 'subtree' : 'visible' })}>
            {t('explore.export')}
          </button>
        </div>
        {st.layout === 'tree' && <PinnedRow connId={connId} pinned={st.pinned} onSelect={reveal} />}
        {st.layout === 'tree' ? (
          <Tree
            connId={connId}
            query={query}
            selected={st.selected}
            onSelect={(p) => select(p)}
            onMenu={(row, x, y) => onMenu(row.path, row.isTopic, x, y)}
            onKey={onListKey}
            revealRef={revealRef}
            version={version}
          />
        ) : (
          <>
            <Table
              connId={connId}
              query={{ ...query, sort: st.sort }}
              columns={st.columns}
              selected={st.selected}
              onSort={(sort) => patch(connId, { sort })}
              onSelect={select}
              onMenu={(row, x, y) => onMenu(row.path, true, x, y)}
              onKey={onListKey}
              onCounts={onCounts}
            />
            <div className="muted small nowrap" style={{ padding: '8px 14px', borderTop: '1px solid var(--line)' }} data-testid="table-footer">
              {t('explore.inView', { count: counts.inView, n: fmtNum(counts.inView) })}
              {fc > 0 ? ' · ' + t('explore.matchFilters', { count: counts.total, n: fmtNum(counts.total) }) : ''}
            </div>
          </>
        )}
        {pop === 'filters' && <FiltersPopover filters={st.filters} onChange={(filters) => patch(connId, { filters })} anchor={filtersBtn.current} onClose={() => setPop(null)} />}
        {pop === 'columns' && <ColumnsPopover columns={st.columns} onChange={(columns) => patch(connId, { columns })} anchor={columnsBtn.current} onClose={() => setPop(null)} />}
      </div>
      {st.selected && (
        <>
          <div className="splitter" role="separator" aria-orientation="vertical" aria-label={t('explore.resize')} tabIndex={0} onPointerDown={startResize}
            onKeyDown={(e) => {
              if (e.key === 'ArrowLeft') setDetailW((w) => Math.min(560, w + 20));
              if (e.key === 'ArrowRight') setDetailW((w) => Math.max(440, w - 20));
            }}
          />
          <Detail connId={connId} topic={st.selected} openDialog={setDialog} />
        </>
      )}
      {dialog?.kind === 'export' && (
        <ExportDialog
          connId={connId}
          topic={dialog.topic || null}
          initialScope={dialog.topic ? dialog.scope : 'visible'}
          visibleTopics={counts.visible}
          onClose={() => setDialog(null)}
        />
      )}
      {dialog?.kind === 'deleteRetained' && <DeleteRetainedDialog connId={connId} topic={dialog.topic} onClose={() => setDialog(null)} />}
      {dialog?.kind === 'deleteSubtree' && <DeleteSubtreeDialog connId={connId} prefix={dialog.topic} onClose={() => setDialog(null)} />}
      {prompt && (
        <PromptDialog
          title={prompt === 'saveAs' ? t('views.saveAsTitle') : t('views.renameTitle')}
          label={t('views.name')}
          initial={prompt === 'rename' ? st.viewName : ''}
          confirmLabel={t('common.save')}
          onClose={() => setPrompt(null)}
          onConfirm={async (name) => {
            if (prompt === 'saveAs') await useExplorer.getState().saveView(connId, name);
            else await useExplorer.getState().saveView(connId, name, st.viewName);
          }}
        />
      )}
      {confirmDelete && (
        <ConfirmDialog
          title={t('views.deleteTitle', { name: st.viewName })}
          confirmLabel={t('common.delete')}
          danger
          onClose={() => setConfirmDelete(false)}
          onConfirm={async () => {
            await useExplorer.getState().deleteView(connId, st.viewName);
            setVersion((v) => v + 1);
          }}
        />
      )}
    </div>
  );
}

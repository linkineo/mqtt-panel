import { memo, useEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useTranslation } from 'react-i18next';
import type { TreeQuery, TreeRow } from '@mqttpanel/engine-client';
import { engine } from '../../services';
import { useApp } from '../../store/app';
import { useSlice } from '../../lib/useSlice';
import { Sparkline } from '../../components/ui';
import { fmtNum, fmtRate } from '../../lib/format';

export const FLASH_MS = 1200;

/**
 * One tree row (mockup 04, change set 8): [twisty + name] [payload on the available width, with the R marker]
 * [sparkline of the tracked value, messages since connect, throughput, subtopics]. The row flashes `.new` for 1.2 s
 * when lastReceivedAt changes (X-04); under reduced motion the highlight switches on and off without any fade.
 */
export const TreeRowView = memo(function TreeRowView(props: {
  row: TreeRow;
  selected: boolean;
  cursor: boolean;
  reducedMotion: boolean;
  valuesInTree: boolean;
  now: number;
  onClick?(e: MouseEvent): void;
  onTwisty?(): void;
  onContextMenu?(e: MouseEvent): void;
}) {
  const { t } = useTranslation();
  const { row } = props;
  const [flash, setFlash] = useState(false);
  const prev = useRef(row.lastReceivedAt);
  const prevPath = useRef(row.path);
  useEffect(() => {
    if (prevPath.current !== row.path) {
      prevPath.current = row.path;
      prev.current = row.lastReceivedAt;
      return;
    }
    if (row.lastReceivedAt !== null && prev.current !== null && row.lastReceivedAt !== prev.current) {
      setFlash(true);
      const h = setTimeout(() => setFlash(false), FLASH_MS);
      prev.current = row.lastReceivedAt;
      return () => clearTimeout(h);
    }
    prev.current = row.lastReceivedAt;
  }, [row.lastReceivedAt, row.path]);

  const cls = ['n', props.selected ? 'sel' : '', flash ? 'new' : '', flash && props.reducedMotion ? 'still' : '', props.cursor ? 'cursor' : ''].filter(Boolean).join(' ');
  // topics below this one (the node itself is not counted)
  const subtopics = row.topicCount - (row.isTopic ? 1 : 0);
  const showValue = props.valuesInTree && row.isTopic && row.value !== null;
  return (
    <div
      className={cls}
      style={{ paddingLeft: 8 + row.depth * 16 }}
      role="treeitem"
      aria-level={row.depth + 1}
      aria-expanded={row.hasChildren ? row.expanded : undefined}
      aria-selected={props.selected}
      data-testid="tree-row"
      data-path={row.path}
      onClick={props.onClick}
      onContextMenu={props.onContextMenu}
    >
      <span className="k">
        <span
          className="tw"
          aria-hidden="true"
          onClick={(e) => {
            if (!row.hasChildren) return;
            e.stopPropagation();
            props.onTwisty?.();
          }}
        >
          {row.hasChildren ? (row.expanded ? '▾' : '▸') : ''}
        </span>
        <span className="name ell">{row.name}</span>
        {row.pinned && <span className="muted" aria-label={t('explore.pinned')}>★</span>}
      </span>
      <span className="p" data-testid="tree-payload">
        {showValue && (
          <em className="selectable" title={row.value ?? undefined}>
            {row.value}
          </em>
        )}
        {row.isTopic && row.retained && <span className="chip">R</span>}
        {!row.isTopic && row.formatHint && <span className="hintfmt">{row.formatHint}</span>}
      </span>
      <span className="v">
        {props.valuesInTree && row.isTopic && row.spark.length > 1 && <Sparkline values={row.spark} times={row.sparkTimes} />}
        <span className="msgs" title={t('tree.messagesTip')} data-testid="tree-msgs">
          {fmtNum(row.messages)}
        </span>
        <span className="rate" title={t('tree.rateTip')} data-testid="tree-rate">
          {t('tree.rate', { rate: fmtRate(row.rate) })}
        </span>
        {row.hasChildren && (
          <span className="cnt" title={t('tree.subtopicsTip')} data-testid="tree-subtopics">
            {fmtNum(subtopics)}
          </span>
        )}
      </span>
    </div>
  );
});

export function Tree(props: {
  connId: string;
  query: TreeQuery;
  selected: string | null;
  onSelect(path: string, isTopic: boolean): void;
  onMenu(row: TreeRow, x: number, y: number): void;
  onKey(e: KeyboardEvent, row: TreeRow | null): boolean;
  revealRef: React.MutableRefObject<((topic: string) => void) | null>;
  version: number;
}) {
  const { t } = useTranslation();
  const seq = useApp((s) => s.seqs[props.connId] ?? 0);
  const reducedMotion = useApp((s) => s.reducedMotion);
  const valuesInTree = useApp((s) => s.settings.valuesInTree);
  const compact = useApp((s) => s.settings.density === 'compact');
  const rowH = compact ? 22 : 26;
  const scrollRef = useRef<HTMLDivElement>(null);
  const [cursor, setCursor] = useState(0);
  const [bump, setBump] = useState(0);
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const h = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(h);
  }, []);

  const qkey = props.connId + '|' + JSON.stringify(props.query);
  const [range, setRange] = useState({ start: 0, end: 60 });
  const slice = useSlice<TreeRow>((s, c) => engine().treeSlice(props.connId, props.query, s, c) as never, qkey, range.start, range.end, seq + (bump + props.version) * 1e9);
  const virt = useVirtualizer({ count: slice.total, getScrollElement: () => scrollRef.current, estimateSize: () => rowH, overscan: 12 });
  const items = virt.getVirtualItems();
  const vStart = items[0]?.index ?? 0;
  const vEnd = items[items.length - 1]?.index ?? 60;
  useEffect(() => {
    if (vStart !== range.start || vEnd !== range.end) setRange({ start: vStart, end: Math.max(vEnd, vStart + 40) });
  }, [vStart, vEnd, range.start, range.end]);

  const rowAt = (i: number): TreeRow | null => slice.rows[i - slice.start] ?? null;
  const refresh = () => setBump((b) => b + 1);

  props.revealRef.current = (topic: string) => {
    void engine()
      .revealTopic(props.connId, topic)
      .then((i) => {
        refresh();
        if (i >= 0) {
          setCursor(i);
          setTimeout(() => virt.scrollToIndex(i, { align: 'auto' }), 50);
        }
      });
  };

  const toggle = async (row: TreeRow) => {
    await engine().setExpanded(props.connId, row.path, !row.expanded);
    refresh();
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const row = rowAt(cursor);
    if (props.onKey(e, row)) return;
    const move = (i: number) => {
      const j = Math.max(0, Math.min(slice.total - 1, i));
      setCursor(j);
      virt.scrollToIndex(j, { align: 'auto' });
    };
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      move(cursor + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      move(cursor - 1);
    } else if (e.key === 'Home') {
      e.preventDefault();
      move(0);
    } else if (e.key === 'End') {
      e.preventDefault();
      move(slice.total - 1);
    } else if (e.key === 'PageDown' || e.key === 'PageUp') {
      e.preventDefault();
      const page = Math.max(1, Math.floor((scrollRef.current?.clientHeight ?? 300) / rowH) - 1);
      move(cursor + (e.key === 'PageDown' ? page : -page));
    } else if (e.key === 'ArrowRight' && row) {
      e.preventDefault();
      if (row.hasChildren && !row.expanded) void toggle(row);
      else if (row.hasChildren) move(cursor + 1);
    } else if (e.key === 'ArrowLeft' && row) {
      e.preventDefault();
      if (row.hasChildren && row.expanded) void toggle(row);
      else {
        for (let i = cursor - 1; i >= slice.start; i--) {
          const r = rowAt(i);
          if (r && r.depth < row.depth) {
            move(i);
            break;
          }
        }
      }
    } else if (e.key === 'Enter' && row) {
      e.preventDefault();
      props.onSelect(row.path, row.isTopic);
      if (row.hasChildren && !row.isTopic) void toggle(row);
    }
  };

  return (
    <div
      ref={scrollRef}
      className="pc tree"
      style={{ padding: '4px 6px' }}
      role="tree"
      aria-label={t('explore.tree')}
      tabIndex={0}
      data-testid="tree"
      onKeyDown={onKeyDown}
    >
      {slice.total === 0 ? (
        <div className="empty small" style={{ fontFamily: 'var(--sans)' }}>
          {props.query.search || Object.values(props.query.filters).some((v) => v !== false && v !== null && v !== '') ? t('explore.noMatch') : t('explore.waiting')}
        </div>
      ) : (
        <div className="vlist" style={{ height: virt.getTotalSize() }}>
          {items.map((it) => {
            const row = rowAt(it.index);
            return (
              <div key={it.key} className="vrow" style={{ top: it.start, height: rowH }}>
                {row && (
                  <TreeRowView
                    row={row}
                    selected={props.selected === row.path}
                    cursor={cursor === it.index}
                    reducedMotion={reducedMotion}
                    valuesInTree={valuesInTree}
                    now={now}
                    onClick={() => {
                      setCursor(it.index);
                      props.onSelect(row.path, row.isTopic);
                      if (row.hasChildren && !row.isTopic) void toggle(row);
                    }}
                    onTwisty={() => void toggle(row)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      setCursor(it.index);
                      props.onMenu(row, e.clientX, e.clientY);
                    }}
                  />
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

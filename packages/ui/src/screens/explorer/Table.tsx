import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useTranslation } from 'react-i18next';
import type { TableColumn, TableQuery, TableRow, TableSort } from '@mqttpanel/engine-client';
import { engine } from '../../services';
import { useApp } from '../../store/app';
import { useSlice } from '../../lib/useSlice';
import { Sparkline } from '../../components/ui';
import { fmtAge, fmtBytes, fmtNum, fmtRate } from '../../lib/format';

export const COL_WIDTH: Record<TableColumn, string> = {
  pin: '32px',
  topic: 'minmax(220px, 2.2fr)',
  value: 'minmax(90px, 1fr)',
  spark: '132px',
  updated: '104px',
  rate: '68px',
  messages: '140px',
  qos: '48px',
  retained: '48px',
  size: '64px',
};

export function colLabel(c: TableColumn, t: (k: string) => string): string {
  return c === 'pin' ? '⌖' : t(`col.${c}`);
}

function cell(c: TableColumn, r: TableRow, now: number) {
  switch (c) {
    case 'pin':
      return <div key={c}>{r.pinned ? '★' : ''}</div>;
    case 'topic':
      return (
        <div key={c} className="mono selectable" title={r.path}>
          <span className="ell">{r.path}</span>
        </div>
      );
    case 'value':
      return (
        <div key={c} className="mono selectable">
          <span className="ell">{r.value ?? ''}</span>
        </div>
      );
    case 'spark':
      return <div key={c}>{r.spark.length > 1 && <Sparkline values={r.spark} times={r.sparkTimes} />}</div>;
    case 'updated':
      return (
        <div key={c} className="mono">
          {r.lastReceivedAt ? fmtAge(now - r.lastReceivedAt) : ''}
        </div>
      );
    case 'rate':
      return (
        <div key={c} className="mono">
          {r.rate > 0 ? `${fmtRate(r.rate)}/s` : '—'}
        </div>
      );
    case 'messages':
      return (
        <div key={c} className="mono">
          {fmtNum(r.messages)}
        </div>
      );
    case 'qos':
      return <div key={c}>{r.qos ?? ''}</div>;
    case 'retained':
      return <div key={c}>{r.retained ? 'R' : ''}</div>;
    case 'size':
      return <div key={c}>{fmtBytes(r.size)}</div>;
  }
}

export function Table(props: {
  connId: string;
  query: TableQuery;
  columns: TableColumn[];
  selected: string | null;
  onSort(s: TableSort): void;
  onSelect(path: string): void;
  onMenu(row: TableRow, x: number, y: number): void;
  onKey(e: KeyboardEvent, row: TableRow | null): boolean;
  onCounts(total: number, inView: number, visible: string[]): void;
}) {
  const { t } = useTranslation();
  const seq = useApp((s) => s.seqs[props.connId] ?? 0);
  const compact = useApp((s) => s.settings.density === 'compact');
  const rowH = compact ? 24 : 28;
  const scrollRef = useRef<HTMLDivElement>(null);
  const [cursor, setCursor] = useState(0);
  const [range, setRange] = useState({ start: 0, end: 60 });
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const h = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(h);
  }, []);
  const qkey = props.connId + '|' + JSON.stringify(props.query);
  const slice = useSlice<TableRow>((s, c) => engine().tableSlice(props.connId, props.query, s, c) as never, qkey, range.start, range.end, seq);
  const virt = useVirtualizer({ count: slice.total, getScrollElement: () => scrollRef.current, estimateSize: () => rowH, overscan: 12 });
  const items = virt.getVirtualItems();
  const vStart = items[0]?.index ?? 0;
  const vEnd = items[items.length - 1]?.index ?? 60;
  useEffect(() => {
    if (vStart !== range.start || vEnd !== range.end) setRange({ start: vStart, end: Math.max(vEnd, vStart + 40) });
  }, [vStart, vEnd, range.start, range.end]);
  const inView = (slice.extra.topicsInView as number | undefined) ?? 0;
  const onCounts = props.onCounts;
  useEffect(() => onCounts(slice.total, inView, slice.rows.map((r) => r.path)), [slice, inView, onCounts]);

  const tmpl = props.columns.map((c) => COL_WIDTH[c]).join(' ');
  const rowAt = (i: number) => slice.rows[i - slice.start] ?? null;
  const sort = props.query.sort;

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
    } else if (e.key === 'Enter' && row) {
      e.preventDefault();
      props.onSelect(row.path);
    }
  };

  return (
    <div className="pane" style={{ flex: 1 }}>
      <div ref={scrollRef} className="pc gridtbl" style={{ padding: 0, alignContent: 'start' }} tabIndex={0} role="grid" aria-label={t('explore.table')} data-testid="table" onKeyDown={onKeyDown}>
        <div className="hr" role="row" style={{ gridTemplateColumns: tmpl, position: 'sticky', top: 0, zIndex: 2 }}>
          {props.columns.map((c) => (
            <div key={c} role="columnheader" aria-sort={sort.column === c ? (sort.dir === 'asc' ? 'ascending' : 'descending') : undefined}>
              <button
                type="button"
                data-testid={`col-${c}`}
                onClick={() => props.onSort({ column: c, dir: sort.column === c && sort.dir === 'asc' ? 'desc' : 'asc' })}
              >
                {colLabel(c, t)}
                {sort.column === c ? (sort.dir === 'asc' ? ' ▴' : ' ▾') : ''}
              </button>
            </div>
          ))}
        </div>
        <div className="vlist" style={{ height: virt.getTotalSize() }}>
          {items.map((it) => {
            const r = rowAt(it.index);
            return (
              <div
                key={it.key}
                className={`vrow tr${r && props.selected === r.path ? ' sel' : ''}${cursor === it.index ? ' cursor' : ''}`}
                role="row"
                data-testid="table-row"
                data-path={r?.path}
                style={{ top: it.start, height: rowH, gridTemplateColumns: tmpl }}
                onClick={() => {
                  setCursor(it.index);
                  if (r) props.onSelect(r.path);
                }}
                onContextMenu={(e) => {
                  e.preventDefault();
                  if (r) props.onMenu(r, e.clientX, e.clientY);
                }}
              >
                {r && props.columns.map((c) => cell(c, r, now))}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

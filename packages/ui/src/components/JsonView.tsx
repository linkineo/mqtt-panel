import { Fragment, useEffect, useLayoutEffect, useRef, useState, type MouseEvent, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import i18next from 'i18next';
import type { DiffLine } from '@mqttpanel/engine-client';
import { platform } from '../services';
import { useApp } from '../store/app';

const BOOL_TEXT = /^(true|false|t|f|on|off|yes|no)$/i;

/** Cast rule for a JSON leaf, as the engine applies it (H1). */
export function castOf(v: unknown): 'number' | 'parsedNumber' | 'boolean' | 'text' | 'null' {
  if (typeof v === 'number') return 'number';
  if (typeof v === 'boolean') return 'boolean';
  if (v === null || v === undefined) return 'null';
  if (typeof v === 'string') {
    const s = v.trim();
    if (s !== '' && !isNaN(Number(s))) return 'parsedNumber';
    if (BOOL_TEXT.test(s)) return 'boolean';
  }
  return 'text';
}
export const plottable = (v: unknown) => {
  const c = castOf(v);
  return c === 'number' || c === 'parsedNumber' || c === 'boolean';
};

export function childPath(parent: string, key: string | number): string {
  if (typeof key === 'number') return `${parent}[${key}]`;
  return /^[A-Za-z_$][\w$]*$/.test(key) ? `${parent}.${key}` : `${parent}["${key}"]`;
}
export function pathLabel(path: string): string {
  return path === '$' ? 'value' : path.replace(/^\$\.?/, '');
}

/** A click (or right-click) on a key or a value of the payload view (change set 8). */
export interface FieldClick {
  path: string;
  value: unknown;
  x: number;
  y: number;
}

export interface FieldActions {
  open(c: FieldClick): void;
}

function clickHandler(a: FieldActions | undefined, path: string, value: () => unknown) {
  if (!a) return undefined;
  return (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    a.open({ path, value: value(), x: e.clientX, y: e.clientY });
  };
}

/**
 * The click menu of the payload view (change set 8, mockup 06): "Use as sparkline", "Add <field> to chart…",
 * "Copy path", "Copy value". Text fields show the first two disabled with "text, not plottable". Rendered in the page
 * (not native) so it sits next to the value that was clicked.
 */
export function FieldMenu(props: {
  at: FieldClick;
  sparklinePath: string | null;
  onSparkline(path: string): void;
  onChart(path: string): void;
  onCopy(text: string): void;
  onClose(): void;
}) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number }>({ left: props.at.x, top: props.at.y });
  const { path, value } = props.at;
  const leaf = value === null || typeof value !== 'object';
  const canPlot = plottable(value);
  const isText = leaf && castOf(value) === 'text';
  const valueText = typeof value === 'string' ? value : JSON.stringify(value);
  const field = pathLabel(path);
  const { onClose } = props;
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    setPos({ left: Math.max(8, Math.min(props.at.x, window.innerWidth - w - 8)), top: Math.max(8, Math.min(props.at.y + 4, window.innerHeight - h - 8)) });
    el.querySelector<HTMLElement>('[role=menuitem]:not([aria-disabled=true])')?.focus();
  }, [props.at]);
  useEffect(() => {
    const down = (e: globalThis.MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
      }
    };
    document.addEventListener('mousedown', down, true);
    document.addEventListener('keydown', key, true);
    return () => {
      document.removeEventListener('mousedown', down, true);
      document.removeEventListener('keydown', key, true);
    };
  }, [onClose]);
  const items: { id: string; label: string; enabled: boolean; on?: boolean; kbd?: string; sep?: boolean }[] = [
    { id: 'spark', label: isText ? t('field.useSparklineText') : t('field.useSparkline'), enabled: canPlot, on: canPlot && props.sparklinePath === path },
    { id: 'chart', label: isText ? t('field.addToChartText', { field }) : t('field.addToChart', { field }), enabled: canPlot, kbd: 'C' },
    { id: 'path', label: t('field.copyPath', { path }), enabled: true, sep: true },
    { id: 'value', label: t('field.copyValue', { value: valueText.length > 40 ? valueText.slice(0, 40) + '…' : valueText }), enabled: true },
  ];
  const act = (id: string) => {
    onClose();
    if (id === 'spark') props.onSparkline(path);
    else if (id === 'chart') props.onChart(path);
    else if (id === 'path') props.onCopy(path);
    else if (id === 'value') props.onCopy(valueText);
  };
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const list = Array.from(ref.current?.querySelectorAll<HTMLElement>('[role=menuitem]:not([aria-disabled=true])') ?? []);
    const i = list.indexOf(document.activeElement as HTMLElement);
    list[(i + (e.key === 'ArrowDown' ? 1 : list.length - 1)) % list.length]?.focus();
  };
  return createPortal(
    <div ref={ref} className="ctx" role="menu" aria-label={t('field.menu', { field })} data-testid="json-menu" data-path={path} style={{ left: pos.left, top: pos.top, minWidth: 240 }} onKeyDown={onKey}>
      {items.map((it) => (
        <Fragment key={it.id}>
          {it.sep && <hr />}
          <div
            role="menuitem"
            tabIndex={-1}
            aria-disabled={!it.enabled || undefined}
            data-id={it.id}
            className={`ctx-item${it.enabled ? '' : ' disabled'}${it.on ? ' on' : ''}`}
            onClick={() => it.enabled && act(it.id)}
            onKeyDown={(e) => {
              if ((e.key === 'Enter' || e.key === ' ') && it.enabled) {
                e.preventDefault();
                act(it.id);
              }
            }}
          >
            <span className="ctx-label">{it.label}</span>
            {it.kbd && <span className="kbd">{it.kbd}</span>}
          </div>
        </Fragment>
      ))}
    </div>,
    document.getElementById('app-root') ?? document.body,
  );
}

function valueClass(v: unknown): string {
  if (typeof v === 'string') return 's';
  if (typeof v === 'number') return 'num';
  if (typeof v === 'boolean') return 'b';
  return 'nl';
}

/**
 * Pretty JSON with the mockup colouring. With `actions`, a click or right-click on any key or value opens the click
 * menu (change set 8: nothing else sits inside the JSON); in pick mode a click on a plottable key or value calls onPick
 * and text fields are greyed (D-18).
 */
export function JsonView(props: {
  value: unknown;
  actions?: FieldActions;
  onPick?(path: string): void;
  pickedPath?: string | null;
  small?: boolean;
  testId?: string;
}) {
  const { t } = useTranslation();
  const lines: ReactNode[] = [];
  const pick = !!props.onPick;
  const walk = (v: unknown, path: string, indent: number, key: string | number | null, last: boolean) => {
    const pad = '  '.repeat(indent);
    const comma = last ? '' : ',';
    const keyNode = (dim: boolean, canPlot: boolean) =>
      key === null || typeof key === 'number' ? null : (
        <>
          <span
            className={`k pick${pick && canPlot ? ' plot' : ''}${dim ? ' dim' : ''}${props.pickedPath === path ? ' on' : ''}`}
            data-path={path}
            onClick={pick ? (canPlot ? () => props.onPick!(path) : undefined) : clickHandler(props.actions, path, () => v)}
            onContextMenu={pick ? undefined : clickHandler(props.actions, path, () => v)}
          >
            "{key}"
          </span>
          {': '}
        </>
      );
    if (v !== null && typeof v === 'object') {
      const isArr = Array.isArray(v);
      const entries: [string | number, unknown][] = isArr ? (v as unknown[]).map((x, i) => [i, x]) : Object.entries(v as Record<string, unknown>);
      lines.push(
        <span className="line" key={path + '{'}>
          {pad}
          {keyNode(pick, false)}
          {isArr ? '[' : '{'}
          {entries.length === 0 ? (isArr ? ']' : '}') + comma : ''}
        </span>,
      );
      if (entries.length === 0) return;
      entries.forEach(([k, x], i) => walk(x, childPath(path, k), indent + 1, k, i === entries.length - 1));
      lines.push(
        <span className="line" key={path + '}'}>
          {pad}
          {isArr ? ']' : '}'}
          {comma}
        </span>,
      );
      return;
    }
    const canPlot = plottable(v);
    const dim = pick && !canPlot;
    const cast = castOf(v);
    lines.push(
      <span className="line" key={path} data-field={path}>
        {pad}
        {keyNode(dim, canPlot)}
        <span
          className={`${valueClass(v)} pick${pick && canPlot ? ' plot' : ''}${dim ? ' dim' : ''}`}
          data-path={path}
          onClick={pick ? (canPlot ? () => props.onPick!(path) : undefined) : clickHandler(props.actions, path, () => v)}
          onContextMenu={pick ? undefined : clickHandler(props.actions, path, () => v)}
        >
          {JSON.stringify(v)}
        </span>
        {comma}
        {pick && !canPlot && cast === 'text' && <span className="muted"> {t('field.notPlottable')}</span>}
        {pick && cast === 'boolean' && <span className="muted"> {t('field.boolean01')}</span>}
      </span>,
    );
  };
  walk(props.value, '$', 0, null, true);
  return (
    <div className="json" style={props.small ? { fontSize: 11, lineHeight: 1.6 } : undefined} data-testid={props.testId}>
      {lines}
    </div>
  );
}

/** Diff rendering (mockup 06): removed struck through in brick, added on sage. */
export function DiffView(props: { lines: DiffLine[]; highlight: boolean; actions?: FieldActions; leafValue(path: string): unknown; testId?: string }) {
  const lines = props.lines;
  return (
    <div className="json" data-testid={props.testId ?? 'diff'}>
      {lines.map((l, i) => {
        const next = lines[i + 1];
        const comma = l.open === null && next && next.close === null && next.indent === l.indent ? ',' : '';
        const pad = '  '.repeat(l.indent);
        const key = l.key !== null ? (
          <>
            <span
              className="k pick"
              data-path={l.path ?? undefined}
              onClick={l.path ? clickHandler(props.actions, l.path, () => props.leafValue(l.path!)) : undefined}
              onContextMenu={l.path ? clickHandler(props.actions, l.path, () => props.leafValue(l.path!)) : undefined}
            >
              "{l.key}"
            </span>
            {': '}
          </>
        ) : null;
        if (l.close !== null) {
          return (
            <span key={i} className="line">
              {pad}
              {l.close}
              {comma}
            </span>
          );
        }
        if (l.open !== null) {
          return (
            <span key={i} className={`line${props.highlight && l.kind !== 'same' ? ' chg' : ''}`}>
              {pad}
              {key}
              {l.open}
            </span>
          );
        }
        const cls = (txt: string | null) => (txt === null ? '' : txt.startsWith('"') ? 's' : txt === 'null' ? 'nl' : 'num');
        const vNew = l.newValue;
        const vOld = l.oldValue;
        const hl = props.highlight;
        return (
          <span key={i} className={`line${hl && l.kind === 'changed' ? ' chg' : ''}`} data-kind={l.kind}>
            {pad}
            {key}
            {l.kind === 'changed' && (
              <>
                <span className={`${cls(vOld)}${hl ? ' del' : ''}`}>{vOld}</span>{' '}
                <span
                  className={`${cls(vNew)} pick${hl ? ' add' : ''}`}
                  data-path={l.path ?? undefined}
                  onClick={l.path ? clickHandler(props.actions, l.path, () => props.leafValue(l.path!)) : undefined}
                  onContextMenu={l.path ? clickHandler(props.actions, l.path, () => props.leafValue(l.path!)) : undefined}
                >
                  {vNew}
                </span>
              </>
            )}
            {l.kind === 'removed' && <span className={`${cls(vOld)}${hl ? ' del' : ''}`}>{vOld}</span>}
            {l.kind === 'added' && <span className={`${cls(vNew)}${hl ? ' add' : ''}`}>{vNew}</span>}
            {l.kind === 'same' && (
              <span
                className={`${cls(vNew)} pick`}
                data-path={l.path ?? undefined}
                onClick={l.path ? clickHandler(props.actions, l.path, () => props.leafValue(l.path!)) : undefined}
                onContextMenu={l.path ? clickHandler(props.actions, l.path, () => props.leafValue(l.path!)) : undefined}
              >
                {vNew}
              </span>
            )}
            {comma}
          </span>
        );
      })}
    </div>
  );
}

/** Copies a path or a value and says so. */
export function copyField(text: string): void {
  void platform().copyText(text);
  useApp.getState().toast({ kind: 'info', text: i18next.t('common.copied') });
}

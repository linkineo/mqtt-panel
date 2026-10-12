import type { MouseEvent, ReactNode } from 'react';
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

export interface FieldActions {
  onChart(path: string): void;
  onCopy(text: string): void;
}

/** Context menu on a key or value (D-17): Add to chart, Copy path, Copy value. */
export async function fieldMenu(path: string, value: unknown, e: MouseEvent, a: FieldActions) {
  e.preventDefault();
  e.stopPropagation();
  const t = i18next.t.bind(i18next);
  const canPlot = plottable(value);
  const valueText = typeof value === 'string' ? value : JSON.stringify(value);
  const id = await platform().showContextMenu(
    [
      {
        id: 'chart',
        label: canPlot ? t('field.addToChart', { field: pathLabel(path) }) : t('field.addToChartText', { field: pathLabel(path) }),
        accelerator: 'C',
        enabled: canPlot,
      },
      { id: 'path', label: t('field.copyPath', { path }) },
      { id: 'value', label: t('field.copyValue', { value: valueText.length > 40 ? valueText.slice(0, 40) + '…' : valueText }) },
    ],
    e.clientX,
    e.clientY,
  );
  if (id === 'chart') a.onChart(path);
  else if (id === 'path') a.onCopy(path);
  else if (id === 'value') a.onCopy(valueText);
}

function valueClass(v: unknown): string {
  if (typeof v === 'string') return 's';
  if (typeof v === 'number') return 'num';
  if (typeof v === 'boolean') return 'b';
  return 'nl';
}

function ChartIcon({ onClick, label }: { onClick(): void; label: string }) {
  return (
    <button type="button" className="charticon" aria-label={label} title={label} onClick={onClick}>
      ⌁
    </button>
  );
}

/**
 * Pretty JSON with the mockup colouring. Leaves are pickable: right-click (or the hover chart icon) offers the
 * field actions; in pick mode a click on a plottable key or value calls onPick and text fields are greyed (D-18).
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
            onClick={pick && canPlot ? () => props.onPick!(path) : undefined}
            onContextMenu={props.actions ? (e) => void fieldMenu(path, v, e, props.actions!) : undefined}
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
          onClick={pick && canPlot ? () => props.onPick!(path) : undefined}
          onContextMenu={props.actions ? (e) => void fieldMenu(path, v, e, props.actions!) : undefined}
        >
          {JSON.stringify(v)}
        </span>
        {comma}
        {pick && !canPlot && cast === 'text' && <span className="muted"> {t('field.notPlottable')}</span>}
        {pick && cast === 'boolean' && <span className="muted"> {t('field.boolean01')}</span>}
        {!pick && props.actions && canPlot && <ChartIcon label={t('field.addToChart', { field: pathLabel(path) })} onClick={() => props.actions!.onChart(path)} />}
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
export function DiffView(props: { lines: DiffLine[]; highlight: boolean; actions?: FieldActions; leafValue(path: string): unknown }) {
  const { t } = useTranslation();
  const lines = props.lines;
  return (
    <div className="json" data-testid="diff">
      {lines.map((l, i) => {
        const next = lines[i + 1];
        const comma = l.open === null && next && next.close === null && next.indent === l.indent ? ',' : '';
        const pad = '  '.repeat(l.indent);
        const key = l.key !== null ? (
          <>
            <span
              className="k pick"
              onContextMenu={props.actions && l.path ? (e) => void fieldMenu(l.path!, props.leafValue(l.path!), e, props.actions!) : undefined}
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
                  onContextMenu={props.actions && l.path ? (e) => void fieldMenu(l.path!, props.leafValue(l.path!), e, props.actions!) : undefined}
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
                onContextMenu={props.actions && l.path ? (e) => void fieldMenu(l.path!, props.leafValue(l.path!), e, props.actions!) : undefined}
              >
                {vNew}
              </span>
            )}
            {comma}
            {props.actions && l.plottable && l.path && (
              <button type="button" className="charticon" aria-label={t('field.addToChart', { field: pathLabel(l.path) })} onClick={() => props.actions!.onChart(l.path!)}>
                ⌁
              </button>
            )}
          </span>
        );
      })}
    </div>
  );
}

export function fieldActions(openPicker: (path: string) => void): FieldActions {
  return {
    onChart: openPicker,
    onCopy: (text) => {
      void platform().copyText(text);
      useApp.getState().toast({ kind: 'info', text: i18next.t('common.copied') });
    },
  };
}

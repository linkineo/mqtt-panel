import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type KeyboardEvent as RKeyboardEvent } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import type { UserMessage } from '@mqttpanel/engine-client';
import { renderMessage } from '../i18n';
import { useApp } from '../store/app';
import { fmtTime } from '../lib/format';

/* ---------------------------------------------------------------- segmented control */

/**
 * Spread on every text input and textarea (change set 8, V-11): no automatic capitalisation, correction or spell-check
 * (macOS WebKit capitalised the first letter typed into host names, topics and client IDs). A Vitest checks that every
 * `<input` and `<textarea` in the sources carries it.
 */
export const NO_AUTO = { autoCapitalize: 'off', autoCorrect: 'off', spellCheck: false } as const;

export interface SegOption<T extends string | number> {
  value: T;
  label: ReactNode;
  disabled?: boolean;
  testId?: string;
}

export function Seg<T extends string | number>(props: {
  value: T | null;
  options: SegOption<NoInfer<T>>[];
  onChange(v: NoInfer<T>): void;
  label: string;
  disabled?: boolean;
  testId?: string;
}) {
  const { value, options, onChange, label, disabled } = props;
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const onKey = (e: RKeyboardEvent, i: number) => {
    if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return;
    e.preventDefault();
    const dir = e.key === 'ArrowRight' ? 1 : -1;
    for (let k = 1; k <= options.length; k++) {
      const j = (i + dir * k + options.length) % options.length;
      if (!options[j].disabled) {
        onChange(options[j].value);
        refs.current[j]?.focus();
        break;
      }
    }
  };
  return (
    <span className="seg" role="radiogroup" aria-label={label} data-testid={props.testId}>
      {options.map((o, i) => {
        const on = o.value === value;
        return (
          <button
            key={String(o.value)}
            ref={(el) => (refs.current[i] = el)}
            type="button"
            role="radio"
            aria-checked={on}
            tabIndex={on || (value === null && i === 0) ? 0 : -1}
            className={on ? 'on' : undefined}
            disabled={disabled || o.disabled}
            data-testid={o.testId}
            onClick={() => onChange(o.value)}
            onKeyDown={(e) => onKey(e, i)}
          >
            {o.label}
          </button>
        );
      })}
    </span>
  );
}

/* ---------------------------------------------------------------- switch and checkbox */

export function Switch(props: { on: boolean; onChange(v: boolean): void; label: string; disabled?: boolean; tone?: 'clay'; testId?: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={props.on}
      aria-label={props.label}
      className={`sw${props.on ? ' on' : ''}${props.tone ? ' ' + props.tone : ''}`}
      disabled={props.disabled}
      data-testid={props.testId}
      onClick={() => props.onChange(!props.on)}
    />
  );
}

export function Check(props: { on: boolean; onChange?(v: boolean): void; label: ReactNode; disabled?: boolean; ariaLabel?: string; testId?: string }) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={props.on}
      aria-label={props.ariaLabel}
      className="chkrow"
      disabled={props.disabled}
      data-testid={props.testId}
      onClick={() => props.onChange?.(!props.on)}
    >
      <span className={`chk${props.on ? ' on' : ''}`} aria-hidden="true" style={props.disabled ? { opacity: 0.35 } : undefined} />
      {props.label}
    </button>
  );
}

/* ---------------------------------------------------------------- popover */

function portalRoot(): HTMLElement {
  return document.getElementById('app-root') ?? document.body;
}

export function Popover(props: {
  anchor: HTMLElement | null;
  onClose(): void;
  children: ReactNode;
  className?: string;
  align?: 'start' | 'end';
  testId?: string;
  role?: string;
  label?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const { anchor, onClose } = props;
  useLayoutEffect(() => {
    if (!anchor || !ref.current) return;
    const r = anchor.getBoundingClientRect();
    const w = ref.current.offsetWidth;
    const h = ref.current.offsetHeight;
    let left = props.align === 'end' ? r.right - w : r.left;
    left = Math.max(8, Math.min(left, window.innerWidth - w - 8));
    let top = r.bottom + 6;
    if (top + h > window.innerHeight - 8) top = Math.max(8, r.top - h - 6);
    setPos({ left, top });
  }, [anchor, props.align]);
  useEffect(() => {
    const down = (e: MouseEvent) => {
      const t = e.target as Node;
      if (ref.current?.contains(t) || anchor?.contains(t)) return;
      if ((t as HTMLElement).closest?.('.ctx')) return;
      onClose();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
        anchor?.focus();
      }
    };
    document.addEventListener('mousedown', down, true);
    document.addEventListener('keydown', key, true);
    return () => {
      document.removeEventListener('mousedown', down, true);
      document.removeEventListener('keydown', key, true);
    };
  }, [anchor, onClose]);
  return createPortal(
    <div
      ref={ref}
      className={`popover ${props.className ?? ''}`}
      style={pos ? { left: pos.left, top: pos.top } : { left: -9999, top: -9999 }}
      data-testid={props.testId}
      role={props.role ?? 'dialog'}
      aria-label={props.label}
    >
      {props.children}
    </div>,
    portalRoot(),
  );
}

/* ---------------------------------------------------------------- info bubble */

/** The ⓘ next to a label: one round 18 px button with an "i" (change set 11, never the pill-shaped chip); it opens a
 * small popover with the plain-word explanation, its bubble text `info.<id>`. `keyStore` appends the sentence on where the master key lives in this build
 * (`info.keyStore.file` or `info.keyStore.keychain`, change set 10) for fields whose value goes to the secret store. */
export function Info(props: { id: string; title: string; tone?: 'clay'; keyStore?: boolean }) {
  const { t } = useTranslation();
  const store = useApp((s) => s.keyStore);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLButtonElement>(null);
  return (
    <>
      <button
        ref={ref}
        type="button"
        className={`info-btn${props.tone === 'clay' ? ' clay' : ''}`}
        aria-label={t('info.aria', { name: props.title })}
        aria-expanded={open}
        data-testid={`info-${props.id}`}
        onClick={(e) => {
          e.stopPropagation();
          e.preventDefault();
          setOpen(!open);
        }}
      >
        i
      </button>
      {open && (
        <Popover anchor={ref.current} onClose={() => setOpen(false)} className="bubble" testId={`bubble-${props.id}`} label={props.title}>
          <b>{props.title}</b>
          <span data-testid={`bubble-text-${props.id}`}>
            {t(`info.${props.id}`)}
            {props.keyStore && store ? ` ${t(store === 'file' ? 'info.keyStore.file' : 'info.keyStore.keychain')}` : ''}
          </span>
        </Popover>
      )}
    </>
  );
}

/* ---------------------------------------------------------------- modal */

export function Modal(props: {
  title: ReactNode;
  onClose(): void;
  children: ReactNode;
  footer?: ReactNode;
  footerStyle?: React.CSSProperties;
  width?: number;
  testId?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const { onClose } = props;
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    const first = ref.current?.querySelector<HTMLElement>('[data-autofocus]') ??
      ref.current?.querySelector<HTMLElement>('input, textarea, button:not([disabled]), [tabindex="0"]');
    first?.focus();
    return () => prev?.focus?.();
  }, []);
  const onKey = (e: RKeyboardEvent) => {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onClose();
    }
    if (e.key === 'Tab' && ref.current) {
      const f = Array.from(ref.current.querySelectorAll<HTMLElement>('input, textarea, button:not([disabled]), [tabindex="0"]')).filter(
        (el) => el.offsetParent !== null || el === document.activeElement,
      );
      if (!f.length) return;
      const i = f.indexOf(document.activeElement as HTMLElement);
      if (e.shiftKey && i <= 0) {
        e.preventDefault();
        f[f.length - 1].focus();
      } else if (!e.shiftKey && i === f.length - 1) {
        e.preventDefault();
        f[0].focus();
      }
    }
  };
  return createPortal(
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        ref={ref}
        className="modal"
        role="dialog"
        aria-modal="true"
        style={props.width ? { width: props.width } : undefined}
        onKeyDown={onKey}
        data-testid={props.testId}
      >
        <div className="mh">{props.title}</div>
        <div className="mb">{props.children}</div>
        {props.footer && (
          <div className="mf" style={props.footerStyle}>
            {props.footer}
          </div>
        )}
      </div>
    </div>,
    portalRoot(),
  );
}

export function ConfirmDialog(props: {
  title: string;
  children?: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  onConfirm(): void;
  onClose(): void;
  testId?: string;
}) {
  const { t } = useTranslation();
  return (
    <Modal
      title={props.title}
      onClose={props.onClose}
      width={520}
      testId={props.testId ?? 'confirm'}
      footer={
        <>
          <button type="button" className="btn" onClick={props.onClose}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className={props.danger ? 'btn danger solid' : 'btn pri'}
            data-testid="confirm-ok"
            onClick={() => {
              props.onConfirm();
              props.onClose();
            }}
          >
            {props.confirmLabel}
          </button>
        </>
      }
    >
      {props.children}
    </Modal>
  );
}

export function PromptDialog(props: {
  title: string;
  label: string;
  initial?: string;
  confirmLabel: string;
  onConfirm(value: string): void;
  onClose(): void;
  extra?: { label: string; initial?: string; suggestions?: string[] };
  onConfirmExtra?(value: string, extra: string): void;
  testId?: string;
}) {
  const { t } = useTranslation();
  const [v, setV] = useState(props.initial ?? '');
  const [x, setX] = useState(props.extra?.initial ?? '');
  const ok = () => {
    if (!v.trim()) return;
    if (props.extra && props.onConfirmExtra) props.onConfirmExtra(v.trim(), x.trim());
    else props.onConfirm(v.trim());
    props.onClose();
  };
  return (
    <Modal
      title={props.title}
      onClose={props.onClose}
      width={460}
      testId={props.testId ?? 'prompt'}
      footer={
        <>
          <button type="button" className="btn" onClick={props.onClose}>
            {t('common.cancel')}
          </button>
          <button type="button" className="btn pri" disabled={!v.trim()} onClick={ok} data-testid="prompt-ok">
            {props.confirmLabel}
          </button>
        </>
      }
    >
      <div className="form">
        <div className="f" style={{ gridTemplateColumns: '110px 1fr' }}>
          <label htmlFor="prompt-v">{props.label}</label>
          <input {...NO_AUTO} id="prompt-v" className="input w-l" value={v} data-autofocus onChange={(e) => setV(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && ok()} />
        </div>
        {props.extra && (
          <div className="f" style={{ gridTemplateColumns: '110px 1fr' }}>
            <label htmlFor="prompt-x">{props.extra.label}</label>
            <input {...NO_AUTO} id="prompt-x" className="input w-l" value={x} list="prompt-x-list" onChange={(e) => setX(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && ok()} />
            <datalist id="prompt-x-list">
              {props.extra.suggestions?.map((s) => <option key={s} value={s} />)}
            </datalist>
          </div>
        )}
      </div>
    </Modal>
  );
}

/* ---------------------------------------------------------------- messages */

export function Msg({ msg }: { msg: UserMessage | null | undefined }) {
  const { t } = useTranslation();
  return <>{renderMessage(t, msg)}</>;
}

export function Toasts() {
  const toasts = useApp((s) => s.toasts);
  const dismiss = useApp((s) => s.dismissToast);
  const { t } = useTranslation();
  return (
    <div className="toasts" aria-live="polite">
      {toasts.map((x) => (
        <div key={x.id} className={`toast ${x.kind}`} role={x.kind === 'err' ? 'alert' : 'status'} data-testid="toast">
          <span className="grow selectable">{x.msg ? renderMessage(t, x.msg) : x.text}</span>
          <button type="button" className="iconbtn" aria-label={t('common.dismiss')} onClick={() => dismiss(x.id)}>
            ✕
          </button>
        </div>
      ))}
    </div>
  );
}

/* ---------------------------------------------------------------- sparkline */

/** A 42×12 polyline of the last 10 cast values (D-22); hovering lists the values with their times. */
export function Sparkline({ values, times, step }: { values: number[]; times?: number[]; step?: boolean }) {
  const v = values.slice(-10);
  const ts = (times ?? []).slice(-10);
  if (v.length < 2) return <svg className="spark" viewBox="0 0 42 12" aria-hidden="true" />;
  const min = Math.min(...v);
  const max = Math.max(...v);
  const span = max - min || 1;
  const pts: string[] = [];
  v.forEach((y, i) => {
    const x = Math.round((i / (v.length - 1)) * 42 * 10) / 10;
    const yy = Math.round((10 - ((y - min) / span) * 8 - 1) * 10) / 10;
    if (step && i > 0) pts.push(`${x},${pts[pts.length - 1].split(',')[1]}`);
    pts.push(`${x},${yy}`);
  });
  const title = v.map((y, i) => (ts[i] ? `${fmtTime(ts[i])}  ${y}` : String(y))).join('\n');
  return (
    <svg className="spark" viewBox="0 0 42 12" data-values={v.join(',')} role="img" aria-label={title}>
      <title>{title}</title>
      <polyline points={pts.join(' ')} />
    </svg>
  );
}

/* ---------------------------------------------------------------- misc */

export function useNow(intervalMs = 1000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const h = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(h);
  }, [intervalMs]);
  return now;
}

/**
 * Change set 9: a clock that ticks just after every whole second, so the one-second rates (the engine's count of the
 * last completed second) are read again as soon as they change, and not in between.
 */
export function useSecondTick(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    let h: ReturnType<typeof setTimeout>;
    const next = () => {
      h = setTimeout(() => {
        setNow(Date.now());
        next();
      }, 1000 - (Date.now() % 1000) + 30);
    };
    next();
    return () => clearTimeout(h);
  }, []);
  return now;
}

export function Kbd({ children }: { children: ReactNode }) {
  return <span className="kbd">{children}</span>;
}

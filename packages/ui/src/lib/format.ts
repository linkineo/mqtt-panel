// Number, time and size formatting shared by every screen. Digit groups use a narrow no-break space
// ("1 284"), as in the mockups, in every language.
export const NNBSP = ' ';

export function fmtNum(n: number, decimals?: number): string {
  if (!Number.isFinite(n)) return '—';
  const fixed = decimals === undefined ? (Number.isInteger(n) ? String(Math.abs(n)) : String(Math.abs(n))) : Math.abs(n).toFixed(decimals);
  const [int, frac] = fixed.split('.');
  const grouped = int.length > 4 ? int.replace(/\B(?=(\d{3})+(?!\d))/g, NNBSP) : int;
  return (n < 0 ? '-' : '') + grouped + (frac !== undefined ? '.' + frac : '');
}

function pad(n: number, w = 2): string {
  return String(n).padStart(w, '0');
}

export function fmtTime(ms: number, withMs = false): string {
  const d = new Date(ms);
  const base = `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  return withMs ? `${base}.${pad(d.getMilliseconds(), 3)}` : base;
}

export function fmtHm(ms: number): string {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function fmtDate(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** "2 s", "14 min", "3 h", "2 d" */
export function fmtAge(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s} s`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h} h`;
  return `${Math.round(h / 24)} d`;
}

/** "2 h 14 min" */
export function fmtUptime(ms: number): string {
  const m = Math.floor(ms / 60000);
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  if (h < 48) return `${h} h ${m % 60} min`;
  return `${Math.floor(h / 24)} d ${h % 24} h`;
}

export function fmtBytes(b: number): string {
  if (b < 1024) return `${fmtNum(b)} B`;
  if (b < 1024 * 1024) {
    const k = b / 1024;
    return `${k < 10 ? k.toFixed(1) : fmtNum(Math.round(k))} KB`;
  }
  const m = b / (1024 * 1024);
  return `${m < 100 ? m.toFixed(1) : fmtNum(Math.round(m))} MB`;
}

export function fmtRate(r: number): string {
  if (r <= 0) return '0';
  if (r < 10) return String(Math.round(r * 10) / 10);
  return fmtNum(Math.round(r));
}

/** Shortens a topic for narrow columns: "…/ahu01/cmd". */
export function shortTopic(topic: string, keep = 2): string {
  const parts = topic.split('/');
  if (parts.length <= keep + 1) return topic;
  return '…/' + parts.slice(-keep).join('/');
}

export function isoDateToMonth(iso: string | null | undefined): string {
  return iso ? iso.slice(0, 7) : '';
}

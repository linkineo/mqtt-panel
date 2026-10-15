// In-memory topic store for the mock engine: ring buffers, decoding, fields and casting, series, tree and
// table slices, search and filters, history, diff and export estimates.
import { lastSecondRate, pruneTimes } from '../rate';
import type {
  DecodedPayload,
  DiffLine,
  DiffResult,
  ExportEstimate,
  ExportOptions,
  FieldInfo,
  Filters,
  HistoryEntry,
  MessageView,
  Mqtt5Props,
  PayloadFormat,
  ProtoSchema,
  Qos,
  SeriesData,
  TableQuery,
  TableRow,
  TableSlice,
  TopicDetail,
  TreeQuery,
  TreeRow,
  TreeSlice,
} from '../types';
import { decodeUtf8, hexDump, toBase64, topicMatches, um } from './util';

export interface StoredMsg {
  seq: number;
  topic: string;
  at: number;
  qos: Qos;
  retain: boolean;
  bytes: Uint8Array;
  text: string | null;
  props: Mqtt5Props | null;
  /** parsed JSON object/array, undefined when not parsed yet, null when not JSON */
  json?: unknown;
}

interface SeriesRec {
  points: [number, number][];
  skipped: number;
  firstSkipReason: string | null;
  rules: Set<string>;
  count: number;
}

export interface TopicRec {
  path: string;
  buffer: StoredMsg[];
  cap: number | null;
  messages: number;
  lastAt: number | null;
  retained: boolean;
  recent: number[];
  series: Map<string, SeriesRec>;
}

interface Node {
  name: string;
  path: string;
  depth: number;
  children: Map<string, Node>;
  sorted: Node[] | null;
  topic: TopicRec | null;
  parent: Node | null;
}

interface Agg {
  topicCount: number;
  messages: number;
  lastAt: number | null;
  rate: number;
}

export interface StoreEnv {
  now: () => number;
  bufferPerTopic: () => number;
  pointsPerSeries: () => number;
  renderLimitBytes: () => number;
  protoSchemas: () => ProtoSchema[];
  /** The value picked with "Use as sparkline" for a topic (profile `topics`, change set 8). */
  sparklinePath?: (topic: string) => string | null;
}

const FIELD_SIZE_LIMIT = 64 * 1024;
const BOOL_TRUE = new Set(['true', 't', 'on', 'yes']);
const BOOL_FALSE = new Set(['false', 'f', 'off', 'no']);

// ------------------------------------------------------------------ casting and fields

export function castValue(v: unknown): { cast: FieldInfo['cast']; num: number | null } {
  if (v === null) return { cast: 'null', num: null };
  if (typeof v === 'number') return { cast: 'number', num: Number.isFinite(v) ? v : null };
  if (typeof v === 'boolean') return { cast: 'boolean', num: v ? 1 : 0 };
  if (typeof v === 'string') {
    const t = v.trim();
    if (t !== '' && Number.isFinite(Number(t))) return { cast: 'parsedNumber', num: Number(t) };
    const l = t.toLowerCase();
    if (BOOL_TRUE.has(l)) return { cast: 'boolean', num: 1 };
    if (BOOL_FALSE.has(l)) return { cast: 'boolean', num: 0 };
    return { cast: 'text', num: null };
  }
  if (Array.isArray(v)) return { cast: 'array', num: null };
  return { cast: 'object', num: null };
}

export function msgJson(m: StoredMsg): unknown {
  if (m.json !== undefined) return m.json;
  m.json = null;
  if (m.text !== null && m.bytes.length <= 4 * 1024 * 1024) {
    const t = m.text.trimStart();
    if (t.startsWith('{') || t.startsWith('[')) {
      try {
        m.json = JSON.parse(m.text);
      } catch {
        m.json = null;
      }
    }
  }
  return m.json;
}

interface Leaf {
  path: string;
  label: string;
  value: unknown;
}

function leaves(v: unknown, path: string, label: string, out: Leaf[]): void {
  if (v !== null && typeof v === 'object') {
    if (Array.isArray(v)) {
      if (v.length === 0) out.push({ path, label, value: v });
      v.forEach((x, i) => leaves(x, `${path}[${i}]`, `${label}[${i}]`, out));
    } else {
      const keys = Object.keys(v as Record<string, unknown>);
      if (keys.length === 0 && path !== '$') out.push({ path, label, value: v });
      for (const k of keys) {
        const seg = /^[A-Za-z_$][\w$]*$/.test(k) ? `.${k}` : `["${k}"]`;
        leaves((v as Record<string, unknown>)[k], `${path}${seg}`, label ? `${label}${seg.startsWith('.') ? seg : seg}` : seg.replace(/^\./, ''), out);
      }
    }
  } else out.push({ path, label, value: v });
}

function msgLeaves(m: StoredMsg): Leaf[] {
  if (m.bytes.length > FIELD_SIZE_LIMIT) return [];
  const j = msgJson(m);
  if (j !== null && typeof j === 'object') {
    const out: Leaf[] = [];
    leaves(j, '$', '', out);
    return out;
  }
  if (m.text !== null && !isImage(m.bytes)) return [{ path: '$', label: 'value', value: m.text }];
  return [];
}

export function fieldsOf(m: StoredMsg): FieldInfo[] {
  return msgLeaves(m).map((l) => {
    const c = castValue(l.value);
    const plottable = c.num !== null;
    const lastValue = l.path === '$' ? String(l.value) : JSON.stringify(l.value);
    return { path: l.path, label: l.label, lastValue: lastValue.length > 120 ? lastValue.slice(0, 119) + '…' : lastValue, cast: c.cast, plottable };
  });
}

/** Reads a value at "$.a.b[0]" from a message. undefined when absent. */
export function valueAt(m: StoredMsg, path: string): unknown {
  if (path === '$') {
    const j = msgJson(m);
    return j !== null ? j : m.text ?? undefined;
  }
  let cur: unknown = msgJson(m);
  if (cur === null) return undefined;
  const re = /\.([A-Za-z_$][\w$]*)|\[(\d+)\]|\["((?:[^"\\]|\\.)*)"\]/g;
  let rest = path.slice(1);
  let mm: RegExpExecArray | null;
  let consumed = 0;
  while ((mm = re.exec(rest)) !== null) {
    if (mm.index !== consumed) return undefined;
    consumed = mm.index + mm[0].length;
    const key = mm[1] ?? mm[3] ?? Number(mm[2]);
    if (cur === null || typeof cur !== 'object') return undefined;
    cur = (cur as Record<string | number, unknown>)[key as string];
    if (cur === undefined) return undefined;
  }
  if (consumed !== rest.length) return undefined;
  rest = '';
  return cur;
}

function isImage(b: Uint8Array): boolean {
  return (b.length > 4 && b[0] === 0x89 && b[1] === 0x50 && b[2] === 0x4e && b[3] === 0x47) || (b.length > 3 && b[0] === 0xff && b[1] === 0xd8 && b[2] === 0xff);
}

export function previewOf(m: StoredMsg | undefined): string | null {
  if (!m) return null;
  if (isImage(m.bytes)) return `${m.bytes[0] === 0x89 ? 'PNG' : 'JPEG'} image · ${m.bytes.length} B`;
  if (m.text === null) return Array.from(m.bytes.subarray(0, 24), (x) => x.toString(16).padStart(2, '0')).join(' ');
  const t = m.text.slice(0, 200).replace(/\s*\n\s*/g, ' ');
  return t.length > 80 ? t.slice(0, 79) + '…' : t;
}

// ------------------------------------------------------------------ query matching

function filtersActive(f: Filters): boolean {
  return f.retainedOnly || f.changedWithinMin !== null || f.payloadContains !== '' || f.qos !== null || f.hasValue;
}

function searchMatcher(search: string): ((path: string) => boolean) | null {
  const s = search.trim();
  if (!s) return null;
  if (s.startsWith('/')) {
    const last = s.lastIndexOf('/');
    let body = s.slice(1);
    let flags = 'i';
    if (last > 0) {
      body = s.slice(1, last);
      const fl = s.slice(last + 1);
      flags = /^[gimsuy]*$/.test(fl) ? (fl.includes('i') ? fl : fl + 'i').replace('g', '') : 'i';
    }
    try {
      const re = new RegExp(body, flags);
      return (p) => re.test(p);
    } catch {
      return () => false;
    }
  }
  if (s.includes('+') || s.includes('#')) {
    // MQTT filter; a partial last level ("building/+/+/temp") also matches as a prefix while typing
    const parts = s.split('/');
    const last = parts[parts.length - 1];
    const loose = last !== '+' && last !== '#' && last !== '' ? parts.slice(0, -1).join('/') + '/+' : null;
    return (p) => {
      if (topicMatches(s, p)) return true;
      if (loose === null || !topicMatches(loose, p)) return false;
      const seg = p.split('/')[parts.length - 1] ?? '';
      return seg.toLowerCase().startsWith(last.toLowerCase());
    };
  }
  const low = s.toLowerCase();
  return (p) => p.toLowerCase().includes(low);
}

// ------------------------------------------------------------------ the store

export class TopicStore {
  root: Node = { name: '', path: '', depth: -1, children: new Map(), sorted: null, topic: null, parent: null };
  topics = new Map<string, TopicRec>();
  seq = 0;
  expanded = new Set<string>();
  pinned = new Set<string>();
  recentAll: number[] = [];
  private aggCache: { key: string; map: Map<Node, Agg> } | null = null;

  constructor(private env: StoreEnv) {}

  // ---------------------------------------------------------------- ingest

  ingest(topic: string, bytes: Uint8Array, qos: Qos, retain: boolean, props: Mqtt5Props | null, at: number): StoredMsg {
    const rec = this.topicRec(topic);
    const m: StoredMsg = { seq: ++this.seq, topic, at, qos, retain, bytes, text: decodeUtf8(bytes), props };
    rec.buffer.push(m);
    const cap = rec.cap ?? this.env.bufferPerTopic();
    if (rec.buffer.length > cap) rec.buffer.splice(0, rec.buffer.length - cap);
    rec.messages++;
    rec.lastAt = rec.lastAt === null ? at : Math.max(rec.lastAt, at);
    rec.retained = retain;
    rec.recent.push(at);
    if (rec.recent.length > 5000) rec.recent.splice(0, rec.recent.length - 5000);
    this.recentAll.push(at);
    if (this.recentAll.length > 20000) this.recentAll.splice(0, this.recentAll.length - 20000);
    this.updateSeries(rec, m);
    return m;
  }

  private topicRec(topic: string): TopicRec {
    let rec = this.topics.get(topic);
    if (rec) return rec;
    rec = { path: topic, buffer: [], cap: null, messages: 0, lastAt: null, retained: false, recent: [], series: new Map() };
    this.topics.set(topic, rec);
    const segs = topic.split('/');
    let node = this.root;
    let path = '';
    segs.forEach((seg, i) => {
      path = i === 0 ? seg : `${path}/${seg}`;
      let child = node.children.get(seg);
      if (!child) {
        child = { name: seg, path, depth: i, children: new Map(), sorted: null, topic: null, parent: node };
        node.children.set(seg, child);
        node.sorted = null;
      }
      node = child;
    });
    node.topic = rec;
    return rec;
  }

  private updateSeries(rec: TopicRec, m: StoredMsg): void {
    const ls = msgLeaves(m);
    if (ls.length === 0 && m.bytes.length > FIELD_SIZE_LIMIT) return;
    const seen = new Set<string>();
    const max = this.env.pointsPerSeries();
    for (const l of ls) {
      const c = castValue(l.value);
      if (c.num === null) continue;
      seen.add(l.path);
      let s = rec.series.get(l.path);
      if (!s) {
        s = { points: [], skipped: 0, firstSkipReason: null, rules: new Set(), count: 0 };
        rec.series.set(l.path, s);
      }
      s.points.push([m.at, c.num]);
      s.count++;
      s.rules.add(c.cast);
      if (s.points.length > max) s.points.splice(0, s.points.length - max);
    }
    for (const [path, s] of rec.series) {
      if (seen.has(path)) continue;
      s.skipped++;
      if (s.firstSkipReason === null) {
        const v = valueAt(m, path);
        s.firstSkipReason = v === undefined ? `${path} missing` : `${JSON.stringify(v)} is not a number`;
      }
    }
  }

  // ---------------------------------------------------------------- aggregates

  private sortedChildren(n: Node): Node[] {
    if (!n.sorted) {
      n.sorted = [...n.children.values()].sort((a, b) => {
        if (n === this.root) {
          const da = a.name.startsWith('$') ? 1 : 0;
          const db = b.name.startsWith('$') ? 1 : 0;
          if (da !== db) return da - db;
        }
        return a.name < b.name ? -1 : a.name > b.name ? 1 : 0;
      });
    }
    return n.sorted;
  }

  /** Change set 9: the count of the last completed second (`lastSecondRate`), as the Rust engine does. */
  private rateOf(rec: TopicRec, now: number): number {
    pruneTimes(rec.recent, now);
    return lastSecondRate(rec.recent, now);
  }

  private aggs(now: number): Map<Node, Agg> {
    const key = `${this.seq}:${Math.floor(now / 250)}`;
    if (this.aggCache && this.aggCache.key === key) return this.aggCache.map;
    const map = new Map<Node, Agg>();
    const walk = (n: Node): Agg => {
      const a: Agg = { topicCount: 0, messages: 0, lastAt: null, rate: 0 };
      if (n.topic) {
        a.topicCount = 1;
        a.messages = n.topic.messages;
        a.lastAt = n.topic.lastAt;
        a.rate = this.rateOf(n.topic, now);
      }
      for (const c of n.children.values()) {
        const ca = walk(c);
        a.topicCount += ca.topicCount;
        a.messages += ca.messages;
        a.rate += ca.rate;
        if (ca.lastAt !== null && (a.lastAt === null || ca.lastAt > a.lastAt)) a.lastAt = ca.lastAt;
      }
      map.set(n, a);
      return a;
    };
    walk(this.root);
    this.aggCache = { key, map };
    return map;
  }

  msgRate(now: number): number {
    pruneTimes(this.recentAll, now);
    return lastSecondRate(this.recentAll, now);
  }

  // ---------------------------------------------------------------- matching

  private topicMatcher(q: TreeQuery, now: number): ((r: TopicRec) => boolean) | null {
    const sm = searchMatcher(q.search);
    const f = q.filters;
    if (!sm && !filtersActive(f)) return null;
    return (r) => this.filterTopic(r, f, now) && (!sm || sm(r.path));
  }

  private filterTopic(r: TopicRec, f: Filters, now: number): boolean {
    const last = r.buffer[r.buffer.length - 1];
    if (f.retainedOnly && !r.retained) return false;
    if (f.changedWithinMin !== null && (r.lastAt === null || now - r.lastAt > f.changedWithinMin * 60_000)) return false;
    if (f.qos !== null && (!last || last.qos !== f.qos)) return false;
    if (f.hasValue && (!last || last.bytes.length === 0)) return false;
    if (f.payloadContains !== '') {
      if (!last || last.text === null || !last.text.toLowerCase().includes(f.payloadContains.toLowerCase())) return false;
    }
    return true;
  }

  private visible(q: TreeQuery | null, now: number): { nodes: Node[]; forced: boolean } {
    const match = q ? this.topicMatcher(q, now) : null;
    const out: Node[] = [];
    if (!match) {
      const walk = (n: Node) => {
        for (const c of this.sortedChildren(n)) {
          out.push(c);
          if (c.children.size > 0 && this.expanded.has(c.path)) walk(c);
        }
      };
      walk(this.root);
      return { nodes: out, forced: false };
    }
    const inSet = new Set<Node>();
    const mark = (n: Node): boolean => {
      let any = n.topic !== null && match(n.topic);
      for (const c of n.children.values()) if (mark(c)) any = true;
      if (any) inSet.add(n);
      return any;
    };
    mark(this.root);
    const walk = (n: Node) => {
      for (const c of this.sortedChildren(n)) {
        if (!inSet.has(c)) continue;
        out.push(c);
        walk(c);
      }
    };
    walk(this.root);
    return { nodes: out, forced: true };
  }

  private row(n: Node, ag: Agg, forced: boolean, now: number, visibleSet?: Set<Node>): TreeRow {
    const rec = n.topic;
    const last = rec?.buffer[rec.buffer.length - 1];
    const spark = rec ? this.spark(rec) : { values: [], times: [] };
    const hasChildren = n.children.size > 0;
    let expanded = hasChildren && this.expanded.has(n.path);
    if (forced) expanded = hasChildren && (!visibleSet || [...n.children.values()].some((c) => visibleSet.has(c)));
    return {
      path: n.path,
      name: n.name,
      depth: n.depth,
      hasChildren,
      expanded,
      topicCount: ag.topicCount,
      messages: ag.messages,
      isTopic: rec !== null,
      value: previewOf(last),
      retained: rec?.retained ?? false,
      lastReceivedAt: ag.lastAt,
      active: ag.lastAt !== null && now - ag.lastAt <= 10_000,
      rate: Math.round(ag.rate * 10) / 10,
      spark: spark.values,
      sparkTimes: spark.times,
      formatHint: n.depth === 0 && n.name === 'spBv1.0' ? 'Sparkplug B' : null,
      pinned: this.pinned.has(n.path),
    };
  }

  /**
   * The tracked value of a topic (change set 8): the whole payload when it casts to a number or boolean (a "$" series
   * exists only then), else the value picked with "Use as sparkline"; nothing is guessed from a structured payload.
   */
  trackedField(rec: TopicRec): string | null {
    if (rec.series.has('$')) return '$';
    const p = this.env.sparklinePath?.(rec.path) ?? null;
    return p && rec.series.has(p) ? p : null;
  }

  spark(rec: TopicRec): { values: number[]; times: number[] } {
    const f = this.trackedField(rec);
    if (!f) return { values: [], times: [] };
    const s = rec.series.get(f);
    if (!s) return { values: [], times: [] };
    const pts = s.points.slice(-10);
    return { values: pts.map((p) => p[1]), times: pts.map((p) => p[0]) };
  }

  treeSlice(q: TreeQuery, start: number, count: number): TreeSlice {
    const now = this.env.now();
    const ag = this.aggs(now);
    const { nodes, forced } = this.visible(q, now);
    const set = forced ? new Set(nodes) : undefined;
    const s = Math.max(0, Math.min(start, nodes.length));
    const rows = nodes.slice(s, s + Math.max(0, count)).map((n) => this.row(n, ag.get(n)!, forced, now, set));
    return { total: nodes.length, start: s, rows, seq: this.seq };
  }

  tableSlice(q: TableQuery, start: number, count: number): TableSlice {
    const now = this.env.now();
    const sm = searchMatcher(q.search);
    const inView = [...this.topics.values()].filter((r) => !sm || sm(r.path));
    const rows = inView.filter((r) => this.filterTopic(r, q.filters, now));
    const dir = q.sort.dir === 'asc' ? 1 : -1;
    const lastOf = (r: TopicRec) => r.buffer[r.buffer.length - 1];
    const num = (s: string | null) => (s !== null && s.trim() !== '' && Number.isFinite(Number(s)) ? Number(s) : null);
    const cmp = (a: TopicRec, b: TopicRec): number => {
      switch (q.sort.column) {
        case 'value': {
          const va = previewOf(lastOf(a)) ?? '';
          const vb = previewOf(lastOf(b)) ?? '';
          const na = num(va);
          const nb = num(vb);
          if (na !== null && nb !== null) return na - nb;
          return va < vb ? -1 : va > vb ? 1 : 0;
        }
        case 'updated':
          return (a.lastAt ?? 0) - (b.lastAt ?? 0);
        case 'rate':
          return this.rateOf(a, now) - this.rateOf(b, now);
        case 'messages':
          return a.messages - b.messages;
        case 'qos':
          return (lastOf(a)?.qos ?? -1) - (lastOf(b)?.qos ?? -1);
        case 'retained':
          return Number(a.retained) - Number(b.retained);
        case 'size':
          return (lastOf(a)?.bytes.length ?? 0) - (lastOf(b)?.bytes.length ?? 0);
        case 'spark':
        case 'pin':
        case 'topic':
        default:
          return a.path < b.path ? -1 : a.path > b.path ? 1 : 0;
      }
    };
    rows.sort((a, b) => {
      const pa = this.pinned.has(a.path) ? 0 : 1;
      const pb = this.pinned.has(b.path) ? 0 : 1;
      if (pa !== pb) return pa - pb;
      const c = cmp(a, b) * dir;
      return c !== 0 ? c : a.path < b.path ? -1 : 1;
    });
    const s = Math.max(0, Math.min(start, rows.length));
    const out: TableRow[] = rows.slice(s, s + Math.max(0, count)).map((r) => {
      const last = lastOf(r);
      const sp = this.spark(r);
      return {
        path: r.path,
        value: previewOf(last),
        spark: sp.values,
        sparkTimes: sp.times,
        lastReceivedAt: r.lastAt,
        rate: Math.round(this.rateOf(r, now) * 10) / 10,
        messages: r.messages,
        qos: last ? last.qos : null,
        retained: r.retained,
        size: last ? last.bytes.length : 0,
        pinned: this.pinned.has(r.path),
      };
    });
    return { total: rows.length, topicsInView: inView.length, start: s, rows: out, seq: this.seq };
  }

  // ---------------------------------------------------------------- expansion

  private nodeAt(path: string): Node | null {
    let n = this.root;
    for (const seg of path.split('/')) {
      const c = n.children.get(seg);
      if (!c) return null;
      n = c;
    }
    return n;
  }

  setExpanded(path: string, on: boolean): void {
    if (on) this.expanded.add(path);
    else this.expanded.delete(path);
  }

  expandToDepth(depth: number): void {
    this.expanded.clear();
    const walk = (n: Node) => {
      for (const c of n.children.values()) {
        if (c.depth < depth && c.children.size > 0) {
          this.expanded.add(c.path);
          walk(c);
        }
      }
    };
    walk(this.root);
  }

  expandChanged(withinMs: number): void {
    const now = this.env.now();
    const ag = this.aggs(now);
    for (const [n, a] of ag) {
      if (n !== this.root && n.children.size > 0 && a.lastAt !== null && now - a.lastAt <= withinMs) this.expanded.add(n.path);
    }
  }

  revealTopic(topic: string): number {
    const n = this.nodeAt(topic);
    if (!n) return -1;
    let p = n.parent;
    while (p && p !== this.root) {
      this.expanded.add(p.path);
      p = p.parent;
    }
    return this.visible(null, this.env.now()).nodes.indexOf(n);
  }

  /** Topics in tree order (depth first, all expanded). */
  private allTopicsInOrder(): TopicRec[] {
    const out: TopicRec[] = [];
    const walk = (n: Node) => {
      for (const c of this.sortedChildren(n)) {
        if (c.topic) out.push(c.topic);
        walk(c);
      }
    };
    walk(this.root);
    return out;
  }

  nextChanged(fromTopic: string | null, withinMs: number): string | null {
    const now = this.env.now();
    const list = this.allTopicsInOrder();
    if (list.length === 0) return null;
    let i = fromTopic ? list.findIndex((r) => r.path === fromTopic) : -1;
    for (let k = 0; k < list.length; k++) {
      i = (i + 1) % list.length;
      const r = list[i];
      if (r.path === fromTopic) continue;
      if (r.lastAt !== null && now - r.lastAt <= withinMs) return r.path;
    }
    return null;
  }

  knownTopics(prefix: string, limit: number): string[] {
    const low = prefix.toLowerCase();
    return [...this.topics.keys()]
      .filter((t) => t.toLowerCase().includes(low))
      .sort()
      .slice(0, Math.max(0, limit));
  }

  // ---------------------------------------------------------------- inspection

  decode(m: StoredMsg, format: PayloadFormat, formatAnyway: boolean): DecodedPayload {
    const b = m.bytes;
    const detected = this.detect(m);
    const base = { hex: hexDump(b), base64: toBase64(b), imageDataUrl: null as string | null, error: null as DecodedPayload['error'], paged: false, json: null as unknown, pretty: null as string | null, text: null as string | null };
    if (b.length > this.env.renderLimitBytes() && !formatAnyway) {
      return { ...base, format: 'text', detected, text: m.text ?? latin1(b), paged: true };
    }
    const use = format === 'auto' ? detected : format;
    const asJson = (errKey: string, detailFor: string): DecodedPayload => {
      const j = msgJson(m);
      if (j === null) {
        let detail = 'not an object or array';
        try {
          JSON.parse(m.text ?? '');
        } catch (e) {
          detail = e instanceof Error ? e.message : String(e);
        }
        return { ...base, format: use as DecodedPayload['format'], detected, text: m.text, error: um(errKey, { detail: detailFor || detail, type: '', }) };
      }
      return { ...base, format: use as DecodedPayload['format'], detected, json: j, pretty: JSON.stringify(j, null, 2) };
    };
    switch (use) {
      case 'json':
        return asJson('decode.json', '');
      case 'sparkplug':
        if (!m.topic.startsWith('spBv1.0/')) return { ...base, format: 'sparkplug', detected, error: um('decode.sparkplug', { detail: 'the topic is not in the spBv1.0 namespace' }) };
        return asJson('decode.sparkplug', 'invalid payload');
      case 'protobuf': {
        const schema = this.env.protoSchemas().find((s) => topicMatches(s.topicFilter, m.topic));
        if (!schema) return { ...base, format: 'protobuf', detected, error: um('decode.protobuf', { type: '(none)', detail: 'no schema is assigned to this topic' }) };
        const j = msgJson(m);
        if (j === null) return { ...base, format: 'protobuf', detected, error: um('decode.protobuf', { type: schema.messageType, detail: 'wire type 3 at byte 0' }) };
        return { ...base, format: 'protobuf', detected, json: j, pretty: JSON.stringify(j, null, 2) };
      }
      case 'image':
        if (!isImage(b)) return { ...base, format: 'image', detected, error: um('decode.notImage') };
        return { ...base, format: 'image', detected, imageDataUrl: `data:image/${b[0] === 0x89 ? 'png' : 'jpeg'};base64,${base.base64}` };
      case 'cbor':
        return { ...base, format: 'cbor', detected, error: um('decode.cbor', { detail: `unexpected major type ${b.length ? b[0] >> 5 : 0} at byte 0` }) };
      case 'msgpack':
        return { ...base, format: 'msgpack', detected, error: um('decode.msgpack', { detail: `invalid type marker 0x${(b[0] ?? 0).toString(16).padStart(2, '0')}` }) };
      case 'hex':
        return { ...base, format: 'hex', detected };
      case 'base64':
        return { ...base, format: 'base64', detected };
      case 'text':
      default:
        return { ...base, format: 'text', detected, text: m.text ?? latin1(b) };
    }
  }

  private detect(m: StoredMsg): DecodedPayload['detected'] {
    if (isImage(m.bytes)) return 'image';
    if (m.topic.startsWith('spBv1.0/')) return 'sparkplug';
    if (m.text === null) return 'hex';
    if (m.bytes.length <= this.env.renderLimitBytes() || true) {
      const t = m.text.trimStart();
      if ((t.startsWith('{') || t.startsWith('[')) && msgJson(m) !== null) return 'json';
    }
    return 'text';
  }

  messageView(m: StoredMsg, format: PayloadFormat, formatAnyway: boolean): MessageView {
    return {
      seq: m.seq,
      topic: m.topic,
      receivedAt: m.at,
      qos: m.qos,
      retain: m.retain,
      dup: false,
      size: m.bytes.length,
      truncated: false,
      decoded: this.decode(m, format, formatAnyway),
      properties: m.props ? { ...m.props, userProperties: m.props.userProperties.map((p) => [p[0], p[1]] as [string, string]) } : null,
    };
  }

  topicDetail(topic: string): TopicDetail {
    const rec = this.topics.get(topic);
    const last = rec?.buffer[rec.buffer.length - 1];
    return {
      topic,
      exists: !!rec,
      last: last ? this.messageView(last, 'auto', false) : null,
      historyCount: rec?.buffer.length ?? 0,
      bufferSize: rec?.cap ?? this.env.bufferPerTopic(),
      messages: rec?.messages ?? 0,
      pinned: this.pinned.has(topic),
      fields: last ? fieldsOf(last) : [],
      defaultField: rec ? this.trackedField(rec) : null,
      seq: this.seq,
    };
  }

  findMessage(topic: string, seq: number): StoredMsg | null {
    const rec = this.topics.get(topic);
    return rec?.buffer.find((m) => m.seq === seq) ?? null;
  }

  history(topic: string): HistoryEntry[] {
    const rec = this.topics.get(topic);
    if (!rec) return [];
    const out: HistoryEntry[] = [];
    let prev: Map<string, string> | null = null;
    let prevText: string | null = null;
    for (const m of rec.buffer) {
      const ls = msgLeaves(m);
      const json = msgJson(m) !== null;
      const cur = new Map(ls.map((l) => [l.label, JSON.stringify(l.value)]));
      const changed: string[] = [];
      if (prev !== null) {
        if (json) {
          for (const [k, v] of cur) if (prev.get(k) !== v) changed.push(k);
          for (const k of prev.keys()) if (!cur.has(k)) changed.push(k);
        } else if (m.text !== prevText) changed.push('value');
      }
      out.push({ seq: m.seq, receivedAt: m.at, size: m.bytes.length, qos: m.qos, retain: m.retain, changedKeys: changed, preview: previewOf(m) ?? '' });
      prev = cur;
      prevText = m.text;
    }
    return out.reverse();
  }

  diff(topic: string, baselineSeq: number, seq: number): DiffResult {
    const a = this.findMessage(topic, baselineSeq);
    const b = this.findMessage(topic, seq);
    const ja = a ? msgJson(a) : null;
    const jb = b ? msgJson(b) : null;
    if (ja !== null && jb !== null && typeof ja === 'object' && typeof jb === 'object') {
      const lines: DiffLine[] = [];
      let changes = 0;
      const emit = (oldV: unknown, newV: unknown, key: string | null, indent: number, path: string) => {
        const oc = oldV !== null && typeof oldV === 'object';
        const nc = newV !== null && typeof newV === 'object';
        if (oc && nc && Array.isArray(oldV) === Array.isArray(newV)) {
          const isArr = Array.isArray(newV);
          lines.push({ indent, key, path: null, oldValue: null, newValue: null, open: isArr ? '[' : '{', close: null, kind: 'same', plottable: false });
          if (isArr) {
            const oa = oldV as unknown[];
            const na = newV as unknown[];
            for (let i = 0; i < Math.max(oa.length, na.length); i++) emit(i < oa.length ? oa[i] : undefined, i < na.length ? na[i] : undefined, null, indent + 1, `${path}[${i}]`);
          } else {
            const oo = oldV as Record<string, unknown>;
            const no = newV as Record<string, unknown>;
            const keys = [...Object.keys(no), ...Object.keys(oo).filter((k) => !(k in no))];
            for (const k of keys) emit(oo[k], no[k], k, indent + 1, `${path}.${k}`);
          }
          lines.push({ indent, key: null, path: null, oldValue: null, newValue: null, open: null, close: isArr ? ']' : '}', kind: 'same', plottable: false });
          return;
        }
        const plottable = castValue(newV === undefined ? oldV : newV).num !== null;
        if (oldV === undefined) {
          changes++;
          lines.push({ indent, key, path, oldValue: null, newValue: JSON.stringify(newV), open: null, close: null, kind: 'added', plottable });
        } else if (newV === undefined) {
          changes++;
          lines.push({ indent, key, path, oldValue: JSON.stringify(oldV), newValue: null, open: null, close: null, kind: 'removed', plottable });
        } else if (JSON.stringify(oldV) === JSON.stringify(newV)) {
          lines.push({ indent, key, path, oldValue: null, newValue: JSON.stringify(newV), open: null, close: null, kind: 'same', plottable });
        } else {
          changes++;
          lines.push({ indent, key, path, oldValue: JSON.stringify(oldV), newValue: JSON.stringify(newV), open: null, close: null, kind: 'changed', plottable });
        }
      };
      emit(ja, jb, null, 0, '$');
      return { mode: 'json', lines, textLines: [], changes, a: baselineSeq, b: seq };
    }
    const la = (a?.text ?? '').split('\n');
    const lb = (b?.text ?? '').split('\n');
    const textLines = lineDiff(la, lb);
    return { mode: 'text', lines: [], textLines, changes: textLines.filter((l) => l.kind !== 'same').length, a: baselineSeq, b: seq };
  }

  setTopicBuffer(topic: string, size: number): void {
    const rec = this.topics.get(topic);
    if (!rec) return;
    rec.cap = Math.max(10, Math.min(100000, Math.floor(size)));
    if (rec.buffer.length > rec.cap) rec.buffer.splice(0, rec.buffer.length - rec.cap);
  }

  private under(prefix: string | null): TopicRec[] {
    return [...this.topics.values()].filter((r) => prefix === null || r.path === prefix || r.path.startsWith(prefix.endsWith('/') ? prefix : prefix + '/'));
  }

  clearHistory(prefix: string | null): void {
    for (const r of this.under(prefix)) {
      r.buffer = [];
      r.messages = 0;
      r.series.clear();
      r.recent = [];
      r.lastAt = null;
    }
    if (prefix === null) this.recentAll = [];
    this.seq++;
  }

  retainedUnder(prefix: string): { topic: string; preview: string }[] {
    return this.under(prefix)
      .filter((r) => r.retained)
      .map((r) => ({ topic: r.path, preview: previewOf(r.buffer[r.buffer.length - 1]) ?? '' }))
      .sort((a, b) => (a.topic < b.topic ? -1 : 1));
  }

  clearRetained(topic: string): boolean {
    const r = this.topics.get(topic);
    if (!r || !r.retained) return false;
    r.retained = false;
    this.seq++;
    return true;
  }

  series(topic: string, path: string, sinceMs: number | null): SeriesData {
    const rec = this.topics.get(topic);
    const empty: SeriesData = { topic, path, points: [], gaps: [], skipped: 0, firstSkipReason: null, castRule: 'none', step: false, precisionLoss: false, count: 0 };
    if (!rec) return empty;
    const s = rec.series.get(path);
    if (s) {
      const pts = sinceMs === null ? s.points : s.points.filter((p) => p[0] >= sinceMs);
      const rule = s.rules.size > 1 ? 'mixed' : ((s.rules.values().next().value as SeriesData['castRule'] | undefined) ?? 'none');
      return { ...empty, points: pts.map((p) => [p[0], p[1]]), skipped: s.skipped, firstSkipReason: s.firstSkipReason, castRule: rule, step: rule === 'boolean', count: s.count };
    }
    // a path not recorded yet: compute from the ring buffer
    const points: [number, number][] = [];
    const rules = new Set<string>();
    let skipped = 0;
    let firstSkipReason: string | null = null;
    for (const m of rec.buffer) {
      const v = valueAt(m, path);
      const c = castValue(v);
      if (c.num === null) {
        skipped++;
        if (firstSkipReason === null) firstSkipReason = v === undefined ? `${path} missing` : `${JSON.stringify(v)} is not a number`;
        continue;
      }
      rules.add(c.cast);
      if (sinceMs === null || m.at >= sinceMs) points.push([m.at, c.num]);
    }
    const rule = rules.size > 1 ? 'mixed' : ((rules.values().next().value as SeriesData['castRule'] | undefined) ?? 'none');
    return { ...empty, points, skipped, firstSkipReason, castRule: rule, step: rule === 'boolean', count: points.length };
  }

  memory(): { bytes: number; points: number; messages: number } {
    let bytes = 0;
    let points = 0;
    let messages = 0;
    for (const r of this.topics.values()) {
      for (const m of r.buffer) bytes += m.bytes.length * 2;
      messages += r.buffer.length;
      for (const s of r.series.values()) points += s.points.length;
    }
    return { bytes: bytes + points * 16, points, messages };
  }

  exportEstimate(o: ExportOptions): ExportEstimate {
    let recs: TopicRec[];
    if (o.scope.kind === 'topic') recs = this.under(null).filter((r) => r.path === (o.scope as { topic: string }).topic);
    else if (o.scope.kind === 'subtree') recs = this.under(o.scope.prefix.replace(/\/#$/, ''));
    else {
      const set = new Set(o.scope.topics);
      recs = [...this.topics.values()].filter((r) => set.has(r.path));
    }
    let messages = 0;
    let bytes = 0;
    let topics = 0;
    for (const r of recs) {
      let n = 0;
      for (const m of r.buffer) {
        if (o.fromMs !== null && m.at < o.fromMs) continue;
        if (o.toMs !== null && m.at > o.toMs) continue;
        n++;
        bytes += (o.rawBase64 ? Math.ceil(m.bytes.length / 3) * 4 : m.bytes.length) + 40 + r.path.length + (o.includeProperties && m.props ? 30 : 0);
      }
      if (n > 0) topics++;
      messages += n;
    }
    return { messages, topics, bytes };
  }
}

function latin1(b: Uint8Array): string {
  let s = '';
  for (let i = 0; i < Math.min(b.length, 1 << 20); i++) s += String.fromCharCode(b[i]);
  return s;
}

function lineDiff(a: string[], b: string[]): { kind: 'same' | 'added' | 'removed' | 'changed'; text: string }[] {
  const n = a.length;
  const m = b.length;
  if (n * m > 4_000_000) {
    return [...a.map((t) => ({ kind: 'removed' as const, text: t })), ...b.map((t) => ({ kind: 'added' as const, text: t }))];
  }
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
  const out: { kind: 'same' | 'added' | 'removed'; text: string }[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      out.push({ kind: 'same', text: b[j] });
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) out.push({ kind: 'removed', text: a[i++] });
    else out.push({ kind: 'added', text: b[j++] });
  }
  while (i < n) out.push({ kind: 'removed', text: a[i++] });
  while (j < m) out.push({ kind: 'added', text: b[j++] });
  return out;
}

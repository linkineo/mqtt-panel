// Complete in-memory EngineClient for tests, Playwright and the browser dev server.
import type { EngineClient } from '../client';
import type {
  AppInfo,
  CertDetails,
  ConnState,
  AppBrokerStepResult,
  ConnectOutcome,
  ConnectionStatus,
  EngineEvent,
  FileInspection,
  FileKind,
  Listener,
  Mqtt5Props,
  NegotiatedVersion,
  Profile,
  PublishRequest,
  Qos,
  RepeatStatus,
  SaveOutcome,
  SentEntry,
  Settings,
  Template,
  TestOutcome,
  TestResult,
  TickEvent,
  TrustPrompt,
  UserMessage,
} from '../types';
import { APP_USER, MockBroker } from './broker';
import {
  WORDS,
  buildDataset,
  certSummary,
  demoProfiles,
  demoTemplates,
  emptyProps,
  file,
  importedProfiles,
  keySummary,
  LAB_FINGERPRINT,
  newProfileDefaults,
  PLANT_FINGERPRINT,
  type GenState,
  type Lang,
  type TopicDef,
} from './data';
import { encodePayloadBytes } from './encode';
import { TopicStore, previewOf } from './topics';
import { clone, delay, randHex, topicMatches, um, utf8 } from './util';

export interface MockOptions {
  scenario?: 'demo' | 'empty';
  lang?: Lang;
  live?: boolean;
  latencyMs?: number;
  now?: () => number;
  /** What `keyStoreKind` answers (default "keychain", as a signed build). */
  keyStore?: 'file' | 'keychain';
}

export interface MockControls {
  inject(connectionId: string, topic: string, payload: string, opts?: { qos?: Qos; retain?: boolean; props?: Mqtt5Props | null; at?: number }): void;
  emitMenu(id: string): void;
  /** Every file the mock "saved" through a native save dialog, oldest first (e2e). */
  savedFiles(): { fileName: string; bytes: number }[];
  /** Forces a connection state (e2e: a reconnecting card and tab). */
  setConnectionState(connectionId: string, state: ConnState): void;
  /** Makes every broker start fail as if another program held a listener's port (e2e: C-17). */
  failBrokerStart(on: boolean): void;
  /** Another program holds (or frees) a port: validation refuses it, a listener switched on there fails alone
   * (e2e: B-25, change set 7). */
  takePort(port: number, taken?: boolean): void;
  tick(): void;
  dispose(): void;
}

export type MockEngineClient = EngineClient & MockControls;

interface RepeatRec {
  status: RepeatStatus;
  timer: ReturnType<typeof setInterval> | null;
  entry: SentEntry;
  dropAfterTick: boolean;
}

interface Conn {
  id: string;
  state: ConnState;
  negotiated: NegotiatedVersion;
  store: TopicStore;
  sent: SentEntry[];
  repeat: RepeatRec | null;
  backfilled: boolean;
  defs: { def: TopicDef; nextAt: number }[];
  gen: GenState;
  /** The open connection skipped certificate verification (profile switch or "only this time"). */
  skipVerify: boolean;
}

const CHANGED_FINGERPRINT = 'C4:10:7E:2B:99:5A:03:DF:61:8C:E2:47:B0:1D:7A:93:5E:C8:24:F6:0B:AD:31:72:E9:46:18:BC:0F:85:D3:2A';
const PRIVATE_FINGERPRINT = '5B:E1:08:93:4C:7F:A2:16:D0:3E:89:65:BA:21:CF:47:93:0A:E8:5D:12:76:C3:9F:04:B8:6E:2D:A1:58:F7:3C';
const EXPIRED_FINGERPRINT = '91:2C:F4:6A:0D:83:E7:15:B9:42:7C:D8:30:AF:65:1E:C2:9B:47:08:F3:5D:A6:21:8E:74:0B:DC:39:E5:12:6F';

function detectPlatform(): AppInfo['platform'] {
  if (typeof navigator === 'undefined') return 'linux';
  const s = `${navigator.platform ?? ''} ${navigator.userAgent ?? ''}`;
  if (s.includes('Mac')) return 'macos';
  if (s.includes('Win')) return 'windows';
  return 'linux';
}

export function createMockEngineClient(options: MockOptions = {}): MockEngineClient {
  const now = options.now ?? (() => Date.now());
  const lang: Lang = options.lang ?? 'en';
  const words = WORDS[lang];
  const live = options.live ?? true;
  const latency = options.latencyMs ?? 350;
  const demo = (options.scenario ?? 'demo') === 'demo';

  let settings: Settings = {
    theme: 'system',
    accent: 'sage',
    density: 'comfortable',
    monoSize: 12,
    valuesInTree: true,
    motion: true,
    language: 'system',
    bufferPerTopic: 200,
    pointsPerSeries: 5000,
    memoryBudgetMb: 256,
    payloadRenderLimitKb: 256,
    updateCheck: true,
    updateChannel: 'stable',
    brokerLogOpen: false,
  };
  let profiles: Profile[] = demo ? demoProfiles(lang, now()) : [];
  const templates = new Map<string, Template[]>();
  if (demo) templates.set('plant', demoTemplates(lang));
  let templateN = 10;
  const conns = new Map<string, Conn>();
  const broker = new MockBroker(demo, now);
  /** Password of the profile "Local broker" (the app-generated password of "mqtt-panel"). */
  let localPassword: string | null = demo ? (broker.passwords.get(APP_USER) ?? null) : null;
  const handlers = new Set<(e: EngineEvent) => void>();
  let pendingUpdates: { connectionId: string; entry: SentEntry }[] = [];
  const pendingTrust = new Map<string, TrustPrompt>();
  const dataset = buildDataset(lang);
  let sentN = 0;
  let corrN = 1041;
  let disposed = false;
  const timers: ReturnType<typeof setInterval>[] = [];
  const timeouts = new Set<ReturnType<typeof setTimeout>>();

  const later = (ms: number, fn: () => void) => {
    if (disposed) return;
    const t = setTimeout(() => {
      timeouts.delete(t);
      if (!disposed) fn();
    }, ms);
    timeouts.add(t);
  };

  // ---------------------------------------------------------------- helpers

  const profileById = (id: string) => profiles.find((p) => p.id === id);
  const needProfile = (id: string): Profile => {
    const p = profileById(id);
    if (!p) throw um('profile.notFound');
    return p;
  };
  const negotiatedFor = (p: Profile): NegotiatedVersion => (p.mqttVersion === '3.1.1' || (p.mqttVersion === 'auto' && p.host === '10.20.4.17') ? '3.1.1' : '5');
  const isTls = (p: Profile) => p.scheme === 'mqtts' || p.scheme === 'wss';

  const status = (c: Conn): ConnectionStatus => {
    const p = profileById(c.id);
    return {
      id: c.id,
      name: p?.name ?? c.id,
      state: c.state,
      negotiatedVersion: c.state === 'connecting' && !c.backfilled ? null : c.negotiated,
      requestedVersion: p?.mqttVersion ?? 'auto',
      topics: c.store.topics.size,
      msgRate: c.state === 'connected' ? c.store.msgRate(now()) : 0,
      skipVerify: c.skipVerify,
      error: null,
      reconnectAttempt: 0,
      maxPacketSize: c.negotiated === '5' ? 1048576 : null,
      isLocalBroker: p?.isLocalBroker ?? false,
      subscriptionErrors: [],
    };
  };

  const emit = (e: EngineEvent) => {
    handlers.forEach((h) => {
      try {
        h(e);
      } catch {
        // a failing handler must not break the mock
      }
    });
  };

  const tick = () => {
    if (disposed) return;
    const repeats: RepeatStatus[] = [];
    for (const c of conns.values()) {
      if (!c.repeat) continue;
      repeats.push({ ...c.repeat.status });
      if (c.repeat.dropAfterTick) c.repeat = null;
      else if (!c.repeat.status.running) c.repeat.dropAfterTick = true;
    }
    const ev: TickEvent = {
      type: 'tick',
      connections: [...conns.values()].map(status),
      changed: Object.fromEntries([...conns.values()].map((c) => [c.id, c.store.seq])),
      repeats,
      sentUpdates: pendingUpdates,
      broker: broker.status(),
      brokerLogSeq: broker.logSeq,
    };
    pendingUpdates = [];
    emit(ev);
  };

  const pushUpdate = (c: Conn, entry: SentEntry) => {
    pendingUpdates.push({ connectionId: c.id, entry: clone(entry) });
  };

  const needConn = (id: string): Conn => {
    const c = conns.get(id);
    if (!c) throw um('error.notConnected');
    return c;
  };
  const needConnected = (id: string): Conn => {
    const c = needConn(id);
    if (c.state !== 'connected') throw um('error.notConnected');
    return c;
  };

  const matchesSubs = (c: Conn, topic: string) => {
    const p = profileById(c.id);
    return !!p && p.subscriptions.some((s) => topicMatches(s.filter, topic));
  };

  const propsFor = (c: Conn, extra?: Partial<Mqtt5Props> | null): Mqtt5Props | null => (c.negotiated === '5' ? { ...emptyProps(), ...(extra ?? {}) } : null);

  const makeConn = (id: string): Conn => {
    const store = new TopicStore({
      now,
      bufferPerTopic: () => settings.bufferPerTopic,
      pointsPerSeries: () => settings.pointsPerSeries,
      renderLimitBytes: () => settings.payloadRenderLimitKb * 1024,
      protoSchemas: () => profileById(id)?.protoSchemas ?? [],
      sparklinePath: (topic) => profileById(id)?.topics?.[topic]?.sparklinePath ?? null,
    });
    return { id, state: 'connecting', negotiated: '5', store, sent: [], repeat: null, backfilled: false, defs: [], gen: { values: new Map(), seq: 0 }, skipVerify: false };
  };

  const genMessage = (c: Conn, def: TopicDef, at: number) => {
    const out = def.gen(c.gen, at);
    const bytes = typeof out.payload === 'string' ? utf8(out.payload) : out.payload;
    c.store.ingest(def.topic, bytes, out.qos, out.retain, propsFor(c, out.props), at);
  };

  const backfill = (c: Conn) => {
    const t = now();
    c.defs = dataset.filter((d) => matchesSubs(c, d.topic)).map((def) => ({ def, nextAt: t + Math.random() * def.period * 1000 }));
    const events: { def: TopicDef; at: number }[] = [];
    const span = 30 * 60_000;
    for (const { def } of c.defs) {
      if (!Number.isFinite(def.period)) {
        events.push({ def, at: t - 25 * 60_000 });
        continue;
      }
      const n = Math.min(60, Math.max(1, Math.floor(1800 / def.period)));
      const step = span / n;
      for (let i = 0; i < n; i++) {
        const at = t - span + (i + 0.5) * step + (Math.random() - 0.5) * step * 0.4;
        events.push({ def, at: Math.min(at, t - 200) });
      }
    }
    events.sort((a, b) => a.at - b.at);
    for (const e of events) genMessage(c, e.def, Math.round(e.at));
    c.backfilled = true;
  };

  const newEntry = (req: PublishRequest, size: number, c: Conn): SentEntry => ({
    id: `s-${++sentN}`,
    at: now(),
    topic: req.topic,
    size,
    qos: req.qos,
    retain: req.retain,
    status: 'pending',
    reason: null,
    repeatCount: null,
    payloadText: req.payload,
    format: req.format,
    request: c.negotiated === '5' && req.request ? { responseTopic: req.request.responseTopic, correlationData: req.request.correlationData } : null,
    reply: null,
    replyTimedOut: false,
  });

  const addSent = (c: Conn, e: SentEntry) => {
    c.sent.unshift(e);
    if (c.sent.length > 500) c.sent.length = 500;
    pushUpdate(c, e);
  };

  const seedSentLog = (c: Conn) => {
    const t = now();
    const mk = (agoS: number, topic: string, qos: Qos, retain: boolean, status: SentEntry['status'], payload: string, extra: Partial<SentEntry> = {}): SentEntry => ({
      id: `s-${++sentN}`,
      at: t - agoS * 1000,
      topic,
      size: utf8(payload).length,
      qos,
      retain,
      status,
      reason: null,
      repeatCount: null,
      payloadText: payload,
      format: 'json',
      request: null,
      reply: null,
      replyTimedOut: false,
      ...extra,
    });
    c.sent = [
      mk(60, 'building/f2/ahu01/cmd', 2, false, 'acked', '{"cmd":"restart","delay":5}', {
        request: { responseTopic: 'building/f2/ahu01/cmd/ack', correlationData: 'c-1041' },
        reply: { at: t - 60_000 + 84, rttMs: 84, payloadText: '{"ok":true,"id":"c-1041"}', correlationData: 'c-1041' },
      }),
      mk(65, 'building/f2/r204/setpoint', 1, true, 'acked', '{"value":21.5}'),
      mk(310, 'building/f2/ahu01/cmd', 2, false, 'failed', '{"cmd":"restart","delay":5}', { reason: um('reason.notAuthorized') }),
      mk(460, 'building/f2/r203/valve/cmd', 0, false, 'sent', '{"position":100}'),
      mk(560, 'sim/room/temperature', 0, false, 'repeat', '{"value": 21.5}', { repeatCount: 240 }),
      mk(1000, 'building/f2/r202/setpoint', 1, true, 'deletedRetained', '', { size: 0 }),
    ];
  };

  // ---------------------------------------------------------------- trust

  const leaf = (subject: string, issuer: string, san: string[], notBefore: string, notAfter: string, fp: string, selfSigned: boolean): CertDetails => ({
    subject,
    issuer,
    san,
    notBefore,
    notAfter,
    fingerprint: fp,
    keyType: 'ECDSA P-256',
    selfSigned,
    isCa: false,
  });

  /** The profile "Local broker" while the embedded broker is not running: nothing listens on its port. */
  const localDown = (p: Profile): UserMessage | null =>
    p.isLocalBroker && broker.state !== 'running' ? um('error.refused', { host: p.host, port: p.port }) : null;

  const trustCheck = (p: Profile, skipOnce = false): TrustPrompt | UserMessage | null => {
    if (p.host.includes('nowhere')) return um('error.dns', { host: p.host });
    if (p.tls.skipVerify || skipOnce || !isTls(p)) return null;
    const trusted = p.tls.trustedFingerprints;
    if (p.host === '10.20.4.17' || p.host.includes('changed')) {
      const fp = p.host.includes('changed') ? CHANGED_FINGERPRINT : LAB_FINGERPRINT;
      if (trusted.includes(fp)) return null;
      const l = leaf('CN=mosquitto-lab', 'CN=mosquitto-lab', ['mosquitto-lab', p.host], '2026-09-01', '2027-09-01', fp, true);
      if (trusted.length > 0) {
        const prev = trusted[trusted.length - 1];
        return {
          profileId: p.id,
          reason: 'fingerprintChanged',
          reasonText: um('tls.reason.fingerprintChanged', { old: prev, new: fp }),
          leaf: l,
          chain: [l],
          caOffer: false,
          previousFingerprint: prev,
          unsafe: false,
        };
      }
      return { profileId: p.id, reason: 'selfSigned', reasonText: um('tls.reason.selfSigned'), leaf: l, chain: [l], caOffer: false, previousFingerprint: null, unsafe: false };
    }
    if (p.host.includes('private')) {
      if (trusted.includes(PRIVATE_FINGERPRINT) || p.tls.caCert) return null;
      const l = leaf(`CN=${p.host}`, 'CN=Lab Private CA', [p.host], '2026-05-01', '2028-05-01', PRIVATE_FINGERPRINT, false);
      const ca: CertDetails = { ...leaf('CN=Lab Private CA', 'CN=Lab Private CA', [], '2025-01-01', '2035-01-01', '0E:44:B1:9D:5A:0E:88:C2:17:6B:F3:2A:9C:41:D8:70:1E:55:A2:3B:8F:64:CC:09:D1:7E:B6:C1:3F:9A:12:7C', true), isCa: true };
      return { profileId: p.id, reason: 'unknownCa', reasonText: um('tls.reason.unknownCa', { issuer: 'CN=Lab Private CA' }), leaf: l, chain: [l, ca], caOffer: true, previousFingerprint: null, unsafe: false };
    }
    if (p.host.includes('expired')) {
      if (trusted.includes(EXPIRED_FINGERPRINT)) return null;
      const l = leaf(`CN=${p.host}`, `CN=${p.host}`, [p.host], '2024-01-01', '2026-01-01', EXPIRED_FINGERPRINT, true);
      return { profileId: p.id, reason: 'expired', reasonText: um('tls.expired', { date: '2026-01-01' }), leaf: l, chain: [l], caOffer: false, previousFingerprint: null, unsafe: true };
    }
    return null;
  };

  const serverCertFor = (p: Profile): CertDetails | null => {
    if (!isTls(p)) return null;
    if (p.id === 'plant' || p.host === 'broker.plant.example') return leaf('CN=broker.plant.example', 'CN=Plant Root CA', ['broker.plant.example'], '2025-06-30', '2027-06-30', PLANT_FINGERPRINT, false);
    if (p.host === 'localhost') return { ...broker.certs.server };
    if (p.host === '10.20.4.17') return leaf('CN=mosquitto-lab', 'CN=mosquitto-lab', ['mosquitto-lab', '10.20.4.17'], '2026-09-01', '2027-09-01', LAB_FINGERPRINT, true);
    return leaf(`CN=${p.host}`, 'CN=Public Root R1', [p.host], '2026-03-01', '2027-03-01', '22:' + PLANT_FINGERPRINT.slice(3), false);
  };

  // ---------------------------------------------------------------- live traffic

  const liveStep = () => {
    const t = now();
    for (const c of conns.values()) {
      if (c.state !== 'connected') continue;
      for (const d of c.defs) {
        if (t < d.nextAt || !Number.isFinite(d.def.period)) continue;
        genMessage(c, d.def, t);
        d.nextAt = t + d.def.period * 1000 * (0.6 + Math.random() * 0.8);
      }
    }
  };

  if (live) {
    timers.push(setInterval(liveStep, 100));
    timers.push(
      setInterval(() => {
        broker.liveStep();
        tick();
      }, 200),
    );
    timers.push(setInterval(() => broker.liveLog(), 4000));
  }

  const changedTick = () => tick();

  // ---------------------------------------------------------------- connection flow

  const connectNow = async (profileId: string, skipOnce = false): Promise<ConnectOutcome> => {
    const p = needProfile(profileId);
    let c = conns.get(profileId);
    if (c && c.state === 'connected') return { kind: 'connected', status: status(c) };
    const isNew = !c;
    if (!c) {
      c = makeConn(profileId);
      conns.set(profileId, c);
    }
    c.state = 'connecting';
    changedTick();
    await delay(latency);
    if (disposed) return { kind: 'failed', error: um('error.notConnected') };
    const check = localDown(p) ?? trustCheck(p, skipOnce);
    if (check !== null) {
      if (isNew) conns.delete(profileId);
      else c.state = 'disconnected';
      changedTick();
      if ('reason' in check && 'leaf' in check) {
        pendingTrust.set(profileId, check);
        return { kind: 'needsTrust', trust: clone(check) };
      }
      return { kind: 'failed', error: check as UserMessage };
    }
    c.negotiated = negotiatedFor(p);
    c.skipVerify = isTls(p) && (p.tls.skipVerify || skipOnce);
    c.state = 'connected';
    if (!c.backfilled) {
      backfill(c);
      if (demo && profileId === 'plant') seedSentLog(c);
    } else {
      c.defs = dataset.filter((d) => matchesSubs(c!, d.topic)).map((def) => ({ def, nextAt: now() + Math.random() * def.period * 1000 }));
    }
    p.lastUsed = now();
    changedTick();
    return { kind: 'connected', status: status(c) };
  };

  const stopRepeat = (c: Conn, reason: UserMessage | null) => {
    if (!c.repeat || !c.repeat.status.running) return;
    if (c.repeat.timer) clearInterval(c.repeat.timer);
    c.repeat.timer = null;
    c.repeat.status.running = false;
    c.repeat.status.nextAt = null;
    c.repeat.status.stoppedReason = reason;
  };

  const ingestIfSubscribed = (c: Conn, topic: string, bytes: Uint8Array, qos: Qos, retain: boolean, props: Mqtt5Props | null) => {
    if (matchesSubs(c, topic)) c.store.ingest(topic, bytes, qos, retain, props, now());
  };

  const publishProps = (c: Conn, req: PublishRequest, corr: string | null): Mqtt5Props | null => {
    if (c.negotiated !== '5') return null;
    const p = req.props;
    return {
      ...emptyProps(),
      contentType: p?.contentType || null,
      payloadFormat: p?.payloadFormat ?? null,
      messageExpiry: p?.messageExpiry ?? null,
      userProperties: p ? p.userProperties.map(([k, v]) => [k, v] as [string, string]) : [],
      responseTopic: req.request && corr ? req.request.responseTopic : null,
      correlationData: corr,
    };
  };

  const doPublish = (c: Conn, req: PublishRequest): SentEntry => {
    const enc = encodePayloadBytes(req.format, req.payload);
    const entry = newEntry(req, enc.ok ? enc.bytes.length : utf8(req.payload).length, c);
    if (!enc.ok) {
      entry.status = 'failed';
      entry.reason = enc.error;
      addSent(c, entry);
      return entry;
    }
    let corr: string | null = null;
    if (entry.request) {
      corr = entry.request.correlationData || `c-${++corrN}`;
      entry.request.correlationData = corr;
    }
    if (req.topic.startsWith('forbidden/')) {
      entry.status = 'pending';
      addSent(c, entry);
      later(120, () => {
        entry.status = 'failed';
        entry.reason = um('reason.notAuthorized');
        pushUpdate(c, entry);
        changedTick();
      });
      return entry;
    }
    ingestIfSubscribed(c, req.topic, enc.bytes, req.qos, req.retain, publishProps(c, req, corr));
    if (req.qos === 0) entry.status = 'sent';
    addSent(c, entry);
    if (req.qos > 0) {
      later(120, () => {
        entry.status = 'acked';
        pushUpdate(c, entry);
        changedTick();
      });
    }
    if (entry.request && corr) {
      const rt = entry.request.responseTopic;
      const corrId = corr;
      if (req.topic.endsWith('/cmd')) {
        later(84, () => {
          const payloadText = JSON.stringify({ ok: true, id: corrId });
          entry.reply = { at: now(), rttMs: 84, payloadText, correlationData: corrId };
          ingestIfSubscribed(c, rt, utf8(payloadText), 1, false, propsFor(c, { correlationData: corrId }));
          pushUpdate(c, entry);
          changedTick();
        });
      } else {
        later(Math.max(1, req.request?.waitS ?? 10) * 1000, () => {
          entry.replyTimedOut = true;
          pushUpdate(c, entry);
          changedTick();
        });
      }
    }
    return entry;
  };

  // ---------------------------------------------------------------- files

  const stem = (path: string) => (path.split(/[\\/]/).pop() ?? path).replace(/\.[^.]+$/, '');
  const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;
  const certKinds: FileKind[] = ['caCert', 'clientCert', 'brokerCert'];

  const inspect = (kind: FileKind, path: string): FileInspection => {
    const fileName = baseName(path);
    const ext = (fileName.match(/\.([^.]+)$/)?.[1] ?? '').toLowerCase();
    const fail = (key: string, params: Record<string, string | number> = {}): FileInspection => ({ ok: false, path, fileName, error: um(key, params) });
    if (kind === 'proto') {
      if (ext !== 'proto') return fail('file.notProto', { detail: 'expected a .proto file' });
      return { ok: true, path, fileName, summary: { ...keySummary(), kind: 'bundle', keyType: null } };
    }
    if (kind === 'csr') {
      if (ext !== 'csr' && ext !== 'pem') return fail('file.notCsr');
      return { ok: true, path, fileName, summary: { ...certSummary(stem(path), '', {}), notAfter: null, notBefore: null } };
    }
    if (kind === 'import' || kind === 'payload') return { ok: true, path, fileName, summary: { ...keySummary(), kind: 'bundle', keyType: null } };
    if (ext === 'p12' || ext === 'pfx') {
      return { ok: true, path, fileName, summary: { ...certSummary(stem(path), '2027-10-12'), kind: 'pkcs12', encrypted: true } };
    }
    if (ext === 'key') {
      if (kind === 'caCert') return fail('file.notCertificate');
      return { ok: true, path, fileName, summary: keySummary(false) };
    }
    if (ext === 'pem' || ext === 'crt' || ext === 'cer') {
      if (kind === 'clientKey' || kind === 'brokerKey') {
        if (ext === 'pem' && /key/i.test(fileName)) return { ok: true, path, fileName, summary: keySummary(false) };
        return fail('file.notKey');
      }
      const cn = stem(path);
      const isCa = kind === 'caCert' || /ca/i.test(cn);
      return { ok: true, path, fileName, summary: certSummary(cn, isCa ? '2031-03-01' : '2027-10-12', { isCa, selfSigned: isCa, issuer: isCa ? `CN=${cn}` : 'CN=Plant Root CA' }) };
    }
    return fail(certKinds.includes(kind) ? 'file.notCertificate' : kind === 'clientKey' || kind === 'brokerKey' ? 'file.notKey' : 'file.notCertificate');
  };

  const picked: Record<FileKind, string> = {
    caCert: '/picked/plant-root-ca.pem',
    clientCert: '/picked/commissioning-laptop.pem',
    clientKey: '/picked/commissioning-laptop.key',
    proto: '/picked/plant.proto',
    import: '/picked/connections.json',
    payload: '/picked/payload.json',
    csr: '/picked/device.csr',
    brokerCert: '/picked/server.pem',
    brokerKey: '/picked/server.key',
  };

  const saves: { fileName: string; bytes: number }[] = [];
  const saved = async (fileName: string, bytes = 0): Promise<SaveOutcome> => {
    saves.push({ fileName, bytes });
    await delay(200);
    return { kind: 'saved', fileName };
  };

  /** One broker change, applied at once (change set 7): the profile "Local broker" follows it and, when its
   * connection is open, reconnects only when its own listener or login changed (C-16). */
  const brokerChange = async (apply: () => void) => {
    const before = localFingerprint();
    apply();
    followLocal();
    if (before !== null && localFingerprint() !== before) await reconnectLocal();
    changedTick();
    return clone(broker.config);
  };

  /** What the connection of "Local broker" depends on: its listener (kind, port, reachability) and its login. */
  const localFingerprint = (): string | null => {
    const p = profiles.find((x) => x.isLocalBroker);
    if (!p) return null;
    const desktop = broker.issued.find((c) => c.system && !c.revoked && !c.superseded);
    return JSON.stringify([p.brokerListenerId, p.scheme, p.host, p.port, broker.config.bind, p.username, p.hasPassword, localPassword, p.tls.caCert ? broker.certs.ca.fingerprint : null, p.tls.clientCert ? (desktop?.serial ?? null) : null]);
  };

  /** Creates or updates the single profile "Local broker" for a listener (never a second one). */
  const applyLocalProfile = (l: Listener, profileName: string) => {
    const anonymous = broker.config.anonymous;
    const secure = l.kind === 'tls' || l.kind === 'wss';
    let p = profiles.find((x) => x.isLocalBroker);
    if (!p) {
      p = newProfileDefaults(profileById('local') ? `p-${randHex(8)}` : 'local', 'mqttpanel-desktop');
      p.isLocalBroker = true;
      p.name = profileName || words.profiles.local;
      profiles.push(p);
    }
    const mtls = secure && broker.config.requireClientCert && !anonymous;
    if (mtls) broker.ensureDesktopCert();
    if (!anonymous) broker.ensureAppUser();
    p.host = 'localhost';
    p.port = l.port;
    p.scheme = l.kind === 'tcp' ? 'mqtt' : l.kind === 'tls' ? 'mqtts' : l.kind;
    p.target = 'app';
    p.brokerListenerId = l.id;
    p.mqttVersion = '5';
    p.username = anonymous ? '' : APP_USER;
    p.hasPassword = !anonymous;
    localPassword = anonymous ? null : (broker.passwords.get(APP_USER) ?? null);
    p.tls.skipVerify = false;
    p.tls.trustedFingerprints = [];
    p.tls.caCert = secure ? file('CA.pem', certSummary('MQTT Panel Local CA', broker.certs.ca.notAfter, { isCa: true, selfSigned: true, issuer: 'CN=MQTT Panel Local CA' })) : null;
    p.tls.clientCert = mtls ? file('mqttpanel-desktop.pem', certSummary('mqttpanel-desktop', '2028-10-12', { issuer: 'CN=MQTT Panel Local CA' })) : null;
    p.tls.clientKey = mtls ? file('mqttpanel-desktop.key', keySummary(), true) : null;
  };

  /** The profile "Local broker" follows every broker change at once (change set 2, C-16). */
  const followLocal = () => {
    const p = profiles.find((x) => x.isLocalBroker);
    if (!p) return;
    const current = broker.config.listeners.find((l) => l.id === p.brokerListenerId && l.enabled);
    const l = current ?? broker.pickListener(null);
    if (l) applyLocalProfile(l, p.name);
  };

  /** After a broker start or a change of its own settings, the open connection of "Local broker" reconnects with its
   * current settings. */
  const reconnectLocal = async () => {
    const p = profiles.find((x) => x.isLocalBroker);
    const c = p ? conns.get(p.id) : undefined;
    if (!p || !c || c.state === 'disconnected' || c.state === 'failed') return;
    c.state = 'reconnecting';
    changedTick();
    await connectNow(p.id);
  };

  // ---------------------------------------------------------------- the client

  const client: MockEngineClient = {
    async appInfo() {
      return { version: '1.0.0-beta.3', platform: detectPlatform(), native: false };
    },
    async keyStoreKind() {
      return options.keyStore ?? 'keychain';
    },
    async getSettings() {
      return clone(settings);
    },
    async saveSettings(s) {
      settings = clone(s);
      changedTick();
      return clone(settings);
    },

    // profiles
    async listProfiles() {
      return clone(profiles);
    },
    async newProfile() {
      return newProfileDefaults(`p-${randHex(8)}`, `mqttpanel-${randHex(4)}`);
    },
    async saveProfile(profile) {
      const i = profiles.findIndex((p) => p.id === profile.id);
      const copy = clone(profile);
      if (!copy.id) copy.id = `p-${randHex(8)}`;
      if (i >= 0) profiles[i] = copy;
      else profiles.push(copy);
      // a sparkline pick (profile `topics`, change set 8) shows at once on the open connection
      const c = conns.get(copy.id);
      if (c) {
        c.store.seq++;
        changedTick();
      }
      return clone(copy);
    },
    async deleteProfile(id) {
      const c = conns.get(id);
      if (c) {
        stopRepeat(c, null);
        conns.delete(id);
      }
      profiles = profiles.filter((p) => p.id !== id);
      templates.delete(id);
      changedTick();
    },
    async duplicateProfile(id) {
      const p = needProfile(id);
      const copy = clone(p);
      copy.id = `p-${randHex(8)}`;
      copy.name = p.name + words.copy;
      copy.isLocalBroker = false;
      copy.lastUsed = null;
      copy.lastTest = null;
      const i = profiles.indexOf(p);
      profiles.splice(i + 1, 0, copy);
      return clone(copy);
    },
    async reorderProfiles(ids) {
      const byId = new Map(profiles.map((p) => [p.id, p]));
      const ordered = ids.map((id) => byId.get(id)).filter((p): p is Profile => !!p);
      const rest = profiles.filter((p) => !ids.includes(p.id));
      profiles = [...ordered, ...rest];
    },
    async setPassword(id, password) {
      const p = needProfile(id);
      p.hasPassword = password !== '';
      if (p.isLocalBroker) localPassword = password || null;
      return clone(p);
    },
    async profilePassword(id) {
      const p = needProfile(id);
      return p.isLocalBroker && p.hasPassword ? localPassword : null;
    },
    async importProfiles(_path) {
      await delay(100);
      const added = importedProfiles(lang, [`p-${randHex(8)}`, `p-${randHex(8)}`]);
      profiles.push(...added);
      return {
        imported: added.map((p) => p.name),
        skipped: [{ name: words.imported.old, reason: um('import.unsupportedField', { detail: 'SOCKS proxy' }) }],
        notes: [um('import.passwordsSkipped')],
      };
    },
    async exportProfiles(_passphrase) {
      return saved('mqttpanel-profiles.json');
    },

    // files
    async pickFile(kind) {
      await delay(50);
      return inspect(kind, picked[kind]);
    },
    async inspectFile(kind, path) {
      return inspect(kind, path);
    },
    async setProfileFile(profileId, field, path, passphrase) {
      const p = needProfile(profileId);
      if (path === null) {
        p.tls[field] = null;
        return clone(p);
      }
      const ins = inspect(field, path);
      if (!ins.ok) throw ins.error;
      const isP12 = ins.summary.kind === 'pkcs12';
      if (isP12 && (passphrase === null || passphrase === 'wrong')) throw um('tls.wrongPassphrase', { file: ins.fileName });
      p.tls[field] = file(ins.fileName, ins.summary, field === 'clientKey' || isP12);
      if (isP12 && field === 'clientCert') p.tls.clientKey = file(ins.fileName, keySummary(true), true);
      return clone(p);
    },
    async addProtoSchema(profileId, path, topicFilter, messageType) {
      const p = needProfile(profileId);
      const fileName = baseName(path);
      const types = ['plant.Payload', 'plant.Telemetry', 'plant.Alarm'];
      const s = { fileName, topicFilter, messageType: messageType ?? types[0], messageTypes: types };
      const i = p.protoSchemas.findIndex((x) => x.fileName === fileName);
      if (i >= 0) p.protoSchemas[i] = s;
      else p.protoSchemas.push(s);
      return clone(p);
    },
    async removeProtoSchema(profileId, fileName) {
      const p = needProfile(profileId);
      p.protoSchemas = p.protoSchemas.filter((s) => s.fileName !== fileName);
      return clone(p);
    },

    // connections
    connectProfile: (id, skipOnce) => connectNow(id, skipOnce ?? false),
    async testProfile(id, skipOnce) {
      const p = needProfile(id);
      await delay(latency);
      const check = localDown(p) ?? trustCheck(p, skipOnce ?? false);
      if (check !== null && 'leaf' in check) {
        pendingTrust.set(id, check);
        return { kind: 'needsTrust', trust: clone(check) } as TestOutcome;
      }
      const v = negotiatedFor(p);
      const result: TestResult =
        check !== null
          ? { ok: false, at: now(), latencyMs: null, negotiatedVersion: null, maxPacketSize: null, serverCert: null, tlsVersion: null, verified: null, error: check as UserMessage }
          : {
              ok: true,
              at: now(),
              latencyMs: 84,
              negotiatedVersion: v,
              maxPacketSize: v === '5' ? 1048576 : null,
              serverCert: serverCertFor(p),
              tlsVersion: isTls(p) ? 'TLS 1.3' : null,
              verified: isTls(p) ? !(p.tls.skipVerify || skipOnce) && !p.tls.trustedFingerprints.length : null,
              error: null,
            };
      p.lastTest = result;
      return { kind: 'tested', result: clone(result) };
    },
    async trustCertificate(profileId, mode) {
      const p = needProfile(profileId);
      const prompt = pendingTrust.get(profileId);
      if (mode === 'skipOnce') {
        // Nothing is saved: the caller retries with skipOnce.
      } else if (mode === 'skipAlways') {
        p.tls.skipVerify = true;
      } else if (mode === 'ca') {
        p.tls.caCert = file('lab-private-ca.pem', certSummary('Lab Private CA', '2035-01-01', { isCa: true, selfSigned: true, issuer: 'CN=Lab Private CA' }));
      } else {
        const fp = prompt?.leaf.fingerprint ?? LAB_FINGERPRINT;
        let list = p.tls.trustedFingerprints;
        if (prompt?.previousFingerprint) list = list.filter((f) => f !== prompt.previousFingerprint);
        if (!list.includes(fp)) list = [...list, fp];
        p.tls.trustedFingerprints = list;
      }
      pendingTrust.delete(profileId);
      return clone(p);
    },
    async disconnect(id) {
      const c = conns.get(id);
      if (!c) return;
      stopRepeat(c, um('repeat.stoppedDisconnect'));
      c.state = 'disconnected';
      changedTick();
    },
    async closeConnection(id) {
      const c = conns.get(id);
      if (!c) return;
      stopRepeat(c, null);
      conns.delete(id);
      changedTick();
    },
    async listConnections() {
      return [...conns.values()].map(status);
    },

    // explorer
    async treeSlice(id, query, start, count) {
      const c = conns.get(id);
      if (!c) return { total: 0, start: 0, rows: [], seq: 0 };
      return c.store.treeSlice(query, start, count);
    },
    async tableSlice(id, query, start, count) {
      const c = conns.get(id);
      if (!c) return { total: 0, topicsInView: 0, start: 0, rows: [], seq: 0 };
      return c.store.tableSlice(query, start, count);
    },
    async setExpanded(id, path, expanded) {
      needConn(id).store.setExpanded(path, expanded);
    },
    async expandToDepth(id, depth) {
      needConn(id).store.expandToDepth(depth);
    },
    async expandChanged(id, withinMs) {
      needConn(id).store.expandChanged(withinMs);
    },
    async collapseAll(id) {
      needConn(id).store.expanded.clear();
    },
    async getExpanded(id) {
      return [...needConn(id).store.expanded];
    },
    async setExpandedSet(id, paths) {
      needConn(id).store.expanded = new Set(paths);
    },
    async setPinned(id, paths) {
      needConn(id).store.pinned = new Set(paths);
    },
    async revealTopic(id, topic) {
      return needConn(id).store.revealTopic(topic);
    },
    async nextChanged(id, fromTopic, withinMs) {
      return needConn(id).store.nextChanged(fromTopic, withinMs);
    },
    async knownTopics(id, prefix, limit) {
      const c = conns.get(id);
      return c ? c.store.knownTopics(prefix, limit) : [];
    },

    // inspection
    async topicDetail(id, topic) {
      return needConn(id).store.topicDetail(topic);
    },
    async message(id, topic, seq, format, formatAnyway) {
      const c = needConn(id);
      const m = c.store.findMessage(topic, seq);
      return m ? c.store.messageView(m, format, formatAnyway) : null;
    },
    async history(id, topic) {
      return needConn(id).store.history(topic);
    },
    async diff(id, topic, baselineSeq, seq) {
      return needConn(id).store.diff(topic, baselineSeq, seq);
    },
    async setTopicBuffer(id, topic, size) {
      needConn(id).store.setTopicBuffer(topic, size);
    },
    async clearHistory(id, prefix) {
      needConn(id).store.clearHistory(prefix);
      changedTick();
    },
    async retainedUnder(id, prefix) {
      return needConn(id).store.retainedUnder(prefix);
    },
    async memory() {
      let bytes = 0;
      let points = 0;
      let messages = 0;
      for (const c of conns.values()) {
        const m = c.store.memory();
        bytes += m.bytes;
        points += m.points;
        messages += m.messages;
      }
      return { bytes, budgetBytes: settings.memoryBudgetMb * 1024 * 1024, points, messages };
    },

    // series
    async series(id, topic, path, sinceMs) {
      const c = conns.get(id);
      if (!c) return { topic, path, points: [], gaps: [], skipped: 0, firstSkipReason: null, castRule: 'none', step: false, precisionLoss: false, count: 0 };
      return c.store.series(topic, path, sinceMs);
    },

    // export
    async exportEstimate(id, o) {
      return needConn(id).store.exportEstimate(o);
    },
    async exportData(id, o) {
      needConn(id);
      return saved(`mqttpanel-export.${o.format}`);
    },
    async exportChartCsv(_id, _series, _sinceMs) {
      return saved('chart.csv');
    },
    async savePng(defaultName, b64) {
      return saved(defaultName, Math.floor((b64.length * 3) / 4));
    },
    async exportSentLog(id) {
      needConn(id);
      return saved('sent-log.csv');
    },

    // publish
    async publish(id, req) {
      const c = needConnected(id);
      const e = doPublish(c, req);
      changedTick();
      return clone(e);
    },
    async deleteRetained(id, topic) {
      const c = needConnected(id);
      const entry = newEntry({ topic, payload: '', format: 'text', qos: 1, retain: true, props: null, request: null }, 0, c);
      entry.status = 'deletedRetained';
      c.store.clearRetained(topic);
      addSent(c, entry);
      changedTick();
      return clone(entry);
    },
    async deleteRetainedMany(id, topics) {
      const c = needConnected(id);
      let n = 0;
      for (const t of topics) {
        if (c.store.clearRetained(t)) {
          n++;
          const entry = newEntry({ topic: t, payload: '', format: 'text', qos: 1, retain: true, props: null, request: null }, 0, c);
          entry.status = 'deletedRetained';
          addSent(c, entry);
        }
      }
      changedTick();
      return n;
    },
    async sentLog(id) {
      const c = conns.get(id);
      return c ? clone(c.sent) : [];
    },
    async clearSentLog(id) {
      const c = conns.get(id);
      if (c) c.sent = [];
    },
    async startRepeat(id, req, intervalS, count) {
      const c = needConnected(id);
      stopRepeat(c, null);
      const enc = encodePayloadBytes(req.format, req.payload);
      if (!enc.ok) throw enc.error;
      const entry = newEntry(req, enc.bytes.length, c);
      entry.status = 'repeat';
      entry.repeatCount = 0;
      addSent(c, entry);
      const rs: RepeatStatus = { connectionId: id, running: true, intervalS, sent: 0, count, nextAt: null, topic: req.topic, stoppedReason: null };
      const rec: RepeatRec = { status: rs, timer: null, entry, dropAfterTick: false };
      c.repeat = rec;
      const fire = () => {
        if (disposed || !rs.running) return;
        if (c.state !== 'connected') {
          stopRepeat(c, um('repeat.stoppedDisconnect'));
          return;
        }
        const payloadText = req.payload.replace(/\{\{rand (\d+)-(\d+)\}\}/g, (_m, a: string, b: string) => String(Number(a) + Math.floor(Math.random() * (Number(b) - Number(a) + 1))));
        const bytes = payloadText === req.payload ? enc.bytes : utf8(payloadText);
        ingestIfSubscribed(c, req.topic, bytes, req.qos, req.retain, publishProps(c, req, null));
        rs.sent++;
        entry.repeatCount = rs.sent;
        entry.at = now();
        rs.nextAt = now() + intervalS * 1000;
        pushUpdate(c, entry);
        if (count !== null && rs.sent >= count) stopRepeat(c, null);
      };
      fire();
      if (rs.running) rec.timer = setInterval(fire, Math.max(0.1, intervalS) * 1000);
      if (rec.timer) timers.push(rec.timer);
      changedTick();
    },
    async stopRepeat(id) {
      const c = conns.get(id);
      if (c) stopRepeat(c, null);
      changedTick();
    },
    async listTemplates(profileId) {
      return clone(templates.get(profileId) ?? []);
    },
    async saveTemplate(profileId, t) {
      const list = templates.get(profileId) ?? [];
      const copy = clone(t);
      if (!copy.id) copy.id = `t-${++templateN}`;
      const i = list.findIndex((x) => x.id === copy.id);
      if (i >= 0) list[i] = copy;
      else list.push(copy);
      templates.set(profileId, list);
      return clone(copy);
    },
    async deleteTemplate(profileId, templateId) {
      templates.set(
        profileId,
        (templates.get(profileId) ?? []).filter((t) => t.id !== templateId),
      );
    },
    async encodePayload(format, payload) {
      const r = encodePayloadBytes(format, payload);
      return r.ok ? { ok: true as const, size: r.bytes.length } : { ok: false as const, error: r.error };
    },

    // broker
    async brokerStatus() {
      return broker.status();
    },
    async brokerConfig() {
      return broker.getConfig();
    },
    async brokerStart() {
      if (broker.isRunning()) {
        // Already running: listeners that could not start are tried again.
        broker.retryFailed();
        changedTick();
        return broker.status();
      }
      broker.state = 'starting';
      changedTick();
      await delay(latency);
      broker.start();
      changedTick();
      if (broker.isRunning()) await reconnectLocal();
      return broker.status();
    },
    async brokerStop() {
      broker.stop();
      const p = profiles.find((x) => x.isLocalBroker);
      const c = p ? conns.get(p.id) : undefined;
      if (c && c.state === 'connected') c.state = 'reconnecting';
      changedTick();
      return broker.status();
    },
    async validatePort(port, listenerId) {
      return broker.validatePort(port, listenerId);
    },
    async addListener(req) {
      const err = broker.validatePort(req.port, null);
      if (err) throw err;
      return brokerChange(() => broker.addListener(req));
    },
    async updateListener(l: Listener) {
      // Like the engine: the port is checked only when it changes (switching on a listener whose port another
      // program took fails that listener alone, change set 7).
      const old = broker.config.listeners.find((x) => x.id === l.id);
      if (!old || old.port !== l.port) {
        const err = broker.validatePort(l.port, l.id);
        if (err) throw err;
      }
      return brokerChange(() => broker.updateListener(l));
    },
    async removeListener(id) {
      return brokerChange(() => broker.removeListener(id));
    },
    async setBrokerBind(bind) {
      return brokerChange(() => broker.setBind(bind));
    },
    async setAnonymous(on) {
      return brokerChange(() => broker.setAnonymous(on));
    },
    async setRequireClientCert(on) {
      return brokerChange(() => broker.setRequireClientCert(on));
    },
    async setVerifyAgainst(source) {
      return brokerChange(() => broker.setVerifyAgainst(source));
    },
    async addUser(username, password) {
      const err = broker.addUser(username, password);
      if (err) throw err;
      changedTick();
      return clone(broker.config);
    },
    async setUserPassword(username, password) {
      return brokerChange(() => broker.setPassword(username, password));
    },
    async brokerUserPassword(username) {
      return broker.passwords.get(username) ?? null;
    },
    async removeUser(username) {
      broker.removeUser(username);
      changedTick();
      return clone(broker.config);
    },
    async issuedCerts() {
      return clone(broker.issued);
    },
    async issueClientCert(req) {
      const r = broker.issue(req);
      if ('key' in r) throw r;
      await delay(200);
      changedTick();
      return r;
    },
    async importCsr(name, validYears) {
      await delay(200);
      const r = broker.importCsr(name, validYears);
      changedTick();
      return r;
    },
    async exportClientCert(name, format, _passphrase) {
      return saved(name + (format === 'pem' ? '.pem' : '.p12'));
    },
    async revokeClientCert(name) {
      broker.revoke(name);
      changedTick();
      return clone(broker.issued);
    },
    async brokerCerts() {
      return clone(broker.certs);
    },
    async exportCa() {
      return saved('CA.pem');
    },
    async regenerateBrokerCerts(sans, keyType) {
      await brokerChange(() => broker.regenerate(sans, keyType));
      return clone(broker.certs);
    },
    async importBrokerCert() {
      await delay(100);
      broker.importServer();
      changedTick();
      return clone(broker.certs);
    },
    async importClientCa() {
      await delay(100);
      broker.importClientCa();
      return clone(broker.certs);
    },
    async rotateCa() {
      await brokerChange(() => broker.rotateCa());
      return clone(broker.certs);
    },
    async brokerClients() {
      return broker.state === 'running' ? clone(broker.clients) : [];
    },
    async disconnectClient(clientId) {
      broker.clients = broker.clients.filter((c) => c.clientId !== clientId);
      broker.addLog('info', `client ${clientId} disconnected by the administrator`);
      changedTick();
    },
    async brokerLog(level, afterSeq) {
      return broker.logAfter(level, afterSeq);
    },
    async appBrokerStep(listenerId, step, profileName) {
      const l = broker.pickListener(listenerId);
      if (!l) throw um('broker.noListener');
      await delay(Math.min(latency, 250));
      const res = (result: AppBrokerStepResult['result'], extra: Partial<AppBrokerStepResult> = {}): AppBrokerStepResult => ({ step, result, password: null, outcome: null, ...extra });
      const anonymous = broker.config.anonymous;
      const secure = l.kind === 'tls' || l.kind === 'wss';
      switch (step) {
        case 'user': {
          if (anonymous) return res('skipped');
          const { created, password } = broker.ensureAppUser();
          changedTick();
          return res(created ? 'created' : 'kept', { password });
        }
        case 'cert': {
          if (anonymous || !secure || !broker.config.requireClientCert) return res('skipped');
          const issued = broker.ensureDesktopCert();
          changedTick();
          return res(issued ? 'issued' : 'kept');
        }
        case 'trust': {
          applyLocalProfile(l, profileName);
          changedTick();
          return res(secure ? 'trusted' : 'skipped');
        }
        case 'broker': {
          // Starts the broker only when it is stopped; nothing restarts (change set 7).
          const action: AppBrokerStepResult['result'] = broker.state === 'running' ? 'running' : 'started';
          if (action === 'started') {
            broker.state = 'starting';
            changedTick();
            await delay(latency);
            broker.start();
            changedTick();
            if (!broker.isRunning() && broker.error) throw clone(broker.error);
          } else broker.retryFailed();
          if (!l.enabled) throw um('broker.noListener');
          const failed = broker.status().listeners.find((x) => x.id === l.id)?.error;
          if (failed) throw failed;
          return res(action);
        }
        case 'connect': {
          const p = profiles.find((x) => x.isLocalBroker);
          if (!p) throw um('broker.noListener');
          const existing = conns.get(p.id);
          if (existing && existing.state === 'connected') existing.state = 'disconnected';
          const outcome = await connectNow(p.id);
          return res(outcome.kind === 'connected' ? 'connected' : 'failed', { outcome });
        }
      }
      throw um('broker.noListener');
    },

    onEvent(handler) {
      handlers.add(handler);
      return () => {
        handlers.delete(handler);
      };
    },

    // controls
    inject(connectionId, topic, payload, opts = {}) {
      const c = conns.get(connectionId);
      if (!c) return;
      c.store.ingest(topic, utf8(payload), opts.qos ?? 0, opts.retain ?? false, opts.props !== undefined ? opts.props : propsFor(c), opts.at ?? now());
      tick();
    },
    savedFiles() {
      return saves.slice();
    },
    emitMenu(id) {
      emit({ type: 'menu', id });
    },
    setConnectionState(connectionId, state) {
      const c = conns.get(connectionId);
      if (c) c.state = state;
      tick();
    },
    failBrokerStart(on) {
      broker.failStart = on;
    },
    takePort(port, taken = true) {
      if (taken) broker.takenPorts.add(port);
      else broker.takenPorts.delete(port);
    },
    tick,
    dispose() {
      disposed = true;
      timers.forEach((t) => clearInterval(t));
      timeouts.forEach((t) => clearTimeout(t));
      timeouts.clear();
      for (const c of conns.values()) if (c.repeat?.timer) clearInterval(c.repeat.timer);
      handlers.clear();
    },
  };
  void previewOf;
  return client;
}

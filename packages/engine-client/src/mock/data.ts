// Demo data for the mock engine, localised per language (screenshot data per language).
import type { CertSummary, FileField, Filters, Mqtt5Props, Profile, Qos, SavedView, Template, TestResult } from '../types';

export type Lang = 'en' | 'fr' | 'de';

export interface Words {
  profiles: { plant: string; local: string; lab: string; emqx: string };
  copy: string;
  imported: { home: string; cloud: string; old: string };
  folders: { commissioning: string; simulation: string };
  templates: { setpoint: string; valve: string; ahu: string; fake: string; nbirth: string };
  cooling: string;
  heating: string;
  running: string;
  open: string;
  closed: string;
  operator: string;
  defaultView: string;
  commissioningView: string;
}

export const WORDS: Record<Lang, Words> = {
  en: {
    profiles: { plant: 'Plant broker', local: 'Local broker', lab: 'Lab Mosquitto', emqx: 'Cloud EMQX (WS)' },
    copy: ' (copy)',
    imported: { home: 'Home lab', cloud: 'Test cloud', old: 'Old gateway' },
    folders: { commissioning: 'Commissioning', simulation: 'Simulation' },
    templates: { setpoint: 'Set room setpoint', valve: 'Force valve', ahu: 'AHU restart', fake: 'Fake temperature', nbirth: 'Sparkplug NBIRTH' },
    cooling: 'cooling',
    heating: 'heating',
    running: 'running',
    open: 'open',
    closed: 'closed',
    operator: 'commissioning',
    defaultView: 'Default',
    commissioningView: 'Commissioning f2',
  },
  fr: {
    profiles: { plant: 'Broker usine', local: 'Broker local', lab: 'Mosquitto labo', emqx: 'EMQX cloud (WS)' },
    copy: ' (copie)',
    imported: { home: 'Labo maison', cloud: 'Cloud de test', old: 'Ancienne passerelle' },
    folders: { commissioning: 'Mise en service', simulation: 'Simulation' },
    templates: { setpoint: 'Consigne de pièce', valve: 'Forcer la vanne', ahu: 'Redémarrer la CTA', fake: 'Température simulée', nbirth: 'NBIRTH Sparkplug' },
    cooling: 'refroidissement',
    heating: 'chauffage',
    running: 'en marche',
    open: 'ouverte',
    closed: 'fermée',
    operator: 'mise-en-service',
    defaultView: 'Default',
    commissioningView: 'Mise en service f2',
  },
  de: {
    profiles: { plant: 'Werks-Broker', local: 'Lokaler Broker', lab: 'Labor-Mosquitto', emqx: 'Cloud-EMQX (WS)' },
    copy: ' (Kopie)',
    imported: { home: 'Heimlabor', cloud: 'Test-Cloud', old: 'Altes Gateway' },
    folders: { commissioning: 'Inbetriebnahme', simulation: 'Simulation' },
    templates: { setpoint: 'Raum-Sollwert setzen', valve: 'Ventil übersteuern', ahu: 'Lüftungsgerät neu starten', fake: 'Simulierte Temperatur', nbirth: 'Sparkplug NBIRTH' },
    cooling: 'Kühlen',
    heating: 'Heizen',
    running: 'läuft',
    open: 'offen',
    closed: 'geschlossen',
    operator: 'inbetriebnahme',
    defaultView: 'Default',
    commissioningView: 'Inbetriebnahme f2',
  },
};

export const DEFAULT_COLUMNS: SavedView['columns'] = ['pin', 'topic', 'value', 'spark', 'updated', 'rate', 'messages', 'qos', 'retained', 'size'];

export function emptyFilters(): Filters {
  return { retainedOnly: false, changedWithinMin: null, payloadContains: '', qos: null, hasValue: false };
}

export function defaultView(pinned: string[] = []): SavedView {
  return {
    name: 'Default',
    layout: 'tree',
    search: '',
    filters: emptyFilters(),
    expanded: [],
    pinned,
    columns: [...DEFAULT_COLUMNS],
    sort: { column: 'topic', dir: 'asc' },
  };
}

export function certSummary(cn: string, notAfter: string, opts: Partial<CertSummary> = {}): CertSummary {
  return {
    kind: 'certificate',
    subject: `CN=${cn}`,
    cn,
    issuer: opts.isCa ? `CN=${cn}` : 'CN=Plant Root CA',
    notBefore: '2026-09-01',
    notAfter,
    keyType: 'ECDSA P-256',
    count: 1,
    selfSigned: !!opts.isCa,
    isCa: false,
    encrypted: false,
    ...opts,
  };
}

export function keySummary(encrypted = false): CertSummary {
  return {
    kind: 'key',
    subject: null,
    cn: null,
    issuer: null,
    notBefore: null,
    notAfter: null,
    keyType: 'ECDSA P-256',
    count: 0,
    selfSigned: false,
    isCa: false,
    encrypted,
  };
}

export function file(fileName: string, summary: CertSummary | null, inKeychain = false): FileField {
  return { fileName, summary, inKeychain };
}

export const PLANT_FINGERPRINT = '7A:31:C4:0E:91:5B:22:F8:6D:A0:3C:E7:19:84:5F:B2:0D:6E:A9:41:C3:58:7B:E2:90:1F:4A:D6:2C:83:B5:17';
export const LAB_FINGERPRINT = '3F:9A:12:7C:E0:44:B1:9D:5A:0E:88:C2:17:6B:F3:2A:9C:41:D8:70:1E:55:A2:3B:8F:64:CC:09:D1:7E:B6:C1';

function blankProfile(): Profile {
  return {
    id: '',
    name: '',
    target: 'other',
    brokerListenerId: null,
    host: 'localhost',
    port: 1883,
    scheme: 'mqtt',
    wsPath: '/mqtt',
    mqttVersion: 'auto',
    clientId: 'mqttpanel-0000',
    username: '',
    hasPassword: false,
    tls: { skipVerify: false, caCert: null, clientCert: null, clientKey: null, trustedFingerprints: [], sni: '', alpn: '' },
    subscriptions: [{ filter: '#', qos: 0 }],
    keepAlive: 60,
    cleanStart: true,
    sessionExpiry: 0,
    lastWill: null,
    reconnect: { enabled: true, minDelayS: 1, maxDelayS: 30 },
    protoSchemas: [],
    views: [defaultView()],
    topics: {},
    lastUsed: null,
    lastTest: null,
    isLocalBroker: false,
  };
}

export function newProfileDefaults(id: string, clientId: string): Profile {
  return { ...blankProfile(), id, clientId };
}

export function demoProfiles(lang: Lang, now: number): Profile[] {
  const w = WORDS[lang];
  const pins = ['building/f2/r204/temperature', 'building/f2/r204/setpoint', 'building/f2/ahu01/status'];
  const lastTest: TestResult = {
    ok: true,
    at: now - 2 * 60_000,
    latencyMs: 84,
    negotiatedVersion: '5',
    maxPacketSize: 1048576,
    serverCert: {
      subject: 'CN=broker.plant.example',
      issuer: 'CN=Plant Root CA',
      san: ['broker.plant.example'],
      notBefore: '2025-06-30',
      notAfter: '2027-06-30',
      fingerprint: PLANT_FINGERPRINT,
      keyType: 'ECDSA P-256',
      selfSigned: false,
      isCa: false,
    },
    tlsVersion: 'TLS 1.3',
    verified: true,
    error: null,
  };
  const plant: Profile = {
    ...blankProfile(),
    id: 'plant',
    name: w.profiles.plant,
    host: 'broker.plant.example',
    port: 8883,
    scheme: 'mqtts',
    mqttVersion: '5',
    clientId: 'mqttpanel-7f3a',
    username: 'commissioning',
    hasPassword: true,
    tls: {
      skipVerify: false,
      caCert: file('plant-root-ca.pem', certSummary('Plant Root CA', '2031-03-01', { isCa: true })),
      clientCert: file('commissioning-laptop.pem', certSummary('commissioning-laptop', '2027-10-12')),
      clientKey: file('commissioning-laptop.key', keySummary(), true),
      trustedFingerprints: [],
      sni: '',
      alpn: 'mqtt',
    },
    subscriptions: [
      { filter: 'building/#', qos: 1 },
      { filter: '$SYS/broker/clients/#', qos: 0 },
      { filter: 'spBv1.0/#', qos: 0 },
    ],
    protoSchemas: [{ fileName: 'plant.proto', topicFilter: 'spBv1.0/#', messageType: 'plant.Payload', messageTypes: ['plant.Payload', 'plant.Telemetry', 'plant.Alarm'] }],
    views: [
      defaultView(pins),
      { ...defaultView(pins), name: w.commissioningView, layout: 'table', search: 'building/f2/#' },
    ],
    // Change set 8: values picked with "Use as sparkline" and the topic charts (mockups 04 and 06).
    topics: {
      'building/f2/r204/temperature': { sparklinePath: '$.value' },
      'building/f2/r204/setpoint': { sparklinePath: '$.value', chartFields: [{ path: '$.value', colour: 'sage', style: 'step', axis: 'left', unit: '°C' }] },
      'building/f2/ahu01/state': {
        sparklinePath: '$.supply.temp',
        chartFields: [
          { path: '$.supply.temp', colour: 'sage', style: 'line', axis: 'left', unit: '°C' },
          { path: '$.supply.fan', colour: 'clay', style: 'line', axis: 'right', unit: '%' },
        ],
      },
    },
    lastUsed: now - 60 * 60_000,
    lastTest,
  };
  const local: Profile = {
    ...blankProfile(),
    id: 'local',
    name: w.profiles.local,
    target: 'app',
    brokerListenerId: 'l-tls',
    host: 'localhost',
    port: 8883,
    scheme: 'mqtts',
    mqttVersion: '5',
    clientId: 'mqttpanel-desktop',
    username: 'mqtt-panel',
    hasPassword: true,
    tls: {
      skipVerify: false,
      caCert: file('CA.pem', certSummary('MQTT Panel Local CA', '2031-10-12', { isCa: true })),
      clientCert: file('mqttpanel-desktop.pem', certSummary('mqttpanel-desktop', '2028-10-12', { issuer: 'CN=MQTT Panel Local CA' })),
      clientKey: file('mqttpanel-desktop.key', keySummary(), true),
      trustedFingerprints: [],
      sni: '',
      alpn: '',
    },
    lastUsed: now - 3 * 60 * 60_000,
    isLocalBroker: true,
  };
  const lab: Profile = {
    ...blankProfile(),
    id: 'lab',
    name: w.profiles.lab,
    host: '10.20.4.17',
    port: 1883,
    scheme: 'mqtt',
    mqttVersion: '3.1.1',
    clientId: 'mqttpanel-lab1',
    subscriptions: [{ filter: 'building/#', qos: 0 }],
    lastUsed: now - 3 * 24 * 60 * 60_000,
  };
  const emqx: Profile = {
    ...blankProfile(),
    id: 'emqx',
    name: w.profiles.emqx,
    host: 'demo.emqx.example',
    port: 8084,
    scheme: 'wss',
    wsPath: '/mqtt',
    mqttVersion: '5',
    clientId: 'mqttpanel-c10d',
    tls: { ...blankProfile().tls, skipVerify: true },
    subscriptions: [
      { filter: 'demo/#', qos: 0 },
      { filter: 'building/f1/#', qos: 0 },
    ],
    lastUsed: now - 8 * 24 * 60 * 60_000,
  };
  return [plant, local, lab, emqx];
}

export function importedProfiles(lang: Lang, ids: [string, string]): Profile[] {
  const w = WORDS[lang];
  return [
    { ...blankProfile(), id: ids[0], name: w.imported.home, host: '192.168.1.20', port: 1883, scheme: 'mqtt', clientId: 'mqttpanel-home' },
    { ...blankProfile(), id: ids[1], name: w.imported.cloud, host: 'test.cloud.example', port: 443, scheme: 'wss', wsPath: '/mqtt', clientId: 'mqttpanel-tcld' },
  ];
}

export function demoTemplates(lang: Lang): Template[] {
  const w = WORDS[lang];
  const base = { format: 'json' as const, qos: 1 as Qos, retain: false, props: null, request: null };
  return [
    { id: 't-1', name: w.templates.setpoint, folder: w.folders.commissioning, repeatS: null, request: { ...base, topic: 'building/f2/{room}/setpoint', payload: '{"value":21.5}', retain: true } },
    { id: 't-2', name: w.templates.valve, folder: w.folders.commissioning, repeatS: null, request: { ...base, topic: 'building/f2/{room}/valve/cmd', payload: '{"position":100}' } },
    {
      id: 't-3',
      name: w.templates.ahu,
      folder: w.folders.commissioning,
      repeatS: null,
      request: {
        ...base,
        topic: 'building/f2/ahu01/cmd',
        payload: '{"cmd":"restart","delay":5}',
        qos: 2,
        request: { responseTopic: 'building/f2/ahu01/cmd/ack', correlationData: '', waitS: 10 },
      },
    },
    { id: 't-4', name: w.templates.fake, folder: w.folders.simulation, repeatS: 2, request: { ...base, topic: 'sim/room/temperature', payload: '{"value": 21.5}', qos: 0 } },
    {
      id: 't-5',
      name: w.templates.nbirth,
      folder: w.folders.simulation,
      repeatS: null,
      request: { ...base, topic: 'spBv1.0/plant/NBIRTH/edge01', payload: 'CAEQABgAIg5iZFNlcQ==', format: 'base64', qos: 0 },
    },
  ];
}

export function emptyProps(): Mqtt5Props {
  return { contentType: null, payloadFormat: null, responseTopic: null, correlationData: null, messageExpiry: null, userProperties: [], subscriptionIds: [], topicAlias: null };
}

// ------------------------------------------------------------------ topic dataset

export interface GenOut {
  payload: string | Uint8Array;
  qos: Qos;
  retain: boolean;
  props?: Partial<Mqtt5Props>;
}

export interface TopicDef {
  topic: string;
  /** average seconds between messages; Infinity = published once */
  period: number;
  gen: (st: GenState, now: number) => GenOut;
}

export interface GenState {
  values: Map<string, number>;
  seq: number;
}

function drift(st: GenState, key: string, start: number, min: number, max: number, step: number, decimals = 1): number {
  const prev = st.values.get(key) ?? start;
  let v = prev + (Math.random() - 0.5) * 2 * step;
  if (v < min) v = min + (min - v);
  if (v > max) v = max - (v - max);
  const f = Math.pow(10, decimals);
  v = Math.round(v * f) / f;
  st.values.set(key, v);
  return v;
}

const TINY_PNG = 'iVBORw0KGgoAAAANSUhEUgAAABAAAAAMCAIAAADkharWAAAAGUlEQVR4nGOomLaKJMQwIjXEVeWRhEakBgBuCh9Q+PdwmgAAAABJRU5ErkJggg==';

export function buildDataset(lang: Lang): TopicDef[] {
  const w = WORDS[lang];
  const defs: TopicDef[] = [];
  const fast = (room: string) => room === 'r204';
  for (const f of [1, 2, 3]) {
    for (let r = 201; r <= 212; r++) {
      const room = `r${r}`;
      const p = `building/f${f}/${room}`;
      const base = 20.5 + ((f * 7 + r) % 30) / 10;
      defs.push({
        topic: `${p}/temperature`,
        period: fast(room) ? 2 : 10,
        gen: (st) => ({ payload: JSON.stringify({ value: drift(st, `${p}/t`, base + 1, 20.5, 24, 0.15), unit: '°C' }), qos: 1, retain: true }),
      });
      defs.push({ topic: `${p}/setpoint`, period: 600, gen: () => ({ payload: JSON.stringify({ value: Math.random() < 0.5 ? 21.5 : 22.0 }), qos: 1, retain: true }) });
      defs.push({ topic: `${p}/humidity`, period: fast(room) ? 2 : 15, gen: (st) => ({ payload: String(drift(st, `${p}/h`, 42, 35, 55, 0.4)), qos: 0, retain: false }) });
      defs.push({ topic: `${p}/co2`, period: fast(room) ? 2 : 15, gen: (st) => ({ payload: String(drift(st, `${p}/c`, 600, 420, 900, 12, 0)), qos: 0, retain: false }) });
      defs.push({ topic: `${p}/occupancy`, period: 400, gen: () => ({ payload: Math.random() < 0.6 ? 'true' : 'false', qos: 1, retain: true }) });
      defs.push({ topic: `${p}/valve/position`, period: 30, gen: (st) => ({ payload: String(drift(st, `${p}/v`, 38, 0, 100, 3, 0)), qos: 0, retain: false }) });
      defs.push({ topic: `${p}/valve/cmd`, period: 900, gen: () => ({ payload: JSON.stringify({ position: 40 }), qos: 1, retain: false }) });
      defs.push({ topic: `${p}/valve/state`, period: 500, gen: () => ({ payload: Math.random() < 0.7 ? w.open : w.closed, qos: 1, retain: true }) });
      defs.push({ topic: `${p}/lights/level`, period: 120, gen: (st) => ({ payload: String(drift(st, `${p}/l`, 70, 0, 100, 5, 0)), qos: 0, retain: false }) });
      defs.push({ topic: `${p}/lights/state`, period: 500, gen: () => ({ payload: Math.random() < 0.7 ? 'on' : 'off', qos: 1, retain: true }) });
      defs.push({ topic: `${p}/blinds/position`, period: 300, gen: (st) => ({ payload: String(drift(st, `${p}/b`, 40, 0, 100, 10, 0)), qos: 0, retain: false }) });
      defs.push({ topic: `${p}/window/open`, period: 900, gen: () => ({ payload: Math.random() < 0.85 ? 'false' : 'true', qos: 1, retain: true }) });
    }
  }
  const a = 'building/f2/ahu01';
  defs.push({
    topic: `${a}/state`,
    period: 3,
    gen: (st, now) => {
      const temp = drift(st, `${a}/st`, 18.2, 16, 21, 0.3);
      const fan = drift(st, `${a}/sf`, 72, 55, 90, 2, 0);
      const mode = (st.values.get(`${a}/mode`) ?? 0) > 0.5 ? w.heating : w.cooling;
      if (Math.random() < 0.05) st.values.set(`${a}/mode`, Math.random());
      return {
        payload: JSON.stringify({ mode, setpoint: 21.0, supply: { temp, fan }, alarm: { active: false }, ts: new Date(now).toISOString().slice(0, 19) + 'Z' }),
        qos: 1,
        retain: true,
      };
    },
  });
  defs.push({ topic: `${a}/status`, period: 900, gen: () => ({ payload: w.running, qos: 1, retain: true }) });
  defs.push({ topic: `${a}/supply_temp`, period: 1, gen: (st) => ({ payload: String(drift(st, `${a}/s`, 18.2, 16, 21, 0.1)), qos: 0, retain: false }) });
  defs.push({ topic: `${a}/return_temp`, period: 2, gen: (st) => ({ payload: String(drift(st, `${a}/r`, 22.9, 21, 25, 0.1)), qos: 0, retain: false }) });
  defs.push({ topic: `${a}/fan/speed`, period: 2, gen: (st) => ({ payload: String(drift(st, `${a}/fs`, 72, 55, 90, 1, 0)), qos: 0, retain: false }) });
  defs.push({ topic: `${a}/fan/state`, period: 900, gen: () => ({ payload: 'on', qos: 1, retain: true }) });
  defs.push({ topic: `${a}/fan/current`, period: 3, gen: (st) => ({ payload: String(drift(st, `${a}/fc`, 3.2, 2.5, 4.5, 0.05, 2)), qos: 0, retain: false }) });
  defs.push({ topic: `${a}/fan/hours`, period: 600, gen: (st) => ({ payload: String((st.values.get(`${a}/fh`) ?? 12840) + 1), qos: 0, retain: true }) });
  defs.push({
    topic: `${a}/cmd`,
    period: 900,
    gen: () => ({
      payload: '{"cmd":"restart","delay":5}',
      qos: 2,
      retain: false,
      props: {
        contentType: 'application/json',
        payloadFormat: 1,
        responseTopic: `${a}/cmd/ack`,
        correlationData: 'c-1041',
        messageExpiry: 30,
        userProperties: [
          ['operator', w.operator],
          ['source', 'mqttpanel-desktop'],
        ],
      },
    }),
  });
  defs.push({ topic: `${a}/cmd/ack`, period: 900, gen: () => ({ payload: '{"ok":true,"id":"c-1041"}', qos: 1, retain: false, props: { correlationData: 'c-1041' } }) });
  for (const al of ['filter', 'frost', 'fire', 'smoke', 'pressure', 'motor']) {
    defs.push({ topic: `${a}/alarms/${al}`, period: 1200, gen: () => ({ payload: 'false', qos: 1, retain: true }) });
  }
  defs.push({ topic: `${a}/camera/snapshot`, period: 600, gen: () => ({ payload: Uint8Array.from(atob(TINY_PNG), (c) => c.charCodeAt(0)), qos: 0, retain: true, props: { contentType: 'image/png' } }) });
  defs.push({ topic: `${a}/log/dump`, period: Infinity, gen: (_st, now) => ({ payload: bigLog(now), qos: 1, retain: true }) });
  const sp = 'spBv1.0/plant';
  const spGen = (metrics: (st: GenState) => { name: string; value: number | boolean | string }[]) => (st: GenState, now: number): GenOut => {
    st.seq = (st.seq + 1) % 256;
    return { payload: JSON.stringify({ timestamp: now, seq: st.seq, metrics: metrics(st) }), qos: 0, retain: false };
  };
  defs.push({ topic: `${sp}/NBIRTH/edge01`, period: 900, gen: spGen(() => [{ name: 'bdSeq', value: 0 }, { name: 'Node Control/Rebirth', value: false }]) });
  defs.push({ topic: `${sp}/NDATA/edge01`, period: 2, gen: spGen((st) => [{ name: 'cpu', value: drift(st, 'sp/cpu', 23, 5, 80, 3) }]) });
  defs.push({ topic: `${sp}/DDATA/edge01/meter1`, period: 3, gen: spGen((st) => [{ name: 'power_kw', value: drift(st, 'sp/m1', 12.4, 8, 20, 0.3) }, { name: 'energy_kwh', value: Math.round((st.values.get('sp/e1') ?? 18233) + 1) }]) });
  defs.push({ topic: `${sp}/DDATA/edge01/meter2`, period: 3, gen: spGen((st) => [{ name: 'power_kw', value: drift(st, 'sp/m2', 7.9, 4, 14, 0.3) }]) });
  const sys = '$SYS/broker';
  defs.push({ topic: `${sys}/version`, period: 1800, gen: () => ({ payload: 'mosquitto version 2.0.18', qos: 0, retain: true }) });
  defs.push({ topic: `${sys}/uptime`, period: 10, gen: (_s, now) => ({ payload: `${Math.floor(now / 1000) % 1000000} seconds`, qos: 0, retain: true }) });
  defs.push({ topic: `${sys}/clients/connected`, period: 10, gen: (st) => ({ payload: String(drift(st, 'sys/cc', 412, 380, 440, 2, 0)), qos: 0, retain: true }) });
  defs.push({ topic: `${sys}/clients/total`, period: 10, gen: (st) => ({ payload: String(drift(st, 'sys/ct', 498, 480, 520, 1, 0)), qos: 0, retain: true }) });
  defs.push({ topic: `${sys}/messages/received`, period: 10, gen: (st) => ({ payload: String(Math.round((st.values.get('sys/mr') ?? 9_812_000) + 3000)), qos: 0, retain: true }) });
  defs.push({ topic: `${sys}/messages/sent`, period: 10, gen: (st) => ({ payload: String(Math.round((st.values.get('sys/ms') ?? 24_105_000) + 7000)), qos: 0, retain: true }) });
  defs.push({ topic: `${sys}/load/bytes/received`, period: 10, gen: (st) => ({ payload: String(drift(st, 'sys/lb', 52000, 40000, 70000, 800, 0)), qos: 0, retain: true }) });
  for (const s of ['a', 'b', 'c']) {
    defs.push({ topic: `demo/sensors/${s}/value`, period: 4, gen: (st) => ({ payload: String(drift(st, `demo/${s}`, 50, 0, 100, 2)), qos: 0, retain: false }) });
  }
  return defs;
}

function bigLog(now: number): string {
  const entries: { t: string; level: string; msg: string; n: number }[] = [];
  let size = 0;
  let i = 0;
  while (size < 300 * 1024) {
    const e = { t: new Date(now - (5000 - i) * 1000).toISOString(), level: i % 7 === 0 ? 'warn' : 'info', msg: `controller cycle ${i} completed, valves ${i % 13}, fan ${60 + (i % 30)} %`, n: i };
    entries.push(e);
    size += 110;
    i++;
  }
  return JSON.stringify({ device: 'ahu01', entries });
}

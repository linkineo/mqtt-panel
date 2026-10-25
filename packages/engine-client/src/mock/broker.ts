// Embedded broker state for the mock engine (mockup 12 data in the demo scenario).
import type {
  AddListenerRequest,
  BrokerCerts,
  BrokerClient,
  BrokerConfig,
  BrokerStatus,
  CertDetails,
  IssueCertRequest,
  IssuedCert,
  Listener,
  LogLine,
  SaveOutcome,
  UserMessage,
} from '../types';
import { clone, randHex, um } from './util';

const LEVELS: LogLine['level'][] = ['debug', 'info', 'warn', 'error'];
const LABEL: Record<Listener['kind'], BrokerClient['transport']> = { tcp: 'TCP', tls: 'TLS', ws: 'WS', wss: 'WSS' };

/** " · 2 sessions dropped: port changed", or nothing. */
function dropped(n: number, why: string): string {
  return n ? ` · ${n} session${n === 1 ? '' : 's'} dropped: ${why}` : '';
}
export const APP_USER = 'mqtt-panel';
export const DESKTOP_CERT = 'mqttpanel-desktop';

/** Security rank: mutual TLS (TLS before WSS), TLS, secure WebSocket, TCP, WebSocket. */
/** Like the engine: trimmed, lower case, no trailing slash. */
export function normalizeOrigin(o: string): string {
  return o.trim().replace(/\/+$/, '').toLowerCase();
}

/** `scheme://host[:port]`, no path, no user info; `null` is never accepted. */
export function validOrigin(o: string): boolean {
  const m = /^([a-z0-9+.-]+):\/\/([^/@?#\s*]+)$/.exec(o);
  return !!m;
}

export function listenerRank(l: Listener, requireClientCert: boolean): number {
  const secure = l.kind === 'tls' || l.kind === 'wss';
  if (secure && requireClientCert) return l.kind === 'tls' ? 6 : 5;
  return { tls: 4, wss: 3, tcp: 2, ws: 1 }[l.kind];
}

function fingerprint(): string {
  return Array.from({ length: 32 }, () => randHex(2).toUpperCase()).join(':');
}

/** The automatic server certificate names of the demo computer (change set 11: interface addresses included). */
export const AUTO_SANS = ['localhost', '127.0.0.1', '::1', 'mqttpanel-host', 'mqttpanel-host.local', '192.168.1.20', 'fd00::20'];

/** An extra name: an IP literal or a host name (same rule as the engine); lower-cased, or null when invalid. */
export function validExtraName(raw: string): string | null {
  const s = raw.trim().replace(/\.$/, '');
  if (!s) return null;
  if (/^\d{1,3}(\.\d{1,3}){3}$/.test(s)) return s.split('.').every((o) => Number(o) <= 255) ? s : null;
  if (s.includes(':')) return /^\[?[0-9a-fA-F:.]+\]?$/.test(s) ? s.replace(/^\[|\]$/g, '').toLowerCase() : null;
  const body = s.startsWith('*.') ? s.slice(2) : s;
  const labels = body.split('.');
  const ok = labels.every((l) => /^[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?$/.test(l));
  return ok && !labels.every((l) => /^\d+$/.test(l)) ? s.toLowerCase() : null;
}

function certDetails(subject: string, issuer: string, san: string[], notBefore: string, notAfter: string, isCa: boolean, keyType = 'ECDSA P-256'): CertDetails {
  return { subject, issuer, san, notBefore, notAfter, fingerprint: fingerprint(), keyType, selfSigned: subject === issuer, isCa };
}

export class MockBroker {
  config: BrokerConfig;
  state: BrokerStatus['state'];
  startedAt: number | null;
  certs: BrokerCerts;
  issued: IssuedCert[];
  clients: BrokerClient[];
  log: LogLine[] = [];
  logSeq = 0;
  inRate = 148;
  outRate = 392;
  retained = 1212;
  private listenerN = 0;
  /** Plain user passwords (the real app keeps them in the keychain). */
  passwords = new Map<string, string>();

  constructor(demo: boolean, private now: () => number) {
    const t = now();
    this.config = {
      listeners: [
        { id: 'l-tcp', kind: 'tcp', port: 1883, enabled: true, preset: 'plain', allowedOrigins: [] },
        { id: 'l-tls', kind: 'tls', port: 8883, enabled: true, preset: demo ? 'mtls' : 'encrypted', allowedOrigins: [] },
        { id: 'l-ws', kind: 'ws', port: 8083, enabled: demo, preset: 'plain', allowedOrigins: [] },
        { id: 'l-wss', kind: 'wss', port: 8084, enabled: false, preset: 'encrypted', allowedOrigins: [] },
      ],
      bind: 'network',
      anonymous: false,
      users: demo
        ? [
            { username: APP_USER, system: true, hasPassword: true },
            { username: 'commissioning', system: false, hasPassword: true },
            { username: 'edge01', system: false, hasPassword: true },
            { username: 'dashboard', system: false, hasPassword: true },
          ]
        : [],
      requireClientCert: demo,
      verifyAgainst: 'local',
      maxClients: 1000,
      extraSans: [],
    };
    if (demo) {
      this.passwords.set(APP_USER, 'Vq7mZ3xLwT9bN4cHs8kR2p');
      this.passwords.set('commissioning', 'site-2026-commissioning');
      this.passwords.set('edge01', 'edge01-Hx4q9');
      this.passwords.set('dashboard', 'dash-board-77');
    }
    this.state = demo ? 'running' : 'stopped';
    this.startedAt = demo ? t - (2 * 60 + 14) * 60_000 : null;
    this.certs = {
      ca: certDetails('CN=MQTT Panel Local CA', 'CN=MQTT Panel Local CA', [], '2026-10-12', '2028-10-12', true),
      server: certDetails('CN=mqttpanel-local', 'CN=MQTT Panel Local CA', [...AUTO_SANS], '2026-10-12', '2028-10-12', false),
      imported: false,
      importedCa: null,
      autoSans: [...AUTO_SANS],
      extraSans: [],
    };
    const ic = (name: string, lastSeen: IssuedCert['lastSeen'], system = false): IssuedCert => ({
      name,
      serial: randHex(16).toUpperCase(),
      issuedAt: '2026-10-12',
      expiresAt: '2028-10-12',
      keyType: 'ECDSA P-256',
      superseded: false,
      lastSeen,
      revoked: false,
      system,
    });
    this.issued = demo
      ? [ic('edge01', 'connected'), ic('sensor-r204', 'connected'), ic('sensor-r205', t - 2 * 60_000), ic('ahu01-ctrl', 'never'), ic('mqttpanel-desktop', 'connected', true)]
      : [];
    const cl = (clientId: string, address: string, protocol: BrokerClient['protocol'], transport: BrokerClient['transport'], subscriptions: number, ago: number): BrokerClient => ({
      clientId,
      address,
      protocol,
      transport,
      connectedAt: t - ago * 60_000,
      subscriptions,
      msgIn: Math.floor(Math.random() * 5000),
      msgOut: Math.floor(Math.random() * 20000),
      certCn: transport === 'TLS' ? clientId : null,
      username: null,
    });
    this.clients = demo
      ? [
          cl('edge01', '10.20.4.31', '5', 'TLS', 3, 130),
          cl('mqttpanel-desktop', 'localhost', '5', 'TLS', 2, 60),
          cl('dashboard-web', 'localhost', '5', 'WS', 8, 45),
          cl('sensor-r204', '10.20.4.88', '3.1.1', 'TLS', 0, 120),
          cl('sensor-r205', '10.20.4.89', '3.1.1', 'TLS', 0, 2),
          cl('ahu01-ctrl', '10.20.4.12', '5', 'TLS', 5, 110),
          cl('cli-sub-21', 'localhost', '5', 'TCP', 1, 5),
        ]
      : [];
    if (demo) {
      const base = t - 2 * 60 * 60_000;
      this.addLog('info', 'broker started · 3 listeners', base);
      this.addLog('debug', 'listener tls 0.0.0.0:8883 up (client certificate required, local CA)', base + 1000);
      this.addLog('warn', 'TLS handshake failed from 10.20.4.90: unknown CA', t - 14 * 60_000);
      this.addLog('info', 'client cli-sub-21 connected (TCP)', t - 4 * 60_000);
      this.addLog('info', 'retained store 1 212 entries', t - 170_000);
      this.addLog('warn', 'client guest-7 rejected: bad username or password', t - 72_000);
      this.addLog('info', 'client dashboard-web subscribed building/f2/#', t - 24_000);
      this.addLog('info', 'client sensor-r205 connected from 10.20.4.89 (TLS, cert CN=sensor-r205)', t - 1000);
    }
  }

  addLog(level: LogLine['level'], text: string, at?: number): void {
    this.log.push({ seq: ++this.logSeq, at: at ?? this.now(), level, text });
    if (this.log.length > 2000) this.log.splice(0, this.log.length - 2000);
  }

  liveStep(): void {
    if (this.state !== 'running') return;
    this.inRate = Math.max(80, Math.min(220, Math.round(this.inRate + (Math.random() - 0.5) * 12)));
    this.outRate = Math.max(250, Math.min(520, Math.round(this.outRate + (Math.random() - 0.5) * 24)));
    if (Math.random() < 0.05) this.retained += Math.random() < 0.5 ? 1 : 0;
  }

  liveLog(): void {
    if (this.state !== 'running') return;
    const samples: [LogLine['level'], string][] = [
      ['debug', `client edge01 PUBLISH building/f2/r20${Math.floor(Math.random() * 9) + 1}/temperature (31 B, QoS 1)`],
      ['info', `client dashboard-web subscribed building/f${Math.floor(Math.random() * 3) + 1}/#`],
      ['debug', 'keep-alive PINGREQ from sensor-r204'],
      ['info', `retained store ${this.retained.toLocaleString('fr-FR').replace(/ | /g, ' ')} entries`],
    ];
    const [lvl, text] = samples[Math.floor(Math.random() * samples.length)];
    this.addLog(lvl, text);
  }

  status(): BrokerStatus {
    const running = this.state === 'running';
    const listeners = this.config.listeners.map((l) => {
      const error = running && l.enabled ? (this.listenerErrors.get(l.id) ?? null) : null;
      return { id: l.id, up: running && l.enabled && !error, error: error ? clone(error) : null };
    });
    // Running while one listener serves; failed when none does (change set 7).
    const none = running && !listeners.some((l) => l.up);
    return {
      state: none ? 'failed' : this.state,
      startedAt: running ? this.startedAt : null,
      clients: running ? this.clients.length : 0,
      maxClients: this.config.maxClients,
      msgInRate: running ? this.inRate : 0,
      msgOutRate: running ? this.outRate : 0,
      retained: this.retained,
      error: this.state === 'failed' ? this.error : none ? (listeners.find((l) => l.error)?.error ?? um('broker.noListener')) : null,
      listeners,
    };
  }

  /** Ports held by another program: validation refuses them, and a listener switched on there fails alone. */
  takenPorts = new Set<number>([8080]);
  /** Why a listener of the running broker could not start, by listener id. */
  listenerErrors = new Map<string, UserMessage>();

  getConfig(): BrokerConfig {
    return clone(this.config);
  }

  /** e2e (C-17): every start fails as if another program held the TCP listener's port. */
  failStart = false;

  /** Why a start fails, or null. */
  startError(): UserMessage | null {
    if (!this.failStart) return null;
    const l = this.config.listeners.find((x) => x.enabled) ?? this.config.listeners[0];
    return um('broker.portInUseBy', { port: l?.port ?? 1883, process: 'node' });
  }

  error: UserMessage | null = null;

  isRunning(): boolean {
    return this.state === 'running';
  }

  start(): void {
    this.error = this.startError();
    if (this.error) {
      this.state = 'failed';
      this.startedAt = null;
      this.addLog('error', 'broker could not start: no listener is up');
      return;
    }
    this.state = 'running';
    this.startedAt = this.now();
    this.listenerErrors.clear();
    for (const l of this.config.listeners.filter((x) => x.enabled && this.takenPorts.has(x.port))) {
      this.listenerErrors.set(l.id, this.portTaken(l));
    }
    const n = this.config.listeners.filter((l) => l.enabled && !this.listenerErrors.has(l.id)).length;
    this.addLog('info', `broker started · ${n} listener${n === 1 ? '' : 's'}`);
  }

  stop(): void {
    this.state = 'stopped';
    this.startedAt = null;
    this.listenerErrors.clear();
    this.addLog('info', 'broker stopped');
  }

  /** While running: tries again the listeners that could not start. */
  retryFailed(): void {
    for (const l of this.config.listeners.filter((x) => x.enabled && this.listenerErrors.has(x.id))) this.startListener(l);
  }

  // ---------------------------------------------------------------- live apply (change set 7)

  private portTaken(l: Listener): UserMessage {
    return um('broker.listenerFailed', { kind: LABEL[l.kind], port: l.port, detail: 'port already in use' });
  }

  /** Starts one listener of the running broker; it fails alone on a taken port. */
  private startListener(l: Listener, suffix = ''): void {
    if (this.state !== 'running') return;
    if (this.takenPorts.has(l.port)) {
      this.listenerErrors.set(l.id, this.portTaken(l));
      this.addLog('error', `listener ${LABEL[l.kind]} ${l.port} could not start: port already in use${suffix}`);
      return;
    }
    this.listenerErrors.delete(l.id);
    this.addLog('info', `listener ${LABEL[l.kind]} ${l.port} started${suffix}`);
  }

  /** The demo clients of a listener (by transport; the mock has one listener per kind in practice). */
  private clientsOf(l: Listener): BrokerClient[] {
    return this.clients.filter((c) => c.transport === LABEL[l.kind]);
  }

  /** Stops one listener: its sessions drop (`reconnect`: they come back at once, as after a port change). */
  private stopListener(l: Listener, why: string | null, reconnect = false): number {
    if (this.state !== 'running') return 0;
    const failed = this.listenerErrors.delete(l.id);
    const mine = failed ? [] : this.clientsOf(l);
    if (reconnect) mine.forEach((c) => (c.connectedAt = this.now()));
    else this.clients = this.clients.filter((c) => !mine.includes(c));
    if (!failed) this.addLog('info', `listener ${LABEL[l.kind]} ${l.port} stopped${why ? dropped(mine.length, why) : ''}`);
    return mine.length;
  }

  /** One applied change: its log line, with the sessions it dropped. */
  private applied(level: LogLine['level'], text: string, n: number, why: string): void {
    this.addLog(level, `${text}${dropped(n, why)}`);
  }

  /** Drops the demo sessions matching `pick` (e.g. password sessions when client certificates become required). */
  private dropSessions(pick: (c: BrokerClient) => boolean): number {
    if (this.state !== 'running') return 0;
    const n = this.clients.filter(pick).length;
    this.clients = this.clients.filter((c) => !pick(c));
    return n;
  }

  validatePort(port: number, listenerId: string | null): UserMessage | null {
    if (!Number.isInteger(port) || port < 1 || port > 65535) return um('broker.portRange');
    if (port < 1024) return um('broker.portPrivileged');
    if (this.config.listeners.some((l) => l.port === port && l.id !== listenerId)) return um('broker.portDuplicate', { port });
    if (this.takenPorts.has(port)) return um('broker.portInUseBy', { port, process: 'node' });
    return null;
  }

  addListener(req: AddListenerRequest): void {
    const l: Listener = {
      id: `l-${++this.listenerN}`,
      kind: req.kind,
      port: req.port,
      enabled: true,
      preset: req.preset,
      allowedOrigins: [],
    };
    this.config.listeners.push(l);
    // "Encrypted + client certificates" sets the broker-wide client certificate requirement to Required.
    if (req.preset === 'mtls') this.setRequireClientCert(true);
    if (this.state === 'running') this.startListener(l);
    else this.addLog('info', `${LABEL[l.kind]} listener added on port ${l.port}`);
  }

  /** Reachability of every listener (change set 4). */
  setBind(bind: BrokerConfig['bind']): void {
    if (this.config.bind === bind) return;
    this.config.bind = bind;
    // The listening sockets bind again; open sessions stay.
    this.addLog('info', bind === 'network' ? 'listeners reachable from the whole network' : 'listeners reachable from this computer only');
  }

  /** Client certificate on encrypted listeners, for the whole broker. */
  setRequireClientCert(on: boolean): void {
    if (this.config.requireClientCert === on) return;
    this.config.requireClientCert = on;
    // Optional -> Required drops the password sessions on encrypted listeners; Required -> Optional drops nothing.
    const n = on && !this.config.anonymous ? this.dropSessions((c) => (c.transport === 'TLS' || c.transport === 'WSS') && !c.certCn) : 0;
    this.applied('info', on ? 'client certificate required on encrypted listeners' : 'client certificate optional on encrypted listeners', n, 'client certificate required');
  }

  /** Kind, port, on/off and setup: a port or kind change restarts that listener alone (change set 7). */
  updateListener(l: Listener): void {
    const i = this.config.listeners.findIndex((x) => x.id === l.id);
    if (i < 0) return;
    const old = this.config.listeners[i];
    // The allowed web origins change only through setAllowedOrigins.
    this.config.listeners[i] = { ...clone(l), allowedOrigins: old.allowedOrigins };
    if (this.state !== 'running') return;
    if (old.enabled && !l.enabled) this.stopListener(old, 'listener switched off');
    else if (!old.enabled && l.enabled) this.startListener(l);
    else if (l.enabled && (old.port !== l.port || old.kind !== l.kind)) {
      const n = this.stopListener(old, null, true);
      this.startListener(l, dropped(n, old.kind !== l.kind ? 'kind changed' : 'port changed'));
    }
  }

  /** Web origins of a WS or WSS listener (like the engine: normalised, duplicates removed, `null` refused). */
  setAllowedOrigins(id: string, origins: string[]): void {
    const l = this.config.listeners.find((x) => x.id === id);
    if (!l || (l.kind !== 'ws' && l.kind !== 'wss')) throw um('broker.notRunning');
    const list: string[] = [];
    for (const o of origins) {
      const n = normalizeOrigin(o);
      if (!validOrigin(n)) throw um('broker.originInvalid', { origin: o });
      if (!list.includes(n)) list.push(n);
    }
    const label = `${LABEL[l.kind]} ${l.port}`;
    for (const o of list.filter((x) => !l.allowedOrigins.includes(x))) this.addLog('info', `web origin ${o} allowed on listener ${label}`);
    for (const o of l.allowedOrigins.filter((x) => !list.includes(x))) this.addLog('info', `web origin ${o} removed from listener ${label}`);
    l.allowedOrigins = list;
  }

  removeListener(id: string): void {
    const l = this.config.listeners.find((x) => x.id === id);
    this.config.listeners = this.config.listeners.filter((x) => x.id !== id);
    if (l?.enabled) this.stopListener(l, 'listener removed');
  }

  /** Anonymous: off drops the sessions that logged in without a login, on drops nothing. */
  setAnonymous(on: boolean): void {
    this.config.anonymous = on;
    this.applied('warn', on ? 'anonymous access on: users and client certificates are not checked' : 'anonymous access off', 0, 'anonymous access off');
  }

  addUser(username: string, password: string, system = false): UserMessage | null {
    if (!username || /\s/.test(username)) return um('broker.userInvalid');
    if (this.config.users.some((u) => u.username === username)) return um('broker.userExists', { user: username });
    this.config.users.push({ username, system, hasPassword: true });
    this.passwords.set(username, password);
    return null;
  }

  removeUser(username: string): void {
    this.config.users = this.config.users.filter((u) => u.username !== username);
    this.passwords.delete(username);
    const n = this.dropSessions((c) => c.username === username);
    this.applied('info', `user ${username} removed`, n, 'user removed');
  }

  setPassword(username: string, password: string): void {
    this.passwords.set(username, password);
    const u = this.config.users.find((x) => x.username === username);
    if (u) u.hasPassword = true;
    const n = this.dropSessions((c) => c.username === username);
    this.applied('info', `password of user ${username} changed`, n, 'password changed');
  }

  /** "mqtt-panel" with a strong password: created once, then kept. */
  ensureAppUser(): { created: boolean; password: string } {
    const existing = this.passwords.get(APP_USER);
    if (existing && this.config.users.some((u) => u.username === APP_USER)) return { created: false, password: existing };
    const a = 'abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789';
    const pw = Array.from({ length: 24 }, () => a[Math.floor(Math.random() * a.length)]).join('');
    this.config.users = this.config.users.filter((u) => u.username !== APP_USER);
    this.addUser(APP_USER, pw, true);
    this.addLog('info', `user ${APP_USER} created for the profile "Local broker"`);
    return { created: true, password: pw };
  }

  /** "mqttpanel-desktop": issued when missing, revoked or superseded, else kept. */
  ensureDesktopCert(): boolean {
    const c = this.issued.find((x) => x.name === DESKTOP_CERT && x.system && !x.revoked && !x.superseded);
    if (c) return false;
    this.issued = this.issued.filter((x) => x.name !== DESKTOP_CERT);
    const now = new Date(this.now());
    const exp = new Date(now);
    exp.setFullYear(exp.getFullYear() + 2);
    this.issued.push({ name: DESKTOP_CERT, serial: randHex(16).toUpperCase(), issuedAt: now.toISOString().slice(0, 10), expiresAt: exp.toISOString().slice(0, 10), keyType: 'ECDSA P-256', lastSeen: 'never', revoked: false, system: true, superseded: false });
    this.addLog('info', `client certificate ${DESKTOP_CERT} issued for the profile "Local broker"`);
    return true;
  }

  issue(req: IssueCertRequest): UserMessage | SaveOutcome {
    if (!/^[A-Za-z0-9._-]+$/.test(req.name)) return um('broker.nameInvalid');
    if (this.issued.some((c) => c.name === req.name && !c.revoked)) return um('broker.certExists', { name: req.name });
    const now = new Date(this.now());
    const exp = new Date(now);
    exp.setFullYear(exp.getFullYear() + req.validYears);
    this.issued.push({
      name: req.name,
      serial: randHex(16).toUpperCase(),
      issuedAt: now.toISOString().slice(0, 10),
      expiresAt: exp.toISOString().slice(0, 10),
      keyType: req.keyType === 'rsa' ? 'RSA 2048' : 'ECDSA P-256',
      lastSeen: 'never',
      revoked: false,
      system: false,
      superseded: false,
    });
    this.addLog('info', `client certificate ${req.name} issued`);
    return { kind: 'saved', fileName: req.name + (req.format === 'pem' ? '.pem' : '.p12') };
  }

  importCsr(name: string | null, years: number): SaveOutcome {
    const n = name || 'device-csr';
    const now = new Date(this.now());
    const exp = new Date(now);
    exp.setFullYear(exp.getFullYear() + (years || 2));
    this.issued.push({ name: n, serial: randHex(16).toUpperCase(), issuedAt: now.toISOString().slice(0, 10), expiresAt: exp.toISOString().slice(0, 10), keyType: 'ECDSA P-256', lastSeen: 'never', revoked: false, system: false, superseded: false });
    return { kind: 'saved', fileName: `${n}.crt` };
  }

  /** Revoking drops the certificate's session at once. */
  revoke(name: string): void {
    const c = this.issued.find((x) => x.name === name);
    if (c) c.revoked = true;
    const n = this.dropSessions((x) => x.certCn === name);
    if (c && n) c.lastSeen = this.now();
    this.applied('warn', `client certificate ${name} revoked`, n, 'certificate revoked');
  }

  /** Change set 11: the automatic names plus the extra names, which are checked and kept in the configuration. */
  regenerate(extra: string[], keyType: 'ecdsa' | 'rsa'): void {
    const names: string[] = [];
    for (const raw of extra.flatMap((x) => x.split(/[\s,]+/)).filter(Boolean)) {
      const n = validExtraName(raw);
      if (n === null) throw um('certs.badName', { name: raw });
      if (!names.includes(n)) names.push(n);
    }
    this.config.extraSans = names;
    this.certs.extraSans = [...names];
    const sans = [...AUTO_SANS, ...names.filter((n) => !AUTO_SANS.includes(n))];
    const kt = keyType === 'rsa' ? 'RSA 2048' : 'ECDSA P-256';
    const today = new Date(this.now()).toISOString().slice(0, 10);
    this.certs.server = certDetails('CN=mqttpanel-local', this.certs.ca.subject, sans, today, this.certs.server.notAfter, false, kt);
    this.certs.imported = false;
    // New handshakes use it; open sessions stay.
    this.addLog('info', 'server certificate regenerated');
  }

  importServer(): void {
    this.certs.server = certDetails('CN=broker.lab.example', 'CN=Lab Issuing CA', ['broker.lab.example'], '2026-01-01', '2027-12-31', false);
    this.certs.imported = true;
    this.addLog('info', 'server certificate imported');
  }

  /** "Verify against": drops the sessions whose certificate the new CA did not sign. */
  setVerifyAgainst(source: BrokerConfig['verifyAgainst']): void {
    if (this.config.verifyAgainst === source) return;
    this.config.verifyAgainst = source;
    const n = this.dropSessions((c) => c.certCn !== null);
    this.applied('info', `client certificates verified against the ${source === 'local' ? 'local' : 'imported'} CA`, n, 'verifying CA changed');
  }

  importClientCa(): void {
    this.certs.importedCa = certDetails('CN=Devices CA', 'CN=Devices CA', [], '2025-01-01', '2035-01-01', true);
  }

  rotateCa(): void {
    const today = new Date(this.now());
    const exp = new Date(today);
    exp.setFullYear(exp.getFullYear() + 5);
    this.certs.ca = certDetails('CN=MQTT Panel Local CA', 'CN=MQTT Panel Local CA', [], today.toISOString().slice(0, 10), exp.toISOString().slice(0, 10), true);
    this.issued = this.issued.map((c) => (c.revoked ? c : { ...c, superseded: true }));
    // The sessions that logged in with a certificate from the old CA drop.
    const n = this.config.verifyAgainst === 'local' ? this.dropSessions((c) => c.certCn !== null) : 0;
    this.applied('warn', 'CA rotated: issued client certificates must be re-issued', n, 'CA rotated');
  }

  logAfter(level: LogLine['level'], afterSeq: number): LogLine[] {
    const min = LEVELS.indexOf(level);
    return this.log.filter((l) => l.seq > afterSeq && LEVELS.indexOf(l.level) >= min).map((l) => ({ ...l }));
  }

  /** Most secure enabled listener: mutual TLS, then TLS, then secure WebSocket, then TCP, then WebSocket. */
  pickListener(id: string | null): Listener | null {
    const enabled = this.config.listeners.filter((l) => l.enabled);
    if (id) return this.config.listeners.find((l) => l.id === id) ?? null;
    const req = this.config.requireClientCert;
    return [...enabled].sort((a, b) => listenerRank(b, req) - listenerRank(a, req))[0] ?? null;
  }
}

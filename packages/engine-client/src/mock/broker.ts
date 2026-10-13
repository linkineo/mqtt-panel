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
export const APP_USER = 'mqtt-panel';
export const DESKTOP_CERT = 'mqttpanel-desktop';

/** Security rank: mutual TLS (TLS before WSS), TLS, secure WebSocket, TCP, WebSocket. */
export function listenerRank(l: Listener, requireClientCert: boolean): number {
  const secure = l.kind === 'tls' || l.kind === 'wss';
  if (secure && requireClientCert) return l.kind === 'tls' ? 6 : 5;
  return { tls: 4, wss: 3, tcp: 2, ws: 1 }[l.kind];
}

function fingerprint(): string {
  return Array.from({ length: 32 }, () => randHex(2).toUpperCase()).join(':');
}

function certDetails(subject: string, issuer: string, san: string[], notBefore: string, notAfter: string, isCa: boolean, keyType = 'ECDSA P-256'): CertDetails {
  return { subject, issuer, san, notBefore, notAfter, fingerprint: fingerprint(), keyType, selfSigned: subject === issuer, isCa };
}

export class MockBroker {
  config: BrokerConfig;
  state: BrokerStatus['state'];
  startedAt: number | null;
  restartNeeded = false;
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
        { id: 'l-tcp', kind: 'tcp', bind: 'network', port: 1883, enabled: true, preset: 'plain' },
        { id: 'l-tls', kind: 'tls', bind: 'network', port: 8883, enabled: true, preset: demo ? 'mtls' : 'encrypted' },
        { id: 'l-ws', kind: 'ws', bind: 'localhost', port: 8083, enabled: demo, preset: 'plain' },
        { id: 'l-wss', kind: 'wss', bind: 'network', port: 8084, enabled: false, preset: 'encrypted' },
      ],
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
      server: certDetails('CN=mqttpanel-local', 'CN=MQTT Panel Local CA', ['localhost', '127.0.0.1', '::1', 'mqttpanel-host.local'], '2026-10-12', '2028-10-12', false),
      imported: false,
      importedCa: null,
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
      this.addLog('debug', 'listener tls 0.0.0.0:8883 up (mutual TLS, local CA)', base + 1000);
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
    return {
      state: this.state,
      startedAt: running ? this.startedAt : null,
      clients: running ? this.clients.length : 0,
      maxClients: this.config.maxClients,
      msgInRate: running ? this.inRate : 0,
      msgOutRate: running ? this.outRate : 0,
      retained: this.retained,
      restartNeeded: this.restartNeeded,
      error: null,
      listeners: this.config.listeners.map((l) => ({ id: l.id, up: running && l.enabled, error: null })),
    };
  }

  getConfig(): BrokerConfig {
    return clone(this.config);
  }

  start(): void {
    this.state = 'running';
    this.startedAt = this.now();
    this.restartNeeded = false;
    this.addLog('info', `broker started · ${this.config.listeners.filter((l) => l.enabled).length} listeners`);
  }

  stop(): void {
    this.state = 'stopped';
    this.startedAt = null;
    this.addLog('info', 'broker stopped');
  }

  validatePort(port: number, listenerId: string | null): UserMessage | null {
    if (!Number.isInteger(port) || port < 1 || port > 65535) return um('broker.portRange');
    if (port < 1024) return um('broker.portPrivileged');
    if (this.config.listeners.some((l) => l.port === port && l.id !== listenerId)) return um('broker.portDuplicate', { port });
    if (port === 8080) return um('broker.portInUseBy', { port, process: 'node' });
    return null;
  }

  changed(): void {
    if (this.state === 'running') this.restartNeeded = true;
  }

  addListener(req: AddListenerRequest): void {
    const l: Listener = {
      id: `l-${++this.listenerN}`,
      kind: req.kind,
      bind: req.bind,
      port: req.port,
      enabled: true,
      preset: req.preset,
    };
    this.config.listeners.push(l);
    // Mutual TLS sets the broker-wide client certificate requirement to Required.
    if (req.preset === 'mtls') this.config.requireClientCert = true;
    this.changed();
  }

  /** Client certificate on encrypted listeners, for the whole broker. */
  setRequireClientCert(on: boolean): void {
    if (this.config.requireClientCert === on) return;
    this.config.requireClientCert = on;
    this.addLog('info', on ? 'client certificate required on encrypted listeners' : 'client certificate optional on encrypted listeners');
    this.changed();
  }

  updateListener(l: Listener): void {
    const i = this.config.listeners.findIndex((x) => x.id === l.id);
    if (i >= 0) this.config.listeners[i] = clone(l);
    this.changed();
  }

  removeListener(id: string): void {
    this.config.listeners = this.config.listeners.filter((l) => l.id !== id);
    this.changed();
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
  }

  /** "mqtt-panel" with a strong password: created once, then kept. */
  ensureAppUser(): { created: boolean; password: string } {
    const existing = this.passwords.get(APP_USER);
    if (existing && this.config.users.some((u) => u.username === APP_USER)) return { created: false, password: existing };
    const a = 'abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789';
    const pw = Array.from({ length: 24 }, () => a[Math.floor(Math.random() * a.length)]).join('');
    this.removeUser(APP_USER);
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

  revoke(name: string): void {
    const c = this.issued.find((x) => x.name === name);
    if (c) c.revoked = true;
    this.addLog('info', `client certificate ${name} revoked`);
  }

  regenerate(sans: string[], keyType: 'ecdsa' | 'rsa'): void {
    const kt = keyType === 'rsa' ? 'RSA 2048' : 'ECDSA P-256';
    const today = new Date(this.now()).toISOString().slice(0, 10);
    this.certs.server = certDetails('CN=mqttpanel-local', this.certs.ca.subject, sans.length ? sans : this.certs.server.san, today, this.certs.server.notAfter, false, kt);
    this.certs.imported = false;
    this.changed();
  }

  importServer(): void {
    this.certs.server = certDetails('CN=broker.lab.example', 'CN=Lab Issuing CA', ['broker.lab.example'], '2026-01-01', '2027-12-31', false);
    this.certs.imported = true;
    this.changed();
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
    this.changed();
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

// Shared helpers for the mock engine: user messages, encoding, MQTT filter matching.
import type { UserMessage } from '../types';

const EN: Record<string, string> = {
  'error.dns': 'Cannot find the host {host}. Check the name or your network.',
  'error.notConnected': 'This connection is not open. Connect first.',
  'reason.notAuthorized': 'not authorized',
  'tls.untrusted': 'This broker presents a certificate MQTT Panel cannot verify.',
  'tls.reason.selfSigned':
    'Reason: the certificate is self-signed. Nobody vouches for it, so compare the fingerprint with the one shown on the broker before trusting it.',
  'tls.reason.unknownCa':
    'Reason: the certificate is signed by an authority this computer does not know ({issuer}). Compare the fingerprint with the one shown on the broker before trusting it.',
  'tls.reason.fingerprintChanged':
    'The broker now presents a different certificate from the one you trusted. Old fingerprint {old}, new fingerprint {new}.',
  'tls.expired': "The broker's certificate expired on {date}.",
  'tls.wrongPassphrase': 'The passphrase for {file} is wrong.',
  'file.notCertificate': 'This file is not a certificate.',
  'file.notKey': 'This file is not a private key.',
  'file.notProto': 'This file is not a Protobuf schema: {detail}',
  'file.notCsr': 'This file is not a certificate request (CSR).',
  'import.passwordsSkipped': 'Passwords and keys were not imported. Enter them in each profile.',
  'import.unsupportedField': 'Skipped: {detail}',
  'payload.invalidJson': 'Invalid JSON at line {line}, column {column}: {detail}',
  'payload.invalidHex': 'Invalid hex at position {position}. Use pairs of 0-9 and A-F; spaces and 0x are allowed.',
  'payload.invalidBase64': 'Invalid base64: {detail}',
  'decode.protobuf': 'Cannot decode as Protobuf {type}: {detail}',
  'decode.sparkplug': 'Cannot decode as Sparkplug B: {detail}',
  'decode.cbor': 'Cannot decode as CBOR: {detail}',
  'decode.msgpack': 'Cannot decode as MessagePack: {detail}',
  'decode.json': 'Not valid JSON: {detail}',
  'decode.base64': 'Not valid base64: {detail}',
  'decode.notImage': 'This payload is not an image MQTT Panel can show.',
  'request.timeout': 'No reply within {seconds} s.',
  'repeat.stoppedDisconnect': 'Repeat stopped: the connection closed.',
  'broker.portPrivileged': 'Ports below 1024 need administrator rights; choose 1024 or higher.',
  'broker.portPrivilegedShort': 'Needs admin rights below 1024. Try {suggestion}.',
  'broker.portRange': 'Choose a port between 1024 and 65535.',
  'broker.portInUse': 'Port {port} is already in use.',
  'broker.portInUseBy': 'Port {port} is already in use by {process}.',
  'broker.portDuplicate': 'Another listener already uses port {port}.',
  'broker.userExists': 'A user named {user} already exists.',
  'broker.userInvalid': 'A username cannot be empty or contain spaces.',
  'broker.certExists': 'A client certificate named {name} already exists.',
  'broker.nameInvalid': 'Use letters, digits, dots, dashes and underscores for the name.',
  'broker.noListener': 'No listener is enabled. Turn one on first.',
  'broker.notRunning': 'The broker is not running. Start it first.',
  'profile.notFound': 'This profile no longer exists.',
};

export function um(key: string, params: Record<string, string | number> = {}): UserMessage {
  const tpl = EN[key] ?? key;
  const text = tpl.replace(/\{(\w+)\}/g, (m, name: string) => (name in params ? String(params[name]) : m));
  return { key, params, text };
}

const enc = new TextEncoder();
export function utf8(s: string): Uint8Array {
  return enc.encode(s);
}
export function decodeUtf8(b: Uint8Array): string | null {
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(b);
  } catch {
    return null;
  }
}

export function toBase64(b: Uint8Array): string {
  let s = '';
  const chunk = 0x8000;
  for (let i = 0; i < b.length; i += chunk) {
    s += String.fromCharCode.apply(null, Array.from(b.subarray(i, i + chunk)));
  }
  return btoa(s);
}
export function fromBase64(s: string): Uint8Array {
  const bin = atob(s);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export function hexDump(b: Uint8Array, maxBytes = 64 * 1024): string {
  const lines: string[] = [];
  const n = Math.min(b.length, maxBytes);
  for (let off = 0; off < n; off += 16) {
    const row = b.subarray(off, Math.min(off + 16, n));
    const hex: string[] = [];
    let ascii = '';
    for (let i = 0; i < 16; i++) {
      if (i < row.length) {
        hex.push(row[i].toString(16).padStart(2, '0'));
        ascii += row[i] >= 32 && row[i] < 127 ? String.fromCharCode(row[i]) : '.';
      } else hex.push('  ');
    }
    lines.push(`${off.toString(16).padStart(8, '0')}  ${hex.slice(0, 8).join(' ')}  ${hex.slice(8).join(' ')}  |${ascii}|`);
  }
  if (b.length > n) lines.push(`… ${b.length - n} more bytes`);
  return lines.join('\n');
}

/** MQTT topic filter matching with + and #. */
export function topicMatches(filter: string, topic: string): boolean {
  if (filter === '#') return !topic.startsWith('$');
  const f = filter.split('/');
  const t = topic.split('/');
  if ((f[0] === '+' || f[0] === '#') && t[0].startsWith('$')) return false;
  for (let i = 0; i < f.length; i++) {
    if (f[i] === '#') return true;
    if (i >= t.length) return false;
    if (f[i] !== '+' && f[i] !== t[i]) return false;
  }
  return f.length === t.length;
}

export function delay(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

export function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T;
}

export function randHex(n: number): string {
  let s = '';
  for (let i = 0; i < n; i++) s += Math.floor(Math.random() * 16).toString(16);
  return s;
}

export function isoDate(ms: number): string {
  return new Date(ms).toISOString().slice(0, 10);
}

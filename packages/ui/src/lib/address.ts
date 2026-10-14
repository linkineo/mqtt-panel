import type { Scheme } from '@mqttpanel/engine-client';

/**
 * Address prefiller (change set 5): the "Host and port" field is a view over the profile's scheme, host, port and
 * WebSocket path. The profile keeps them as separate fields (host bare, without scheme, brackets or port); this module
 * turns them into the field text and reads the field text back. Pure functions, no React.
 */

export const DEFAULT_PORT: Record<Scheme, number> = { mqtt: 1883, mqtts: 8883, ws: 8083, wss: 8084 };

/** Prefix spellings the field accepts, case-insensitive: the four schemes plus tcp (mqtt), ssl and tls (mqtts). */
const ALIASES: Record<string, Scheme> = { mqtt: 'mqtt', mqtts: 'mqtts', ws: 'ws', wss: 'wss', tcp: 'mqtt', ssl: 'mqtts', tls: 'mqtts' };

export const isWebSocket = (s: Scheme): boolean => s === 'ws' || s === 'wss';

/** "::1" → "[::1]"; names and IPv4 addresses unchanged; an already bracketed host stays as it is. */
export function hostForDisplay(host: string): string {
  return host.includes(':') && !host.startsWith('[') ? `[${host}]` : host;
}

/** The field text: "<scheme>://<host>:<port>"; the ":port" suffix follows the host, so an empty host shows the prefix only. */
export function formatAddress(a: { scheme: Scheme; host: string; port: number }): string {
  const host = hostForDisplay(a.host.trim());
  return `${a.scheme}://${host}${host && a.port > 0 ? `:${a.port}` : ''}`;
}

export interface ParsedAddress {
  /** The protocol the prefix selects; null when the text has no known prefix (the protocol then stays as it is). */
  scheme: Scheme | null;
  /** Bare host: no scheme, no brackets, no port. */
  host: string;
  /** Port typed after the host; null when none (or not a valid port number). */
  port: number | null;
  /** Path after the host ("/mqtt"); null when none. Only used for ws and wss. */
  path: string | null;
}

/**
 * True while the text is only the beginning of a prefix with its colon ("ws:", "mqtt:/"): nothing to read
 * yet, the profile is left as it is. Plain letters ("ws") are read as a host name; an empty field is an empty host.
 */
export function isPartialPrefix(text: string): boolean {
  const s = text.trim().toLowerCase();
  if (!s) return false;
  return Object.keys(ALIASES).some((k) => `${k}://`.startsWith(s)) && !/^[a-z]+$/.test(s) && !s.endsWith('://');
}

/** Reads the field text back. Accepts "host", "host:port", "scheme://host:port/path", "[v6]:port" and bare IPv6 literals. */
export function parseAddress(text: string): ParsedAddress {
  let s = text.trim();
  let scheme: Scheme | null = null;
  const m = /^([a-z][a-z0-9+.-]*):\/\//i.exec(s);
  if (m) {
    scheme = ALIASES[m[1].toLowerCase()] ?? null;
    s = s.slice(m[0].length);
  }
  let path: string | null = null;
  const slash = s.search(/[/?#]/);
  if (slash >= 0) {
    path = s.slice(slash) || null;
    s = s.slice(0, slash);
  }
  let host = s;
  let portText: string | null = null;
  if (s.startsWith('[')) {
    const end = s.indexOf(']');
    if (end < 0) host = s.slice(1);
    else {
      host = s.slice(1, end);
      const rest = /^:(.*)$/.exec(s.slice(end + 1));
      if (rest) portText = rest[1];
    }
  } else if ((s.match(/:/g) ?? []).length === 1) {
    const i = s.indexOf(':');
    host = s.slice(0, i);
    portText = s.slice(i + 1);
  }
  // more than one colon without brackets: a bare IPv6 literal, no port
  let port: number | null = null;
  if (portText && /^\d{1,5}$/.test(portText)) {
    const n = Number(portText);
    if (n >= 1 && n <= 65535) port = n;
  }
  return { scheme, host: host.trim(), port, path };
}

export interface AddressFields {
  scheme: Scheme;
  host: string;
  port: number;
  wsPath: string;
}

/**
 * What the profile becomes after the user typed `text` in the field. A missing prefix keeps the protocol; a new
 * prefix selects its protocol and, when the port still held the old protocol's default, switches the port to the new
 * default (unless the text carries a port); a path is taken for ws and wss only.
 */
export function applyAddressText(cur: AddressFields, text: string): AddressFields {
  if (isPartialPrefix(text)) return cur;
  const a = parseAddress(text);
  const scheme = a.scheme ?? cur.scheme;
  let port = cur.port;
  if (a.port !== null) port = a.port;
  else if (scheme !== cur.scheme && cur.port === DEFAULT_PORT[cur.scheme]) port = DEFAULT_PORT[scheme];
  const wsPath = isWebSocket(scheme) && a.path ? a.path : cur.wsPath;
  return { scheme, host: a.host, port, wsPath };
}

/** Choosing a protocol in the segment: the port follows when it still held the previous protocol's default. */
export function applyScheme(cur: AddressFields, scheme: Scheme): AddressFields {
  return { ...cur, scheme, port: cur.port === DEFAULT_PORT[cur.scheme] ? DEFAULT_PORT[scheme] : cur.port };
}

// Payload validation and encoding as it would go on the wire (P-08).
import type { PayloadEncoding, UserMessage } from '../types';
import { fromBase64, um, utf8 } from './util';

export type Encoded = { ok: true; bytes: Uint8Array } | { ok: false; error: UserMessage };

function lineCol(text: string, pos: number): { line: number; column: number } {
  const before = text.slice(0, Math.max(0, Math.min(pos, text.length)));
  const lines = before.split('\n');
  return { line: lines.length, column: lines[lines.length - 1].length + 1 };
}

export function encodePayloadBytes(format: PayloadEncoding, payload: string): Encoded {
  if (format === 'json') {
    try {
      JSON.parse(payload);
      return { ok: true, bytes: utf8(payload) };
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      let line: number;
      let column: number;
      const lc = /line (\d+) column (\d+)/.exec(msg);
      const pm = /position (\d+)/.exec(msg);
      if (lc) {
        line = Number(lc[1]);
        column = Number(lc[2]);
      } else if (pm) {
        ({ line, column } = lineCol(payload, Number(pm[1])));
      } else {
        ({ line, column } = lineCol(payload, payload.length));
      }
      let detail = msg
        .replace(/\s*\(line \d+ column \d+\)/, '')
        .replace(/\s*in JSON at position \d+.*$/, '')
        .replace(/, ".*" is not valid JSON$/s, '')
        .trim();
      if (!detail) detail = 'unexpected input';
      return { ok: false, error: um('payload.invalidJson', { line, column, detail }) };
    }
  }
  if (format === 'text') return { ok: true, bytes: utf8(payload) };
  if (format === 'hex') {
    const digits: string[] = [];
    for (let i = 0; i < payload.length; i++) {
      const c = payload[i];
      if (/[\s,]/.test(c)) continue;
      if (c === '0' && (payload[i + 1] === 'x' || payload[i + 1] === 'X')) {
        i++;
        continue;
      }
      if (!/[0-9a-fA-F]/.test(c)) return { ok: false, error: um('payload.invalidHex', { position: i + 1 }) };
      digits.push(c);
    }
    if (digits.length % 2 !== 0) return { ok: false, error: um('payload.invalidHex', { position: payload.length }) };
    const out = new Uint8Array(digits.length / 2);
    for (let i = 0; i < out.length; i++) out[i] = parseInt(digits[2 * i] + digits[2 * i + 1], 16);
    return { ok: true, bytes: out };
  }
  const s = payload.replace(/\s/g, '');
  const bad = s.search(/[^A-Za-z0-9+/=]/);
  if (bad >= 0) return { ok: false, error: um('payload.invalidBase64', { detail: `unexpected character "${s[bad]}" at position ${bad + 1}` }) };
  if (s.length % 4 !== 0 || /=[^=]/.test(s)) return { ok: false, error: um('payload.invalidBase64', { detail: 'the length is not a multiple of 4' }) };
  try {
    return { ok: true, bytes: fromBase64(s) };
  } catch (e) {
    return { ok: false, error: um('payload.invalidBase64', { detail: e instanceof Error ? e.message : String(e) }) };
  }
}

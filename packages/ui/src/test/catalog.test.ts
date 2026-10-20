import { describe, expect, it } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import en from '../i18n/en/ui.json';
import enErrors from '../i18n/en/errors.json';

const root = join(__dirname, '..');
function files(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) return f === 'test' ? [] : files(p);
    return /\.(ts|tsx)$/.test(f) && !/\.test\./.test(f) ? [p] : [];
  });
}

const cat = en as Record<string, string>;
const has = (k: string) => k in cat || `${k}_one` in cat || `${k}_other` in cat || k in (enErrors as Record<string, string>);

describe('Catalog completeness', () => {
  const sources = files(root).map((f) => ({ f, s: readFileSync(f, 'utf8') }));

  it('every literal key passed to t() exists in en/ui.json', () => {
    const missing: string[] = [];
    for (const { f, s } of sources) {
      for (const m of s.matchAll(/\bt\(\s*'([^'$]+)'/g)) if (!has(m[1])) missing.push(`${m[1]} (${f.replace(root, 'src')})`);
    }
    expect(missing).toEqual([]);
  });

  it('every info bubble id has a text', () => {
    const missing: string[] = [];
    for (const { s } of sources) for (const m of s.matchAll(/<Info id="([^"]+)"/g)) if (!has(`info.${m[1]}`)) missing.push(m[1]);
    for (const p of ['plain', 'encrypted', 'passwords', 'mtls']) if (!has(`info.preset.${p}`)) missing.push(`preset.${p}`);
    expect(missing).toEqual([]);
  });

  it('dynamic key families are complete', () => {
    const families: Record<string, string[]> = {
      rail: ['connect', 'explore', 'publish', 'charts', 'broker', 'settings'],
      settings: ['appearance', 'language', 'data', 'security', 'assistant', 'updates', 'shortcuts', 'privacy', 'about'],
      'assistant.outcome': ['ok', 'refused', 'failed', 'cancelled', 'timeout'],
      'assistant.req': ['password', 'caCert', 'clientCert', 'signedCert'],
      'assistant.host': ['claudeDesktop', 'claudeCode', 'chatgpt'],
      kind: ['tcp', 'tls', 'ws', 'wss'],
      preset: ['plain', 'encrypted', 'passwords', 'mtls'],
      bstate: ['running', 'stopped', 'starting', 'failed'],
      format: ['auto', 'json', 'text', 'hex', 'base64', 'image', 'cbor', 'msgpack', 'sparkplug', 'protobuf'],
      encoding: ['json', 'text', 'hex', 'base64'],
      col: ['topic', 'value', 'spark', 'updated', 'rate', 'messages', 'qos', 'retained', 'size'],
      colour: ['sage', 'clay', 'slate', 'brick', 'ink2'],
      series: ['line', 'step', 'points'],
      'export.desc': ['csv', 'json', 'ndjson'],
      'addl.after': ['plain', 'encrypted', 'passwords', 'mtls'],
    };
    const missing: string[] = [];
    for (const [fam, ids] of Object.entries(families)) for (const id of ids) if (!has(`${fam}.${id}`)) missing.push(`${fam}.${id}`);
    for (const p of families.preset) for (const suffix of ['short', 'whatTitle', 'what']) if (!has(`preset.${p}.${suffix}`)) missing.push(`preset.${p}.${suffix}`);
    expect(missing).toEqual([]);
  });

  it('French and German only use keys that exist in English, with the same placeholders', async () => {
    for (const lang of ['fr', 'de']) {
      const other = (await import(`../i18n/${lang}/ui.json`)).default as Record<string, string>;
      const bad: string[] = [];
      for (const [k, v] of Object.entries(other)) {
        if (!(k in cat)) bad.push(`${lang}: unknown ${k}`);
        else {
          const ph = (s: string) => Array.from(s.matchAll(/\{(\w+)\}/g), (m) => m[1]).sort().join(',');
          if (ph(v) !== ph(cat[k])) bad.push(`${lang}: placeholders differ in ${k}`);
        }
        for (const re of forbidden) if (re.test(v)) bad.push(`${lang}: forbidden word in ${k}`);
      }
      expect(bad).toEqual([]);
    }
  });

  it('no catalog (EN, FR, DE) contains a word blocked by scripts/check-forbidden-words.sh', async () => {
    expect(forbidden.length).toBeGreaterThan(0);
    const bad: string[] = [];
    for (const lang of ['en', 'fr', 'de']) {
      for (const file of ['ui', 'errors']) {
        const c = (await import(`../i18n/${lang}/${file}.json`)).default as Record<string, string>;
        for (const [k, v] of Object.entries(c)) for (const re of forbidden) if (re.test(v)) bad.push(`${lang}/${file}: ${k}`);
      }
    }
    expect(bad).toEqual([]);
  });
});

/** The same patterns as the commit guard, read from the script so the list lives in one place. */
const forbidden: RegExp[] = (() => {
  const script = readFileSync(join(root, '..', '..', '..', 'scripts', 'check-forbidden-words.sh'), 'utf8');
  const block = /patterns="([\s\S]*?)"\nstatus=/.exec(script.replace(/""/g, ''));
  if (!block) return [];
  return block[1]
    .split('\n')
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => new RegExp(p.replace(/[.*+?^${}()|\\/]/g, (ch) => (ch === '[' || ch === ']' ? ch : `\\${ch}`)), 'i'));
})();

describe('Typography and register (V-05)', () => {
  it('French puts a no-break space before : ; ? ! and inside « »', async () => {
    const bad: string[] = [];
    for (const file of ['ui', 'errors']) {
      const c = (await import(`../i18n/fr/${file}.json`)).default as Record<string, string>;
      for (const [k, v] of Object.entries(c)) {
        const s = v.replace(/\{\w+\}/g, 'X').replace(/https?:\/\/\S+/g, '');
        if (/[^  ][:;?!](\s|$)/.test(s)) bad.push(`${file}: ${k}`);
        if (/«[^  ]|[^  ]»/.test(s)) bad.push(`${file}: ${k} (guillemets)`);
      }
    }
    expect(bad).toEqual([]);
  });

  it('German uses "du": no formal Sie, Ihnen or Ihr in the middle of a sentence', async () => {
    const bad: string[] = [];
    for (const file of ['ui', 'errors']) {
      const c = (await import(`../i18n/de/${file}.json`)).default as Record<string, string>;
      for (const [k, v] of Object.entries(c)) if (/[a-zäöüß,]\s+(Sie|Ihnen|Ihr|Ihre|Ihren|Ihrem|Ihrer)\b/.test(v)) bad.push(`${file}: ${k}`);
    }
    expect(bad).toEqual([]);
  });
});

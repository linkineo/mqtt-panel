import { create } from 'zustand';
import type { PayloadEncoding, PublishRequest, Qos, SentEntry, Template } from '@mqttpanel/engine-client';

export interface Composer {
  topic: string;
  payload: string;
  qos: Qos;
  retain: boolean;
  format: PayloadEncoding;
  responseTopic: string;
  correlation: string;
  waitS: number;
  propsOpen: boolean;
  contentType: string;
  payloadFormat: number | null;
  expiry: string;
  userProps: [string, string][];
  repeatS: string;
  repeatCount: string;
}

export const EMPTY_COMPOSER: Composer = {
  topic: '',
  payload: '',
  qos: 0,
  retain: false,
  format: 'json',
  responseTopic: '',
  correlation: '',
  waitS: 10,
  propsOpen: false,
  contentType: '',
  payloadFormat: null,
  expiry: '',
  userProps: [],
  repeatS: '',
  repeatCount: '',
};

interface PublishState {
  composer: Composer;
  sent: Record<string, SentEntry[]>;
  selectedSent: string | null;
  setComposer(p: Partial<Composer>): void;
  clear(): void;
  setSent(connId: string, list: SentEntry[]): void;
  upsertSent(connId: string, e: SentEntry): void;
  select(id: string | null): void;
}

export const usePublish = create<PublishState>((set, get) => ({
  composer: { ...EMPTY_COMPOSER },
  sent: {},
  selectedSent: null,
  setComposer: (p) => set({ composer: { ...get().composer, ...p } }),
  clear: () => set({ composer: { ...EMPTY_COMPOSER, format: get().composer.format } }),
  setSent: (connId, list) => set({ sent: { ...get().sent, [connId]: list } }),
  upsertSent: (connId, e) => {
    const list = get().sent[connId] ?? [];
    const i = list.findIndex((x) => x.id === e.id);
    const next = i >= 0 ? list.map((x) => (x.id === e.id ? e : x)) : [e, ...list].slice(0, 500);
    set({ sent: { ...get().sent, [connId]: next } });
  },
  select: (selectedSent) => set({ selectedSent }),
}));

/** Counts the MQTT 5 properties set in the composer ("MQTT 5 properties · n set"). */
export function propsSet(c: Composer): number {
  return (c.contentType ? 1 : 0) + (c.payloadFormat !== null ? 1 : 0) + (c.expiry ? 1 : 0) + c.userProps.filter(([k]) => k).length;
}

/** Builds the publish request from the composer; request and properties only on MQTT 5. */
export function buildRequest(c: Composer, mqtt5: boolean): PublishRequest {
  const props = mqtt5 && propsSet(c) > 0
    ? {
        contentType: c.contentType,
        payloadFormat: c.payloadFormat,
        messageExpiry: c.expiry ? Number(c.expiry.replace(/\D/g, '')) || null : null,
        userProperties: c.userProps.filter(([k]) => k),
      }
    : null;
  const request = mqtt5 && c.responseTopic.trim() ? { responseTopic: c.responseTopic.trim(), correlationData: c.correlation.trim(), waitS: c.waitS || 10 } : null;
  return { topic: c.topic.trim(), payload: c.payload, format: c.format, qos: c.qos, retain: c.retain, props, request };
}

export function composerFromRequest(r: PublishRequest, repeatS: number | null = null): Partial<Composer> {
  return {
    topic: r.topic,
    payload: r.payload,
    format: r.format,
    qos: r.qos,
    retain: r.retain,
    responseTopic: r.request?.responseTopic ?? '',
    correlation: r.request?.correlationData ?? '',
    waitS: r.request?.waitS ?? 10,
    contentType: r.props?.contentType ?? '',
    payloadFormat: r.props?.payloadFormat ?? null,
    expiry: r.props?.messageExpiry ? String(r.props.messageExpiry) : '',
    userProps: r.props?.userProperties ?? [],
    propsOpen: !!r.props,
    repeatS: repeatS ? String(repeatS) : '',
  };
}

export function templateFolders(list: Template[]): string[] {
  return Array.from(new Set(list.map((t) => t.folder).filter(Boolean)));
}

/** Placeholders like {room} in a topic or payload (P-07); JSON braces are not placeholders. */
export function findPlaceholders(...texts: string[]): string[] {
  const out = new Set<string>();
  for (const s of texts) for (const m of s.matchAll(/\{([A-Za-z_][A-Za-z0-9_-]*)\}/g)) out.add(m[1]);
  return Array.from(out);
}

export function fillPlaceholders(s: string, values: Record<string, string>): string {
  return s.replace(/\{([A-Za-z_][A-Za-z0-9_-]*)\}/g, (m, k: string) => (k in values ? values[k] : m));
}

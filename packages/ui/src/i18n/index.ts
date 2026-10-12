import i18next, { type TFunction } from 'i18next';
import { initReactI18next } from 'react-i18next';
import type { UserMessage } from '@mqttpanel/engine-client';
import enUi from './en/ui.json';
import frUi from './fr/ui.json';
import deUi from './de/ui.json';
import enErrors from './en/errors.json';
import frErrors from './fr/errors.json';
import deErrors from './de/errors.json';

export type Lang = 'en' | 'fr' | 'de';

export const resources = {
  en: { ui: enUi, errors: enErrors },
  fr: { ui: frUi, errors: frErrors },
  de: { ui: deUi, errors: deErrors },
} as const;

export function systemLang(): Lang {
  const l = (typeof navigator !== 'undefined' ? navigator.language : 'en').slice(0, 2).toLowerCase();
  return l === 'fr' || l === 'de' ? l : 'en';
}

export function resolveLang(pref: 'system' | Lang): Lang {
  return pref === 'system' ? systemLang() : pref;
}

let initialised = false;

export function initI18n(lang: Lang): typeof i18next {
  if (!initialised) {
    initialised = true;
    void i18next.use(initReactI18next).init({
      resources,
      lng: lang,
      fallbackLng: 'en',
      ns: ['ui', 'errors'],
      defaultNS: 'ui',
      keySeparator: false,
      nsSeparator: false,
      interpolation: { prefix: '{', suffix: '}', escapeValue: false },
      returnNull: false,
      initImmediate: false,
      react: { useSuspense: false },
    } as Parameters<typeof i18next.init>[0]);
  } else if (i18next.language !== lang) {
    void i18next.changeLanguage(lang);
  }
  if (typeof document !== 'undefined') document.documentElement.lang = lang;
  return i18next;
}

export function setLanguage(lang: Lang): void {
  void i18next.changeLanguage(lang);
  if (typeof document !== 'undefined') document.documentElement.lang = lang;
}

/** Renders an engine message: catalog key in the "errors" namespace, falling back to the English text. */
export function renderMessage(t: TFunction, msg: UserMessage | null | undefined): string {
  if (!msg) return '';
  // A string param starting with "@" is itself a catalog key in the errors namespace.
  const params: Record<string, string | number> = {};
  for (const [k, v] of Object.entries(msg.params ?? {})) {
    params[k] = typeof v === 'string' && v.startsWith('@') && i18next.exists(v.slice(1), { ns: 'errors' }) ? (t(v.slice(1), { ns: 'errors' }) as string) : v;
  }
  if (i18next.exists(msg.key, { ns: 'errors' })) return t(msg.key, { ns: 'errors', ...params }) as string;
  if (i18next.exists(msg.key, { ns: 'ui' })) return t(msg.key, { ns: 'ui', ...params }) as string;
  return msg.text;
}

/** Turns anything a rejected engine call produced into a UserMessage. */
export function toMessage(e: unknown): UserMessage {
  if (e && typeof e === 'object' && 'key' in e && 'text' in e) return e as UserMessage;
  return { key: 'error.internal', params: { detail: e instanceof Error ? e.message : String(e) }, text: 'MQTT Panel hit an internal problem.' };
}

export default i18next;

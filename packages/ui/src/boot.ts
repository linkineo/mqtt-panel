import type { EngineClient, Platform, Settings } from '@mqttpanel/engine-client';
import { setServices, engine, platform } from './services';
import { initI18n, resolveLang, systemLang, type Lang } from './i18n';
import { applySettings, useApp, SCREENS, type Screen } from './store/app';

export interface BootOverrides {
  lang?: Lang;
  theme?: Settings['theme'];
  accent?: Settings['accent'];
  screen?: Screen;
  /** One profile id or several, comma-separated; the first one becomes the active connection. */
  connect?: string;
  /** Mock only: a connected profile shown as reconnecting (mockup 02c). */
  reconnecting?: string;
  motion?: boolean;
}

/** Reads test and screenshot switches from the URL: ?mock=empty|demo&lang=de&theme=dark&screen=broker&connect=plant */
export function readOverrides(search: string): BootOverrides & { mock: 'demo' | 'empty'; latency?: number } {
  const p = new URLSearchParams(search);
  const lang = p.get('lang');
  const theme = p.get('theme');
  const accent = p.get('accent');
  const screen = p.get('screen');
  return {
    mock: p.get('mock') === 'empty' ? 'empty' : 'demo',
    lang: lang === 'en' || lang === 'fr' || lang === 'de' ? lang : undefined,
    theme: theme === 'light' || theme === 'dark' || theme === 'system' ? theme : undefined,
    accent: accent === 'sage' || accent === 'slate' || accent === 'clay' ? accent : undefined,
    screen: screen && (SCREENS as string[]).includes(screen) ? (screen as Screen) : undefined,
    connect: p.get('connect') ?? undefined,
    reconnecting: p.get('reconnecting') ?? undefined,
    motion: p.get('motion') === 'off' ? false : undefined,
    // mock only: slower engine steps, so screenshots can catch a flow half-way (02b steps)
    latency: p.get('latency') ? Math.min(20_000, Number(p.get('latency')) || 0) : undefined,
  };
}

/** Chooses the engine: the Tauri adapter inside the desktop shell, the in-memory mock everywhere else. */
export async function createServices(mock: 'demo' | 'empty', lang: Lang, latencyMs?: number): Promise<{ engine: EngineClient; platform: Platform }> {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    const m = await import('@mqttpanel/engine-client/tauri');
    return { engine: m.createTauriEngineClient(), platform: m.createTauriPlatform() };
  }
  const m = await import('@mqttpanel/engine-client/mock');
  const services = { engine: m.createMockEngineClient({ scenario: mock, lang, latencyMs }), platform: m.createMockPlatform() };
  // browser and e2e only (never in the desktop shell): lets Playwright flows inject messages and read the clipboard and saves
  (window as unknown as { __mqttpanelMock?: typeof services }).__mqttpanelMock = services;
  return services;
}

let unsubscribe: (() => void) | null = null;
let menuHooked = false;

/** Loads app info, settings, profiles, connections and broker state, and subscribes to engine events. */
export async function startApp(eng: EngineClient, plat: Platform, o: BootOverrides = {}): Promise<void> {
  setServices(eng, plat);
  const [appInfo, settings0] = await Promise.all([eng.appInfo(), eng.getSettings()]);
  let settings = settings0;
  const patch: Partial<Settings> = {};
  if (o.lang) patch.language = o.lang;
  if (o.theme) patch.theme = o.theme;
  if (o.accent) patch.accent = o.accent;
  if (o.motion === false) patch.motion = false;
  if (Object.keys(patch).length) settings = await eng.saveSettings({ ...settings, ...patch });
  const i18n = initI18n(resolveLang(settings.language));
  // the native menu bar follows the app language (labels from nativeMenu.* in the catalogs)
  const relabel = (l: string) => void platform().setMenuLanguage(l.slice(0, 2)).catch(() => undefined);
  relabel(i18n.language);
  if (!menuHooked) {
    menuHooked = true;
    i18n.on('languageChanged', (l) => relabel(l));
  }
  useApp.setState({ appInfo, settings });
  void eng
    .keyStoreKind()
    .then((keyStore) => useApp.setState({ keyStore }))
    .catch(() => undefined);
  applySettings(settings, appInfo);
  const [profiles, connections] = await Promise.all([eng.listProfiles(), eng.listConnections()]);
  let broker = null;
  let brokerConfig = null;
  try {
    [broker, brokerConfig] = await Promise.all([eng.brokerStatus(), eng.brokerConfig()]);
  } catch {
    /* broker not available */
  }
  const open = connections.filter((c) => c.state !== 'disconnected');
  useApp.setState({
    profiles,
    connections,
    activeConnId: open[0]?.id ?? null,
    broker,
    brokerConfig,
    selectedProfileId: profiles[0]?.id ?? null,
    screen: o.screen ?? 'connect',
  });
  unsubscribe?.();
  unsubscribe = eng.onEvent((e) => useApp.getState().handleEvent(e));
  if (typeof window !== 'undefined' && window.matchMedia) {
    const mq = window.matchMedia('(prefers-reduced-motion: reduce)');
    mq.addEventListener?.('change', () => applySettings(useApp.getState().settings, useApp.getState().appInfo));
  }
  if (o.connect) {
    const ids = o.connect.split(',').filter(Boolean);
    let first: string | null = null;
    for (const id of ids) {
      const out = await engine().connectProfile(id);
      if (out.kind === 'connected') first ??= id;
    }
    const mock = eng as Partial<{ setConnectionState(id: string, s: 'reconnecting'): void }>;
    if (o.reconnecting && mock.setConnectionState) mock.setConnectionState(o.reconnecting, 'reconnecting');
    if (first) useApp.setState({ activeConnId: first, connections: await engine().listConnections(), profiles: await engine().listProfiles() });
  }
  useApp.setState({ ready: true });
}

export function defaultLang(o: BootOverrides): Lang {
  return o.lang ?? systemLang();
}

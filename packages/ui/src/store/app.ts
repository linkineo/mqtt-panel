import { create } from 'zustand';
import type {
  AppInfo,
  BrokerConfig,
  BrokerStatus,
  ConnectionStatus,
  EngineEvent,
  Profile,
  RepeatStatus,
  SentEntry,
  Settings,
  UserMessage,
} from '@mqttpanel/engine-client';
import { engine, platform } from '../services';
import { resolveLang, setLanguage } from '../i18n';

export type Screen = 'connect' | 'explore' | 'publish' | 'charts' | 'broker' | 'settings';
export const SCREENS: Screen[] = ['connect', 'explore', 'publish', 'charts', 'broker', 'settings'];

export interface Toast {
  id: number;
  kind: 'info' | 'err' | 'warn';
  text?: string;
  msg?: UserMessage | null;
}

/** A saved listener or certificate change waiting for a broker restart (restart prompt, mockup 12c). */
export interface PendingChange {
  key: string; // rs.ch.*
  params: Record<string, string | number>;
  listenerId?: string;
  field?: 'port';
}

export type Fold = 'security' | 'advanced' | 'brokerAdvanced';

type SentListener = (updates: { connectionId: string; entry: SentEntry }[]) => void;
type MenuListener = (id: string) => void;

interface AppState {
  ready: boolean;
  appInfo: AppInfo;
  settings: Settings;
  screen: Screen;
  profiles: Profile[];
  selectedProfileId: string | null;
  connections: ConnectionStatus[];
  activeConnId: string | null;
  seqs: Record<string, number>;
  broker: BrokerStatus | null;
  brokerConfig: BrokerConfig | null;
  brokerLogSeq: number;
  repeats: RepeatStatus[];
  paletteOpen: boolean;
  toasts: Toast[];
  reducedMotion: boolean;
  /** Pending request from another screen (e.g. "Publish here"), consumed by the target screen. */
  publishPrefill: { topic: string; payload: string; format: 'json' | 'text' | 'hex' | 'base64'; nonce: number } | null;
  settingsPage: string;
  /** Changes saved but not applied yet (the broker reports restartNeeded); cleared when the broker restarts. */
  pendingChanges: PendingChange[];
  /** The restart prompt is open. */
  restartPrompt: boolean;
  /** Folds (change set 4): profile form "Security" and "Advanced", Broker "Advanced security". Closed when the app
   * starts, kept while it runs (not per profile, not saved). */
  folds: Record<Fold, boolean>;
  /** "Only this time" chosen under Advanced (change set 6): the next connect or test of this profile skips certificate
   * verification once, then the flag is cleared. Not saved. */
  skipOnce: Record<string, boolean>;
  /** Profiles created with "New broker…" in this run: their Name follows the host until the user types another name
   * (change set 6). Not saved: an existing profile is never renamed. */
  autoNamed: Record<string, boolean>;

  setScreen(s: Screen): void;
  toggleFold(f: Fold): void;
  setSettingsPage(p: string): void;
  setPalette(open: boolean): void;
  refreshProfiles(): Promise<void>;
  selectProfile(id: string | null): void;
  setActiveConn(id: string | null): void;
  saveSettings(patch: Partial<Settings>): Promise<void>;
  refreshBrokerConfig(): Promise<void>;
  setBrokerConfig(c: BrokerConfig): void;
  toast(t: Omit<Toast, 'id'>): void;
  dismissToast(id: number): void;
  handleEvent(e: EngineEvent): void;
  onSent(l: SentListener): () => void;
  onMenu(l: MenuListener): () => void;
  prefillPublish(topic: string, payload: string, format: 'json' | 'text' | 'hex' | 'base64'): void;
}

export const DEFAULT_SETTINGS: Settings = {
  theme: 'system',
  accent: 'sage',
  density: 'comfortable',
  monoSize: 12,
  valuesInTree: true,
  motion: true,
  language: 'system',
  bufferPerTopic: 200,
  pointsPerSeries: 5000,
  memoryBudgetMb: 256,
  payloadRenderLimitKb: 256,
  updateCheck: true,
  updateChannel: 'stable',
  brokerLogOpen: false,
};

const sentListeners = new Set<SentListener>();
const menuListeners = new Set<MenuListener>();
let toastId = 1;

export const useApp = create<AppState>((set, get) => ({
  ready: false,
  appInfo: { version: '', platform: 'web', native: false },
  settings: DEFAULT_SETTINGS,
  screen: 'connect',
  profiles: [],
  selectedProfileId: null,
  connections: [],
  activeConnId: null,
  seqs: {},
  broker: null,
  brokerConfig: null,
  brokerLogSeq: 0,
  repeats: [],
  paletteOpen: false,
  toasts: [],
  reducedMotion: false,
  publishPrefill: null,
  settingsPage: 'appearance',
  pendingChanges: [],
  restartPrompt: false,
  folds: { security: false, advanced: false, brokerAdvanced: false },
  skipOnce: {},
  autoNamed: {},

  setScreen: (screen) => set({ screen, paletteOpen: false }),
  toggleFold: (f) => set({ folds: { ...get().folds, [f]: !get().folds[f] } }),
  setSettingsPage: (settingsPage) => set({ settingsPage }),
  setPalette: (paletteOpen) => set({ paletteOpen }),
  refreshProfiles: async () => {
    const profiles = await engine().listProfiles();
    set({ profiles });
  },
  selectProfile: (selectedProfileId) => set({ selectedProfileId }),
  setActiveConn: (activeConnId) => set({ activeConnId }),
  saveSettings: async (patch) => {
    const next = { ...get().settings, ...patch };
    set({ settings: next });
    applySettings(next, get().appInfo);
    try {
      const saved = await engine().saveSettings(next);
      set({ settings: saved });
    } catch {
      /* keep the local value; the engine reports problems through toasts elsewhere */
    }
  },
  refreshBrokerConfig: async () => {
    try {
      const brokerConfig = await engine().brokerConfig();
      set({ brokerConfig });
    } catch {
      /* broker unavailable */
    }
  },
  setBrokerConfig: (brokerConfig) => set({ brokerConfig }),
  toast: (t) => {
    const id = toastId++;
    set({ toasts: [...get().toasts, { ...t, id }].slice(-4) });
    setTimeout(() => get().dismissToast(id), t.kind === 'err' ? 8000 : 4000);
  },
  dismissToast: (id) => set({ toasts: get().toasts.filter((x) => x.id !== id) }),
  handleEvent: (e) => {
    if (e.type === 'menu') {
      menuListeners.forEach((l) => l(e.id));
      return;
    }
    const st = get();
    const patch: Partial<AppState> = {
      connections: e.connections,
      seqs: { ...st.seqs, ...e.changed },
      repeats: e.repeats,
      brokerLogSeq: e.brokerLogSeq,
    };
    if (e.broker) {
      patch.broker = e.broker;
      if (!e.broker.restartNeeded && st.pendingChanges.length) patch.pendingChanges = [];
    }
    // keep the active connection valid: when it is closed or no longer open, another open one takes over
    const isOpenState = (s: ConnectionStatus['state']) => s === 'connected' || s === 'connecting' || s === 'reconnecting';
    const open = e.connections.filter((c) => isOpenState(c.state));
    const active = e.connections.find((c) => c.id === st.activeConnId);
    if (st.activeConnId && !active) patch.activeConnId = open[0]?.id ?? e.connections[0]?.id ?? null;
    else if (active && !isOpenState(active.state) && open.length) patch.activeConnId = open[0].id;
    set(patch);
    if (e.sentUpdates.length) sentListeners.forEach((l) => l(e.sentUpdates));
  },
  onSent: (l) => {
    sentListeners.add(l);
    return () => sentListeners.delete(l);
  },
  onMenu: (l) => {
    menuListeners.add(l);
    return () => menuListeners.delete(l);
  },
  prefillPublish: (topic, payload, format) =>
    set({ publishPrefill: { topic, payload, format, nonce: Date.now() + Math.random() }, screen: 'publish' }),
}));

/** Applies theme, accent, density, language and motion settings to the document. */
export function applySettings(s: Settings, info: AppInfo): void {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  if (s.theme === 'system') root.removeAttribute('data-theme');
  else root.setAttribute('data-theme', s.theme);
  if (s.accent === 'sage') root.removeAttribute('data-accent');
  else root.setAttribute('data-accent', s.accent);
  root.setAttribute('data-density', s.density);
  root.setAttribute('data-platform', info.platform);
  root.setAttribute('data-motion', s.motion ? 'system' : 'off');
  root.style.setProperty('--mono-size', `${Math.min(16, Math.max(10, s.monoSize))}px`);
  setLanguage(resolveLang(s.language));
  let reduced = !s.motion;
  try {
    reduced = reduced || platform().prefersReducedMotion();
  } catch {
    /* platform not ready */
  }
  if (useApp.getState().reducedMotion !== reduced) useApp.setState({ reducedMotion: reduced });
}

export function activeConnection(): ConnectionStatus | null {
  const st = useApp.getState();
  return st.connections.find((c) => c.id === st.activeConnId) ?? null;
}

export function useActiveConnection(): ConnectionStatus | null {
  return useApp((s) => s.connections.find((c) => c.id === s.activeConnId) ?? null);
}

export function useActiveProfile(): Profile | null {
  return useApp((s) => s.profiles.find((p) => p.id === s.activeConnId) ?? null);
}

export function isMac(info: AppInfo): boolean {
  if (info.platform === 'macos') return true;
  if (info.platform === 'web' && typeof navigator !== 'undefined') return /Mac/.test(navigator.platform || navigator.userAgent);
  return false;
}

/** Display form of the primary modifier: "⌘" on macOS, "Ctrl+" elsewhere. */
export function modKey(info: AppInfo): string {
  return isMac(info) ? '⌘' : 'Ctrl+';
}

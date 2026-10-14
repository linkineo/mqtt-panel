import { create } from 'zustand';
import type { ConnectOutcome, Profile, SaveOutcome, TrustMode, TrustPrompt, UserMessage } from '@mqttpanel/engine-client';
import { engine } from '../services';
import { toMessage } from '../i18n';
import { useApp } from '../store/app';

/** The certificate trust prompt (mockup 03) and the action to retry once the user trusts. */
interface TrustState {
  prompt: TrustPrompt | null;
  action: 'connect' | 'test' | null;
  set(prompt: TrustPrompt | null, action: 'connect' | 'test' | null): void;
}
export const useTrust = create<TrustState>((set) => ({
  prompt: null,
  action: null,
  set: (prompt, action) => set({ prompt, action }),
}));

/** Work in progress per profile: connecting, testing, or starting the embedded broker first ("Local broker"). */
export const useBusy = create<{ connecting: string | null; testing: string | null; startingBroker: string | null }>(() => ({
  connecting: null,
  testing: null,
  startingBroker: null,
}));

/**
 * Connect and Test on the profile "Local broker" start the embedded broker first when it is not running (owner,
 * 13 Oct 2026, change set 4). Returns the broker's reason when it cannot start, so the connection fails with it.
 */
async function ensureLocalBroker(id: string): Promise<UserMessage | null> {
  const p = useApp.getState().profiles.find((x) => x.id === id);
  if (!p?.isLocalBroker) return null;
  const st = await engine().brokerStatus();
  if (st.state === 'running') return null;
  useBusy.setState({ startingBroker: id });
  try {
    const s = await engine().brokerStart();
    useApp.setState({ broker: s });
    if (s.state === 'running') return null;
    return s.error ?? s.listeners.find((l) => l.error)?.error ?? { key: 'broker.noListener', params: {}, text: 'No listener is enabled. Turn one on first.' };
  } finally {
    useBusy.setState({ startingBroker: null });
  }
}

/** Sets or clears "only this time" for the next attempt of a profile (change set 6). */
export function setSkipOnce(id: string, on: boolean): void {
  const cur = useApp.getState().skipOnce;
  if (!!cur[id] === on) return;
  const next = { ...cur };
  if (on) next[id] = true;
  else delete next[id];
  useApp.setState({ skipOnce: next });
}

/** Takes the "only this time" flag of a profile for the attempt about to start (it covers one attempt). */
function takeSkipOnce(id: string): boolean {
  const on = !!useApp.getState().skipOnce[id];
  if (on) setSkipOnce(id, false);
  return on;
}

async function refresh(): Promise<void> {
  const [profiles, connections] = await Promise.all([engine().listProfiles(), engine().listConnections()]);
  useApp.setState({ profiles, connections });
}

export async function handleConnectOutcome(id: string, out: ConnectOutcome, switchScreen = true): Promise<boolean> {
  if (out.kind === 'connected') {
    await refresh();
    useApp.setState({ activeConnId: out.status.id });
    if (switchScreen) useApp.getState().setScreen('explore');
    return true;
  }
  if (out.kind === 'needsTrust') {
    useTrust.getState().set(out.trust, 'connect');
    return false;
  }
  await refresh();
  useApp.getState().toast({ kind: 'err', msg: out.error });
  void id;
  return false;
}

export async function connectProfile(id: string, switchScreen = true, skipOnce = false): Promise<boolean> {
  useBusy.setState({ connecting: id });
  try {
    const down = await ensureLocalBroker(id);
    if (down) {
      await refresh();
      useApp.getState().toast({ kind: 'err', msg: down });
      return false;
    }
    const once = takeSkipOnce(id) || skipOnce;
    const out = await engine().connectProfile(id, once || undefined);
    return await handleConnectOutcome(id, out, switchScreen);
  } catch (e) {
    useApp.getState().toast({ kind: 'err', msg: toMessage(e) });
    return false;
  } finally {
    useBusy.setState({ connecting: null });
  }
}

export async function testProfile(id: string, skipOnce = false): Promise<void> {
  useBusy.setState({ testing: id });
  try {
    const down = await ensureLocalBroker(id);
    if (down) {
      await refresh();
      useApp.getState().toast({ kind: 'err', msg: down });
      return;
    }
    const once = takeSkipOnce(id) || skipOnce;
    const out = await engine().testProfile(id, once || undefined);
    if (out.kind === 'needsTrust') useTrust.getState().set(out.trust, 'test');
    await refresh();
  } catch (e) {
    useApp.getState().toast({ kind: 'err', msg: toMessage(e) });
  } finally {
    useBusy.setState({ testing: null });
  }
}

/**
 * Applies the choice of the certificate prompt (change set 6) and retries: pin the certificate, trust its CA, skip
 * verification for this attempt only (nothing saved), or always for this connection (saved in the profile).
 */
export async function trustAndRetry(mode: TrustMode): Promise<void> {
  const { prompt, action } = useTrust.getState();
  useTrust.getState().set(null, null);
  if (!prompt) return;
  try {
    await engine().trustCertificate(prompt.profileId, mode);
    await refresh();
  } catch (e) {
    useApp.getState().toast({ kind: 'err', msg: toMessage(e) });
    return;
  }
  const once = mode === 'skipOnce';
  if (action === 'test') await testProfile(prompt.profileId, once);
  else await connectProfile(prompt.profileId, true, once);
}

export async function disconnect(id: string): Promise<void> {
  await engine().disconnect(id);
  await refresh();
}

/** Deletes a profile; an open connection of it is disconnected first (change set 2). */
export async function deleteProfile(id: string): Promise<void> {
  const c = useApp.getState().connections.find((x) => x.id === id);
  if (c && c.state !== 'disconnected') await engine().disconnect(id);
  await engine().deleteProfile(id);
  await refresh();
  const st = useApp.getState();
  if (st.selectedProfileId === id || !st.profiles.some((p) => p.id === st.selectedProfileId)) st.selectProfile(st.profiles[0]?.id ?? null);
  if (st.activeConnId === id) useApp.setState({ activeConnId: st.connections.find((x) => x.state === 'connected')?.id ?? null });
}

/** Duplicates a profile and selects the copy. */
export async function duplicateProfile(id: string): Promise<void> {
  const d = await run(engine().duplicateProfile(id));
  if (!d) return;
  await useApp.getState().refreshProfiles();
  useApp.getState().selectProfile(d.id);
}

export async function saveProfile(p: Profile): Promise<Profile> {
  const saved = await engine().saveProfile(p);
  const profiles = useApp.getState().profiles;
  const i = profiles.findIndex((x) => x.id === saved.id);
  useApp.setState({ profiles: i >= 0 ? profiles.map((x) => (x.id === saved.id ? saved : x)) : [...profiles, saved] });
  return saved;
}

export function replaceProfile(p: Profile): void {
  const profiles = useApp.getState().profiles;
  useApp.setState({ profiles: profiles.map((x) => (x.id === p.id ? p : x)) });
}

/** Shows the outcome of a native save dialog as a toast. */
export function reportSave(out: SaveOutcome, t: (k: string, o?: Record<string, unknown>) => string): void {
  if (out.kind === 'saved') useApp.getState().toast({ kind: 'info', text: t('common.savedAs', { file: out.fileName }) });
  else if (out.kind === 'failed') useApp.getState().toast({ kind: 'err', msg: out.error });
}

export async function run<T>(p: Promise<T>): Promise<T | null> {
  try {
    return await p;
  } catch (e) {
    useApp.getState().toast({ kind: 'err', msg: toMessage(e) });
    return null;
  }
}

/** ⌘N and the native menu: opens Connections with a new profile. */
export function newConnection(): void {
  useApp.getState().setScreen('connect');
  setTimeout(() => window.dispatchEvent(new CustomEvent('mqttpanel:new-connection')), 0);
}

// Self-update of the direct macOS download (docs/UPDATER.md). Store builds never get here (AppInfo selfUpdate false).
import type { EngineClient, UpdateStatus } from '@mqttpanel/engine-client';
import i18next from '../i18n';
import { useApp } from '../store/app';

/** The toast "MQTT Panel 1.0.2 is available" with the button "Open Updates". */
export function announceUpdate(version: string): void {
  useApp.getState().toast({
    kind: 'info',
    msg: { key: 'updates.available', params: { version }, text: `MQTT Panel ${version} is available` },
    action: { label: i18next.t('updates.open'), run: () => useApp.getState().openSettings('updates'), testId: 'toast-open-updates' },
  });
}

/** At launch, once the window is up: loads the status and runs the automatic check (the app decides whether it is
 * on and due); silent unless an update is found. */
export async function startUpdates(eng: EngineClient): Promise<void> {
  try {
    useApp.setState({ update: await eng.updateStatus() });
    const st: UpdateStatus = await eng.updateCheck(false);
    useApp.setState({ update: st });
    if (st.state === 'available' && st.version) announceUpdate(st.version);
  } catch {
    /* the automatic check is silent when it fails */
  }
}

/** True while an update waits for the user (the dot on the Settings rail item). */
export function updateWaiting(u: UpdateStatus | null, later: string | null): boolean {
  if (!u?.selfUpdate) return false;
  if (u.state === 'ready') return true;
  return u.state === 'available' && u.version !== later;
}

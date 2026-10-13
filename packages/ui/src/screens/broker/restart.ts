// Restart prompt (owner, 13 Oct 2026; mockup 12c): a change that needs a restart asks at once, "Later" keeps it pending.
import type { BrokerStatus } from '@mqttpanel/engine-client';
import { engine } from '../../services';
import { useApp, type PendingChange } from '../../store/app';

/**
 * Call after a saved change that needs a restart (listener added, removed, switched, port, reachability; certificates).
 * When the running broker reports a pending restart, the change is remembered and the prompt opens. Users and
 * Anonymous apply live and never call this.
 */
export async function noteRestartChange(change: PendingChange): Promise<void> {
  let st: BrokerStatus;
  try {
    st = await engine().brokerStatus();
  } catch {
    return;
  }
  useApp.setState({ broker: st });
  if (st.state !== 'running' || !st.restartNeeded) return;
  const cur = useApp.getState().pendingChanges;
  // a second port change of the same listener replaces the first, keeping the original port
  const same = change.field === 'port' ? cur.find((c) => c.field === 'port' && c.listenerId === change.listenerId) : undefined;
  const next = same ? cur.map((c) => (c === same ? { ...change, params: { ...change.params, from: same.params.from } } : c)) : [...cur, change];
  useApp.setState({ pendingChanges: next, restartPrompt: true });
}

/** "Restart now" and the pulsing "Restart broker and apply changes". */
export async function restartBroker(): Promise<void> {
  useApp.setState({ restartPrompt: false });
  const st = await engine().brokerRestart();
  useApp.setState({ broker: st, pendingChanges: st.restartNeeded ? useApp.getState().pendingChanges : [] });
}

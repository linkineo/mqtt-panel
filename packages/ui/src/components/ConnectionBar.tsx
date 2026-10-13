import { useTranslation } from 'react-i18next';
import i18next from 'i18next';
import type { ConnectionStatus } from '@mqttpanel/engine-client';
import { useApp } from '../store/app';
import { platform } from '../services';
import { fmtNum, fmtRate } from '../lib/format';
import { disconnect, useBusy } from '../lib/actions';

/** Open connections: connected, connecting or reconnecting (the connection bar lists these). */
export function isOpen(c: ConnectionStatus): boolean {
  return c.state === 'connected' || c.state === 'connecting' || c.state === 'reconnecting';
}

/** Makes a connection the active one and opens the Explorer on it (connection bar, ⌘⇧L, command palette). */
export function openConnection(id: string): void {
  const st = useApp.getState();
  st.setActiveConn(id);
  st.setScreen('explore');
}

/** Opens the connection switcher (⌘⇧L). */
export async function switchConnectionMenu(x: number, y: number): Promise<void> {
  const st = useApp.getState();
  const open = st.connections.filter(isOpen);
  const t = i18next.t.bind(i18next);
  const items: { id: string; label: string; separator?: boolean }[] = open.map((c) => ({
    id: `use:${c.id}`,
    label: (c.id === st.activeConnId ? '✓ ' : '') + c.name,
  }));
  if (items.length) items.push({ id: 'sep', label: '', separator: true });
  const active = open.find((c) => c.id === st.activeConnId);
  if (active) items.push({ id: `disc:${active.id}`, label: t('pill.disconnect', { name: active.name }) });
  items.push({ id: 'go-connect', label: open.length ? t('pill.otherConnection') : t('pill.chooseConnection') });
  const id = await platform().showContextMenu(items, x, y);
  if (!id) return;
  if (id === 'go-connect') st.setScreen('connect');
  else if (id.startsWith('use:')) openConnection(id.slice(4));
  else if (id.startsWith('disc:')) await disconnect(id.slice(5));
}

/** Short live figures of a tab: "1 284 topics · 312 msg/s · MQTT 5", or the state while not connected. */
function figures(c: ConnectionStatus, t: (k: string, o?: Record<string, unknown>) => string): string {
  if (c.state === 'connecting') return t('conn.stateConnecting');
  if (c.state === 'reconnecting') return t('conn.stateReconnecting');
  const parts = [t('pill.topics', { count: c.topics, n: fmtNum(c.topics) }), t('pill.rate', { rate: fmtRate(c.msgRate) })];
  if (c.negotiatedVersion) parts.push(`MQTT ${c.negotiatedVersion}`);
  return parts.join(' · ');
}

/**
 * Connection bar (owner, 13 Oct 2026, change set 2): a tab per open connection under the title bar, with name, status
 * dot, short live figures and ✕ (disconnect). A click opens the Explorer on that connection. Hidden while nothing is
 * open. A connection still being set up (before the engine reports it) shows as "connecting".
 */
/** The tabs of the connection bar: the open connections, plus one being set up before the engine reports it. */
export function useOpenTabs(): ConnectionStatus[] {
  const connections = useApp((s) => s.connections);
  const profiles = useApp((s) => s.profiles);
  const busyId = useBusy((s) => s.connecting);
  const open = connections.filter(isOpen);
  if (busyId && !open.some((c) => c.id === busyId)) {
    const p = profiles.find((x) => x.id === busyId);
    if (p) open.push({ id: p.id, name: p.name, state: 'connecting', negotiatedVersion: null, requestedVersion: p.mqttVersion, topics: 0, msgRate: 0, skipVerify: false, error: null, reconnectAttempt: 0, maxPacketSize: null, isLocalBroker: p.isLocalBroker, subscriptionErrors: [] });
  }
  return open;
}

export function ConnectionBar({ tabs: open }: { tabs: ConnectionStatus[] }) {
  const { t } = useTranslation();
  const activeId = useApp((s) => s.activeConnId);
  if (!open.length) return null;
  return (
    <div className="cbar" role="tablist" aria-label={t('cbar.label')} data-testid="cbar">
      {open.map((c) => {
        const on = c.id === activeId;
        const dot = c.state === 'connected' ? '' : ' warn';
        return (
          <div
            key={c.id}
            role="tab"
            tabIndex={on ? 0 : -1}
            aria-selected={on}
            className={`tab${on ? ' on' : ''}`}
            data-testid="cbar-tab"
            data-conn={c.id}
            data-state={c.state}
            title={c.name}
            onClick={() => openConnection(c.id)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                openConnection(c.id);
              } else if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
                e.preventDefault();
                const tabs = Array.from((e.currentTarget.parentElement as HTMLElement).querySelectorAll<HTMLElement>('[role="tab"]'));
                const i = tabs.indexOf(e.currentTarget);
                tabs[(i + (e.key === 'ArrowRight' ? 1 : tabs.length - 1)) % tabs.length]?.focus();
              }
            }}
          >
            <span className={`dot${dot}`} />
            <span className="nm">{c.name}</span>
            <small>· {figures(c, t)}</small>
            <button
              type="button"
              className="x"
              aria-label={t('cbar.close', { name: c.name })}
              title={t('cbar.close', { name: c.name })}
              data-testid="cbar-close"
              onClick={(e) => {
                e.stopPropagation();
                void disconnect(c.id);
              }}
            >
              ✕
            </button>
          </div>
        );
      })}
    </div>
  );
}

/**
 * The title bar pill (change set 2): the embedded broker's state ("Local broker · running · 7 clients", "anonymous",
 * "restart pending") and the lab flags (skip verification). With nothing to show it reads "Not connected"; while
 * connections are open and there is nothing to show, it is hidden (the connection bar shows them, mockup 04).
 */
export function TitlePill() {
  const { t } = useTranslation();
  const screen = useApp((s) => s.screen);
  const broker = useApp((s) => s.broker);
  const brokerConfig = useApp((s) => s.brokerConfig);
  const skip = useApp((s) => s.settings.skipVerify);
  const connections = useApp((s) => s.connections);
  const anyOpen = connections.some(isOpen);
  const connSkip = connections.some((c) => isOpen(c) && c.skipVerify);

  const parts: string[] = [];
  let dot = 'off';
  let brokerShown = false;
  if (broker && (broker.state !== 'stopped' || screen === 'broker')) {
    brokerShown = true;
    parts.push(t('pill.localBroker'));
    if (broker.state === 'running') {
      parts.push(t('pill.running'));
      if (brokerConfig?.anonymous) parts.push(t('pill.anonymous'));
      // mockup 12c: "Local broker · running · restart pending" (the pending restart replaces the client count)
      if (broker.restartNeeded) parts.push(t('pill.restartPending'));
      else parts.push(t('pill.clients', { count: broker.clients, n: fmtNum(broker.clients) }));
      dot = brokerConfig?.anonymous ? 'warn' : 'on';
    } else if (broker.state === 'starting') {
      parts.push(t('pill.starting'));
      dot = 'warn';
    } else if (broker.state === 'failed') {
      parts.push(t('pill.failed'));
      dot = 'err';
    } else parts.push(t('pill.stopped'));
  }
  if (skip || connSkip) {
    parts.push(t('pill.skipVerify'));
    dot = 'warn';
  }
  if (!parts.length) {
    if (anyOpen) return <span data-testid="pill-none" />;
    parts.push(t('pill.notConnected'));
  }
  const text = parts.join(' · ');
  return (
    <button
      type="button"
      className="conn"
      data-testid="pill"
      aria-label={t('pill.aria', { status: text })}
      onClick={() => useApp.getState().setScreen(brokerShown ? 'broker' : 'connect')}
    >
      <span className={`dot${dot === 'on' ? '' : ' ' + dot}`} />
      <span className="ell">{text}</span>
    </button>
  );
}

import { useTranslation } from 'react-i18next';
import i18next from 'i18next';
import type { ConnectionStatus, Profile } from '@mqttpanel/engine-client';
import { useApp } from '../store/app';
import { platform } from '../services';
import { fmtNum, fmtRate } from '../lib/format';
import { disconnect, useBusy } from '../lib/actions';
import { newConnectionMenu } from '../screens/connections/ConnectionsScreen';

/** Open connections: connected, connecting or reconnecting (the connection bar lists these). */
export function isOpen(c: ConnectionStatus): boolean {
  return c.state === 'connected' || c.state === 'connecting' || c.state === 'reconnecting';
}

/**
 * The ⚠ icon of a card and a tab (change set 6): 'skip' (clay) when this connection accepts any broker certificate,
 * saved in the profile or "only this time" on the open connection; 'pinned' (muted) when the profile trusts a
 * certificate by its fingerprint; null otherwise or on an unencrypted protocol.
 */
export function certWarning(c: ConnectionStatus | null | undefined, p: Profile | null | undefined): 'skip' | 'pinned' | null {
  if (!p || (p.scheme !== 'mqtts' && p.scheme !== 'wss')) return null;
  if (p.tls.skipVerify || (c && isOpen(c) && c.skipVerify)) return 'skip';
  if (p.tls.trustedFingerprints.length > 0) return 'pinned';
  return null;
}

/** The ⚠ icon with its tooltip (also its accessible name). */
export function CertWarning({ kind, testId }: { kind: 'skip' | 'pinned' | null; testId: string }) {
  const { t } = useTranslation();
  if (!kind) return null;
  const text = t(kind === 'skip' ? 'conn.warnSkip' : 'conn.warnPinned');
  return (
    <span className={`warn-i${kind === 'pinned' ? ' pin' : ''}`} role="img" title={text} aria-label={text} data-testid={testId} data-kind={kind}>
      ⚠
    </span>
  );
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
  if (active) items.push({ id: `disc:${active.id}`, label: t('cbar.close', { name: active.name }) });
  items.push({ id: 'go-connect', label: open.length ? t('cbar.other') : t('cbar.choose') });
  const id = await platform().showContextMenu(items, x, y);
  if (!id) return;
  if (id === 'go-connect') st.setScreen('connect');
  else if (id.startsWith('use:')) openConnection(id.slice(4));
  else if (id.startsWith('disc:')) await disconnect(id.slice(5));
}

/** Short live figures of a tab: "1 284 topics · 312 msg/s · MQTT 5", or the state while not connected. */
function figures(c: ConnectionStatus, t: (k: string, o?: Record<string, unknown>) => string, startingBroker: boolean): string {
  if (startingBroker) return t('conn.stateStartingBroker');
  if (c.state === 'connecting') return t('conn.stateConnecting');
  if (c.state === 'reconnecting') return t('conn.stateReconnecting');
  const parts = [t('cbar.topics', { count: c.topics, n: fmtNum(c.topics) }), t('cbar.rate', { rate: fmtRate(c.msgRate) })];
  if (c.negotiatedVersion) parts.push(`MQTT ${c.negotiatedVersion}`);
  return parts.join(' · ');
}

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

/** Opens the built-in broker dialog of the Connections screen ("+" of the empty bar). */
function openAppBroker(d: 'pick' | 'add' | null): void {
  if (!d) return;
  useApp.getState().setScreen('connect');
  setTimeout(() => window.dispatchEvent(new CustomEvent('mqttpanel:app-broker', { detail: d })), 0);
}

/**
 * Connection bar (owner, 13 Oct 2026, change set 2): a tab per open connection, with name, status dot, short live
 * figures and ✕ (disconnect). A click opens the Explorer on that connection. A connection still being set up (before
 * the engine reports it) shows as "connecting". Change set 8: always shown, as the first row of the main column to
 * the right of the rail; with nothing open it reads "Choose a connection" and ends with a "+" that opens the New
 * connection menu.
 */
export function ConnectionBar({ tabs: open }: { tabs: ConnectionStatus[] }) {
  const { t } = useTranslation();
  const activeId = useApp((s) => s.activeConnId);
  const startingId = useBusy((s) => s.startingBroker);
  const profiles = useApp((s) => s.profiles);
  if (!open.length) {
    return (
      <div className="cbar empty" data-testid="cbar" data-empty="true">
        <span className="hint" data-testid="cbar-hint">
          {t('cbar.empty')}
        </span>
        <button
          type="button"
          className="plus"
          aria-label={t('conn.new')}
          title={t('conn.new')}
          aria-haspopup="menu"
          data-testid="cbar-new"
          onClick={(e) => void newConnectionMenu(e.currentTarget, t, openAppBroker)}
        >
          +
        </button>
      </div>
    );
  }
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
            <small>· {figures(c, t, startingId === c.id)}</small>
            <CertWarning kind={certWarning(c, profiles.find((p) => p.id === c.id))} testId="cbar-warn" />
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

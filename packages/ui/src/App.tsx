import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useApp, isMac, SCREENS, type Screen } from './store/app';
import { useExplorer } from './store/explorer';
import { RailIcon, AppMark } from './components/icons';
import { Toasts } from './components/ui';
import { ConnectionBar, useOpenTabs } from './components/ConnectionBar';
import { TrustDialog } from './components/TrustDialog';
import { CommandPalette } from './components/CommandPalette';
import { FieldPicker } from './screens/explorer/FieldPicker';
import { ConnectionsScreen } from './screens/connections/ConnectionsScreen';
import { ExplorerScreen } from './screens/explorer/ExplorerScreen';
import { PublishScreen } from './screens/publish/PublishScreen';
import { ChartsScreen } from './screens/charts/ChartsScreen';
import { BrokerScreen } from './screens/broker/BrokerScreen';
import { SettingsScreen } from './screens/settings/SettingsScreen';
import { modHeld } from './lib/keys';
import { switchConnectionMenu } from './components/ConnectionBar';
import { newConnection } from './lib/actions';

/**
 * The rail (change set 4: state lives here instead of a title bar pill): the Broker icon carries a small play badge
 * while the embedded broker runs (clay while it starts), the Connect icon a count bubble with the number of open
 * connections (connected, connecting, reconnecting), nothing at zero.
 */
function Rail() {
  const { t } = useTranslation();
  const screen = useApp((s) => s.screen);
  const setScreen = useApp((s) => s.setScreen);
  const brokerState = useApp((s) => s.broker?.state ?? 'stopped');
  const open = useOpenTabs().length;
  const badge = (k: Screen) => {
    if (k === 'broker' && (brokerState === 'running' || brokerState === 'starting')) {
      return <span className={`badge${brokerState === 'starting' ? ' starting' : ''}`} data-testid="rail-broker-badge" data-state={brokerState} aria-hidden="true" />;
    }
    if (k === 'connect' && open > 0) {
      return (
        <span className="cnt" data-testid="rail-connect-count" aria-hidden="true">
          {open}
        </span>
      );
    }
    return null;
  };
  const label = (k: Screen) => {
    if (k === 'broker' && brokerState === 'running') return `${t('rail.broker')}, ${t('rail.brokerRunning')}`;
    if (k === 'broker' && brokerState === 'starting') return `${t('rail.broker')}, ${t('rail.brokerStarting')}`;
    if (k === 'connect' && open > 0) return `${t('rail.connect')}, ${t('rail.openCount', { count: open })}`;
    return undefined;
  };
  const item = (k: Screen) => (
    <button
      key={k}
      type="button"
      className={screen === k ? 'on' : undefined}
      aria-current={screen === k ? 'page' : undefined}
      aria-label={label(k)}
      data-testid={`rail-${k}`}
      onClick={() => setScreen(k)}
    >
      <RailIcon name={k} />
      {t(`rail.${k}`)}
      {badge(k)}
    </button>
  );
  return (
    <nav className="rail" aria-label={t('rail.label')}>
      {(['connect', 'explore', 'publish', 'charts', 'broker'] as Screen[]).map(item)}
      <span className="sp" />
      {item('settings')}
    </nav>
  );
}

function TitleBar() {
  const info = useApp((s) => s.appInfo);
  const mac = info.native && info.platform === 'macos';
  return (
    <div className={`tb${mac ? ' mac' : ''}`} data-tauri-drag-region>
      <span className="name" data-tauri-drag-region>
        MQTT Panel
      </span>
      <span data-tauri-drag-region />
      <span className="mark" data-tauri-drag-region>
        <AppMark />
      </span>
    </div>
  );
}

function CurrentScreen() {
  const screen = useApp((s) => s.screen);
  switch (screen) {
    case 'connect':
      return <ConnectionsScreen />;
    case 'explore':
      return <ExplorerScreen />;
    case 'publish':
      return <PublishScreen />;
    case 'charts':
      return <ChartsScreen />;
    case 'broker':
      return <BrokerScreen />;
    case 'settings':
      return <SettingsScreen />;
  }
}

/** Global shortcuts (S2): ⌘1..⌘5 screens, ⌘, settings, ⌘K palette, ⌘N new connection, ⌘E export, ⌘⇧L switch connection. */
function useGlobalShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const st = useApp.getState();
      const mod = modHeld(st.appInfo, e);
      if (!mod) return;
      const k = e.key.toLowerCase();
      if (k === 'k') {
        e.preventDefault();
        st.setPalette(!st.paletteOpen);
      } else if (k >= '1' && k <= '5' && !e.shiftKey) {
        e.preventDefault();
        st.setScreen(SCREENS[Number(k) - 1]);
      } else if (k === ',') {
        e.preventDefault();
        st.setScreen('settings');
      } else if (k === 'n' && !e.shiftKey) {
        e.preventDefault();
        newConnection();
      } else if (k === 'e' && !e.shiftKey) {
        e.preventDefault();
        if (st.screen !== 'explore') st.setScreen('explore');
        useExplorer.getState().run('exportSelected');
      } else if (k === 'l' && e.shiftKey) {
        e.preventDefault();
        void switchConnectionMenu(window.innerWidth / 2, 44);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
}


/** Native menu items (EngineEvent 'menu'). */
function useMenuEvents() {
  useEffect(
    () =>
      useApp.getState().onMenu((id) => {
        const st = useApp.getState();
        const go: Record<string, Screen> = {
          'go-connect': 'connect',
          'go-explore': 'explore',
          'go-publish': 'publish',
          'go-charts': 'charts',
          'go-broker': 'broker',
          'open-settings': 'settings',
        };
        if (go[id]) st.setScreen(go[id]);
        else if (id === 'new-connection') newConnection();
        else if (id === 'command-palette') st.setPalette(true);
        else if (id === 'find') {
          st.setScreen('explore');
          useExplorer.getState().run('focusSearch');
        } else if (id === 'export') {
          st.setScreen('explore');
          useExplorer.getState().run('exportSelected');
        } else if (id === 'clear-history') {
          st.setScreen('explore');
          useExplorer.getState().run('clearHistory');
        } else if (id === 'about') {
          st.setSettingsPage('about');
          st.setScreen('settings');
        }
      }),
    [],
  );
}

export function App() {
  const ready = useApp((s) => s.ready);
  const palette = useApp((s) => s.paletteOpen);
  const info = useApp((s) => s.appInfo);
  const tabs = useOpenTabs();
  const withBar = tabs.length > 0;
  useGlobalShortcuts();
  useMenuEvents();
  useEffect(() => {
    // keep text cursor and selection off chrome, prevent default drag images
    const onDrag = (e: DragEvent) => {
      if (!(e.target as HTMLElement)?.closest?.('[data-draggable]')) e.preventDefault();
    };
    window.addEventListener('dragstart', onDrag);
    return () => window.removeEventListener('dragstart', onDrag);
  }, []);
  if (!ready) return null;
  return (
    <div className={`app${withBar ? ' with-bar' : ''}`} id="app-root" data-mac={isMac(info) ? 'true' : 'false'}>
      <TitleBar />
      <ConnectionBar tabs={tabs} />
      <div className="body">
        <Rail />
        <main className="main">
          <CurrentScreen />
        </main>
      </div>
      <TrustDialog />
      <FieldPicker />
      {palette && <CommandPalette />}
      <Toasts />
    </div>
  );
}

import { useTranslation } from 'react-i18next';
import i18next from 'i18next';
import { useApp } from '../store/app';
import { platform } from '../services';
import { fmtNum, fmtRate } from '../lib/format';
import { disconnect, useBusy, useTrust } from '../lib/actions';

/** Opens the connection switcher (title bar pill, ⌘⇧L). */
export async function switchConnectionMenu(x: number, y: number): Promise<void> {
  const st = useApp.getState();
  const open = st.connections.filter((c) => c.state !== 'disconnected');
  const t = i18next.t.bind(i18next);
  const items: { id: string; label: string; separator?: boolean }[] = open.map((c) => ({
    id: `use:${c.id}`,
    label: (c.id === st.activeConnId ? '✓ ' : '') + c.name,
  }));
  if (items.length) items.push({ id: 'sep', label: '', separator: true });
  const active = open.find((c) => c.id === st.activeConnId);
  if (active) items.push({ id: `disc:${active.id}`, label: t('pill.disconnect', { name: active.name }) });
  items.push({ id: 'go-connect', label: open.length ? t('pill.otherConnection') : t('pill.chooseConnection') });
  const list = items;
  const id = await platform().showContextMenu(list, x, y);
  if (!id) return;
  if (id === 'go-connect') st.setScreen('connect');
  else if (id.startsWith('use:')) st.setActiveConn(id.slice(4));
  else if (id.startsWith('disc:')) await disconnect(id.slice(5));
}

export function TitlePill() {
  const { t } = useTranslation();
  const screen = useApp((s) => s.screen);
  const conn = useApp((s) => s.connections.find((c) => c.id === s.activeConnId) ?? null);
  const broker = useApp((s) => s.broker);
  const brokerConfig = useApp((s) => s.brokerConfig);
  const skip = useApp((s) => s.settings.skipVerify);
  const busyId = useBusy((s) => s.connecting);
  const trustId = useTrust((s) => s.prompt?.profileId ?? null);
  const pendingId = busyId ?? trustId;
  const pendingName = useApp((s) => (pendingId ? (s.profiles.find((p) => p.id === pendingId)?.name ?? null) : null));

  let dot = 'off';
  let text: string;
  if (screen === 'broker' && broker) {
    const parts = [t('pill.localBroker')];
    if (broker.state === 'running') {
      parts.push(t('pill.running'));
      if (brokerConfig?.anonymous) parts.push(t('pill.anonymous'));
      parts.push(t('pill.clients', { count: broker.clients, n: fmtNum(broker.clients) }));
      dot = brokerConfig?.anonymous ? 'warn' : 'on';
    } else if (broker.state === 'starting') {
      parts.push(t('pill.starting'));
      dot = 'warn';
    } else if (broker.state === 'failed') {
      parts.push(t('pill.failed'));
      dot = 'err';
    } else parts.push(t('pill.stopped'));
    text = parts.join(' · ');
  } else if (pendingName && (!conn || conn.id !== pendingId || conn.state !== 'connected')) {
    text = `${pendingName} · ${t('pill.connecting')}`;
    dot = 'warn';
  } else if (conn) {
    const parts = [conn.name];
    if (conn.state === 'connecting') {
      parts.push(t('pill.connecting'));
      dot = 'warn';
    } else if (conn.state === 'reconnecting') {
      parts.push(t('pill.reconnecting'));
      dot = 'warn';
    } else if (conn.state === 'failed') {
      parts.push(t('pill.failed'));
      dot = 'err';
    } else if (conn.state === 'disconnected') {
      parts.push(t('pill.disconnected'));
    } else {
      dot = 'on';
      parts.push(t('pill.topics', { count: conn.topics, n: fmtNum(conn.topics) }));
      parts.push(t('pill.rate', { rate: fmtRate(conn.msgRate) }));
      if (conn.negotiatedVersion) parts.push(`MQTT ${conn.negotiatedVersion}`);
    }
    if (skip || conn.skipVerify) {
      parts.push(t('pill.skipVerify'));
      dot = 'warn';
    }
    text = parts.join(' · ');
  } else {
    text = t('pill.notConnected');
    if (skip) {
      text += ' · ' + t('pill.skipVerify');
      dot = 'warn';
    }
  }
  return (
    <button
      type="button"
      className="conn"
      data-testid="pill"
      aria-label={t('pill.aria', { status: text })}
      onClick={(e) => {
        const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
        void switchConnectionMenu(r.left, r.bottom + 4);
      }}
    >
      <span className={`dot${dot === 'on' ? '' : ' ' + dot}`} />
      <span className="ell">{text}</span>
    </button>
  );
}

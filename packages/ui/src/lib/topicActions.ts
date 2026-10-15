import i18next from 'i18next';
import type { MessageView, Profile, PayloadEncoding } from '@mqttpanel/engine-client';
import { engine, platform } from '../services';
import { useApp } from '../store/app';
import { useCharts } from '../store/charts';
import { useExplorer } from '../store/explorer';
import { run } from './actions';
import { setSparklineOff, topicPrefsOf } from './topicPrefs';

/** Quotes a string for a POSIX shell: 'it'\''s'. */
export function shq(s: string): string {
  return `'${s.replace(/'/g, `'\\''`)}'`;
}

/** `mqttpanel pub -h <host> -p <port> -t '<topic>' -m '<payload>' [-q n] [-r]` for a profile. */
export function cliCommand(profile: Pick<Profile, 'host' | 'port'> | null, topic: string, payload: string, qos: number, retain: boolean): string {
  const parts = ['mqttpanel pub'];
  if (profile) parts.push(`-h ${profile.host}`, `-p ${profile.port}`);
  parts.push(`-t ${shq(topic)}`, `-m ${shq(payload)}`);
  if (qos) parts.push(`-q ${qos}`);
  if (retain) parts.push('-r');
  return parts.join(' ');
}

/** The payload as text, as a user would type it again. */
export function payloadText(m: MessageView | null): string {
  if (!m) return '';
  const d = m.decoded;
  if (d.format === 'json' && d.json !== null) return JSON.stringify(d.json);
  if (d.text !== null) return d.text;
  if (d.base64 !== null) return d.base64;
  return '';
}

export function payloadFormat(m: MessageView | null): PayloadEncoding {
  if (!m) return 'text';
  const d = m.decoded;
  if (d.detected === 'json') return 'json';
  if (d.detected === 'text') return 'text';
  return 'base64';
}

function toast(text: string) {
  useApp.getState().toast({ kind: 'info', text });
}

export async function copyTopic(topic: string) {
  await platform().copyText(topic);
  toast(i18next.t('common.copied'));
}

export async function copyPayload(connId: string, topic: string) {
  const d = await run(engine().topicDetail(connId, topic));
  await platform().copyText(payloadText(d?.last ?? null));
  toast(i18next.t('common.copied'));
}

export async function copyCli(connId: string, topic: string) {
  const d = await run(engine().topicDetail(connId, topic));
  const profile = useApp.getState().profiles.find((p) => p.id === connId) ?? null;
  const m = d?.last ?? null;
  await platform().copyText(cliCommand(profile, topic, payloadText(m), m?.qos ?? 0, m?.retain ?? false));
  toast(i18next.t('common.copied'));
}

export async function publishHere(connId: string, topic: string) {
  const d = await run(engine().topicDetail(connId, topic));
  useApp.getState().prefillPublish(topic, payloadText(d?.last ?? null), payloadFormat(d?.last ?? null));
}

/** "Add to chart" from the Explorer (context menu, click menu, "+ Add field…"): edits the topic's own chart. */
export function addToChart(connId: string, topic: string, preselect: string | null = null) {
  useCharts.getState().openPicker({ connId, topic, preselect, chartId: null, mode: 'topic' });
}

export async function clearHistory(connId: string, prefix: string | null) {
  await run(engine().clearHistory(connId, prefix));
  toast(i18next.t('explore.historyCleared'));
}

export type TopicDialog =
  | { kind: 'deleteRetained'; topic: string }
  | { kind: 'deleteSubtree'; topic: string }
  | { kind: 'export'; topic: string; scope: 'topic' | 'subtree' | 'visible' };

/** The topic context menu (mockup 04), shown natively. */
export async function topicMenu(
  connId: string,
  topic: string,
  isTopic: boolean,
  x: number,
  y: number,
  mac: boolean,
  openDialog: (d: TopicDialog) => void,
): Promise<void> {
  const t = i18next.t.bind(i18next);
  const pinned = useExplorer.getState().get(connId).pinned.includes(topic);
  const sparkOff = isTopic && !!topicPrefsOf(connId, topic).sparklineOff;
  const id = await platform().showContextMenu(
    [
      { id: 'pin', label: pinned ? t('menu.unpin') : t('menu.pin'), accelerator: 'P' },
      { id: 'publish', label: t('menu.publishHere'), accelerator: '⇧P' },
      { id: 'chart', label: t('menu.addToChart'), accelerator: 'C', enabled: isTopic },
      // change set 9: a hidden sparkline cannot be right-clicked, so it comes back from here
      ...(sparkOff ? [{ id: 'showSpark', label: t('menu.showSparkline') }] : []),
      { id: 's1', label: '', separator: true },
      { id: 'copyTopic', label: t('menu.copyTopic'), accelerator: mac ? '⌘C' : 'Ctrl+C' },
      { id: 'copyPayload', label: t('menu.copyPayload'), enabled: isTopic },
      { id: 'copyCli', label: t('menu.copyCli'), enabled: isTopic },
      { id: 'export', label: t('menu.exportSubtree') },
      { id: 's2', label: '', separator: true },
      { id: 'delRet', label: t('menu.deleteRetained'), danger: true, enabled: isTopic },
      { id: 'delSub', label: t('menu.deleteRetainedSubtree'), danger: true },
      { id: 'clear', label: t('menu.clearHistory') },
    ],
    x,
    y,
  );
  switch (id) {
    case 'pin':
      return useExplorer.getState().togglePin(connId, topic);
    case 'publish':
      return publishHere(connId, topic);
    case 'chart':
      return addToChart(connId, topic);
    case 'showSpark':
      return setSparklineOff(connId, topic, false);
    case 'copyTopic':
      return copyTopic(topic);
    case 'copyPayload':
      return copyPayload(connId, topic);
    case 'copyCli':
      return copyCli(connId, topic);
    case 'export':
      return openDialog({ kind: 'export', topic, scope: 'subtree' });
    case 'delRet':
      return openDialog({ kind: 'deleteRetained', topic });
    case 'delSub':
      return openDialog({ kind: 'deleteSubtree', topic });
    case 'clear':
      return clearHistory(connId, topic);
  }
}

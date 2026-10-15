import type { ChartField, TopicPrefs } from '@mqttpanel/engine-client';
import { useApp } from '../store/app';
import { COLORS, type ChartColor } from '../store/charts';
import { saveProfile } from './actions';

/**
 * What the profile remembers per topic (change set 8): the value picked with "Use as sparkline" and the fields of the
 * topic's own chart under the payload. Both are saved with the profile, so they survive a restart.
 */
const NONE: TopicPrefs = {};

export function topicPrefsOf(connId: string, topic: string): TopicPrefs {
  const p = useApp.getState().profiles.find((x) => x.id === connId);
  return p?.topics?.[topic] ?? NONE;
}

export function useTopicPrefs(connId: string, topic: string): TopicPrefs {
  return useApp((s) => s.profiles.find((x) => x.id === connId)?.topics?.[topic] ?? NONE);
}

async function update(connId: string, topic: string, f: (prefs: TopicPrefs) => TopicPrefs): Promise<void> {
  const p = useApp.getState().profiles.find((x) => x.id === connId);
  if (!p) return;
  const topics = { ...(p.topics ?? {}) };
  const next = f({ ...(topics[topic] ?? {}) });
  if (!next.sparklinePath) delete next.sparklinePath;
  if (!next.chartFields?.length) delete next.chartFields;
  if (!next.sparklineOff) delete next.sparklineOff;
  if (Object.keys(next).length) topics[topic] = next;
  else delete topics[topic];
  await saveProfile({ ...p, topics });
}

/** "Use as sparkline": one per topic, a new pick replaces the previous one and shows a hidden sparkline again. */
export function setSparklinePath(connId: string, topic: string, path: string | null): Promise<void> {
  return update(connId, topic, (x) => ({ ...x, sparklinePath: path ?? undefined, sparklineOff: false }));
}

/** Change set 9: "Hide sparkline" (right-click on a sparkline) and "Show sparkline" (topic context menu). */
export function setSparklineOff(connId: string, topic: string, off: boolean): Promise<void> {
  return update(connId, topic, (x) => ({ ...x, sparklineOff: off }));
}

export function setChartFields(connId: string, topic: string, fields: ChartField[]): Promise<void> {
  return update(connId, topic, (x) => ({ ...x, chartFields: fields }));
}

/** The chart fields for the checked paths: the fields kept stay in place with their settings, new ones follow. */
export function withFields(existing: ChartField[], items: { path: string; step: boolean }[]): ChartField[] {
  const kept = existing.filter((f) => items.some((it) => it.path === f.path));
  const used = new Set<ChartColor>(kept.map((f) => f.colour));
  const added = items
    .filter((it) => !kept.some((f) => f.path === it.path))
    .map((it, i): ChartField => {
      const colour = COLORS.find((c) => !used.has(c)) ?? COLORS[(kept.length + i) % COLORS.length];
      used.add(colour);
      return { path: it.path, colour, style: it.step ? 'step' : 'line', axis: 'left', unit: '' };
    });
  return [...kept, ...added];
}

import { isOpen, openConnection } from './TitlePill';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { engine } from '../services';
import { useApp, type Screen } from '../store/app';
import { useExplorer } from '../store/explorer';
import { connectProfile, newConnection } from '../lib/actions';
import { shortcut } from '../lib/keys';
import { addToChart, publishHere } from '../lib/topicActions';

interface Item {
  id: string;
  group: 'commands' | 'topics' | 'connections';
  label: string;
  keys?: string;
  run(): void;
}

/** Fuzzy match: every query character in order; lower score is better. */
export function fuzzyScore(text: string, q: string): number | null {
  if (!q) return 0;
  const s = text.toLowerCase();
  const query = q.toLowerCase();
  const direct = s.indexOf(query);
  if (direct >= 0) return direct;
  let i = 0;
  let score = 100;
  let last = -1;
  for (const ch of query) {
    const j = s.indexOf(ch, i);
    if (j < 0) return null;
    score += j - last - 1;
    last = j;
    i = j + 1;
  }
  return score;
}

/** Mockup 18: ⌘K / Ctrl+K lists commands, topics and connections together; keyboard only. */
export function CommandPalette() {
  const { t } = useTranslation();
  const info = useApp((s) => s.appInfo);
  const profiles = useApp((s) => s.profiles);
  const connections = useApp((s) => s.connections);
  const activeId = useApp((s) => s.activeConnId);
  const selected = useExplorer((s) => (activeId ? s.get(activeId).selected : null));
  const [q, setQ] = useState('');
  const [i, setI] = useState(0);
  const [topics, setTopics] = useState<string[]>([]);
  const input = useRef<HTMLInputElement>(null);
  const close = () => useApp.getState().setPalette(false);
  const go = (s: Screen) => useApp.getState().setScreen(s);

  useEffect(() => input.current?.focus(), []);
  useEffect(() => {
    if (!activeId || q.length < 2) {
      setTopics([]);
      return;
    }
    const h = setTimeout(() => {
      void engine()
        .knownTopics(activeId, q, 20)
        .then(setTopics)
        .catch(() => setTopics([]));
    }, 80);
    return () => clearTimeout(h);
  }, [q, activeId]);

  const items = useMemo(() => {
    const out: Item[] = [];
    const cmd = (id: string, label: string, keys: string | undefined, run: () => void) => out.push({ id, group: 'commands', label, keys, run });
    if (activeId && selected) {
      cmd('pubsel', t('palette.publishTo', { topic: selected }), shortcut(info, 'shift+P'), () => void publishHere(activeId, selected));
      cmd('pinsel', t('palette.pin', { topic: selected }), 'P', () => void useExplorer.getState().togglePin(activeId, selected));
      cmd('chartsel', t('palette.chart', { topic: selected }), 'C', () => addToChart(activeId, selected));
    }
    cmd('connect', t('palette.open', { screen: t('rail.connect') }), shortcut(info, 'mod+1'), () => go('connect'));
    cmd('explore', t('palette.open', { screen: t('rail.explore') }), shortcut(info, 'mod+2'), () => go('explore'));
    cmd('publish', t('palette.open', { screen: t('rail.publish') }), shortcut(info, 'mod+3'), () => go('publish'));
    cmd('charts', t('palette.open', { screen: t('rail.charts') }), shortcut(info, 'mod+4'), () => go('charts'));
    cmd('broker', t('palette.open', { screen: t('rail.broker') }), shortcut(info, 'mod+5'), () => go('broker'));
    cmd('settings', t('palette.open', { screen: t('rail.settings') }), shortcut(info, 'mod+,'), () => go('settings'));
    if (activeId) {
      cmd('pause', useExplorer.getState().get(activeId).follow ? t('palette.pause') : t('palette.resume'), t('palette.space'), () => {
        go('explore');
        useExplorer.getState().run('togglePause');
      });
      cmd('next', t('palette.next'), 'J', () => {
        go('explore');
        useExplorer.getState().run('nextChanged');
      });
      cmd('find', t('palette.find'), '/', () => {
        go('explore');
        setTimeout(() => useExplorer.getState().run('focusSearch'), 50);
      });
      cmd('export', selected ? t('palette.exportSubtree') : t('palette.export'), shortcut(info, 'mod+E'), () => {
        go('explore');
        useExplorer.getState().run('exportSelected');
      });
    }
    cmd('new', t('palette.newConnection'), shortcut(info, 'mod+N'), () => newConnection());
    // "Switch connection" lists the open connections of the connection bar and opens the Explorer on the chosen one
    for (const c of connections.filter((c) => isOpen(c) && c.id !== activeId)) {
      out.push({ id: `sw:${c.id}`, group: 'connections', label: t('palette.switchTo', { name: c.name }), keys: shortcut(info, 'mod+shift+L'), run: () => openConnection(c.id) });
    }
    for (const p of profiles.filter((p) => !connections.some((c) => c.id === p.id && isOpen(c)))) {
      out.push({ id: `co:${p.id}`, group: 'connections', label: t('palette.connectTo', { name: p.name }), run: () => void connectProfile(p.id) });
    }
    for (const topic of topics) {
      out.push({
        id: `t:${topic}`,
        group: 'topics',
        label: topic,
        keys: t('palette.goTo'),
        run: () => {
          if (!activeId) return;
          useExplorer.getState().select(activeId, topic);
          go('explore');
          void engine().revealTopic(activeId, topic);
        },
      });
    }
    const scored = out
      .map((x) => ({ x, s: x.group === 'topics' ? 50 : fuzzyScore(x.label, q) }))
      .filter((y) => y.s !== null) as { x: Item; s: number }[];
    scored.sort((a, b) => a.s - b.s);
    return scored.map((y) => y.x);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [q, topics, info, profiles, connections, activeId, selected, t]);

  useEffect(() => setI(0), [q]);
  const choose = (it: Item | undefined) => {
    if (!it) return;
    close();
    it.run();
  };

  return (
    <>
      <div className="overlay" style={{ background: 'rgba(29,29,27,.12)' }} onMouseDown={close} />
      <div className="palette" role="dialog" aria-label={t('palette.label')} data-testid="palette">
        <div className="in">
          <span className="muted">›</span>
          <input
            ref={input}
            value={q}
            placeholder={t('palette.placeholder')}
            aria-label={t('palette.placeholder')}
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-rows"
            aria-activedescendant={items[i] ? `pal-${i}` : undefined}
            data-testid="palette-input"
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Escape') {
                e.preventDefault();
                close();
              } else if (e.key === 'ArrowDown') {
                e.preventDefault();
                setI(Math.min(items.length - 1, i + 1));
              } else if (e.key === 'ArrowUp') {
                e.preventDefault();
                setI(Math.max(0, i - 1));
              } else if (e.key === 'Enter') {
                e.preventDefault();
                choose(items[i]);
              }
            }}
          />
          <span className="kbd">esc</span>
        </div>
        <div className="rows" id="palette-rows" role="listbox">
          {items.map((it, j) => (
            <div
              key={it.id}
              id={`pal-${j}`}
              role="option"
              aria-selected={j === i}
              className={`prow${j === i ? ' on' : ''}`}
              data-testid="palette-row"
              onMouseEnter={() => setI(j)}
              onMouseDown={(e) => {
                e.preventDefault();
                choose(it);
              }}
            >
              <span className={it.group === 'topics' ? 'mono' : undefined}>{it.label}</span>
              <span>{it.keys ?? ''}</span>
            </div>
          ))}
          {items.length === 0 && <div className="prow muted">{t('palette.nothing')}</div>}
        </div>
      </div>
    </>
  );
}

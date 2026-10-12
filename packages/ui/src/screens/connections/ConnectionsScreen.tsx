import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { ImportSummary, Profile } from '@mqttpanel/engine-client';
import { engine, platform } from '../../services';
import { toMessage } from '../../i18n';
import { useApp } from '../../store/app';
import { handleConnectOutcome, run, saveProfile } from '../../lib/actions';
import { shortcut } from '../../lib/keys';
import { Kbd, Modal, Msg } from '../../components/ui';
import { ProfileForm } from './ProfileForm';

export function profileUrl(p: Profile): string {
  const path = p.scheme === 'ws' || p.scheme === 'wss' ? p.wsPath : '';
  return `${p.scheme}://${p.host}:${p.port}${path}`;
}

export function tlsChip(p: Profile): { label: string; tone: 'on' | 'clay' } {
  if (p.scheme === 'wss') return { label: 'WSS', tone: 'on' };
  if (p.scheme === 'mqtts') return { label: p.tls.clientCert ? 'TLS · mTLS' : 'TLS', tone: 'on' };
  return { label: 'noTls', tone: 'clay' };
}

function lastUsedKey(ms: number | null): [string, Record<string, unknown>] {
  if (!ms) return ['conn.neverUsed', {}];
  const startOfToday = new Date();
  startOfToday.setHours(0, 0, 0, 0);
  const days = Math.ceil((startOfToday.getTime() - ms) / 86_400_000);
  if (ms >= startOfToday.getTime()) return ['conn.usedToday', {}];
  if (days <= 1) return ['conn.usedYesterday', {}];
  if (days < 7) return ['conn.usedDaysAgo', { count: days }];
  if (days < 14) return ['conn.usedLastWeek', {}];
  return ['conn.usedWeeksAgo', { count: Math.floor(days / 7) }];
}

function ProfileCard({ p, on, onSelect, dragProps }: { p: Profile; on: boolean; onSelect(): void; dragProps: React.HTMLAttributes<HTMLDivElement> }) {
  const { t } = useTranslation();
  const conn = useApp((s) => s.connections.find((c) => c.id === p.id) ?? null);
  const chip = tlsChip(p);
  const [lk, lp] = lastUsedKey(p.lastUsed);
  const version = p.mqttVersion === 'auto' ? t('conn.versionAuto') : `MQTT ${p.mqttVersion}`;
  const meta = [
    p.isLocalBroker ? t('conn.embedded') : null,
    version,
    p.isLocalBroker ? null : t('conn.subCount', { count: p.subscriptions.length }),
    t(lk, lp),
  ].filter(Boolean);
  return (
    <div
      role="option"
      aria-selected={on}
      tabIndex={on ? 0 : -1}
      className={`card${on ? ' on' : ''}`}
      data-testid="profile-card"
      data-profile={p.id}
      onClick={onSelect}
      {...dragProps}
    >
      <div className="t">
        <span>
          {conn && conn.state === 'connected' && <span className="dot" style={{ marginRight: 6, verticalAlign: 1 }} />}
          {p.name || t('conn.untitled')}
        </span>
        <span className={`chip ${chip.tone}`}>{chip.label === 'noTls' ? t('conn.noTls') : chip.label}</span>
      </div>
      <div className="s">
        <span className="mono ell">{profileUrl(p)}</span>
      </div>
      <div className="s">{meta.join(' · ')}</div>
    </div>
  );
}

function ImportSummaryDialog({ summary, onClose }: { summary: ImportSummary; onClose(): void }) {
  const { t } = useTranslation();
  return (
    <Modal title={t('import.title')} width={520} onClose={onClose} testId="import-summary" footer={<button type="button" className="btn pri" onClick={onClose}>{t('common.done')}</button>}>
      <div>{t('import.imported', { count: summary.imported.length })}</div>
      {summary.imported.length > 0 && <div className="mono small selectable">{summary.imported.join(', ')}</div>}
      {summary.skipped.map((s) => (
        <div key={s.name} className="small">
          <b>{s.name}</b>: <Msg msg={s.reason} />
        </div>
      ))}
      {summary.notes.map((n, i) => (
        <div key={i} className="warnstrip">
          <Msg msg={n} />
        </div>
      ))}
    </Modal>
  );
}

async function doImport(path: string | null, show: (s: ImportSummary) => void) {
  const s = await run(engine().importProfiles(path));
  if (s) {
    await useApp.getState().refreshProfiles();
    show(s);
  }
}

async function newProfile(name: string) {
  const p = await run(engine().newProfile());
  if (!p) return;
  const saved = await saveProfile({ ...p, name });
  useApp.getState().selectProfile(saved.id);
}

function EmptyState({ onImport }: { onImport(s: ImportSummary): void }) {
  const { t } = useTranslation();
  const info = useApp((s) => s.appInfo);
  const [starting, setStarting] = useState(false);
  return (
    <div className="pane" data-testid="welcome">
      <div className="pc" style={{ display: 'grid', alignContent: 'center', gap: 18, padding: 40 }}>
        <div>
          <h4 style={{ margin: 0, fontSize: 20 }}>{t('welcome.title')}</h4>
          <p className="muted" style={{ marginTop: 6, maxWidth: '60ch' }}>
            {t('welcome.lead')}
          </p>
        </div>
        <div className="big">
          <div className="card">
            <div className="t">{t('welcome.newTitle')}</div>
            <span className="s">{t('welcome.newText')}</span>
            <button type="button" className="btn pri" style={{ justifySelf: 'start', marginTop: 6 }} data-testid="welcome-new" onClick={() => void newProfile(t('conn.newName'))}>
              {t('welcome.newButton')}
            </button>
          </div>
          <div className="card">
            <div className="t">{t('welcome.brokerTitle')}</div>
            <span className="s">{t('welcome.brokerText')}</span>
            <button
              type="button"
              className="btn"
              style={{ justifySelf: 'start', marginTop: 6 }}
              disabled={starting}
              data-testid="welcome-broker"
              onClick={async () => {
                setStarting(true);
                try {
                  const st = await engine().brokerStart();
                  useApp.setState({ broker: st });
                  await useApp.getState().refreshBrokerConfig();
                  const out = await engine().connectExplorerHere(null);
                  await handleConnectOutcome('', out);
                } catch (e) {
                  useApp.getState().toast({ kind: 'err', msg: toMessage(e) });
                } finally {
                  setStarting(false);
                }
              }}
            >
              {t('welcome.brokerButton')}
            </button>
          </div>
          <div className="card">
            <div className="t">{t('welcome.importTitle')}</div>
            <span className="s">{t('welcome.importText')}</span>
            <button type="button" className="btn" style={{ justifySelf: 'start', marginTop: 6 }} onClick={() => void doImport(null, onImport)}>
              {t('welcome.importButton')}
            </button>
          </div>
        </div>
        <div className="muted small">
          {t('welcome.paletteBefore')} <Kbd>{shortcut(info, 'mod+K')}</Kbd> {t('welcome.paletteAfter')}
        </div>
      </div>
    </div>
  );
}

export function ConnectionsScreen() {
  const { t } = useTranslation();
  const profiles = useApp((s) => s.profiles);
  const selected = useApp((s) => s.selectedProfileId);
  const info = useApp((s) => s.appInfo);
  const [q, setQ] = useState('');
  const [summary, setSummary] = useState<ImportSummary | null>(null);
  const [drag, setDrag] = useState<{ id: string; over: number } | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const sideRef = useRef<HTMLDivElement>(null);

  const shown = useMemo(() => {
    const s = q.trim().toLowerCase();
    return s ? profiles.filter((p) => (p.name + ' ' + profileUrl(p)).toLowerCase().includes(s)) : profiles;
  }, [profiles, q]);
  const sel = profiles.find((p) => p.id === selected) ?? profiles[0] ?? null;

  useEffect(() => {
    const onNew = () => void newProfile(t('conn.newName'));
    window.addEventListener('mqttpanel:new-connection', onNew);
    return () => window.removeEventListener('mqttpanel:new-connection', onNew);
  }, [t]);

  // a file dropped on the list is an import
  useEffect(
    () =>
      platform().onFileDrop((paths, x, y) => {
        const r = sideRef.current?.getBoundingClientRect();
        if (!r || !paths.length || x < r.left || x > r.right || y < r.top || y > r.bottom) return;
        void doImport(paths[0], setSummary);
      }),
    [],
  );

  const reorder = async (id: string, to: number) => {
    const ids = profiles.map((p) => p.id).filter((x) => x !== id);
    ids.splice(Math.max(0, Math.min(to, ids.length)), 0, id);
    useApp.setState({ profiles: ids.map((i) => profiles.find((p) => p.id === i)!) });
    await run(engine().reorderProfiles(ids));
  };

  const cardDrag = (p: Profile, index: number): React.HTMLAttributes<HTMLDivElement> => ({
    onPointerDown: (e) => {
      if (e.button !== 0 || q) return;
      const startY = e.clientY;
      let moved = false;
      const move = (ev: PointerEvent) => {
        if (!moved && Math.abs(ev.clientY - startY) < 6) return;
        moved = true;
        const cards = Array.from(listRef.current?.querySelectorAll<HTMLElement>('[data-testid="profile-card"]') ?? []);
        let over = cards.length;
        for (let i = 0; i < cards.length; i++) {
          const r = cards[i].getBoundingClientRect();
          if (ev.clientY < r.top + r.height / 2) {
            over = i;
            break;
          }
        }
        setDrag({ id: p.id, over });
      };
      const up = () => {
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', up);
        setDrag((d) => {
          if (d && moved) void reorder(d.id, d.over > index ? d.over - 1 : d.over);
          return null;
        });
      };
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', up);
    },
    onKeyDown: (e) => {
      if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
        e.preventDefault();
        void reorder(p.id, index + (e.key === 'ArrowUp' ? -1 : 1));
        return;
      }
      if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        e.preventDefault();
        const next = shown[index + (e.key === 'ArrowUp' ? -1 : 1)];
        if (next) {
          useApp.getState().selectProfile(next.id);
          setTimeout(() => listRef.current?.querySelector<HTMLElement>(`[data-profile="${next.id}"]`)?.focus());
        }
      }
    },
    onContextMenu: async (e) => {
      e.preventDefault();
      const id = await platform().showContextMenu(
        [
          { id: 'dup', label: t('conn.duplicate') },
          { id: 'up', label: t('conn.moveUp'), accelerator: '⌥↑', enabled: index > 0 },
          { id: 'down', label: t('conn.moveDown'), accelerator: '⌥↓', enabled: index < profiles.length - 1 },
        ],
        e.clientX,
        e.clientY,
      );
      if (id === 'dup') {
        const d = await run(engine().duplicateProfile(p.id));
        if (d) {
          await useApp.getState().refreshProfiles();
          useApp.getState().selectProfile(d.id);
        }
      } else if (id === 'up') void reorder(p.id, index - 1);
      else if (id === 'down') void reorder(p.id, index + 1);
    },
  });

  return (
    <div className="cols" style={{ gridTemplateColumns: '320px 1fr' }}>
      <div className="pane side" ref={sideRef}>
        <div className="ph">
          <h5>{t('conn.title')}</h5>
          <button type="button" className="btn pri sm" data-testid="new-profile" onClick={() => void newProfile(t('conn.newName'))}>
            {t('conn.new')}
          </button>
        </div>
        <div className="pc">
          {profiles.length === 0 ? (
            <div className="empty" style={{ padding: '30px 10px' }}>
              <h4>{t('conn.emptyTitle')}</h4>
              <span className="small">{t('conn.emptyText')}</span>
            </div>
          ) : (
            <>
              <div className="search" style={{ marginBottom: 10 }}>
                <input value={q} placeholder={t('conn.search')} aria-label={t('conn.search')} onChange={(e) => setQ(e.target.value)} />
                <Kbd>{shortcut(info, 'mod+K')}</Kbd>
              </div>
              <div className="list" role="listbox" aria-label={t('conn.title')} ref={listRef}>
                {shown.map((p, i) => (
                  <div key={p.id} className={drag && drag.over === i && drag.id !== p.id ? 'card dropbefore' : undefined} style={{ border: 0, padding: 0, borderRadius: 10 }}>
                    <ProfileCard p={p} on={sel?.id === p.id} onSelect={() => useApp.getState().selectProfile(p.id)} dragProps={cardDrag(p, i)} />
                  </div>
                ))}
              </div>
              <div className="sec" style={{ marginTop: 16 }}>
                {t('conn.import')}
              </div>
              <div className="muted small" style={{ marginTop: 4, whiteSpace: 'nowrap' }}>
                {t('conn.importHint')}{' '}
                <button type="button" className="btn link small" data-testid="import" onClick={() => void doImport(null, setSummary)}>
                  {t('conn.importLink')}
                </button>
              </div>
            </>
          )}
        </div>
      </div>
      {sel ? <ProfileForm key={sel.id} profileId={sel.id} /> : <EmptyState onImport={setSummary} />}
      {summary && <ImportSummaryDialog summary={summary} onClose={() => setSummary(null)} />}
    </div>
  );
}

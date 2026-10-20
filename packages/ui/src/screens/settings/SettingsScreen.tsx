import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { MemoryEstimate, Settings } from '@mqttpanel/engine-client';
import { engine } from '../../services';
import { useApp } from '../../store/app';
import { run, saveProfile } from '../../lib/actions';
import { shortcut } from '../../lib/keys';
import { ConfirmDialog, Seg, Switch, NO_AUTO } from '../../components/ui';
import { fmtBytes, fmtNum } from '../../lib/format';
import { AssistantSettings } from './AssistantSettings';

const PAGES = ['appearance', 'language', 'data', 'security', 'assistant', 'updates', 'shortcuts', 'privacy', 'about'] as const;
type Page = (typeof PAGES)[number];

/** Shortcut list shown in Settings › Shortcuts and used by the command palette. */
export const SHORTCUTS: { keys: string; label: string }[] = [
  { keys: 'mod+K', label: 'sc.palette' },
  { keys: 'mod+1', label: 'sc.connect' },
  { keys: 'mod+2', label: 'sc.explore' },
  { keys: 'mod+3', label: 'sc.publish' },
  { keys: 'mod+4', label: 'sc.charts' },
  { keys: 'mod+5', label: 'sc.broker' },
  { keys: 'mod+,', label: 'sc.settings' },
  { keys: 'mod+N', label: 'sc.newConnection' },
  { keys: 'mod+E', label: 'sc.export' },
  { keys: 'mod+shift+L', label: 'sc.switchConnection' },
  { keys: 'mod+enter', label: 'sc.send' },
  { keys: '/', label: 'sc.search' },
  { keys: 'P', label: 'sc.pin' },
  { keys: 'shift+P', label: 'sc.publishHere' },
  { keys: 'C', label: 'sc.addToChart' },
  { keys: 'J', label: 'sc.nextChanged' },
  { keys: 'Space', label: 'sc.pause' },
  { keys: 'mod+C', label: 'sc.copyTopic' },
  { keys: '↑ ↓ ← →', label: 'sc.arrows' },
  { keys: 'Enter', label: 'sc.select' },
  { keys: 'Esc', label: 'sc.close' },
];

/** A number field, width S (change set 6). */
function NumField(props: { value: number; min: number; max: number; suffix?: string; label: string; onCommit(n: number): void; testId?: string }) {
  const [v, setV] = useState(String(props.value));
  useEffect(() => setV(String(props.value)), [props.value]);
  const commit = () => {
    const n = Math.round(Math.min(props.max, Math.max(props.min, Number(v.replace(/\D/g, '')) || props.min)));
    setV(String(n));
    if (n !== props.value) props.onCommit(n);
  };
  return (
    <span className="row" style={{ gap: 4, flex: 'none' }}>
      <input {...NO_AUTO}
        className="input mono w-s"
        inputMode="numeric"
        aria-label={props.label}
        value={v}
        data-testid={props.testId}
        onChange={(e) => setV(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === 'Enter' && commit()}
      />
      {props.suffix && <span className="hint">{props.suffix}</span>}
    </span>
  );
}

export function SettingsScreen() {
  const { t } = useTranslation();
  const s = useApp((x) => x.settings);
  const info = useApp((x) => x.appInfo);
  const profiles = useApp((x) => x.profiles);
  const connections = useApp((x) => x.connections);
  const page = useApp((x) => x.settingsPage) as Page;
  const nonce = useApp((x) => x.settingsNonce);
  const save = (p: Partial<Settings>) => void useApp.getState().saveSettings(p);
  const [mem, setMem] = useState<MemoryEstimate | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [scq, setScq] = useState('');
  const [active, setActive] = useState<Page>(page);
  const scroller = useRef<HTMLDivElement>(null);
  const refs = useRef<Record<string, HTMLElement | null>>({});

  useEffect(() => {
    void engine()
      .memory()
      .then(setMem)
      .catch(() => undefined);
  }, []);
  useEffect(() => {
    scrollTo(page);
    setActive(page);
  }, [page, nonce]);

  const offsetOf = (p: Page) => {
    const el = refs.current[p];
    const sc = scroller.current;
    if (!el || !sc) return 0;
    return el.getBoundingClientRect().top - sc.getBoundingClientRect().top + sc.scrollTop;
  };
  function scrollTo(p: Page) {
    if (scroller.current) scroller.current.scrollTop = p === 'appearance' ? 0 : offsetOf(p) - 12;
  }
  const onScroll = () => {
    const sc = scroller.current;
    if (!sc) return;
    const top = sc.scrollTop + 40;
    let cur: Page = 'appearance';
    for (const p of PAGES) if (offsetOf(p) <= top) cur = p;
    if (sc.scrollTop + sc.clientHeight >= sc.scrollHeight - 2) return;
    setActive(cur);
  };

  const fingerprints = profiles.flatMap((p) => p.tls.trustedFingerprints.map((fp) => ({ p, fp })));
  const shortcuts = useMemo(
    () => SHORTCUTS.map((x) => ({ keys: x.keys.includes('+') || x.keys.length === 1 ? shortcut(info, x.keys) : x.keys, label: t(x.label) })).filter((x) => (x.label + ' ' + x.keys).toLowerCase().includes(scq.toLowerCase())),
    [info, t, scq],
  );

  const sec = (p: Page, children: React.ReactNode, first = false) => (
    <section ref={(el) => (refs.current[p] = el)} data-testid={`settings-${p}`} style={{ marginTop: first ? 0 : 24 }}>
      {!first && <div className="sec">{t(`settings.${p}`)}</div>}
      <div style={{ marginTop: first ? 0 : 8 }}>{children}</div>
    </section>
  );

  return (
    <div className="cols" style={{ gridTemplateColumns: '220px minmax(0,1fr)' }}>
      <div className="pane side">
        <div className="pc">
          <div className="list" role="tablist" aria-label={t('rail.settings')}>
            {PAGES.map((p) => (
              <button key={p} type="button" role="tab" aria-selected={active === p} className={`card${active === p ? ' on' : ''}`} data-testid={`settings-nav-${p}`} onClick={() => {
                useApp.getState().setSettingsPage(p);
                scrollTo(p);
                setActive(p);
              }}>
                <div className="t">
                  <span>{t(`settings.${p}`)}</span>
                </div>
              </button>
            ))}
          </div>
        </div>
      </div>
      <div className="pane">
        <div className="ph">
          <h5>{t(`settings.${active}`)}</h5>
        </div>
        <div className="pc" ref={scroller} onScroll={onScroll}>
          {sec(
            'appearance',
            <div className="form">
              <div className="f">
                <span className="lbl">{t('settings.theme')}</span>
                <Seg
                  label={t('settings.theme')}
                  value={s.theme}
                  onChange={(theme) => save({ theme })}
                  options={[
                    { value: 'system', label: t('settings.system') },
                    { value: 'light', label: t('settings.light') },
                    { value: 'dark', label: t('settings.dark') },
                  ]}
                />
              </div>
              <div className="f">
                <span className="lbl">{t('settings.accent')}</span>
                <span className="row2">
                  <Seg
                    label={t('settings.accent')}
                    value={s.accent}
                    onChange={(accent) => save({ accent })}
                    options={[
                      { value: 'sage', label: t('settings.sage') },
                      { value: 'slate', label: t('settings.slate') },
                      { value: 'clay', label: t('settings.clay') },
                    ]}
                  />
                  <span className="hint">{t('settings.accentHint')}</span>
                </span>
              </div>
              <div className="f">
                <span className="lbl">{t('settings.density')}</span>
                <Seg
                  label={t('settings.density')}
                  value={s.density}
                  onChange={(density) => save({ density })}
                  options={[
                    { value: 'comfortable', label: t('settings.comfortable') },
                    { value: 'compact', label: t('settings.compact') },
                  ]}
                />
              </div>
              <div className="f">
                <span className="lbl">{t('settings.monoSize')}</span>
                <span className="row2">
                  <NumField value={s.monoSize} min={10} max={16} suffix="px" label={t('settings.monoSize')} onCommit={(monoSize) => save({ monoSize })} />
                  <span className="hint">{t('settings.monoHint')}</span>
                </span>
              </div>
              <div className="f">
                <span className="lbl">{t('settings.valuesInTree')}</span>
                <span className="row2">
                  <Switch on={s.valuesInTree} label={t('settings.valuesInTree')} onChange={(valuesInTree) => save({ valuesInTree })} />
                  <span className="hint">{t('settings.valuesHint')}</span>
                </span>
              </div>
              <div className="f">
                <span className="lbl">{t('settings.motion')}</span>
                <span className="row2">
                  <Switch on={s.motion} label={t('settings.motion')} onChange={(motion) => save({ motion })} />
                  <span className="hint">{s.motion ? t('settings.motionHint') : t('settings.motionOff')}</span>
                </span>
              </div>
            </div>,
            true,
          )}
          {sec(
            'language',
            <div className="form">
              <div className="f">
                <span className="lbl">{t('settings.interface')}</span>
                <span className="row2">
                  <Seg
                    label={t('settings.interface')}
                    value={s.language}
                    testId="language"
                    onChange={(language) => save({ language })}
                    options={[
                      { value: 'system', label: t('settings.system'), testId: 'lang-system' },
                      { value: 'en', label: 'English', testId: 'lang-en' },
                      { value: 'fr', label: 'Français', testId: 'lang-fr' },
                      { value: 'de', label: 'Deutsch', testId: 'lang-de' },
                    ]}
                  />
                  <span className="hint">{t('settings.protocolWords')}</span>
                </span>
              </div>
            </div>,
          )}
          {sec(
            'data',
            <div className="form">
              <div className="f">
                <span className="lbl">{t('settings.bufferPerTopic')}</span>
                <span className="row2">
                  <NumField value={s.bufferPerTopic} min={10} max={10000} label={t('settings.bufferPerTopic')} onCommit={(bufferPerTopic) => save({ bufferPerTopic })} />
                  <span className="hint">{t('settings.bufferRange', { min: fmtNum(10), max: fmtNum(10000) })}</span>
                </span>
              </div>
              <div className="f">
                <span className="lbl">{t('settings.pointsPerSeries')}</span>
                <NumField value={s.pointsPerSeries} min={100} max={1000000} label={t('settings.pointsPerSeries')} onCommit={(pointsPerSeries) => save({ pointsPerSeries })} />
              </div>
              <div className="f">
                <span className="lbl">{t('settings.memoryBudget')}</span>
                <span className="row2">
                  <NumField value={s.memoryBudgetMb} min={32} max={16384} suffix="MB" label={t('settings.memoryBudget')} onCommit={(memoryBudgetMb) => save({ memoryBudgetMb })} />
                  <span className="hint">{t('settings.inUse', { size: fmtBytes(mem?.bytes ?? 0) })}</span>
                </span>
              </div>
              <div className="f">
                <span className="lbl">{t('settings.renderLimit')}</span>
                <span className="row2">
                  <NumField value={s.payloadRenderLimitKb} min={16} max={65536} suffix="KB" label={t('settings.renderLimit')} onCommit={(payloadRenderLimitKb) => save({ payloadRenderLimitKb })} />
                  <span className="hint">{t('settings.renderHint')}</span>
                </span>
              </div>
              <div className="f">
                <span className="lbl">{t('settings.historyDisk')}</span>
                <span className="row2">
                  <span className="chip">{t('settings.never')}</span>
                  <span className="hint">{t('settings.exportToKeep')}</span>
                </span>
              </div>
              <div className="f">
                <span />
                <span>
                  <button type="button" className="btn danger" data-testid="clear-all" onClick={() => setConfirmClear(true)}>
                    {t('settings.clearAll')}
                  </button>
                </span>
              </div>
            </div>,
          )}
          {sec(
            'security',
            <div className="form">
              <div className="f">
                <span className="lbl">{t('settings.fingerprints')}</span>
                <span className="row2 wrap">
                  {fingerprints.length === 0 && <span className="hint">{t('settings.noFingerprints')}</span>}
                  {fingerprints.map(({ p, fp }) => (
                    <span key={p.id + fp} className="chip mono" title={fp}>
                      {p.name} · {fp.slice(0, 5)}…{fp.slice(-2)}
                      <button
                        type="button"
                        className="x"
                        aria-label={t('settings.forget', { name: p.name })}
                        onClick={() => void saveProfile({ ...p, tls: { ...p.tls, trustedFingerprints: p.tls.trustedFingerprints.filter((x) => x !== fp) } })}
                      >
                        ✕
                      </button>
                    </span>
                  ))}
                </span>
              </div>
            </div>,
          )}
          {sec('assistant', <AssistantSettings />)}
          {sec(
            'updates',
            <div className="form">
              <div className="f">
                <span className="lbl">{t('settings.checkAuto')}</span>
                <span className="row2">
                  <Switch on={s.updateCheck} label={t('settings.checkAuto')} onChange={(updateCheck) => save({ updateCheck })} />
                  <span className="hint">{t('settings.checkHint')}</span>
                </span>
              </div>
              <div className="f">
                <span className="lbl">{t('settings.channel')}</span>
                <Seg
                  label={t('settings.channel')}
                  value={s.updateChannel}
                  onChange={(updateChannel) => save({ updateChannel })}
                  options={[
                    { value: 'stable', label: t('settings.stable') },
                    { value: 'beta', label: t('settings.beta') },
                  ]}
                />
              </div>
              <div className="f">
                <span className="lbl">{t('settings.current')}</span>
                <span className="row2">
                  <span className="hint">{t('settings.version', { version: info.version })}</span>
                  <button type="button" className="btn sm" onClick={() => useApp.getState().toast({ kind: 'info', text: t('settings.noUpdates') })}>
                    {t('settings.checkNow')}
                  </button>
                </span>
              </div>
            </div>,
          )}
          {sec(
            'shortcuts',
            <>
              <div className="search" style={{ maxWidth: 360 }}>
                <input {...NO_AUTO} value={scq} placeholder={t('settings.searchShortcuts')} aria-label={t('settings.searchShortcuts')} onChange={(e) => setScq(e.target.value)} />
              </div>
              <table className="tbl" style={{ marginTop: 8, maxWidth: 560 }}>
                <tbody>
                  {shortcuts.map((x) => (
                    <tr key={x.label}>
                      <td>{x.label}</td>
                      <td style={{ textAlign: 'right' }}>
                        <span className="kbd">{x.keys}</span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>,
          )}
          {sec(
            'privacy',
            <p className="small muted" style={{ maxWidth: '90ch' }}>
              {t('settings.privacyText')}
            </p>,
          )}
          {sec(
            'about',
            <div style={{ display: 'grid', gap: 6 }} className="small">
              <div>
                <strong>MQTT Panel</strong> {info.version}
              </div>
              <div className="muted">{t('settings.licence')}</div>
              <div className="muted">{t('settings.copyright')}</div>
              <div className="muted" style={{ userSelect: 'text' }}>{t('settings.website')}</div>
              <div className="muted" style={{ marginTop: 6 }}>
                {t('settings.thirdParty')}
              </div>
              <div className="muted mono" style={{ fontSize: 11 }}>
                React (MIT) · Zustand (MIT) · TanStack Virtual (MIT) · uPlot (MIT) · CodeMirror (MIT) · i18next (MIT) · Tauri (MIT / Apache 2.0)
              </div>
              <div style={{ height: 200 }} />
            </div>,
          )}
        </div>
      </div>
      {confirmClear && (
        <ConfirmDialog
          title={t('settings.clearTitle')}
          confirmLabel={t('settings.clearConfirm')}
          danger
          onClose={() => setConfirmClear(false)}
          onConfirm={async () => {
            for (const c of connections) await run(engine().clearHistory(c.id, null));
            setMem(await engine().memory());
            useApp.getState().toast({ kind: 'info', text: t('explore.historyCleared') });
          }}
        >
          <div className="small">{t('settings.clearBody')}</div>
        </ConfirmDialog>
      )}
    </div>
  );
}

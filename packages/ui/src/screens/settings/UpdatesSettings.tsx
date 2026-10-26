import { useTranslation } from 'react-i18next';
import type { UpdateStatus } from '@mqttpanel/engine-client';
import { engine } from '../../services';
import { toMessage } from '../../i18n';
import { useApp } from '../../store/app';
import { Info, Msg, Switch } from '../../components/ui';
import { fmtDate, fmtHm } from '../../lib/format';

/**
 * Settings → Updates of the direct macOS download (docs/UPDATER.md): the automatic check, the last check, "Check now",
 * and the state of an update (found, downloading, ready to restart). Store builds show the store sentence instead.
 */
export function UpdatesSettings() {
  const { t } = useTranslation();
  const u = useApp((s) => s.update);
  const later = useApp((s) => s.updateLater);
  if (!u) return null;
  const apply = (p: Promise<UpdateStatus>) =>
    void p
      .then((update) => useApp.setState({ update }))
      .catch((e) => useApp.getState().toast({ kind: 'err', msg: toMessage(e) }));
  const setAuto = (on: boolean) => {
    useApp.setState({ update: { ...u, auto: on }, settings: { ...useApp.getState().settings, updates: { ...useApp.getState().settings.updates, auto: on } } });
    apply(engine().updateSetAuto(on));
  };
  const last = u.lastCheck ? Date.parse(u.lastCheck) : NaN;
  const busy = u.state === 'checking' || u.state === 'downloading';
  const pct = u.total ? Math.min(100, Math.round((u.downloaded / u.total) * 100)) : null;
  return (
    <div data-testid="updates-self">
      <div className="form">
        <div className="f">
          <span className="lbl">{t('updates.auto')}</span>
          <span className="row2">
            <Switch on={u.auto} label={t('updates.auto')} testId="updates-auto" onChange={setAuto} />
            <Info id="updates.auto" title={t('updates.auto')} />
          </span>
        </div>
        <div className="f">
          <span className="lbl">{t('updates.lastCheck')}</span>
          <span className="row2">
            <span className="hint" data-testid="updates-last-check">
              {Number.isFinite(last) ? `${fmtDate(last)} ${fmtHm(last)}` : t('updates.never')}
            </span>
            <button type="button" className="btn sm" data-testid="updates-check" disabled={busy || u.state === 'ready'} onClick={() => apply(engine().updateCheck(true))}>
              {u.state === 'checking' ? t('updates.checking') : t('updates.checkNow')}
            </button>
          </span>
        </div>
      </div>
      <div style={{ marginTop: 10, maxWidth: '90ch' }} data-testid="updates-state" data-state={u.state}>
        {u.state === 'latest' && <p className="small" style={{ margin: 0 }}>{t('updates.latest')}</p>}
        {u.state === 'error' && (
          <p className="small" role="alert" style={{ margin: 0, color: 'var(--brick)' }} data-testid="updates-error">
            <Msg msg={u.error} />
          </p>
        )}
        {(u.state === 'available' || u.state === 'downloading') && u.version && (
          <div className="callout" data-testid="updates-available">
            <p>
              <strong>{t('updates.available', { version: u.version })}</strong>
              {u.date && <span className="muted"> · {t('updates.published', { date: u.date })}</span>}
            </p>
            {u.notes && (
              <p className="muted" style={{ marginTop: 4, whiteSpace: 'pre-wrap' }} data-testid="updates-notes">
                {u.notes}
              </p>
            )}
            {u.error && (
              <p role="alert" style={{ marginTop: 4, color: 'var(--brick)' }} data-testid="updates-error">
                <Msg msg={u.error} />
              </p>
            )}
            <div className="row" style={{ gap: 8, marginTop: 8, alignItems: 'center' }}>
              {u.state === 'downloading' ? (
                <>
                  <span className="upbar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct ?? undefined} aria-label={t('updates.install')} data-testid="updates-progress">
                    <span style={{ width: `${pct ?? 0}%` }} />
                  </span>
                  <span className="hint">{t('updates.downloading', { percent: pct === null ? '' : `${pct} %` }).trim()}</span>
                </>
              ) : u.canInstall ? (
                <>
                  <button type="button" className="btn sm pri" data-testid="updates-install" onClick={() => apply(engine().updateInstall())}>
                    {t('updates.install')}
                  </button>
                  {later !== u.version && (
                    <button type="button" className="btn sm" data-testid="updates-later" onClick={() => useApp.setState({ updateLater: u.version })}>
                      {t('updates.later')}
                    </button>
                  )}
                </>
              ) : (
                <span className="small" data-testid="updates-move-first" style={{ color: 'var(--clay)' }}>
                  {t('updates.moveFirst')}
                </span>
              )}
            </div>
          </div>
        )}
        {u.state === 'ready' && (
          <div className="callout" data-testid="updates-ready">
            <p>{t('updates.ready', { version: u.version ?? '' })}</p>
            <div className="row" style={{ gap: 8, marginTop: 8 }}>
              <button
                type="button"
                className="btn sm pri"
                data-testid="updates-restart"
                onClick={() => void engine().updateRestart().catch((e) => useApp.getState().toast({ kind: 'err', msg: toMessage(e) }))}
              >
                {t('updates.restart')}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

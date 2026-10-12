import { useTranslation } from 'react-i18next';
import { useApp } from '../store/app';
import { connectProfile, useBusy } from '../lib/actions';
import { profileUrl } from '../screens/connections/ConnectionsScreen';

/** Explorer, Publish and Charts before any connection: "Choose a connection" with the profiles to connect. */
export function NoConnection() {
  const { t } = useTranslation();
  const profiles = useApp((s) => s.profiles);
  const busy = useBusy((s) => s.connecting);
  return (
    <div className="pane" data-testid="no-connection">
      <div className="empty" style={{ height: '100%' }}>
        <h4>{t('noconn.title')}</h4>
        <span className="small">{profiles.length ? t('noconn.text') : t('noconn.none')}</span>
        <div className="list" style={{ width: 420, marginTop: 6 }}>
          {profiles.map((p) => (
            <button key={p.id} type="button" className="card" disabled={busy === p.id} data-testid="noconn-profile" onClick={() => void connectProfile(p.id, false)}>
              <div className="t">
                <span>{p.name || t('conn.untitled')}</span>
                <span className="btn sm pri">{busy === p.id ? t('conn.connecting') : t('conn.connect')}</span>
              </div>
              <div className="s mono">{profileUrl(p)}</div>
            </button>
          ))}
        </div>
        {!profiles.length && (
          <button type="button" className="btn pri" onClick={() => useApp.getState().setScreen('connect')}>
            {t('noconn.goConnect')}
          </button>
        )}
      </div>
    </div>
  );
}

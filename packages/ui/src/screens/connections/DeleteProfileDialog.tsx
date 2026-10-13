import { useTranslation } from 'react-i18next';
import type { Profile } from '@mqttpanel/engine-client';
import { deleteProfile, run } from '../../lib/actions';
import { ConfirmDialog } from '../../components/ui';

/** "Delete the connection "X"?" with Delete and Cancel; a connected profile is disconnected first. */
export function DeleteProfileDialog({ profile, onClose, beforeDelete }: { profile: Profile; onClose(): void; beforeDelete?(): void }) {
  const { t } = useTranslation();
  return (
    <ConfirmDialog
      title={t('conn.deleteTitle', { name: profile.name || t('conn.untitled') })}
      confirmLabel={t('conn.deleteBtn')}
      danger
      testId="delete-profile"
      onClose={onClose}
      onConfirm={async () => {
        beforeDelete?.();
        await run(deleteProfile(profile.id));
      }}
    >
      <div className="muted small">{t('conn.deleteBody')}</div>
    </ConfirmDialog>
  );
}

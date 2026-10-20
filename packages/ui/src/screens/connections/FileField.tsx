import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { FileInspection, FileKind, Profile, UserMessage } from '@mqttpanel/engine-client';
import { engine, platform } from '../../services';
import { toMessage } from '../../i18n';
import { Modal, Msg, NO_AUTO } from '../../components/ui';

type Field = 'caCert' | 'clientCert' | 'clientKey';

/**
 * A certificate or key field: read-only file name, Browse…, the accepted formats in muted text (change set 11: exactly
 * the picker's filters), drop target, and a plain error when the file is not what the field expects (C3). Nothing
 * parsed is shown next to the name (owner, 13 Oct 2026, change set 4).
 *
 * `kind`, `browseLabel` and `applyPath` serve "Import signed certificate…" of the generated way (change set 11): the
 * picker kind, the button text and the engine call that takes the file.
 */
export function FileField(props: {
  field: Field;
  profile: Profile;
  label: string;
  beforeChange(): Promise<Profile>;
  onProfile(p: Profile): void;
  kind?: FileKind;
  browseLabel?: string;
  applyPath?(profileId: string, path: string): Promise<Profile>;
  disabled?: boolean;
  testId?: string;
}) {
  const { t } = useTranslation();
  const { field, profile } = props;
  const kind: FileKind = props.kind ?? field;
  const tid = props.testId ?? field;
  const value = profile.tls[field];
  const ref = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<UserMessage | null>(null);
  const [busy, setBusy] = useState(false);
  const [pass, setPass] = useState<{ path: string; fileName: string } | null>(null);
  const [passphrase, setPassphrase] = useState('');
  const [passError, setPassError] = useState<UserMessage | null>(null);

  const apply = async (path: string | null, passphrase: string | null) => {
    setBusy(true);
    try {
      const saved = await props.beforeChange();
      const p = path !== null && props.applyPath ? await props.applyPath(saved.id, path) : await engine().setProfileFile(saved.id, field, path, passphrase);
      setError(null);
      props.onProfile(p);
      return true;
    } catch (e) {
      const m = toMessage(e);
      if (passphrase !== null) setPassError(m);
      else setError(m);
      return false;
    } finally {
      setBusy(false);
    }
  };

  const accept = async (ins: FileInspection) => {
    if (!ins.ok) {
      setError(ins.error);
      return;
    }
    if (ins.summary.kind === 'pkcs12' || ins.summary.encrypted) {
      setPassphrase('');
      setPassError(null);
      setPass({ path: ins.path, fileName: ins.fileName });
      return;
    }
    await apply(ins.path, null);
  };

  const browse = async () => {
    try {
      const ins = await engine().pickFile(kind);
      if (ins) await accept(ins);
    } catch (e) {
      setError(toMessage(e));
    }
  };

  // window-level drops (Tauri gives paths and a position): hit-test against this field
  const acceptRef = useRef(accept);
  acceptRef.current = accept;
  useEffect(
    () =>
      platform().onFileDrop((paths, x, y) => {
        const r = ref.current?.getBoundingClientRect();
        if (!r || !paths.length) return;
        if (x < r.left || x > r.right || y < r.top || y > r.bottom) return;
        if (props.disabled) return;
        void engine()
          .inspectFile(kind, paths[0])
          .then((ins) => acceptRef.current(ins))
          .catch((e) => setError(toMessage(e)));
      }),
    [kind, props.disabled],
  );

  return (
    <>
      <div className="row2" ref={ref} data-testid={`file-${tid}`}>
        <span className={`input mono ro ${props.browseLabel ? 'w-l' : 'w-xl'}${error ? ' err' : ''}`} aria-label={props.label} title={value?.fileName}>
          {value ? value.fileName : <span className="ph-text">{t('file.none')}</span>}
        </span>
        <button type="button" className="btn sm" onClick={() => void browse()} disabled={busy || props.disabled} data-testid={`browse-${tid}`}>
          {props.browseLabel ?? t('common.browse')}
        </button>
        <span className="fmt" data-testid={`formats-${tid}`}>
          {t(`file.formats.${kind}`)}
        </span>
        {error && (
          <span className="hint err" data-testid={`file-error-${tid}`}>
            <Msg msg={error} />
          </span>
        )}
        {value && (
          <button type="button" className="iconbtn" aria-label={t('file.clear', { name: props.label })} onClick={() => void apply(null, null)}>
            ✕
          </button>
        )}
      </div>
      {pass && (
        <Modal
          title={t('file.passphraseTitle', { file: pass.fileName })}
          width={440}
          onClose={() => setPass(null)}
          testId="pkcs12-dialog"
          footer={
            <>
              <button type="button" className="btn" onClick={() => setPass(null)}>
                {t('common.cancel')}
              </button>
              <button
                type="button"
                className="btn pri"
                onClick={async () => {
                  if (await apply(pass.path, passphrase)) setPass(null);
                }}
              >
                {t('file.import')}
              </button>
            </>
          }
        >
          <div className="muted small">{t('file.passphraseHelp')}</div>
          <input {...NO_AUTO}
            className={`input w-l${passError ? ' err' : ''}`}
            type="password"
            value={passphrase}
            aria-label={t('file.passphrase')}
            placeholder={t('file.passphrase')}
            onChange={(e) => setPassphrase(e.target.value)}
            data-autofocus
          />
          {passError && (
            <div className="hint err">
              <Msg msg={passError} />
            </div>
          )}
        </Modal>
      )}
    </>
  );
}

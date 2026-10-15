import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import i18next from 'i18next';
import { platform } from '../services';
import { useApp } from '../store/app';
import { NO_AUTO } from './ui';

const MASK = '••••••••••••';

const ICONS = {
  eye: '<path d="M2 12s3.6-6.5 10-6.5S22 12 22 12s-3.6 6.5-10 6.5S2 12 2 12z"/><circle cx="12" cy="12" r="2.8"/>',
  eyeOff: '<path d="M3 3l18 18"/><path d="M10.6 5.6A9.6 9.6 0 0 1 12 5.5c6.4 0 10 6.5 10 6.5a17 17 0 0 1-3.1 3.9M6.6 6.6C3.7 8.4 2 12 2 12s3.6 6.5 10 6.5a9.5 9.5 0 0 0 4.4-1.1"/><path d="M9.9 9.9a2.8 2.8 0 0 0 4 4"/>',
  copy: '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/>',
};

function Icon({ name }: { name: keyof typeof ICONS }) {
  return <svg viewBox="0 0 24 24" aria-hidden="true" dangerouslySetInnerHTML={{ __html: ICONS[name] }} />;
}

/** The eye (reveal) and copy pair used next to every password and passphrase the app owns (B-16). */
export function SecretButtons(props: { shown: boolean; onToggle(): void; onCopy(): void; disabled?: boolean; testId?: string }) {
  const { t } = useTranslation();
  const eyeLabel = props.shown ? t('secret.hide') : t('secret.show');
  return (
    <span className="secret-btns">
      <button
        type="button"
        className="eye"
        title={eyeLabel}
        aria-label={eyeLabel}
        aria-pressed={props.shown}
        disabled={props.disabled}
        data-testid={props.testId ? `${props.testId}-eye` : undefined}
        onClick={props.onToggle}
      >
        <Icon name={props.shown ? 'eyeOff' : 'eye'} />
      </button>
      <button
        type="button"
        className="eye"
        title={t('secret.copy')}
        aria-label={t('secret.copy')}
        disabled={props.disabled}
        data-testid={props.testId ? `${props.testId}-copy` : undefined}
        onClick={props.onCopy}
      >
        <Icon name="copy" />
      </button>
    </span>
  );
}

async function copySecret(value: string) {
  await platform().copyText(value);
  useApp.getState().toast({ kind: 'info', text: i18next.t('secret.copied') });
}

/** Long secrets are shown as their first and last four characters (mockup 12); copy always takes the whole value. */
export function abbreviate(s: string): string {
  return s.length > 12 ? `${s.slice(0, 4)}…${s.slice(-4)}` : s;
}

/**
 * A masked secret the app owns, revealed by the eye and copied by the copy icon. `value` when the secret is already
 * known, else `load` fetches it on demand (from the keychain through the engine).
 */
export function SecretText(props: { value?: string | null; load?: () => Promise<string | null>; disabled?: boolean; testId?: string; full?: boolean }) {
  const [shown, setShown] = useState(false);
  const [loaded, setLoaded] = useState<string | null>(null);
  const get = async (): Promise<string | null> => {
    if (props.value != null) return props.value;
    if (loaded != null) return loaded;
    const v = props.load ? await props.load() : null;
    setLoaded(v);
    return v;
  };
  const current = props.value ?? loaded;
  return (
    <span className="secret" data-testid={props.testId}>
      <span className="mono selectable" data-testid={props.testId ? `${props.testId}-value` : undefined} title={shown && current ? current : undefined}>
        {shown && current ? (props.full ? current : abbreviate(current)) : MASK}
      </span>
      <SecretButtons
        shown={shown}
        disabled={props.disabled}
        testId={props.testId}
        onToggle={async () => {
          if (shown) setShown(false);
          else if (await get()) setShown(true);
        }}
        onCopy={async () => {
          const v = await get();
          if (v) await copySecret(v);
        }}
      />
    </span>
  );
}

/** A password or passphrase input with the eye and copy icons (fields whose value the app holds). */
export function SecretInput(props: Omit<React.InputHTMLAttributes<HTMLInputElement>, 'type' | 'value' | 'onChange'> & { value: string; onValue(v: string): void; testId?: string }) {
  const { value, onValue, testId, style, ...rest } = props;
  const [shown, setShown] = useState(false);
  return (
    <span className="secretin" style={style}>
      <input {...NO_AUTO} {...rest} className={rest.className ?? 'input w-l'} type={shown ? 'text' : 'password'} value={value} data-testid={testId} onChange={(e) => onValue(e.target.value)} />
      <SecretButtons shown={shown} disabled={!value} testId={testId} onToggle={() => setShown(!shown)} onCopy={() => void copySecret(value)} />
    </span>
  );
}

import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { AppBrokerStep, AppBrokerStepResult, BrokerCerts, BrokerConfig, BrokerStatus, Listener, UserMessage } from '@mqttpanel/engine-client';
import { engine } from '../../services';
import { toMessage } from '../../i18n';
import { useApp } from '../../store/app';
import { handleConnectOutcome } from '../../lib/actions';
import { Info, Modal, Msg } from '../../components/ui';
import { SecretText } from '../../components/Secret';
import { AddListenerDialog, kindLabel } from '../broker/BrokerDialogs';

const secure = (l: Listener) => l.kind === 'tls' || l.kind === 'wss';

/** Security rank for the preselection: mutual TLS, then TLS, then secure WebSocket, then TCP, then WebSocket. The
 * client certificate requirement is broker-wide (change set 2). */
export function listenerRank(l: Listener, requireClientCert: boolean): number {
  if (secure(l) && requireClientCert) return l.kind === 'tls' ? 6 : 5;
  return { tls: 4, wss: 3, tcp: 2, ws: 1 }[l.kind];
}

/** Enabled listeners, most secure first. */
export function rankedListeners(ls: Listener[], requireClientCert: boolean): Listener[] {
  return ls.filter((l) => l.enabled).sort((a, b) => listenerRank(b, requireClientCert) - listenerRank(a, requireClientCert));
}

interface Line {
  step: AppBrokerStep;
  state: 'wait' | 'run' | 'done' | 'err';
  result?: AppBrokerStepResult['result'];
  password?: string | null;
}

/** The steps shown for a listener, decided before the run (mockup 02b). */
function plan(l: Listener, anonymous: boolean, requireClientCert: boolean, status: BrokerStatus | null): Line[] {
  const steps: AppBrokerStep[] = ['user'];
  if (!anonymous && secure(l) && requireClientCert) steps.push('cert');
  steps.push('trust');
  if (!status || status.state !== 'running' || status.restartNeeded) steps.push('broker');
  steps.push('connect');
  return steps.map((step) => ({ step, state: 'wait' }));
}

/**
 * "Connect to this app's broker" (mockup 02b): pick a listener (the most secure one is preselected) or add one, then
 * one button runs the steps: user "mqtt-panel", client certificate when required, CA trust, start or restart, connect.
 * The result is always the single profile "Local broker"; the Explorer opens on it.
 */
export function AppBrokerDialog({ start, onClose }: { start: 'pick' | 'add'; onClose(): void }) {
  const { t } = useTranslation();
  const [cfg, setCfg] = useState<BrokerConfig | null>(null);
  const [status, setStatus] = useState<BrokerStatus | null>(null);
  const [certs, setCerts] = useState<BrokerCerts | null>(null);
  const [sel, setSel] = useState<string | null>(null);
  const [adding, setAdding] = useState(start === 'add');
  const [lines, setLines] = useState<Line[] | null>(null);
  const [error, setError] = useState<UserMessage | null>(null);
  const [running, setRunning] = useState(false);
  const alive = useRef(true);
  useEffect(
    () => () => {
      alive.current = false;
    },
    [],
  );

  const load = async (): Promise<BrokerConfig | null> => {
    try {
      const [c, st, ce] = await Promise.all([engine().brokerConfig(), engine().brokerStatus(), engine().brokerCerts()]);
      if (!alive.current) return null;
      setCfg(c);
      setStatus(st);
      setCerts(ce);
      useApp.setState({ brokerConfig: c, broker: st });
      return c;
    } catch (e) {
      setError(toMessage(e));
      return null;
    }
  };
  useEffect(() => {
    void load().then((c) => {
      if (!c) return;
      const best = rankedListeners(c.listeners, c.requireClientCert)[0];
      setSel((s) => s ?? (best ? best.id : 'add'));
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const runFor = async (listenerId: string, c: BrokerConfig, st: BrokerStatus | null) => {
    const l = c.listeners.find((x) => x.id === listenerId);
    if (!l) return;
    const ls = plan(l, c.anonymous, c.requireClientCert, st);
    setLines(ls);
    setError(null);
    setRunning(true);
    const set = (i: number, patch: Partial<Line>) => {
      ls[i] = { ...ls[i], ...patch };
      if (alive.current) setLines([...ls]);
    };
    try {
      for (let i = 0; i < ls.length; i++) {
        if (!alive.current) return;
        set(i, { state: 'run' });
        const r = await engine().appBrokerStep(listenerId, ls[i].step, t('pill.localBroker'));
        if (!alive.current) return;
        if (r.step === 'connect') {
          if (r.outcome?.kind === 'failed') {
            set(i, { state: 'err' });
            setError(r.outcome.error);
            return;
          }
          set(i, { state: 'done', result: r.result });
          await useApp.getState().refreshProfiles();
          void useApp.getState().refreshBrokerConfig();
          if (r.outcome) await handleConnectOutcome(r.outcome.kind === 'connected' ? r.outcome.status.id : '', r.outcome);
          onClose();
          return;
        }
        set(i, { state: 'done', result: r.result, password: r.password });
      }
    } catch (e) {
      const idx = ls.findIndex((x) => x.state === 'run');
      if (idx >= 0) set(idx, { state: 'err' });
      if (alive.current) setError(toMessage(e));
    } finally {
      if (alive.current) setRunning(false);
    }
  };

  const connect = async () => {
    if (sel === 'add') {
      setAdding(true);
      return;
    }
    if (!sel || !cfg) return;
    const st = await engine().brokerStatus().catch(() => status);
    await runFor(sel, cfg, st);
  };

  if (adding) {
    if (!cfg) return null;
    return (
      <AddListenerDialog
        config={cfg}
        certs={certs}
        context="flow"
        onConfig={(c) => setCfg(c)}
        onClose={() => {
          setAdding(false);
          if (start === 'add' && !lines) onClose();
        }}
        onAdded={async (added) => {
          const id = added.id;
          setAdding(false);
          setSel(id);
          const c = await load();
          const st = await engine().brokerStatus().catch(() => null);
          if (c) await runFor(id, c, st);
        }}
      />
    );
  }

  const listeners = cfg ? rankedListeners(cfg.listeners, cfg.requireClientCert) : [];
  const best = listeners[0]?.id;
  const anonymous = cfg?.anonymous ?? false;

  const lineText = (ln: Line, l: Listener | undefined): string => {
    const done = ln.state === 'done';
    switch (ln.step) {
      case 'user':
        if (done) return ln.result === 'skipped' ? t('appb.s.userSkipped') : ln.result === 'created' ? t('appb.s.userCreated') : t('appb.s.userKept');
        return anonymous ? t('appb.s.userSkipped') : t('appb.s.user');
      case 'cert':
        if (done) return ln.result === 'issued' ? t('appb.s.certIssued') : t('appb.s.certKept');
        return t('appb.s.cert');
      case 'trust': {
        const tls = l ? secure(l) : true;
        if (done) return tls ? t('appb.s.trusted') : t('appb.s.profileDone');
        return tls ? t('appb.s.trust') : t('appb.s.profile');
      }
      case 'broker':
        if (done) return ln.result === 'started' ? t('appb.s.started') : ln.result === 'restarted' ? t('appb.s.restarted') : t('appb.s.running');
        return status && status.state === 'running' ? t('appb.s.restart') : t('appb.s.start');
      case 'connect':
        return done ? t('appb.s.connected') : t('appb.s.connect');
    }
  };
  const selL = cfg?.listeners.find((l) => l.id === sel);

  return (
    <Modal
      title={t('appb.title')}
      width={620}
      onClose={onClose}
      testId="app-broker"
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <span className="grow" />
          <button type="button" className="btn pri" data-testid="appb-connect" disabled={running || !sel} onClick={() => void connect()}>
            {t('conn.connect')}
          </button>
        </>
      }
    >
      <div className="muted small">{t('appb.which')}</div>
      <div style={{ display: 'grid', gap: 6 }} role="radiogroup" aria-label={t('appb.which')}>
        {listeners.map((l) => {
          const mtls = secure(l) && !!cfg?.requireClientCert;
          return (
            <div
              key={l.id}
              className={`card${sel === l.id ? ' on' : ''}`}
              role="radio"
              aria-checked={sel === l.id}
              tabIndex={sel === l.id ? 0 : -1}
              data-testid="appb-listener"
              data-listener={l.id}
              onClick={() => !running && setSel(l.id)}
            >
              <div className="radio">
                <i className={sel === l.id ? 'on' : ''} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div className="t">
                    <span>{t('appb.listener', { kind: kindLabel(l.kind, t), port: l.port })}</span>
                    {mtls ? (
                      <span className="chip on">{t('preset.mtls')}</span>
                    ) : secure(l) ? (
                      <span className="chip on">{t('preset.encrypted')}</span>
                    ) : (
                      <span className="chip clay">{t('broker.unencrypted')}</span>
                    )}
                  </div>
                  <div className="s">
                    {t(mtls ? 'appb.desc.mtls' : secure(l) ? 'appb.desc.encrypted' : 'appb.desc.plain')}
                    {l.id === best && listeners.length > 1 ? ' ' + t('appb.mostSecure') : ''}
                  </div>
                </div>
              </div>
            </div>
          );
        })}
        <div
          className={`card${sel === 'add' ? ' on' : ''}`}
          role="radio"
          aria-checked={sel === 'add'}
          tabIndex={sel === 'add' ? 0 : -1}
          data-testid="appb-add"
          onClick={() => !running && setSel('add')}
        >
          <div className="radio">
            <i className={sel === 'add' ? 'on' : ''} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div className="t">
                <span>{t('conn.newAppAdd')}</span>
                <Info id="appbAdd" title={t('conn.newAppAdd')} />
              </div>
              <div className="s">{t('appb.addText')}</div>
            </div>
          </div>
        </div>
      </div>
      {cfg && listeners.length === 0 && <div className="hint warn">{t('appb.none')}</div>}
      <div className="callout" style={{ padding: '8px 12px' }} data-testid="appb-what">
        <p className="small" style={{ margin: 0 }}>
          <strong>{t('appb.whatTitle')}</strong> {anonymous ? t('appb.whatAnon') : t('appb.what')}
        </p>
      </div>
      {lines && (
        <div className="steps" data-testid="appb-steps" aria-live="polite">
          {lines.map((ln) => (
            <div key={ln.step} className={`st ${ln.state}`} data-testid={`appb-step-${ln.step}`} data-state={ln.state} data-result={ln.result ?? ''}>
              <span className="mk">{ln.state === 'done' ? '✓' : ln.state === 'run' ? '◌' : ln.state === 'err' ? '✕' : '○'}</span>
              <span>{ln.state === 'wait' ? lineText(ln, selL).replace(/…$/, '') : lineText(ln, selL)}</span>
              {ln.step === 'user' && ln.state === 'done' && ln.password && <SecretText value={ln.password} testId="appb-password" />}
            </div>
          ))}
        </div>
      )}
      {error && (
        <div className="hint err" data-testid="appb-error" style={{ whiteSpace: 'normal' }}>
          {t('appb.stopped')} <Msg msg={error} />
        </div>
      )}
    </Modal>
  );
}

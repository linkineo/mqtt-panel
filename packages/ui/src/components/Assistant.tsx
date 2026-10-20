import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { AssistantAnswer, AssistantEventBody, AssistantRequestKind, FileInspection, FileKind, UserMessage } from '@mqttpanel/engine-client';
import { engine, platform } from '../services';
import { useApp } from '../store/app';
import { useExplorer } from '../store/explorer';
import { useCharts } from '../store/charts';
import { Modal, Msg, NO_AUTO } from './ui';
import { toMessage } from '../i18n';

/**
 * The window side of the assistant interface (docs/MCP.md sections 3 and 4): toasts for writing calls, navigation with
 * "The assistant moved the view", refreshes after changes, the one-time acknowledgement dialog, and the prompts and
 * pickers the assistant asks for (ask_in_app). Mounted once in App.
 */
export function AssistantLayer() {
  const { t } = useTranslation();
  const [ack, setAck] = useState(false);
  const [requests, setRequests] = useState<Extract<AssistantEventBody, { kind: 'request' }>[]>([]);

  useEffect(
    () =>
      useApp.getState().onAssistant((e) => {
        const st = useApp.getState();
        switch (e.kind) {
          case 'toast':
            st.toast({ kind: 'info', text: e.text });
            break;
          case 'navigate':
            navigate(e);
            st.toast({ kind: 'info', text: t('assistant.movedView') });
            break;
          case 'changed':
            if (e.what === 'profiles') void st.refreshProfiles();
            if (e.what === 'broker') void st.refreshBrokerConfig();
            if (e.what === 'connections')
              void engine()
                .listConnections()
                .then((connections) => {
                  useApp.setState({ connections });
                  return st.refreshProfiles();
                })
                .catch(() => undefined);
            break;
          case 'acknowledge':
            if (!useApp.getState().settings.assistantAcknowledged) setAck(true);
            break;
          case 'acknowledgeDone':
            setAck(false);
            void engine()
              .getSettings()
              .then((settings) => useApp.setState({ settings }))
              .catch(() => undefined);
            break;
          case 'request':
            setRequests((r) => [...r.filter((x) => x.id !== e.id), e]);
            break;
          case 'requestDone':
            setRequests((r) => r.filter((x) => x.id !== e.id));
            break;
        }
      }),
    [t],
  );

  const req = requests[0];
  return (
    <>
      {ack && <AcknowledgeDialog onClose={() => setAck(false)} />}
      {req && (
        <RequestDialog
          key={req.id}
          kind={req.request}
          name={req.profileName}
          onAnswer={(answer) => {
            setRequests((r) => r.filter((x) => x.id !== req.id));
            void engine()
              .assistantAnswer(req.id, answer)
              .catch(() => undefined);
          }}
        />
      )}
    </>
  );
}

function navigate(e: Extract<AssistantEventBody, { kind: 'navigate' }>) {
  const st = useApp.getState();
  switch (e.screen) {
    case 'connect':
      if (e.profileId) st.selectProfile(e.profileId);
      st.setScreen('connect');
      break;
    case 'explore':
      if (e.profileId) {
        st.setActiveConn(e.profileId);
        if (e.topic) {
          useExplorer.getState().select(e.profileId, e.topic);
          void engine()
            .revealTopic(e.profileId, e.topic)
            .catch(() => undefined);
        }
      }
      st.setScreen('explore');
      break;
    case 'charts':
      if (e.chartId && useCharts.getState().charts.some((c) => c.id === e.chartId)) useCharts.getState().setActive(e.chartId);
      st.setScreen('charts');
      break;
    case 'broker':
      st.setScreen('broker');
      break;
    case 'settings':
      st.setSettingsPage(e.page ?? 'assistant');
      st.setScreen('settings');
      break;
  }
}

/** The one-time acknowledgement (docs/MCP.md section 3), also on Settings, Assistant. */
export function AcknowledgeDialog(props: { onClose(): void }) {
  const { t } = useTranslation();
  const host = useApp((s) => s.assistantHost);
  const accept = async () => {
    try {
      const st = await engine().assistantAccept();
      const s = useApp.getState();
      useApp.setState({ settings: { ...s.settings, assistantAcknowledged: st.acknowledged } });
    } catch (e) {
      useApp.getState().toast({ kind: 'err', msg: toMessage(e) });
    }
    props.onClose();
  };
  const decline = () => {
    void engine()
      .assistantDecline()
      .catch(() => undefined);
    props.onClose();
  };
  return (
    <Modal
      title={t('assistant.ackTitle')}
      onClose={decline}
      width={500}
      testId="assistant-ack"
      footer={
        <>
          <button type="button" className="btn" onClick={decline} data-testid="assistant-ack-later">
            {t('assistant.notNow')}
          </button>
          <button type="button" className="btn pri" onClick={() => void accept()} data-testid="assistant-ack-accept" data-autofocus>
            {t('assistant.accept')}
          </button>
        </>
      }
    >
      {host && <div className="small muted">{t('assistant.ackFrom', { host })}</div>}
      <p className="small" style={{ margin: '6px 0 0' }} data-testid="assistant-ack-text">
        {t('assistant.ackText')}
      </p>
    </Modal>
  );
}

/** One file of a request: Browse… (native dialog) or a drop on the row; shows the file name or the problem. */
function PickRow(props: { kind: FileKind; label: string; value: FileInspection | null; onValue(v: FileInspection): void; testId: string }) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<UserMessage | null>(null);
  const onValue = useRef(props.onValue);
  onValue.current = props.onValue;
  const take = (ins: FileInspection) => {
    setError(ins.ok ? null : ins.error);
    if (ins.ok) onValue.current(ins);
  };
  useEffect(
    () =>
      platform().onFileDrop((paths, x, y) => {
        const r = ref.current?.getBoundingClientRect();
        if (!r || !paths.length || x < r.left || x > r.right || y < r.top || y > r.bottom) return;
        void engine()
          .inspectFile(props.kind, paths[0])
          .then(take)
          .catch((e) => setError(toMessage(e)));
      }),
    [props.kind],
  );
  return (
    <div className="f">
      <span className="lbl">{props.label}</span>
      <div className="row2" ref={ref} data-testid={`file-${props.testId}`}>
        <span className={`input mono ro w-l${error ? ' err' : ''}`} aria-label={props.label}>
          {props.value?.fileName ?? <span className="ph-text">{t('file.none')}</span>}
        </span>
        <button
          type="button"
          className="btn sm"
          data-testid={`browse-${props.testId}`}
          onClick={() =>
            void engine()
              .pickFile(props.kind)
              .then((ins) => ins && take(ins))
              .catch((e) => setError(toMessage(e)))
          }
        >
          {t('common.browse')}
        </button>
        {error && (
          <span className="hint err">
            <Msg msg={error} />
          </span>
        )}
      </div>
    </div>
  );
}

/** What the assistant asks for (ask_in_app): the secret or file stays in the app; the assistant gets the result. */
export function RequestDialog(props: { kind: AssistantRequestKind; name: string; onAnswer(a: AssistantAnswer): void }) {
  const { t } = useTranslation();
  const [password, setPassword] = useState('');
  const [file, setFile] = useState<FileInspection | null>(null);
  const [key, setKey] = useState<FileInspection | null>(null);
  const [passphrase, setPassphrase] = useState('');
  const fileKind: FileKind = props.kind === 'password' ? 'caCert' : props.kind;
  const bundle = file?.ok && file.summary.kind === 'pkcs12';
  const encrypted = (file?.ok && file.summary.encrypted) || (key?.ok && key.summary.encrypted);
  const ready = props.kind === 'password' ? true : !!file?.ok && (props.kind !== 'clientCert' || bundle || !!key?.ok);
  const give = () => {
    if (!ready) return;
    if (props.kind === 'password') props.onAnswer({ password });
    else props.onAnswer({ path: file?.path ?? null, keyPath: !bundle ? (key?.path ?? null) : null, passphrase: passphrase || null });
  };
  return (
    <Modal
      title={t(`assistant.req.${props.kind}`, { name: props.name })}
      onClose={() => props.onAnswer({ cancelled: true })}
      width={560}
      testId="assistant-request"
      footer={
        <>
          <button type="button" className="btn" onClick={() => props.onAnswer({ cancelled: true })} data-testid="assistant-request-cancel">
            {t('common.cancel')}
          </button>
          <button type="button" className="btn pri" disabled={!ready} onClick={give} data-testid="assistant-request-ok">
            {t('common.save')}
          </button>
        </>
      }
    >
      <div className="small muted">{t(props.kind === 'password' ? 'assistant.req.secretText' : 'assistant.req.fileText')}</div>
      <div className="form" style={{ marginTop: 10 }}>
        {props.kind === 'password' ? (
          <div className="f">
            <label htmlFor="assistant-password">{t('assistant.req.passwordLabel')}</label>
            <input {...NO_AUTO} id="assistant-password" type="password" className="input w-l" value={password} data-autofocus data-testid="assistant-password" onChange={(e) => setPassword(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && give()} />
          </div>
        ) : (
          <>
            <PickRow kind={fileKind} label={t(props.kind === 'clientCert' ? 'assistant.req.cert' : 'assistant.req.file')} value={file} onValue={setFile} testId="assistant-file" />
            {props.kind === 'clientCert' && !bundle && <PickRow kind="clientKey" label={t('assistant.req.key')} value={key} onValue={setKey} testId="assistant-key" />}
            {props.kind === 'clientCert' && (bundle || encrypted) && (
              <div className="f">
                <label htmlFor="assistant-passphrase">{t('assistant.req.passphrase')}</label>
                <input {...NO_AUTO} id="assistant-passphrase" type="password" className="input w-l" value={passphrase} data-testid="assistant-passphrase" onChange={(e) => setPassphrase(e.target.value)} />
              </div>
            )}
          </>
        )}
      </div>
    </Modal>
  );
}

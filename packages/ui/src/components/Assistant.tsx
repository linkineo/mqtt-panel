import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { AssistantAnswer, AssistantEventBody, AssistantRequestKind, FileInspection, FileKind, UserMessage } from '@mqttpanel/engine-client';
import { engine, platform } from '../services';
import { useApp } from '../store/app';
import { useExplorer } from '../store/explorer';
import { useCharts, type Chart } from '../store/charts';
import { chartPngBase64, waitForChartCanvas } from '../lib/chartImage';
import { Modal, Msg, NO_AUTO } from './ui';
import i18n, { toMessage } from '../i18n';

/**
 * The window side of the assistant interface (docs/MCP.md sections 3 and 4): toasts for writing calls, navigation with
 * "The assistant moved the view", refreshes after changes, the one-time acknowledgement dialog, the prompts and
 * pickers the assistant asks for (ask_in_app: secrets, files, save places, folders, a new client certificate), the
 * chart image for export_chart, and the dashboard charts both ways (syncCharts, `charts` events). Mounted once in App.
 */
export function AssistantLayer() {
  const { t } = useTranslation();
  const [ack, setAck] = useState(false);
  const [requests, setRequests] = useState<Extract<AssistantEventBody, { kind: 'request' }>[]>([]);

  // The app side knows the dashboard charts (the assistant's chart tools read and change them).
  useEffect(() => {
    void engine()
      .syncCharts(useCharts.getState().charts)
      .catch(() => undefined);
    return useCharts.subscribe((s, prev) => {
      if (s.charts !== prev.charts)
        void engine()
          .syncCharts(s.charts)
          .catch(() => undefined);
    });
  }, []);

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
            if (e.what === 'broker') {
              void st.refreshBrokerConfig();
              void engine()
                .brokerStatus()
                .then((broker) => useApp.setState({ broker }))
                .catch(() => undefined);
            }
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
            if (e.request === 'chartPng') void answerChartPng(e.id, e.chartId ?? '');
            else setRequests((r) => [...r.filter((x) => x.id !== e.id), e]);
            break;
          case 'charts': {
            const charts = e.charts as Chart[];
            useCharts.setState((c) => ({ charts, activeId: c.activeId && charts.some((x) => x.id === c.activeId) ? c.activeId : (charts[0]?.id ?? null) }));
            break;
          }
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
          defaultName={req.defaultName}
          format={req.format}
          withPassphrase={req.withPassphrase}
          text={req.text}
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
      if (e.chartId) showChart(e.chartId);
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

/** Selects a dashboard chart and the connection it belongs to (the Charts screen shows the active connection). */
function showChart(chartId: string): boolean {
  const ch = useCharts.getState().charts.find((c) => c.id === chartId);
  if (!ch) return false;
  if (useApp.getState().profiles.some((p) => p.id === ch.connId)) useApp.getState().setActiveConn(ch.connId);
  useCharts.getState().setActive(chartId);
  return true;
}

/** export_chart as PNG: the window shows the chart, waits until it is drawn and answers with the image. */
async function answerChartPng(requestId: string, chartId: string) {
  const st = useApp.getState();
  let data: string | null = null;
  if (showChart(chartId)) {
    st.setScreen('charts');
    st.toast({ kind: 'info', text: i18n.t('assistant.movedView') });
    const canvas = await waitForChartCanvas();
    if (canvas) data = chartPngBase64(canvas);
  }
  await engine()
    .assistantAnswer(requestId, data ? { data } : { cancelled: true })
    .catch(() => undefined);
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

/** "Choose…" for a save place or a folder: shows the chosen path. */
function ChooseRow(props: { label: string; value: string | null; placeholder: string; onChoose(): void; testId: string }) {
  const { t } = useTranslation();
  return (
    <div className="f">
      <span className="lbl">{props.label}</span>
      <div className="row2" data-testid={`file-${props.testId}`}>
        <span className="input mono ro w-l" aria-label={props.label} title={props.value ?? undefined}>
          {props.value ?? <span className="ph-text">{props.placeholder}</span>}
        </span>
        <button type="button" className="btn sm" data-testid={`choose-${props.testId}`} onClick={props.onChoose}>
          {t('assistant.req.choose')}
        </button>
      </div>
    </div>
  );
}

/** What the assistant asks for (ask_in_app): the secret or file stays in the app; the assistant gets the result. */
export function RequestDialog(props: {
  kind: AssistantRequestKind;
  name: string;
  defaultName?: string;
  format?: 'pem' | 'pkcs12';
  withPassphrase?: boolean;
  /** confirm: "The assistant wants to …". */
  text?: string;
  onAnswer(a: AssistantAnswer): void;
}) {
  if (props.kind === 'confirm') return <ConfirmRequest text={props.text ?? ''} onAnswer={props.onAnswer} />;
  return <AskRequest {...props} />;
}

/**
 * Allow or Deny for an action of the assistant that weakens security, sends a stored credential elsewhere or writes
 * a file outside the usual folders (docs/MCP.md section 3). Deny is the default; closing the dialog denies; the app
 * side denies after 60 s without an answer.
 */
function ConfirmRequest(props: { text: string; onAnswer(a: AssistantAnswer): void }) {
  const { t } = useTranslation();
  const deny = () => props.onAnswer({ cancelled: true });
  return (
    <Modal
      title={t('assistant.req.confirm')}
      onClose={deny}
      width={520}
      testId="assistant-confirm-action"
      footer={
        <>
          <button type="button" className="btn" onClick={deny} data-testid="assistant-deny" data-autofocus>
            {t('assistant.deny')}
          </button>
          <button type="button" className="btn pri" onClick={() => props.onAnswer({})} data-testid="assistant-allow">
            {t('assistant.allow')}
          </button>
        </>
      }
    >
      <p className="small" style={{ margin: 0 }} data-testid="assistant-confirm-text">
        {props.text}
      </p>
      <div className="small muted" style={{ marginTop: 8 }}>
        {t('assistant.req.confirmText')}
      </div>
    </Modal>
  );
}

function AskRequest(props: {
  kind: AssistantRequestKind;
  name: string;
  defaultName?: string;
  format?: 'pem' | 'pkcs12';
  withPassphrase?: boolean;
  onAnswer(a: AssistantAnswer): void;
}) {
  const { t } = useTranslation();
  const [password, setPassword] = useState('');
  const [file, setFile] = useState<FileInspection | null>(null);
  const [key, setKey] = useState<FileInspection | null>(null);
  const [passphrase, setPassphrase] = useState('');
  const [place, setPlace] = useState<string | null>(null);
  const secret = props.kind === 'password' || props.kind === 'brokerPassword';
  const placeKind = props.kind === 'saveFile' || props.kind === 'folder' || props.kind === 'issueCert';
  const fileKind: FileKind = props.kind === 'clientCert' || props.kind === 'signedCert' ? props.kind : 'caCert';
  const bundle = file?.ok && file.summary.kind === 'pkcs12';
  const encrypted = (file?.ok && file.summary.encrypted) || (key?.ok && key.summary.encrypted);
  const askPassphrase = props.kind === 'issueCert' && !!props.withPassphrase;
  const needPassphrase = askPassphrase && props.format === 'pkcs12';
  const ready = secret
    ? props.kind === 'password' || password.length > 0
    : placeKind
      ? !!place && (!needPassphrase || passphrase.length > 0)
      : !!file?.ok && (props.kind !== 'clientCert' || bundle || !!key?.ok);
  const give = () => {
    if (!ready) return;
    if (secret) props.onAnswer({ password });
    else if (placeKind) props.onAnswer({ path: place, passphrase: askPassphrase && passphrase ? passphrase : null });
    else props.onAnswer({ path: file?.path ?? null, keyPath: !bundle ? (key?.path ?? null) : null, passphrase: passphrase || null });
  };
  const choose = () => {
    const pick = props.kind === 'saveFile' ? engine().assistantPickSave(props.defaultName ?? props.name) : engine().assistantPickFolder();
    void pick.then((p) => p && setPlace(p)).catch(() => undefined);
  };
  const textKey = secret
    ? 'assistant.req.secretText'
    : props.kind === 'saveFile'
      ? 'assistant.req.saveText'
      : props.kind === 'folder'
        ? 'assistant.req.folderText'
        : props.kind === 'issueCert'
          ? 'assistant.req.issueText'
          : 'assistant.req.fileText';
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
      <div className="small muted">{t(textKey)}</div>
      <div className="form" style={{ marginTop: 10 }}>
        {secret ? (
          <div className="f">
            <label htmlFor="assistant-password">{t('assistant.req.passwordLabel')}</label>
            <input {...NO_AUTO} id="assistant-password" type="password" className="input w-l" value={password} data-autofocus data-testid="assistant-password" onChange={(e) => setPassword(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && give()} />
          </div>
        ) : placeKind ? (
          <>
            <ChooseRow
              label={t(props.kind === 'saveFile' ? 'assistant.req.saveTo' : 'assistant.req.folderLabel')}
              value={place}
              placeholder={props.kind === 'saveFile' ? (props.defaultName ?? '') : t('file.none')}
              onChoose={choose}
              testId="assistant-place"
            />
            {askPassphrase && (
              <div className="f">
                <label htmlFor="assistant-passphrase">{t('assistant.req.passphrase')}</label>
                <input
                  {...NO_AUTO}
                  id="assistant-passphrase"
                  type="password"
                  className="input w-l"
                  value={passphrase}
                  placeholder={needPassphrase ? '' : t('common.optional')}
                  data-testid="assistant-passphrase"
                  onChange={(e) => setPassphrase(e.target.value)}
                />
              </div>
            )}
          </>
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

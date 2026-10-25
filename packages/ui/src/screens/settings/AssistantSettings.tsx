import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { AssistantHostFile, AssistantLogEntry, AssistantStatus, AssistantWriteReport } from '@mqttpanel/engine-client';
import { engine, platform } from '../../services';
import { useApp } from '../../store/app';
import { ConfirmDialog, Info, Msg } from '../../components/ui';
import { AcknowledgeDialog } from '../../components/Assistant';
import { fmtDate, fmtTime } from '../../lib/format';
import { toMessage } from '../../i18n';

/**
 * Settings, Assistant (docs/MCP.md section 6): the acknowledgement, the host session, "Add to Claude", "Add to
 * ChatGPT", "Other assistant apps" (snippet to copy) and the Assistant log (Clear, Export…). Every row has its ⓘ.
 */
export function AssistantSettings() {
  const { t } = useTranslation();
  const acknowledged = useApp((s) => s.settings.assistantAcknowledged);
  const connected = useApp((s) => s.assistantConnected);
  const host = useApp((s) => s.assistantHost);
  const logSeq = useApp((s) => s.assistantLogSeq);
  const [status, setStatus] = useState<AssistantStatus | null>(null);
  const [log, setLog] = useState<AssistantLogEntry[]>([]);
  const [confirm, setConfirm] = useState<'claude' | 'chatgpt' | null>(null);
  const [ackOpen, setAckOpen] = useState(false);

  const refresh = () =>
    void engine()
      .assistantStatus()
      .then(setStatus)
      .catch(() => undefined);
  useEffect(refresh, [connected, acknowledged]);
  useEffect(() => {
    void engine()
      .assistantLog()
      .then(setLog)
      .catch(() => undefined);
  }, [logSeq]);

  const files = (family: AssistantHostFile['family']) => (status?.hosts ?? []).filter((h) => h.family === family);
  const configured = (family: AssistantHostFile['family']) => files(family).filter((h) => h.configured);
  const hostName = (h: AssistantHostFile) => t(`assistant.host.${h.host}`);

  const names = (hosts: AssistantHostFile['host'][]) => {
    const n = hosts.map((h) => t(`assistant.host.${h}`));
    return n.length === 2 ? t('assistant.and', { a: n[0], b: n[1] }) : n.join(', ');
  };
  /** Says which assistant apps got MQTT Panel, which are not installed, and what to do next. */
  const report = (r: AssistantWriteReport) => {
    const st = useApp.getState();
    if (r.sandboxed) {
      sandboxed(r.writtenHosts.length ? r.writtenHosts : ['claudeDesktop']);
      return;
    }
    if (r.writtenHosts.length) {
      const parts = [t('assistant.added', { hosts: names(r.writtenHosts) })];
      if (r.missingHosts.length === 1) parts.push(t('assistant.notInstalled', { host: t(`assistant.host.${r.missingHosts[0]}`) }));
      else if (r.missingHosts.length > 1) parts.push(t('assistant.notInstalledPlural', { hosts: names(r.missingHosts) }));
      for (const h of r.writtenHosts) parts.push(t(`assistant.reopen.${h}`));
      st.toast({ kind: 'info', text: parts.join(' ') });
    } else if (!r.failed.length) {
      st.toast({ kind: 'warn', text: r.missingHosts.some((h) => h !== 'chatgpt') ? t('assistant.noClaude') : t('assistant.nothingWritten') });
    }
    for (const f of r.failed) st.toast({ kind: 'err', msg: f.error });
    refresh();
  };
  /** The Mac App Store copy cannot write other apps' files: copy the configuration and say where to paste it. */
  const sandboxed = (hosts: AssistantHostFile['host'][], snippet = status?.snippet ?? '') =>
    void platform()
      .copyText(snippet)
      .then(() => useApp.getState().toast({ kind: 'warn', text: t('assistant.sandboxed', { app: names(hosts) }) }));
  const write = async (family: 'claude' | 'chatgpt') => {
    try {
      report(family === 'claude' ? await engine().assistantAddToClaude() : await engine().assistantAddToChatgpt());
    } catch (e) {
      useApp.getState().toast({ kind: 'err', msg: toMessage(e) });
    }
  };
  /** Reads the files again at the click (an assistant app may have been installed since the page opened). */
  const add = async (family: 'claude' | 'chatgpt') => {
    let s = status;
    try {
      s = await engine().assistantStatus();
      setStatus(s);
    } catch {
      /* keep what the page has */
    }
    if (s?.sandboxed) {
      sandboxed(family === 'claude' ? ['claudeDesktop'] : ['chatgpt'], s.snippet);
      return;
    }
    if (family === 'claude' && !(s?.hosts ?? []).some((h) => h.family === 'claude' && (h.exists || h.installed))) {
      useApp.getState().toast({ kind: 'warn', text: t('assistant.noClaude') });
      return;
    }
    setConfirm(family);
  };

  const setUp = (family: AssistantHostFile['family']) => {
    const c = configured(family);
    return c.length ? t('assistant.setUpIn', { hosts: c.map(hostName).join(', ') }) : t('assistant.notSetUp');
  };

  return (
    <>
      <p className="small muted" style={{ maxWidth: '90ch', margin: '0 0 6px' }} data-testid="assistant-beta">
        {t('assistant.beta')}
      </p>
      <p className="small muted" style={{ maxWidth: '90ch', margin: '0 0 10px' }} data-testid="assistant-ack-sentence">
        {t('assistant.ackText')}
      </p>
      {status?.socketError && (
        <div className="callout err" style={{ maxWidth: '90ch', marginBottom: 10 }} role="alert" data-testid="assistant-socket-error">
          <Msg msg={status.socketError} />
        </div>
      )}
      <div className="form">
        <div className="f">
          <span className="lbl">
            {t('assistant.ack')} <Info id="assistant.ack" title={t('assistant.ack')} />
          </span>
          <span className="row2">
            {acknowledged ? (
              <span className="chip" data-testid="assistant-accepted">
                {t('assistant.acceptedOn', { date: fmtDate(Date.parse(`${acknowledged}T12:00:00Z`)) })}
              </span>
            ) : (
              <>
                <button type="button" className="btn sm pri" data-testid="assistant-accept" onClick={() => setAckOpen(true)}>
                  {t('assistant.accept')}
                </button>
                <span className="hint">{t('assistant.notAccepted')}</span>
              </>
            )}
          </span>
        </div>
        <div className="f">
          <span className="lbl">
            {t('assistant.session')} <Info id="assistant.session" title={t('assistant.session')} />
          </span>
          <span className="hint" data-testid="assistant-session">
            {connected || status?.connected ? t('assistant.connectedHost', { host: host || status?.host || t('assistant.someHost') }) : t('assistant.noSession')}
          </span>
        </div>
        <div className="f">
          <span className="lbl">
            Claude <Info id="assistant.claude" title="Claude" />
          </span>
          <span className="row2">
            <button type="button" className="btn sm" data-testid="assistant-add-claude" onClick={() => void add('claude')}>
              {t('assistant.addToClaude')}
            </button>
            <span className="hint" data-testid="assistant-claude-state">
              {setUp('claude')}
            </span>
          </span>
        </div>
        <div className="f">
          <span className="lbl">
            ChatGPT <Info id="assistant.chatgpt" title="ChatGPT" />
          </span>
          <span className="row2">
            <button type="button" className="btn sm" data-testid="assistant-add-chatgpt" onClick={() => void add('chatgpt')}>
              {t('assistant.addToChatgpt')}
            </button>
            <span className="hint" data-testid="assistant-chatgpt-state">
              {configured('chatgpt').length ? t('assistant.setUp') : t('assistant.desktopOnly')}
            </span>
          </span>
        </div>
        <div className="f">
          <span className="lbl">
            {t('assistant.other')} <Info id="assistant.other" title={t('assistant.other')} />
          </span>
          <span className="row2">
            <button
              type="button"
              className="btn sm"
              data-testid="assistant-copy"
              disabled={!status}
              onClick={() =>
                void platform()
                  .copyText(status?.snippet ?? '')
                  .then(() => useApp.getState().toast({ kind: 'info', text: t('common.copied') }))
              }
            >
              {t('assistant.copySnippet')}
            </button>
            <span className="hint">{t('assistant.otherHint')}</span>
          </span>
        </div>
      </div>
      {status && (
        <pre className="json" style={{ maxWidth: 640, marginTop: 8 }} data-testid="assistant-snippet">
          {status.snippet}
        </pre>
      )}
      <div className="form" style={{ marginTop: 12 }}>
        <div className="f">
          <span className="lbl">
            {t('assistant.log')} <Info id="assistant.log" title={t('assistant.log')} />
          </span>
          <span className="row2">
            <button type="button" className="btn sm" data-testid="assistant-export-log" disabled={!log.length} onClick={() => void engine().assistantExportLog().catch(() => undefined)}>
              {t('assistant.exportLog')}
            </button>
            <button type="button" className="btn sm" data-testid="assistant-clear-log" disabled={!log.length} onClick={() => void engine().assistantClearLog().then(() => setLog([]))}>
              {t('assistant.clearLog')}
            </button>
          </span>
        </div>
      </div>
      {log.length === 0 ? (
        <div className="hint" style={{ marginTop: 6 }} data-testid="assistant-log-empty">
          {t('assistant.logEmpty')}
        </div>
      ) : (
        <table className="tbl" style={{ marginTop: 8, maxWidth: 900 }} data-testid="assistant-log">
          <thead>
            <tr>
              <th>{t('assistant.col.time')}</th>
              <th>{t('assistant.col.host')}</th>
              <th>{t('assistant.col.tool')}</th>
              <th>{t('assistant.col.outcome')}</th>
              <th>{t('assistant.col.detail')}</th>
            </tr>
          </thead>
          <tbody>
            {log
              .slice()
              .reverse()
              .map((r, i) => (
                <tr key={`${r.at}-${i}`} data-testid="assistant-log-row" title={JSON.stringify(r.arguments)}>
                  <td className="mono">{fmtTime(r.at)}</td>
                  <td>{r.host}</td>
                  <td className="mono">{r.tool}</td>
                  <td>
                    <span className={`chip${r.outcome === 'ok' ? '' : ' clay'}`}>{t(`assistant.outcome.${r.outcome}`)}</span>
                  </td>
                  <td>
                    {r.detail}
                    {r.note && <span className="hint"> · {r.note}</span>}
                  </td>
                </tr>
              ))}
          </tbody>
        </table>
      )}
      {ackOpen && <AcknowledgeDialog onClose={() => setAckOpen(false)} />}
      {confirm && (
        <ConfirmDialog
          title={t(confirm === 'claude' ? 'assistant.confirmClaude' : 'assistant.confirmChatgpt')}
          confirmLabel={t('common.add')}
          testId="assistant-confirm"
          onClose={() => setConfirm(null)}
          onConfirm={() => void write(confirm)}
        >
          <div className="small">{t('assistant.confirmBody')}</div>
          <ul className="small mono" style={{ margin: '8px 0', paddingLeft: 18 }} data-testid="assistant-confirm-files">
            {files(confirm)
              .filter((h) => confirm === 'chatgpt' || h.exists || h.installed)
              .map((h) => (
                <li key={h.path}>
                  {h.path} <span className="muted">({hostName(h)})</span>
                </li>
              ))}
          </ul>
          {confirm === 'chatgpt' && <div className="small muted">{t('assistant.chatgptNote')}</div>}
          <div className="small muted">{t(`assistant.restart.${confirm}`)}</div>
        </ConfirmDialog>
      )}
    </>
  );
}

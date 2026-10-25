// The assistant (MCP) side of the mock: the acknowledgement, a virtual home with the hosts' configuration files (so
// "Add to Claude" and "Add to ChatGPT" can be checked end to end in the browser), the Assistant log and the answers
// the app gives to assistant requests.
import type { AssistantAnswer, AssistantChart, AssistantEventBody, AssistantHostFile, AssistantLogEntry, AssistantStatus, AssistantWriteReport } from '../types';

export const MOCK_EXECUTABLE = '/Applications/MQTT Panel.app/Contents/MacOS/MQTT Panel';
/** Where the mock's save and folder dialogs point (as displayed). */
export const MOCK_DOCUMENTS = '~/Documents';

const FILES = {
  claudeDesktop: '~/Library/Application Support/Claude/claude_desktop_config.json',
  claudeCode: '~/.claude.json',
  chatgpt: '~/.codex/config.toml',
} as const;

export class MockAssistant {
  /** Path (as displayed) → content. The Claude Desktop and Claude Code files exist; the Codex file does not. */
  home = new Map<string, string>([
    [FILES.claudeDesktop, '{\n  "mcpServers": {}\n}\n'],
    [FILES.claudeCode, '{\n  "numStartups": 4\n}\n'],
  ]);
  log: AssistantLogEntry[] = [];
  answers: { requestId: string; answer: AssistantAnswer }[] = [];
  session: string | null = null;
  charts: AssistantChart[] = [];
  pickCancelled = false;
  /** Simulates the Mac App Store variant (App Sandbox). */
  sandboxed = false;
  /** Simulates a socket (named pipe) that could not be opened at start. */
  socketError: AssistantStatus['socketError'] = null;

  constructor(private emit: (e: AssistantEventBody) => void) {}

  private jsonConfigured(path: string): boolean {
    try {
      const v = JSON.parse(this.home.get(path) ?? '');
      const e = v?.mcpServers?.mqttpanel;
      return e?.command === MOCK_EXECUTABLE && JSON.stringify(e?.args) === '["mcp"]';
    } catch {
      return false;
    }
  }

  hosts(): AssistantHostFile[] {
    const codex = this.home.get(FILES.chatgpt) ?? '';
    return [
      { family: 'claude', host: 'claudeDesktop', path: FILES.claudeDesktop, exists: this.home.has(FILES.claudeDesktop), installed: this.home.has(FILES.claudeDesktop), configured: this.jsonConfigured(FILES.claudeDesktop) },
      { family: 'claude', host: 'claudeCode', path: FILES.claudeCode, exists: this.home.has(FILES.claudeCode), installed: this.home.has(FILES.claudeCode), configured: this.jsonConfigured(FILES.claudeCode) },
      { family: 'chatgpt', host: 'chatgpt', path: FILES.chatgpt, exists: this.home.has(FILES.chatgpt), installed: true, configured: codex.includes('[mcp_servers.mqttpanel]') && codex.includes(MOCK_EXECUTABLE) },
    ];
  }

  status(acknowledged: string | null): AssistantStatus {
    return {
      acknowledged,
      connected: this.session !== null,
      host: this.session,
      executable: MOCK_EXECUTABLE,
      snippet: JSON.stringify({ mcpServers: { mqttpanel: { command: MOCK_EXECUTABLE, args: ['mcp'] } } }, null, 2),
      hosts: this.hosts(),
      sandboxed: this.sandboxed,
      socketError: this.socketError,
    };
  }

  addToClaude(): AssistantWriteReport {
    const r: AssistantWriteReport = { written: [], writtenHosts: [], missing: [], missingHosts: [], failed: [], sandboxed: this.sandboxed };
    if (this.sandboxed) return r;
    for (const [host, path, entry] of [
      ['claudeDesktop', FILES.claudeDesktop, { command: MOCK_EXECUTABLE, args: ['mcp'] }],
      ['claudeCode', FILES.claudeCode, { type: 'stdio', command: MOCK_EXECUTABLE, args: ['mcp'], env: {} }],
    ] as const) {
      const text = this.home.get(path);
      if (text === undefined) {
        r.missing.push(path);
        r.missingHosts.push(host);
        continue;
      }
      const v = JSON.parse(text || '{}');
      v.mcpServers = { ...(v.mcpServers ?? {}), mqttpanel: entry };
      this.home.set(path, JSON.stringify(v, null, 2) + '\n');
      r.written.push(path);
      r.writtenHosts.push(host);
    }
    return r;
  }

  addToChatgpt(): AssistantWriteReport {
    if (this.sandboxed) return { written: [], writtenHosts: [], missing: [], missingHosts: [], failed: [], sandboxed: true };
    const before = (this.home.get(FILES.chatgpt) ?? '').replace(/\n?\[mcp_servers\.mqttpanel\]\n(?:[^[\n].*\n?)*/g, '');
    const entry = `[mcp_servers.mqttpanel]\ncommand = "${MOCK_EXECUTABLE}"\nargs = ["mcp"]\n`;
    this.home.set(FILES.chatgpt, before ? `${before.replace(/\n*$/, '')}\n\n${entry}` : entry);
    return { written: [FILES.chatgpt], writtenHosts: ['chatgpt'], missing: [], missingHosts: [], failed: [], sandboxed: false };
  }

  logText(): string {
    const head = 'Time\tHost\tTool\tArguments\tOutcome\tDetail\n';
    return head + this.log.map((r) => `${new Date(r.at).toISOString()}\t${r.host}\t${r.tool}\t${JSON.stringify(r.arguments)}\t${r.outcome}\t${r.detail}${r.note ? ` (${r.note})` : ''}\n`).join('');
  }

  push(e: AssistantEventBody) {
    if (e.kind === 'session') this.session = e.connected ? e.host : null;
    this.emit(e);
  }
}

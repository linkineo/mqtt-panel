// The assistant (MCP) side of the mock: the acknowledgement, a virtual home with the hosts' configuration files (so
// "Add to Claude" and "Add to ChatGPT" can be checked end to end in the browser), the Assistant log and the answers
// the app gives to assistant requests.
import type { AssistantAnswer, AssistantEventBody, AssistantHostFile, AssistantLogEntry, AssistantStatus, AssistantWriteReport } from '../types';

export const MOCK_EXECUTABLE = '/Applications/MQTT Panel.app/Contents/MacOS/MQTT Panel';

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
      { family: 'claude', host: 'claudeDesktop', path: FILES.claudeDesktop, exists: this.home.has(FILES.claudeDesktop), configured: this.jsonConfigured(FILES.claudeDesktop) },
      { family: 'claude', host: 'claudeCode', path: FILES.claudeCode, exists: this.home.has(FILES.claudeCode), configured: this.jsonConfigured(FILES.claudeCode) },
      { family: 'chatgpt', host: 'chatgpt', path: FILES.chatgpt, exists: this.home.has(FILES.chatgpt), configured: codex.includes('[mcp_servers.mqttpanel]') && codex.includes(MOCK_EXECUTABLE) },
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
    };
  }

  addToClaude(): AssistantWriteReport {
    const r: AssistantWriteReport = { written: [], missing: [], failed: [] };
    for (const [path, entry] of [
      [FILES.claudeDesktop, { command: MOCK_EXECUTABLE, args: ['mcp'] }],
      [FILES.claudeCode, { type: 'stdio', command: MOCK_EXECUTABLE, args: ['mcp'], env: {} }],
    ] as const) {
      const text = this.home.get(path);
      if (text === undefined) {
        r.missing.push(path);
        continue;
      }
      const v = JSON.parse(text || '{}');
      v.mcpServers = { ...(v.mcpServers ?? {}), mqttpanel: entry };
      this.home.set(path, JSON.stringify(v, null, 2) + '\n');
      r.written.push(path);
    }
    return r;
  }

  addToChatgpt(): AssistantWriteReport {
    const before = (this.home.get(FILES.chatgpt) ?? '').replace(/\n?\[mcp_servers\.mqttpanel\]\n(?:[^[\n].*\n?)*/g, '');
    const entry = `[mcp_servers.mqttpanel]\ncommand = "${MOCK_EXECUTABLE}"\nargs = ["mcp"]\n`;
    this.home.set(FILES.chatgpt, before ? `${before.replace(/\n*$/, '')}\n\n${entry}` : entry);
    return { written: [FILES.chatgpt], missing: [], failed: [] };
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

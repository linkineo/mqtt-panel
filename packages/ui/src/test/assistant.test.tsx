import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { resources } from '../i18n';
import { boot } from './util';
import { useApp } from '../store/app';
import { AssistantSettings } from '../screens/settings/AssistantSettings';
import { AssistantLayer } from '../components/Assistant';
import { Toasts } from '../components/ui';

// Settings, Assistant and the window side of the assistant interface (docs/MCP.md sections 3 and 6; TESTPLAN M-10).

const ACK = 'Over this interface an assistant can change everything in MQTT Panel, including deleting connections, retained messages and broker settings. You stay in control of what you ask it.';

describe('Settings, Assistant', () => {
  it('shows the acknowledgement sentence of the spec and accepts it once', async () => {
    const { engine } = await boot();
    render(<AssistantSettings />);
    expect(screen.getByTestId('assistant-beta').textContent).toBe('Beta: tools and texts may still change; tell us what you need.');
    expect(screen.getByTestId('assistant-ack-sentence').textContent).toBe(ACK);
    fireEvent.click(screen.getByTestId('assistant-accept'));
    const dlg = await screen.findByTestId('assistant-ack');
    expect(within(dlg).getByTestId('assistant-ack-text').textContent).toBe(ACK);
    await act(async () => fireEvent.click(within(dlg).getByTestId('assistant-ack-accept')));
    await waitFor(() => expect(screen.getByTestId('assistant-accepted')).toBeTruthy());
    expect(useApp.getState().settings.assistantAcknowledged).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect((await engine.getSettings()).assistantAcknowledged).toBe(useApp.getState().settings.assistantAcknowledged);
    expect(screen.queryByTestId('assistant-accept')).toBeNull();
  });

  it('"Add to Claude" names the existing files, writes them and reports it', async () => {
    const { engine } = await boot();
    render(
      <>
        <AssistantSettings />
        <Toasts />
      </>,
    );
    await waitFor(() => expect(screen.getByTestId('assistant-claude-state').textContent).toBe('Not set up'));
    fireEvent.click(screen.getByTestId('assistant-add-claude'));
    const files = await screen.findByTestId('assistant-confirm-files');
    expect(files.textContent).toContain('~/Library/Application Support/Claude/claude_desktop_config.json');
    expect(files.textContent).toContain('~/.claude.json');
    await act(async () => fireEvent.click(screen.getByTestId('confirm-ok')));
    await waitFor(() => expect(screen.getByTestId('assistant-claude-state').textContent).toBe('Set up in Claude Desktop, Claude Code'));
    const toast = screen.getAllByTestId('toast').map((t) => t.textContent).join(' ');
    expect(toast).toContain('Added to Claude Desktop and Claude Code.');
    expect(toast).toContain('Quit and reopen Claude Desktop so it loads MQTT Panel.');
    expect(toast).toContain('Claude Code picks it up at its next start.');
    expect(toast).not.toMatch(/\bhost\b/);
    const code = JSON.parse(engine.hostFiles()['~/.claude.json']);
    expect(code.numStartups).toBe(4);
    expect(code.mcpServers.mqttpanel.args).toEqual(['mcp']);
  });

  it('"Add to Claude" without any Claude file says so and writes nothing', async () => {
    const { engine } = await boot();
    engine.setHostFile('~/.claude.json', null);
    engine.setHostFile('~/Library/Application Support/Claude/claude_desktop_config.json', null);
    render(
      <>
        <AssistantSettings />
        <Toasts />
      </>,
    );
    await waitFor(() => expect(screen.getByTestId('assistant-claude-state')).toBeTruthy());
    await act(async () => fireEvent.click(screen.getByTestId('assistant-add-claude')));
    expect(screen.queryByTestId('assistant-confirm')).toBeNull();
    expect(screen.getAllByTestId('toast').map((t) => t.textContent).join(' ')).toContain('Neither Claude Desktop nor Claude Code');
  });

  it('"Add to Claude" names the app that is not installed', async () => {
    const { engine } = await boot();
    engine.setHostFile('~/.claude.json', null);
    render(
      <>
        <AssistantSettings />
        <Toasts />
      </>,
    );
    await waitFor(() => expect(screen.getByTestId('assistant-claude-state').textContent).toBe('Not set up'));
    fireEvent.click(screen.getByTestId('assistant-add-claude'));
    const files = await screen.findByTestId('assistant-confirm-files');
    expect(files.textContent).not.toContain('~/.claude.json');
    await act(async () => fireEvent.click(screen.getByTestId('confirm-ok')));
    await waitFor(() => expect(screen.getAllByTestId('toast').map((t) => t.textContent).join(' ')).toContain('Added to Claude Desktop. Claude Code is not installed.'));
  });

  it('"Add to Claude" reads the files again at the click', async () => {
    const { engine } = await boot();
    engine.setHostFile('~/.claude.json', null);
    engine.setHostFile('~/Library/Application Support/Claude/claude_desktop_config.json', null);
    render(<AssistantSettings />);
    await waitFor(() => expect(screen.getByTestId('assistant-claude-state').textContent).toBe('Not set up'));
    // Claude Code is installed while the page is open
    engine.setHostFile('~/.claude.json', '{}');
    fireEvent.click(screen.getByTestId('assistant-add-claude'));
    const files = await screen.findByTestId('assistant-confirm-files');
    expect(files.textContent).toContain('~/.claude.json');
  });

  it('the App Store copy writes nothing, copies the configuration and says so', async () => {
    const { engine, platform } = await boot();
    engine.setAssistantSandboxed(true);
    render(
      <>
        <AssistantSettings />
        <Toasts />
      </>,
    );
    await waitFor(() => expect(screen.getByTestId('assistant-snippet')).toBeTruthy());
    await act(async () => fireEvent.click(screen.getByTestId('assistant-add-claude')));
    await waitFor(() => expect(screen.getAllByTestId('toast').map((t) => t.textContent).join(' ')).toContain('comes from the App Store'));
    expect(screen.queryByTestId('assistant-confirm')).toBeNull();
    expect(platform.lastCopied()).toContain('"mqttpanel"');
    expect(JSON.parse(engine.hostFiles()['~/.claude.json']).mcpServers).toBeUndefined();
  });

  it('"Add to ChatGPT" notes the desktop app and creates the Codex file', async () => {
    const { engine } = await boot();
    render(<AssistantSettings />);
    fireEvent.click(screen.getByTestId('assistant-add-chatgpt'));
    const dlg = await screen.findByTestId('assistant-confirm');
    expect(dlg.textContent).toContain('~/.codex/config.toml');
    expect(dlg.textContent).toContain('ChatGPT on the web cannot reach a program on this computer');
    expect(dlg.textContent).toContain('quit and reopen the ChatGPT app so it loads MQTT Panel.');
    await act(async () => fireEvent.click(screen.getByTestId('confirm-ok')));
    await waitFor(() => expect(screen.getByTestId('assistant-chatgpt-state').textContent).toBe('Set up'));
    expect(engine.hostFiles()['~/.codex/config.toml']).toContain('[mcp_servers.mqttpanel]\ncommand = "');
  });

  it('copies the snippet for other hosts and shows the log with Clear and Export…', async () => {
    const { engine, platform } = await boot();
    render(<AssistantSettings />);
    const snippet = await screen.findByTestId('assistant-snippet');
    expect(JSON.parse(snippet.textContent!).mcpServers.mqttpanel.args).toEqual(['mcp']);
    await act(async () => fireEvent.click(screen.getByTestId('assistant-copy')));
    expect(platform.lastCopied()).toBe(snippet.textContent);
    expect(screen.getByTestId('assistant-log-empty')).toBeTruthy();
    act(() => engine.addAssistantLog({ at: Date.now(), host: 'Claude Code', tool: 'set_connection_password', arguments: { id: 'plant', password: '•••' }, outcome: 'ok', detail: 'Saved the password of Plant broker.', note: 'secret given through the assistant' }));
    const row = await screen.findByTestId('assistant-log-row');
    expect(row.textContent).toContain('Claude Code');
    expect(row.textContent).toContain('set_connection_password');
    expect(row.textContent).toContain('secret given through the assistant');
    expect(row.getAttribute('title')).toContain('•••');
    await act(async () => fireEvent.click(screen.getByTestId('assistant-export-log')));
    await waitFor(() => expect(engine.savedFiles().map((f) => f.fileName)).toContain('mqttpanel-assistant-log.txt'));
    await act(async () => fireEvent.click(screen.getByTestId('assistant-clear-log')));
    await waitFor(() => expect(screen.getByTestId('assistant-log-empty')).toBeTruthy());
  });

  it('every row has its round ⓘ with a text in English, French and German', async () => {
    await boot();
    render(<AssistantSettings />);
    for (const id of ['ack', 'session', 'claude', 'chatgpt', 'other', 'log']) {
      expect(screen.getByTestId(`info-assistant.${id}`)).toBeTruthy();
      for (const lang of ['en', 'fr', 'de'] as const) expect((resources[lang].ui as Record<string, string>)[`info.assistant.${id}`], `${lang} ${id}`).toBeTruthy();
    }
  });
});

describe('Assistant layer', () => {
  it('shows the acknowledgement dialog when a host connects, and "Not now" declines', async () => {
    const { engine } = await boot();
    render(<AssistantLayer />);
    act(() => engine.assistantEvent({ kind: 'session', connected: true, host: 'Claude Desktop' }));
    act(() => engine.assistantEvent({ kind: 'acknowledge' }));
    const dlg = await screen.findByTestId('assistant-ack');
    expect(dlg.textContent).toContain('Claude Desktop is connected to MQTT Panel.');
    await act(async () => fireEvent.click(screen.getByTestId('assistant-ack-later')));
    expect(screen.queryByTestId('assistant-ack')).toBeNull();
    expect(useApp.getState().settings.assistantAcknowledged).toBeNull();
  });

  it('asks for a password in the app and answers the request; Cancel answers cancelled', async () => {
    const { engine } = await boot();
    render(<AssistantLayer />);
    act(() => engine.assistantEvent({ kind: 'request', id: 'r1', request: 'password', profileId: 'plant', profileName: 'Plant broker' }));
    const dlg = await screen.findByTestId('assistant-request');
    expect(dlg.textContent).toContain('The assistant asks for the password of Plant broker');
    fireEvent.change(screen.getByTestId('assistant-password'), { target: { value: 'typed-here' } });
    await act(async () => fireEvent.click(screen.getByTestId('assistant-request-ok')));
    expect(engine.assistantAnswers()).toEqual([{ requestId: 'r1', answer: { password: 'typed-here' } }]);
    act(() => engine.assistantEvent({ kind: 'request', id: 'r2', request: 'caCert', profileId: 'plant', profileName: 'Plant broker' }));
    await screen.findByTestId('assistant-request');
    expect((screen.getByTestId('assistant-request-ok') as HTMLButtonElement).disabled).toBe(true);
    await act(async () => fireEvent.click(screen.getByTestId('browse-assistant-file')));
    await waitFor(() => expect((screen.getByTestId('assistant-request-ok') as HTMLButtonElement).disabled).toBe(false));
    await act(async () => fireEvent.click(screen.getByTestId('assistant-request-ok')));
    expect(engine.assistantAnswers()[1].answer.path).toBe('/picked/plant-root-ca.pem');
    act(() => engine.assistantEvent({ kind: 'request', id: 'r3', request: 'password', profileId: 'plant', profileName: 'Plant broker' }));
    await screen.findByTestId('assistant-request');
    await act(async () => fireEvent.click(screen.getByTestId('assistant-request-cancel')));
    expect(engine.assistantAnswers()[2].answer).toEqual({ cancelled: true });
  });

  it('a request that timed out closes its prompt', async () => {
    const { engine } = await boot();
    render(<AssistantLayer />);
    act(() => engine.assistantEvent({ kind: 'request', id: 'r9', request: 'password', profileId: 'plant', profileName: 'Plant broker' }));
    await screen.findByTestId('assistant-request');
    act(() => engine.assistantEvent({ kind: 'requestDone', id: 'r9' }));
    await waitFor(() => expect(screen.queryByTestId('assistant-request')).toBeNull());
  });

  it('navigation opens the place and says the assistant moved the view; toasts show', async () => {
    const { engine } = await boot();
    render(
      <>
        <AssistantLayer />
        <Toasts />
      </>,
    );
    act(() => engine.assistantEvent({ kind: 'navigate', screen: 'settings', page: 'assistant' }));
    expect(useApp.getState().screen).toBe('settings');
    expect(useApp.getState().settingsPage).toBe('assistant');
    act(() => engine.assistantEvent({ kind: 'navigate', screen: 'connect', profileId: 'lab' }));
    expect(useApp.getState().screen).toBe('connect');
    expect(useApp.getState().selectedProfileId).toBe('lab');
    act(() => engine.assistantEvent({ kind: 'toast', text: 'Assistant: Connected Plant broker (MQTT 5).' }));
    const texts = screen.getAllByTestId('toast').map((t) => t.textContent);
    expect(texts.some((t) => t?.includes('The assistant moved the view.'))).toBe(true);
    expect(texts.some((t) => t?.includes('Assistant: Connected Plant broker'))).toBe(true);
  });
});

describe('Assistant texts name the application, never "the host" (25 Oct 2026)', () => {
  it('no assistant, info or mcp text says host, hôte or Host outside placeholders', () => {
    for (const lang of ['en', 'fr', 'de'] as const) {
      const all = resources[lang] as unknown as Record<string, Record<string, string>>;
      for (const ns of Object.values(all)) {
        for (const [k, v] of Object.entries(ns)) {
          if (!/^(assistant\.|info\.assistant\.|rail\.ai|mcp\.(?!prompt\.))/.test(k)) continue;
          expect(v.replace(/\{hosts?\}/g, ''), `${lang} ${k}`).not.toMatch(/\bhosts?\b|hôte/i);
        }
      }
    }
  });
});

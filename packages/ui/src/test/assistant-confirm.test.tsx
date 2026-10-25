import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { boot } from './util';
import { AssistantLayer } from '../components/Assistant';
import { AssistantSettings } from '../screens/settings/AssistantSettings';

// The confirmation of security-reducing assistant actions and the socket error on Settings, Assistant
// (docs/MCP.md section 3; TESTPLAN M-38, M-39).

const TEXT = 'The assistant wants to turn on anonymous access to the built-in broker. Any client that reaches it could connect without a login, read every topic and publish to your devices.';

describe('Assistant confirmations', () => {
  it('shows what the assistant wants; Allow answers without cancelled, Deny with it', async () => {
    const { engine } = await boot();
    render(<AssistantLayer />);
    act(() => engine.assistantEvent({ kind: 'request', id: 'c1', request: 'confirm', profileId: '', profileName: '', text: TEXT }));
    const dlg = await screen.findByTestId('assistant-confirm-action');
    expect(dlg.textContent).toContain('Allow this assistant action?');
    expect(screen.getByTestId('assistant-confirm-text').textContent).toBe(TEXT);
    expect(dlg.textContent).toContain('Without an answer within 60 s, the action is denied.');
    await act(async () => fireEvent.click(screen.getByTestId('assistant-allow')));
    expect(engine.assistantAnswers()).toEqual([{ requestId: 'c1', answer: {} }]);
    expect(screen.queryByTestId('assistant-confirm-action')).toBeNull();
    act(() => engine.assistantEvent({ kind: 'request', id: 'c2', request: 'confirm', profileId: '', profileName: '', text: TEXT }));
    await screen.findByTestId('assistant-confirm-action');
    await act(async () => fireEvent.click(screen.getByTestId('assistant-deny')));
    expect(engine.assistantAnswers()[1]).toEqual({ requestId: 'c2', answer: { cancelled: true } });
  });

  it('the dialog reads in French and German', async () => {
    const fr = await boot('demo', 'fr');
    const { unmount } = render(<AssistantLayer />);
    act(() => fr.engine.assistantEvent({ kind: 'request', id: 'c3', request: 'confirm', profileId: '', profileName: '', text: 'x' }));
    const dlg = await screen.findByTestId('assistant-confirm-action');
    expect(dlg.textContent).toContain('Autoriser cette action de l’assistant ?');
    expect(screen.getByTestId('assistant-allow').textContent).toBe('Autoriser');
    expect(screen.getByTestId('assistant-deny').textContent).toBe('Refuser');
    unmount();
    const de = await boot('demo', 'de');
    render(<AssistantLayer />);
    act(() => de.engine.assistantEvent({ kind: 'request', id: 'c4', request: 'confirm', profileId: '', profileName: '', text: 'x' }));
    expect((await screen.findByTestId('assistant-confirm-action')).textContent).toContain('Diese Aktion des Assistenten erlauben?');
    expect(screen.getByTestId('assistant-allow').textContent).toBe('Erlauben');
  });

  it('a denied call reads "denied" in the Assistant log', async () => {
    const { engine } = await boot();
    engine.addAssistantLog({ at: Date.now(), host: 'Claude', tool: 'broker_set_anonymous', arguments: { on: true }, outcome: 'denied', detail: 'The user denied this in MQTT Panel.' });
    engine.addAssistantLog({ at: Date.now(), host: 'Claude', tool: 'broker_set_reachability', arguments: { reach: 'whole_network' }, outcome: 'ok', detail: 'Reachable from the whole network.', note: 'confirmed by the user' });
    render(<AssistantSettings />);
    const rows = await screen.findAllByTestId('assistant-log-row');
    expect(rows.map((r) => r.textContent).join(' ')).toContain('denied');
    expect(rows.map((r) => r.textContent).join(' ')).toContain('confirmed by the user');
  });
});

describe('Assistant socket error', () => {
  it('Settings, Assistant shows why assistants cannot reach the app', async () => {
    const { engine } = await boot();
    engine.setAssistantSocketError({ key: 'mcp.socketTaken', params: {}, text: '' });
    render(<AssistantSettings />);
    await waitFor(() => expect(screen.getByTestId('assistant-socket-error').textContent).toContain("Another program already uses MQTT Panel's assistant connection point"));
  });

  it('nothing is shown when the socket listens', async () => {
    await boot();
    render(<AssistantSettings />);
    await waitFor(() => expect(screen.getByTestId('assistant-snippet')).toBeTruthy());
    expect(screen.queryByTestId('assistant-socket-error')).toBeNull();
  });
});

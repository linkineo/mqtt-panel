import { beforeEach, describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { AssistantChart } from '@mqttpanel/engine-client';
import { boot } from './util';
import { AssistantLayer } from '../components/Assistant';
import { useCharts } from '../store/charts';
import { useApp } from '../store/app';

// The window side of the assistant's broker, data and chart tools (docs/MCP.md section 4; TESTPLAN M-16, M-17): the
// save place, folder, new client certificate and broker password requests, and the dashboard charts both ways.

const ok = () => screen.getByTestId('assistant-request-ok') as HTMLButtonElement;

const CHART: AssistantChart = {
  id: 'ac1',
  name: 'Room 204',
  connId: 'plant',
  range: '1h',
  customFrom: null,
  customTo: null,
  paused: false,
  pausedAt: null,
  series: [{ id: 'as2', name: 'temperature', topic: 'building/f2/r204/temperature', path: '$', color: 'sage', style: 'line', axis: 'left', unit: '°C', enabled: true }],
};

beforeEach(() => {
  useCharts.setState({ charts: [], activeId: null, picker: null });
});

describe('Assistant requests for exports and certificates', () => {
  it('a save place is chosen in the app; the chat gets only the path', async () => {
    const { engine } = await boot();
    render(<AssistantLayer />);
    act(() => engine.assistantEvent({ kind: 'request', id: 'r1', request: 'saveFile', profileId: '', profileName: 'mqtt-export-20261020-1400.csv', defaultName: 'mqtt-export-20261020-1400.csv' }));
    const dlg = await screen.findByTestId('assistant-request');
    expect(dlg.textContent).toContain('The assistant saves mqtt-export-20261020-1400.csv');
    expect(dlg.textContent).toContain('The chat learns only its path.');
    expect(ok().disabled).toBe(true);
    // a cancelled dialog keeps the request open
    engine.setAssistantPickCancelled(true);
    await act(async () => fireEvent.click(screen.getByTestId('choose-assistant-place')));
    expect(ok().disabled).toBe(true);
    engine.setAssistantPickCancelled(false);
    await act(async () => fireEvent.click(screen.getByTestId('choose-assistant-place')));
    await waitFor(() => expect(screen.getByTestId('file-assistant-place').textContent).toContain('~/Documents/mqtt-export-20261020-1400.csv'));
    await act(async () => fireEvent.click(ok()));
    expect(engine.assistantAnswers()).toEqual([{ requestId: 'r1', answer: { path: '~/Documents/mqtt-export-20261020-1400.csv', passphrase: null } }]);
  });

  it('a folder for CA.pem is chosen in the app', async () => {
    const { engine } = await boot();
    render(<AssistantLayer />);
    act(() => engine.assistantEvent({ kind: 'request', id: 'r2', request: 'folder', profileId: '', profileName: 'CA.pem', defaultName: 'CA.pem' }));
    await screen.findByTestId('assistant-request');
    expect(screen.getByTestId('assistant-request').textContent).toContain('The assistant saves CA.pem');
    await act(async () => fireEvent.click(screen.getByTestId('choose-assistant-place')));
    await waitFor(() => expect(ok().disabled).toBe(false));
    await act(async () => fireEvent.click(ok()));
    expect(engine.assistantAnswers()[0].answer.path).toBe('~/Documents');
  });

  it('a new PKCS#12 certificate needs the folder and a passphrase typed in the app; PEM takes an optional one', async () => {
    const { engine } = await boot();
    render(<AssistantLayer />);
    act(() => engine.assistantEvent({ kind: 'request', id: 'r3', request: 'issueCert', profileId: '', profileName: 'sensor-12', format: 'pkcs12', withPassphrase: true }));
    const dlg = await screen.findByTestId('assistant-request');
    expect(dlg.textContent).toContain('The assistant issues the client certificate sensor-12');
    expect(dlg.textContent).toContain('the chat learns only the paths');
    await act(async () => fireEvent.click(screen.getByTestId('choose-assistant-place')));
    await waitFor(() => expect(screen.getByTestId('file-assistant-place').textContent).toContain('~/Documents'));
    expect(ok().disabled).toBe(true);
    fireEvent.change(screen.getByTestId('assistant-passphrase'), { target: { value: 'bundle-typed-here' } });
    expect(ok().disabled).toBe(false);
    await act(async () => fireEvent.click(ok()));
    expect(engine.assistantAnswers()[0].answer).toEqual({ path: '~/Documents', passphrase: 'bundle-typed-here' });
    act(() => engine.assistantEvent({ kind: 'request', id: 'r4', request: 'issueCert', profileId: '', profileName: 'sensor-13', format: 'pem', withPassphrase: true }));
    await screen.findByTestId('assistant-request');
    expect((screen.getByTestId('assistant-passphrase') as HTMLInputElement).placeholder).toBe('optional');
    await act(async () => fireEvent.click(screen.getByTestId('choose-assistant-place')));
    await waitFor(() => expect(ok().disabled).toBe(false));
    await act(async () => fireEvent.click(ok()));
    expect(engine.assistantAnswers()[1].answer).toEqual({ path: '~/Documents', passphrase: null });
    // the passphrase came through the chat: only the folder is asked
    act(() => engine.assistantEvent({ kind: 'request', id: 'r5', request: 'issueCert', profileId: '', profileName: 'sensor-14', format: 'pkcs12', withPassphrase: false }));
    await screen.findByTestId('assistant-request');
    expect(screen.queryByTestId('assistant-passphrase')).toBeNull();
  });

  it('the password of a broker user is typed in the app and cannot be empty', async () => {
    const { engine } = await boot();
    render(<AssistantLayer />);
    act(() => engine.assistantEvent({ kind: 'request', id: 'r6', request: 'brokerPassword', profileId: '', profileName: 'line-3' }));
    const dlg = await screen.findByTestId('assistant-request');
    expect(dlg.textContent).toContain('The assistant asks for the password of the broker user line-3');
    expect(ok().disabled).toBe(true);
    fireEvent.change(screen.getByTestId('assistant-password'), { target: { value: 'typed-for-line-3' } });
    await act(async () => fireEvent.click(ok()));
    expect(engine.assistantAnswers()[0].answer).toEqual({ password: 'typed-for-line-3' });
  });

  it('the requests read in French and German', async () => {
    const fr = await boot('demo', 'fr');
    const { unmount } = render(<AssistantLayer />);
    act(() => fr.engine.assistantEvent({ kind: 'request', id: 'r7', request: 'issueCert', profileId: '', profileName: 'capteur-1', format: 'pkcs12', withPassphrase: true }));
    expect((await screen.findByTestId('assistant-request')).textContent).toContain('L’assistant émet le certificat client capteur-1');
    expect(screen.getByTestId('choose-assistant-place').textContent).toBe('Choisir…');
    unmount();
    const de = await boot('demo', 'de');
    render(<AssistantLayer />);
    act(() => de.engine.assistantEvent({ kind: 'request', id: 'r8', request: 'saveFile', profileId: '', profileName: 'chart.csv', defaultName: 'chart.csv' }));
    expect((await screen.findByTestId('assistant-request')).textContent).toContain('Der Assistent speichert chart.csv');
  });
});

describe('Dashboard charts both ways', () => {
  it('the charts the assistant changed replace the list; changes in the window go back with syncCharts', async () => {
    const { engine } = await boot();
    render(<AssistantLayer />);
    act(() => engine.assistantEvent({ kind: 'charts', charts: [CHART] }));
    expect(useCharts.getState().charts.map((c) => c.id)).toEqual(['ac1']);
    expect(useCharts.getState().activeId).toBe('ac1');
    await waitFor(() => expect(engine.assistantCharts().map((c) => c.id)).toEqual(['ac1']));
    act(() => useCharts.getState().createChart('plant', 'Mine'));
    await waitFor(() => expect(engine.assistantCharts().map((c) => c.name)).toEqual(['Room 204', 'Mine']));
    act(() => engine.assistantEvent({ kind: 'charts', charts: [] }));
    expect(useCharts.getState().charts).toEqual([]);
    expect(useCharts.getState().activeId).toBeNull();
  });

  it('show_chart selects the chart and its connection', async () => {
    const { engine } = await boot();
    render(<AssistantLayer />);
    act(() => engine.assistantEvent({ kind: 'charts', charts: [CHART, { ...CHART, id: 'ac3', name: 'Lab', connId: 'lab', series: [] }] }));
    act(() => engine.assistantEvent({ kind: 'navigate', screen: 'charts', chartId: 'ac3' }));
    expect(useApp.getState().screen).toBe('charts');
    expect(useCharts.getState().activeId).toBe('ac3');
    expect(useApp.getState().activeConnId).toBe('lab');
  });

  it('a chart image for an unknown chart answers cancelled, without a dialog', async () => {
    const { engine } = await boot();
    render(<AssistantLayer />);
    act(() => engine.assistantEvent({ kind: 'request', id: 'r9', request: 'chartPng', profileId: '', profileName: 'Gone', chartId: 'ac404' }));
    await waitFor(() => expect(engine.assistantAnswers()).toEqual([{ requestId: 'r9', answer: { cancelled: true } }]));
    expect(screen.queryByTestId('assistant-request')).toBeNull();
  });
});

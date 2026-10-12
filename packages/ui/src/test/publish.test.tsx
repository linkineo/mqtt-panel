import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { boot, connect } from './util';
import { engine } from '../services';
import { usePublish } from '../store/publish';
import { PublishScreen } from '../screens/publish/PublishScreen';

describe('Publish (P-06)', () => {
  it('hides request and response on a 3.1.1 connection, with the one-line note', async () => {
    await boot('demo');
    await connect('lab');
    render(<PublishScreen />);
    expect(screen.queryByTestId('request-section')).toBeNull();
    expect(screen.getByTestId('request-unavailable').textContent).toBe('Request and response needs MQTT 5; this broker negotiated 3.1.1.');
  });

  it('shows request and response on MQTT 5', async () => {
    await boot('demo');
    await connect('plant');
    render(<PublishScreen />);
    expect(screen.getByTestId('request-section')).toBeInTheDocument();
    expect(screen.queryByTestId('request-unavailable')).toBeNull();
  });
});

describe('Publish (P-07)', () => {
  it('sending never creates a template; Save as template does', async () => {
    await boot('demo');
    await connect('plant');
    render(<PublishScreen />);
    const before = (await engine().listTemplates('plant')).length;
    act(() => usePublish.getState().setComposer({ topic: 'test/one', payload: 'hello', format: 'text', qos: 0 }));
    await act(async () => fireEvent.click(screen.getByTestId('send')));
    await waitFor(() => expect(screen.getAllByTestId('sent-row').length).toBeGreaterThan(0));
    expect(screen.getAllByTestId('sent-row')[0].textContent).toContain('test/one');
    expect((await engine().listTemplates('plant')).length).toBe(before);

    await act(async () => fireEvent.click(screen.getByTestId('save-template')));
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'My template' } });
    await act(async () => fireEvent.click(screen.getByTestId('prompt-ok')));
    await waitFor(async () => expect((await engine().listTemplates('plant')).length).toBe(before + 1));
    expect(await screen.findByText('My template')).toBeInTheDocument();
  });

  it('asks for placeholder values on send', async () => {
    await boot('demo');
    await connect('plant');
    render(<PublishScreen />);
    act(() => usePublish.getState().setComposer({ topic: 'building/f2/{room}/setpoint', payload: '{"value": 21}', format: 'json', qos: 0 }));
    await act(async () => fireEvent.click(screen.getByTestId('send')));
    expect(screen.getByTestId('placeholders')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('{room}'), { target: { value: 'r204' } });
    await act(async () => fireEvent.click(screen.getByTestId('placeholders-ok')));
    await waitFor(() => expect(screen.getAllByTestId('sent-row')[0].getAttribute('title') ?? screen.getAllByTestId('sent-row')[0].textContent).toContain('setpoint'));
    const log = await engine().sentLog('plant');
    expect(log[0].topic).toBe('building/f2/r204/setpoint');
  });
});

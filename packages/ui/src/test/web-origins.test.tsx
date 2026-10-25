import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { boot } from './util';
import { engine } from '../services';
import { useApp } from '../store/app';
import { BrokerScreen } from '../screens/broker/BrokerScreen';
import { WebOriginLayer } from '../components/WebOriginPrompt';

// SPEC 6, allowed web origins; TESTPLAN B-34 (UI side).

async function brokerScreen() {
  const b = await boot('demo');
  useApp.setState({ brokerConfig: await engine().brokerConfig(), screen: 'broker' });
  render(<BrokerScreen />);
  await waitFor(() => expect(screen.getAllByTestId('listener-row').length).toBeGreaterThan(0));
  return b;
}

const row = (id: string) => screen.getAllByTestId('listener-row').find((r) => r.getAttribute('data-listener') === id)!;

describe('Allowed web pages on a WS listener', () => {
  it('opens from a click on a WS row only, with chips, "+ Add…" and removal', async () => {
    await brokerScreen();
    // A TCP row has no detail; the Setup text is unchanged.
    fireEvent.click(row('l-tcp'));
    expect(screen.queryByTestId('listener-detail-l-tcp')).toBeNull();
    expect(row('l-ws').textContent).not.toContain('Allowed web pages');
    fireEvent.click(row('l-ws'));
    const detail = await screen.findByTestId('listener-detail-l-ws');
    expect(detail.textContent).toContain('Allowed web pages');
    expect(within(detail).getByTestId('no-origins').textContent).toBe('None');
    expect(within(detail).getByTestId('info-allowedOrigins')).toBeTruthy();
    // Add, with validation.
    fireEvent.click(within(detail).getByTestId('add-origin-l-ws'));
    const input = await screen.findByTestId('add-origin-input');
    fireEvent.change(input, { target: { value: 'dashboard.example' } });
    await act(async () => fireEvent.click(screen.getByTestId('add-origin-ok')));
    expect(screen.getByTestId('add-origin-dialog')).toBeTruthy();
    fireEvent.change(input, { target: { value: 'https://Dashboard.example/' } });
    await act(async () => fireEvent.click(screen.getByTestId('add-origin-ok')));
    await waitFor(() => expect(screen.queryByTestId('add-origin-dialog')).toBeNull());
    const chips = () => within(screen.getByTestId('listener-detail-l-ws')).queryAllByTestId('origin-chip').map((c) => c.textContent);
    expect(chips()).toEqual(['https://dashboard.example✕']);
    expect((await engine().brokerConfig()).listeners.find((l) => l.id === 'l-ws')!.allowedOrigins).toEqual(['https://dashboard.example']);
    // Remove.
    await act(async () => fireEvent.click(screen.getByTestId('remove-origin-https://dashboard.example')));
    await waitFor(() => expect(chips()).toEqual([]));
    // A click on the row again closes the detail; a click on the switch does not open it.
    fireEvent.click(row('l-ws'));
    expect(screen.queryByTestId('listener-detail-l-ws')).toBeNull();
  });
});

describe('Web page prompt', () => {
  it('shows the origin; Allow adds it to the listener, Deny refuses', async () => {
    const { engine: mock } = await boot('demo');
    render(<WebOriginLayer />);
    act(() => void mock.brokerOrigin('l-ws', 'https://page.example'));
    const dlg = await screen.findByTestId('web-origin-prompt');
    expect(dlg.textContent).toContain('Web page wants to connect');
    expect(screen.getByTestId('web-origin-text').textContent).toBe(
      'A web page at https://page.example wants to connect to the broker without a login. It will be able to read every topic and publish to your devices. Allow this website?',
    );
    // Deny is the default button.
    expect(document.activeElement).toBe(screen.getByTestId('origin-deny'));
    await act(async () => fireEvent.click(screen.getByTestId('origin-allow')));
    expect(screen.queryByTestId('web-origin-prompt')).toBeNull();
    expect((await engine().brokerConfig()).listeners.find((l) => l.id === 'l-ws')!.allowedOrigins).toEqual(['https://page.example']);
    act(() => void mock.brokerOrigin('l-ws', 'https://evil.example'));
    await screen.findByTestId('web-origin-prompt');
    await act(async () => fireEvent.click(screen.getByTestId('origin-deny')));
    expect(mock.originAnswers().map((a) => a.allow)).toEqual([true, false]);
    expect((await engine().brokerConfig()).listeners.find((l) => l.id === 'l-ws')!.allowedOrigins).toEqual(['https://page.example']);
  });

  it('reads in French and German', async () => {
    const fr = await boot('demo', 'fr');
    const { unmount } = render(<WebOriginLayer />);
    act(() => void fr.engine.brokerOrigin('l-ws', 'https://page.example'));
    expect((await screen.findByTestId('web-origin-text')).textContent).toBe(
      'Une page web de https://page.example veut se connecter au broker sans identifiant. Elle pourra lire chaque topic et publier vers vos appareils. Autoriser ce site web ?',
    );
    expect(screen.getByTestId('origin-allow').textContent).toBe('Autoriser');
    unmount();
    const de = await boot('demo', 'de');
    render(<WebOriginLayer />);
    act(() => void de.engine.brokerOrigin('l-ws', 'https://page.example'));
    expect((await screen.findByTestId('web-origin-prompt')).textContent).toContain('Eine Webseite möchte sich verbinden');
    expect(screen.getByTestId('origin-deny').textContent).toBe('Ablehnen');
  });
});

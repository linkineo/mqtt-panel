import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { boot } from './util';
import { engine } from '../services';
import { useApp } from '../store/app';
import { connectProfile, disconnect } from '../lib/actions';
import { App } from '../App';

describe('Owner feedback 28 Oct 2026: connected toast, right-click, disconnect all, back to the list', () => {
  it('toasts once in the app colour when a connection becomes connected', async () => {
    await boot('demo');
    render(<App />);
    await act(async () => void (await connectProfile('plant')));
    const toasts = screen.getAllByTestId('toast');
    expect(toasts).toHaveLength(1);
    expect(toasts[0].className).toContain('ok');
    expect(toasts[0].textContent).toContain('Connected to Plant broker.');
    // a later refresh of the same connection does not repeat it
    await act(async () => useApp.getState().handleEvent({ type: 'engine', connections: await engine().listConnections(), changed: {}, repeats: [], brokerLogSeq: 0, sentUpdates: [] } as never));
    expect(screen.getAllByTestId('toast')).toHaveLength(1);
  });

  it('blocks the WebView context menu except in text fields', async () => {
    await boot('demo');
    render(<App />);
    const onRail = fireEvent.contextMenu(screen.getByTestId('rail-explore'));
    expect(onRail).toBe(false); // preventDefault called
    const input = document.createElement('input');
    document.body.appendChild(input);
    expect(fireEvent.contextMenu(input)).toBe(true);
    input.remove();
  });

  it('right-click on Connect disconnects every connection and the app goes back to the client list', async () => {
    await boot('demo');
    render(<App />);
    await act(async () => void (await connectProfile('plant')));
    await act(async () => void (await connectProfile('local')));
    expect(screen.getAllByTestId('cbar-tab')).toHaveLength(2);
    expect(useApp.getState().screen).toBe('explore');
    await act(async () => fireEvent.contextMenu(screen.getByTestId('rail-connect')));
    const menu = screen.getByTestId('context-menu');
    expect(within(menu).getByText('Disconnect all connections')).toBeTruthy();
    await act(async () => fireEvent.click(within(menu).getByText('Disconnect all connections')));
    expect(screen.queryAllByTestId('cbar-tab')).toHaveLength(0);
    expect(useApp.getState().screen).toBe('connect');
  });

  it('closing the last tab with ✕ returns to the client list, but not from the Broker screen', async () => {
    await boot('demo');
    render(<App />);
    await act(async () => void (await connectProfile('plant')));
    useApp.getState().setScreen('broker');
    await act(async () => disconnect('plant'));
    expect(useApp.getState().screen).toBe('broker');
    await act(async () => void (await connectProfile('plant', false)));
    useApp.getState().setScreen('publish');
    await act(async () => disconnect('plant'));
    expect(useApp.getState().screen).toBe('connect');
  });
});

import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { boot, connect } from './util';
import { useApp } from '../store/app';
import { useExplorer } from '../store/explorer';
import { Detail } from '../screens/explorer/Detail';

const T = 'lab/devices/4/telemetry';
const U = 'lab/devices/5/telemetry';
const tick = () => act(async () => new Promise((r) => setTimeout(r, 20)));

/** The demo scenario plus two topics with known payloads (ids repeat; n always changes). */
async function setup(ids: string[]) {
  const { engine } = await boot('demo');
  await act(async () => connect('plant'));
  const at = Date.now() - 60_000;
  ids.forEach((id, i) => engine.inject('plant', T, JSON.stringify({ device: { id }, n: i }), { at: at + i * 1000 }));
  for (let i = 0; i < 3; i++) engine.inject('plant', U, JSON.stringify({ device: { id: 'u' }, n: i }), { at: at + i * 1000 });
  useApp.setState({ screen: 'explore' });
  useExplorer.setState({ byConn: {} });
  useExplorer.getState().patch('plant', { selected: T });
  return engine;
}

const values = () => screen.getAllByTestId('history-value').map((c) => c.textContent);

describe('History: owner feedback (filter on changes, count opens Raise, chevron, paused pane)', () => {
  it('the payload view keeps a two-line minimum height in a box', async () => {
    await setup(['a', 'a']);
    render(<Detail connId="plant" topic={T} openDialog={() => undefined} />);
    const box = await screen.findByTestId('payload-box');
    expect(box).toHaveClass('pl-box');
    await waitFor(() => expect(box.querySelector('.json')).not.toBeNull());
  });

  it('the count in the History title opens the Raise… prompt', async () => {
    await setup(['a', 'b', 'c']);
    render(<Detail connId="plant" topic={T} openDialog={() => undefined} />);
    await waitFor(() => expect(screen.getAllByTestId('history-row')).toHaveLength(3));
    expect(screen.getByTestId('history-count')).toHaveTextContent('3');
    await act(async () => fireEvent.click(screen.getByTestId('history-count')));
    expect(screen.getByRole('dialog')).toHaveTextContent('Buffer for this topic');
  });

  it('a field filter keeps only the rows where the field changed, plus the oldest row; the chip says so', async () => {
    await setup(['a', 'a', 'b', 'b', 'b', 'a']);
    render(<Detail connId="plant" topic={T} openDialog={() => undefined} />);
    await waitFor(() => expect(screen.getAllByTestId('history-row')).toHaveLength(6));
    await act(async () => useExplorer.getState().setHistoryFilter('plant', T, '$.device.id'));
    await waitFor(() => expect(values()).toEqual(['a', 'b', 'a']));
    expect(screen.getByTestId('history-filter-chip')).toHaveTextContent('Changes of device.id');
    // a field that changes every time keeps every row
    await act(async () => useExplorer.getState().setHistoryFilter('plant', T, '$.n'));
    await waitFor(() => expect(values()).toEqual(['5', '4', '3', '2', '1', '0']));
  });

  it('a constant field shows the oldest row only', async () => {
    await setup(['x', 'x', 'x', 'x']);
    render(<Detail connId="plant" topic={T} openDialog={() => undefined} />);
    await waitFor(() => expect(screen.getAllByTestId('history-row')).toHaveLength(4));
    await act(async () => useExplorer.getState().setHistoryFilter('plant', T, '$.device.id'));
    await waitFor(() => expect(values()).toEqual(['x']));
  });

  it('paused: the filter column fills and a chevron shows the payload even after the messages left the buffer', async () => {
    const engine = await setup(['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j']);
    await engine.setTopicBuffer('plant', T, 10);
    render(<Detail connId="plant" topic={T} openDialog={() => undefined} />);
    await waitFor(() => expect(screen.getAllByTestId('history-row')).toHaveLength(10));
    await act(async () => useExplorer.getState().patch('plant', { follow: false }));
    await tick();
    // the ring buffer moves on: every paused row's message is gone from the engine
    for (let i = 0; i < 12; i++) engine.inject('plant', T, JSON.stringify({ device: { id: 'z' }, n: 100 + i }));
    await act(async () => useExplorer.getState().setHistoryFilter('plant', T, '$.device.id'));
    await waitFor(() => expect(values()).toEqual(['j', 'i', 'h', 'g', 'f', 'e', 'd', 'c', 'b', 'a']));
    const last = screen.getAllByTestId('history-expand').at(-1)!;
    await act(async () => fireEvent.click(last));
    const exp = await screen.findByTestId('history-payload');
    await waitFor(() => expect(within(exp).getByTestId('history-json')).toHaveTextContent('"a"'));
  });

  it('paused: switching topics shows the new topic\'s rows, and its chevrons expand', async () => {
    await setup(['a', 'b']);
    const { rerender } = render(<Detail connId="plant" topic={T} openDialog={() => undefined} />);
    await waitFor(() => expect(screen.getAllByTestId('history-row')).toHaveLength(2));
    await act(async () => useExplorer.getState().patch('plant', { follow: false }));
    rerender(<Detail connId="plant" topic={U} openDialog={() => undefined} />);
    await waitFor(() => expect(screen.getAllByTestId('history-row')).toHaveLength(3));
    await act(async () => fireEvent.click(screen.getAllByTestId('history-expand')[0]));
    await waitFor(() => expect(screen.getByTestId('history-json')).toHaveTextContent('"u"'));
  });
});

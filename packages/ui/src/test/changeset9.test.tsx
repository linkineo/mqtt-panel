import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { lastSecondRate, pruneTimes } from '@mqttpanel/engine-client';
import type { DiffLine, TreeRow } from '@mqttpanel/engine-client';
import { boot, connect } from './util';
import { engine } from '../services';
import { useApp } from '../store/app';
import { useExplorer } from '../store/explorer';
import { Detail } from '../screens/explorer/Detail';
import { TreeRowView } from '../screens/explorer/Tree';
import { JsonTree, filterCell, treeOfDiff, treeOfJson, valueAt } from '../components/JsonView';
import { setSparklineOff, setSparklinePath, topicPrefsOf } from '../lib/topicPrefs';
import { topicMenu } from '../lib/topicActions';

const STATE = 'building/f2/ahu01/state';

async function detail(topic = STATE) {
  await boot('demo');
  await act(async () => connect('plant'));
  useApp.setState({ screen: 'explore' });
  useExplorer.setState({ byConn: {} });
  useExplorer.getState().patch('plant', { selected: topic });
  render(<Detail connId="plant" topic={topic} openDialog={() => undefined} />);
  await screen.findByTestId('detail');
  await waitFor(() => expect(document.querySelector('[data-testid=payload-json], [data-testid=payload-text]')).not.toBeNull());
}

const menuLabels = () => Array.from(screen.getByTestId('json-menu').querySelectorAll('.ctx-label')).map((x) => x.textContent);

describe('Change set 9: one-second rate (X-24)', () => {
  it('50 messages in one second read 50 during the whole next second, then 0; the running second does not count', () => {
    const times = Array.from({ length: 50 }, (_, i) => 10_000 + i * 19);
    expect(lastSecondRate(times, 10_500)).toBe(0); // first value only after the first full second
    expect(lastSecondRate(times, 11_000)).toBe(50);
    expect(lastSecondRate(times, 11_999)).toBe(50);
    times.push(11_200, 11_400);
    expect(lastSecondRate(times, 11_900)).toBe(50); // unchanged between two ticks
    expect(lastSecondRate(times, 12_000)).toBe(2);
    expect(lastSecondRate(times, 13_000)).toBe(0);
    pruneTimes(times, 12_500);
    expect(times).toEqual([11_200, 11_400]);
  });

  it('the mock engine reports the same figure on the connection and the tree rows', async () => {
    let now = 100_000;
    const { createMockEngineClient } = await import('@mqttpanel/engine-client/mock');
    const e = createMockEngineClient({ scenario: 'demo', lang: 'en', live: false, latencyMs: 0, now: () => now });
    const out = await e.connectProfile('plant');
    expect(out.kind).toBe('connected');
    now = 200_000;
    for (let i = 0; i < 50; i++) e.inject('plant', 'rate/x', String(i), { at: 200_000 + i * 19 });
    now = 201_000;
    const conn = (await e.listConnections()).find((c) => c.id === 'plant')!;
    expect(conn.msgRate).toBe(50);
    const rateRow = async () => (await e.treeSlice('plant', { search: 'rate', filters: { retainedOnly: false, changedWithinMin: null, payloadContains: '', qos: null, hasValue: false } } as never, 0, 10)).rows.find((r) => r.path === 'rate/x');
    await e.setExpandedSet('plant', ['rate']);
    expect((await rateRow())?.rate).toBe(50);
    now = 201_900;
    e.inject('plant', 'rate/x', 'late', { at: 201_500 });
    expect((await e.listConnections()).find((c) => c.id === 'plant')!.msgRate).toBe(50);
    now = 202_000;
    expect((await e.listConnections()).find((c) => c.id === 'plant')!.msgRate).toBe(1);
  });

  it('the rate reads "n msg/s" in EN, FR, DE', async () => {
    for (const lang of ['en', 'fr', 'de'] as const) {
      await boot('demo', lang);
      const { unmount } = render(<TreeRowView row={row({ rate: 5 })} selected={false} cursor={false} reducedMotion={false} valuesInTree now={0} />);
      expect(screen.getByTestId('tree-rate')).toHaveTextContent('5 msg/s');
      unmount();
    }
  });
});

function row(p: Partial<TreeRow> = {}): TreeRow {
  return {
    path: 'building/f2/r204/setpoint',
    name: 'setpoint',
    depth: 3,
    hasChildren: false,
    expanded: false,
    topicCount: 1,
    messages: 4,
    lastReceivedAt: 1,
    rate: 0,
    isTopic: true,
    retained: true,
    value: '{"value":21.5}',
    format: 'json',
    formatHint: null,
    spark: [22, 22, 21.5],
    sparkTimes: [1, 2, 3],
    pinned: false,
    ...p,
  } as TreeRow;
}

describe('Change set 9: aligned columns and sparkline right-click (X-25, X-28)', () => {
  it('every row renders the same four cells; empty cells stay', async () => {
    await boot('demo');
    const { container } = render(
      <>
        <TreeRowView row={row()} selected={false} cursor={false} reducedMotion={false} valuesInTree now={0} />
        <TreeRowView row={row({ path: 'building/f2', name: 'f2', hasChildren: true, isTopic: false, topicCount: 12, spark: [], value: null })} selected={false} cursor={false} reducedMotion={false} valuesInTree now={0} />
      </>,
    );
    const shapes = Array.from(container.querySelectorAll('.v')).map((v) => Array.from(v.children).map((c) => c.className));
    expect(shapes).toEqual([
      ['c-spark', 'msgs', 'rate', 'c-cnt'],
      ['c-spark', 'msgs', 'rate', 'c-cnt'],
    ]);
    expect(container.querySelectorAll('.c-spark')[1].textContent).toBe('');
  });

  it('right-click on the sparkline opens its own menu, not the topic menu; a hidden sparkline is not drawn', async () => {
    await boot('demo');
    const onSpark = vi.fn();
    const onRow = vi.fn();
    const { rerender } = render(<TreeRowView row={row()} selected={false} cursor={false} reducedMotion={false} valuesInTree now={0} onSparkMenu={onSpark} onContextMenu={onRow} />);
    fireEvent.contextMenu(screen.getByTestId('tree-spark'));
    expect(onSpark).toHaveBeenCalledTimes(1);
    expect(onRow).not.toHaveBeenCalled();
    rerender(<TreeRowView row={row()} selected={false} cursor={false} reducedMotion={false} valuesInTree sparkOff now={0} />);
    expect(screen.queryByTestId('tree-spark')).toBeNull();
    expect(screen.getByTestId('tree-spark-cell')).toBeInTheDocument();
  });

  it('sparklineOff is saved; the topic menu offers "Show sparkline" while off; a new pick clears it', async () => {
    await boot('demo');
    await act(async () => connect('plant'));
    const topic = 'building/f2/r204/temperature';
    await act(async () => setSparklineOff('plant', topic, true));
    expect((await engine().listProfiles()).find((p) => p.id === 'plant')!.topics[topic].sparklineOff).toBe(true);
    const p = topicMenu('plant', topic, true, 10, 10, true, () => undefined);
    const menu = await screen.findByTestId('context-menu');
    const item = within(menu).getByText('Show sparkline');
    await act(async () => fireEvent.click(item));
    await p;
    await waitFor(() => expect(topicPrefsOf('plant', topic).sparklineOff).toBeUndefined());
    await act(async () => setSparklineOff('plant', topic, true));
    await act(async () => setSparklinePath('plant', topic, '$.value'));
    expect(topicPrefsOf('plant', topic)).toEqual({ sparklinePath: '$.value' });
  });
});

describe('Change set 9: paths and the Tree model', () => {
  it('valueAt reads dotted, indexed and quoted paths; blank when absent', () => {
    const j = { supply: { temp: 18.2 }, list: [1, { a: 'x' }], 'a b': true };
    expect(valueAt(j, '$')).toBe(j);
    expect(valueAt(j, '$.supply.temp')).toBe(18.2);
    expect(valueAt(j, '$.list[1].a')).toBe('x');
    expect(valueAt(j, '$["a b"]')).toBe(true);
    expect(valueAt(j, '$.missing')).toBeUndefined();
    expect(valueAt(j, '$.list[5]')).toBeUndefined();
    expect(filterCell(undefined)).toBe('');
    expect(filterCell('cooling')).toBe('cooling');
    expect(filterCell(null)).toBe('null');
    expect(filterCell(19.6)).toBe('19.6');
  });

  it('Tree renders folding key rows, indexed arrays and coloured scalars; a fold hides the children', async () => {
    await boot('demo');
    const root = treeOfJson({ mode: 'cooling', supply: { temp: 18.2, fan: 72 }, list: [true, null] });
    render(<JsonTree root={root} valueOf={() => null} testId="t" />);
    const rows = () => Array.from(screen.getByTestId('t').querySelectorAll('[data-testid=jtree-row]')).map((r) => r.getAttribute('data-path'));
    expect(rows()).toEqual(['$.mode', '$.supply', '$.supply.temp', '$.supply.fan', '$.list', '$.list[0]', '$.list[1]']);
    const temp = screen.getByTestId('t').querySelector('[data-path="$.supply.temp"]')!;
    expect(temp.querySelector('.k')!.textContent).toBe('temp');
    expect(temp.querySelector('.num')!.textContent).toBe('18.2');
    expect(screen.getByTestId('t').querySelector('[data-path="$.list[0]"] .k')!.textContent).toBe('[0]');
    expect(screen.getByTestId('t').querySelector('[data-path="$.mode"] .s')!.textContent).toBe('"cooling"');
    fireEvent.click(screen.getByTestId('t').querySelector('[data-path="$.supply"] .tchev')!);
    expect(rows()).toEqual(['$.mode', '$.supply', '$.list', '$.list[0]', '$.list[1]']);
  });

  it('the diff colours apply per row', async () => {
    await boot('demo');
    const L = (p: Partial<DiffLine>): DiffLine => ({ indent: 0, key: null, path: null, oldValue: null, newValue: null, open: null, close: null, kind: 'same', plottable: false, ...p });
    const lines: DiffLine[] = [
      L({ open: '{' }),
      L({ indent: 1, key: 'mode', path: '$.mode', oldValue: '"heating"', newValue: '"cooling"', kind: 'changed' }),
      L({ indent: 1, key: 'supply', open: '{' }),
      L({ indent: 2, key: 'temp', path: '$.supply.temp', newValue: '18.2', kind: 'same', plottable: true }),
      L({ indent: 2, key: 'new', path: '$.supply.new', newValue: '1', kind: 'added', plottable: true }),
      L({ indent: 1, close: '}' }),
      L({ indent: 1, key: 'gone', path: '$.gone', oldValue: '3', kind: 'removed' }),
      L({ close: '}' }),
    ];
    const root = treeOfDiff(lines)!;
    render(<JsonTree root={root} highlight valueOf={() => null} testId="t" />);
    const r = (p: string) => screen.getByTestId('t').querySelector(`[data-path="${p}"]`)!;
    expect(r('$.mode')).toHaveClass('chg');
    expect(r('$.mode').querySelector('.del')!.textContent).toBe('"heating"');
    expect(r('$.mode').querySelector('.add')!.textContent).toBe('"cooling"');
    expect(r('$.supply.new').querySelector('.num.add')!.textContent).toBe('1');
    expect(r('$.gone').querySelector('.num.del')!.textContent).toBe('3');
    expect(r('$.supply.temp').querySelector('.add, .del')).toBeNull();
  });
});

describe('Change set 9: JSON / Tree and History (X-26, X-27)', () => {
  it('the segment switches the payload to Tree, the click menu works on a row, and the choice is kept per connection', async () => {
    await detail();
    const seg = screen.getByTestId('payload-form');
    await act(async () => fireEvent.click(within(seg).getByText('Tree')));
    const tree = await screen.findByTestId('payload-tree');
    expect(useExplorer.getState().get('plant').payloadForm).toBe('tree');
    await act(async () => fireEvent.click(tree.querySelector('[data-path="$.supply.temp"]')!));
    expect(menuLabels()).toEqual(['Use as sparkline', 'Add supply.temp to chart…', 'Show supply.temp in History', 'Copy path · $.supply.temp', expect.stringMatching(/^Copy value · /)]);
  });

  it('"Show <field> in History" puts the path in the header, its value per row and the chip; the entry, the chip or the button cancels', async () => {
    await detail();
    const json = screen.getByTestId('payload-json');
    await act(async () => fireEvent.click(json.querySelector('.k[data-path="$.supply.temp"]')!));
    await act(async () => fireEvent.click(within(screen.getByTestId('json-menu')).getByText('Show supply.temp in History')));
    expect(useExplorer.getState().get('plant').historyFilter[STATE]).toBe('$.supply.temp');
    expect(screen.getByTestId('history-col')).toHaveTextContent('supply.temp');
    expect(screen.getByTestId('history-filter-chip')).toHaveTextContent('supply.temp');
    expect(screen.getByTestId('history-filter')).toHaveAttribute('aria-pressed', 'true');
    await waitFor(() => {
      const cells = screen.getAllByTestId('history-value').map((c) => c.textContent);
      expect(cells.length).toBeGreaterThan(0);
      expect(cells.every((c) => c !== '' && !isNaN(Number(c)))).toBe(true);
    });
    // the menu entry now cancels
    await act(async () => fireEvent.click(screen.getByTestId('payload-json').querySelector('.k[data-path="$.supply.temp"]')!));
    await act(async () => fireEvent.click(within(screen.getByTestId('json-menu')).getByText('Stop showing supply.temp in History')));
    expect(screen.queryByTestId('history-filter-chip')).toBeNull();
    expect(screen.getByTestId('history-col')).toHaveTextContent('Changed keys');
    // the filter button opens the field list; a choice applies, the chip's ✕ cancels
    await act(async () => fireEvent.click(screen.getByTestId('history-filter')));
    const pop = await screen.findByTestId('history-filter-pop');
    await act(async () => fireEvent.click(pop.querySelector('[data-path="$.mode"]')!));
    expect(screen.getByTestId('history-filter-chip')).toHaveTextContent('mode');
    await act(async () => fireEvent.click(screen.getByTestId('history-filter-clear')));
    expect(screen.queryByTestId('history-filter-chip')).toBeNull();
    // a typed path, then the button cancels
    await act(async () => fireEvent.click(screen.getByTestId('history-filter')));
    fireEvent.change(screen.getByTestId('history-filter-path'), { target: { value: 'supply.fan' } });
    await act(async () => fireEvent.click(screen.getByTestId('history-filter-apply')));
    expect(useExplorer.getState().get('plant').historyFilter[STATE]).toBe('$.supply.fan');
    await act(async () => fireEvent.click(screen.getByTestId('history-filter')));
    expect(useExplorer.getState().get('plant').historyFilter[STATE]).toBeUndefined();
  });

  it('a chevron expands a row in place in the chosen form; several rows stay open; the row click still selects', async () => {
    await detail();
    const chevrons = screen.getAllByTestId('history-expand');
    expect(chevrons.length).toBeGreaterThan(1);
    await act(async () => fireEvent.click(chevrons[0]));
    await act(async () => fireEvent.click(chevrons[1]));
    await waitFor(() => expect(screen.getAllByTestId('history-json')).toHaveLength(2));
    expect(useExplorer.getState().get('plant').viewSeq).toBeNull();
    expect(screen.getAllByTestId('history-payload')[0].querySelector('.add, .del')).toBeNull();
    await act(async () => fireEvent.click(within(screen.getByTestId('payload-form')).getByText('Tree')));
    await waitFor(() => expect(screen.getAllByTestId('history-tree')).toHaveLength(2));
    await act(async () => fireEvent.click(screen.getAllByTestId('history-expand')[0]));
    await waitFor(() => expect(screen.getAllByTestId('history-tree')).toHaveLength(1));
    const second = screen.getAllByTestId('history-row')[1];
    await act(async () => fireEvent.click(second.querySelector('td.mono')!));
    expect(useExplorer.getState().get('plant').viewSeq).toBe(Number(second.getAttribute('data-seq')));
  });
});

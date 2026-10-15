import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { boot, connect } from './util';
import { engine } from '../services';
import { useApp } from '../store/app';
import { useCharts } from '../store/charts';
import { useExplorer } from '../store/explorer';
import { App } from '../App';
import { Detail } from '../screens/explorer/Detail';
import { FieldPicker, FieldPickerView } from '../screens/explorer/FieldPicker';
import { ChartsScreen } from '../screens/charts/ChartsScreen';
import { TreeRowView } from '../screens/explorer/Tree';
import { setSparklinePath } from '../lib/topicPrefs';
import type { FieldInfo, TreeRow } from '@mqttpanel/engine-client';

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

describe('Change set 8: no auto-capitalisation (V-11)', () => {
  const root = join(__dirname, '..');
  const files = (dir: string): string[] =>
    readdirSync(dir).flatMap((f) => {
      const p = join(dir, f);
      if (statSync(p).isDirectory()) return f === 'test' ? [] : files(p);
      return /\.tsx$/.test(f) ? [p] : [];
    });

  it('every <input and <textarea in the sources carries NO_AUTO (the lint rule)', () => {
    const bad: string[] = [];
    for (const f of files(root)) {
      const s = readFileSync(f, 'utf8');
      for (const m of s.matchAll(/<(input|textarea)(?=[\s/>])[^>]*/g)) if (!m[0].includes('{...NO_AUTO}')) bad.push(`${f.replace(root, 'src')}: ${m[0].slice(0, 40)}`);
    }
    expect(bad, bad.join('\n')).toEqual([]);
  });

  it('every input and textarea on every screen declares autocapitalize, autocorrect and spellcheck off', async () => {
    await boot('demo');
    await act(async () => connect('plant'));
    useApp.setState({ broker: await engine().brokerStatus(), brokerConfig: await engine().brokerConfig() });
    useApp.setState({ folds: { security: true, advanced: true, brokerAdvanced: true } });
    useApp.getState().selectProfile('plant');
    useExplorer.getState().patch('plant', { selected: STATE });
    useCharts.getState().createChart('plant', 'Room 204');
    useCharts.getState().addSeries(useCharts.getState().charts[0].id, [{ topic: 'building/f2/r204/temperature', path: '$.value', name: 'temperature', step: false }]);
    render(<App />);
    const seen = new Set<string>();
    for (const s of ['connect', 'explore', 'publish', 'charts', 'broker', 'settings'] as const) {
      await act(async () => useApp.getState().setScreen(s));
      await act(async () => new Promise((r) => setTimeout(r, 30)));
      const fields = Array.from(document.querySelectorAll('input, textarea')).filter((e) => !['checkbox', 'radio', 'file', 'range', 'hidden'].includes((e as HTMLInputElement).type));
      if (s !== 'broker') expect(fields.length, s).toBeGreaterThan(0);
      for (const el of fields) {
        const id = `${s}:${el.getAttribute('data-testid') ?? el.getAttribute('aria-label') ?? el.id}`;
        seen.add(id);
        expect(el.getAttribute('autocapitalize'), id).toBe('off');
        expect(el.getAttribute('autocorrect'), id).toBe('off');
        expect(el.getAttribute('spellcheck'), id).toBe('false');
      }
    }
    expect(seen.size).toBeGreaterThan(20);
    // a typed host name keeps its lower case
    await act(async () => useApp.getState().setScreen('connect'));
    const cid = document.getElementById('pf-cid') as HTMLInputElement;
    fireEvent.change(cid, { target: { value: 'myclient' } });
    expect(cid.value).toBe('myclient');
    expect(document.getElementById('pf-host')).toHaveAttribute('autocapitalize', 'off');
  });
});

describe('Change set 8: connection bar always shown (X-22)', () => {
  it('with nothing open: one strip in the main column, empty since change set 9 (X-23)', async () => {
    await boot('demo');
    useApp.setState({ screen: 'explore' });
    render(<App />);
    const bar = screen.getByTestId('cbar');
    expect(bar.closest('.main-col')).not.toBeNull();
    expect(document.querySelector('.body > .rail')).not.toBeNull();
    expect(bar).toHaveAttribute('data-empty', 'true');
    expect(bar.textContent).toBe('');
    expect(bar.children).toHaveLength(0);
  });
});

describe('Change set 8: tree rows (X-15)', () => {
  const leaf = (p: Partial<TreeRow> = {}): TreeRow => ({
    path: 'building/f2/r204/setpoint',
    name: 'setpoint',
    depth: 3,
    hasChildren: false,
    expanded: false,
    topicCount: 1,
    messages: 4,
    isTopic: true,
    value: '{"value":21.5}',
    retained: true,
    lastReceivedAt: 1000,
    active: false,
    rate: 0,
    spark: [22, 22, 21.5],
    sparkTimes: [1, 2, 3],
    formatHint: null,
    pinned: false,
    ...p,
  });

  it('name, payload with R after it, then sparkline, messages, throughput; no age, no dot', async () => {
    await boot('empty');
    render(<TreeRowView row={leaf()} selected={false} cursor={false} reducedMotion={false} valuesInTree now={600_000} />);
    const row = screen.getByTestId('tree-row');
    const parts = Array.from(row.children).map((c) => c.className);
    expect(parts).toEqual(['k', 'p', 'v']);
    const p = screen.getByTestId('tree-payload');
    expect(p.querySelector('em')!.textContent).toBe('{"value":21.5}');
    expect(p.lastElementChild!.textContent).toBe('R');
    const v = row.querySelector('.v')!;
    // change set 9: four fixed cells, the subtopics cell empty on a leaf
    expect(Array.from(v.children).map((c) => c.getAttribute('data-testid') ?? c.className)).toEqual(['tree-spark-cell', 'tree-msgs', 'tree-rate', 'c-cnt']);
    expect(v.querySelector('[data-testid=tree-spark-cell] svg')).not.toBeNull();
    expect(v.lastElementChild!.textContent).toBe('');
    expect(row.textContent).not.toMatch(/\d+ (s|min)\b/);
    expect(row.querySelector('[data-testid=activity-dot]')).toBeNull();
  });

  it('a topic with children counts the subtopics below it, not itself', async () => {
    await boot('empty');
    render(<TreeRowView row={leaf({ hasChildren: true, topicCount: 4, isTopic: true })} selected={false} cursor={false} reducedMotion={false} valuesInTree now={0} />);
    expect(screen.getByTestId('tree-subtopics')).toHaveTextContent('3');
  });

  it('tooltips in French and German', async () => {
    await boot('empty', 'fr');
    const { unmount } = render(<TreeRowView row={leaf({ hasChildren: true, topicCount: 3 })} selected={false} cursor={false} reducedMotion={false} valuesInTree now={0} />);
    expect(screen.getByTestId('tree-msgs')).toHaveAttribute('title', 'Messages depuis l’ouverture de la connexion');
    expect(screen.getByTestId('tree-rate')).toHaveAttribute('title', 'Messages par seconde');
    expect(screen.getByTestId('tree-subtopics')).toHaveAttribute('title', 'Sous-topics sous celui-ci');
    unmount();
    await boot('empty', 'de');
    render(<TreeRowView row={leaf({ hasChildren: true, topicCount: 3 })} selected={false} cursor={false} reducedMotion={false} valuesInTree now={0} />);
    expect(screen.getByTestId('tree-subtopics')).toHaveAttribute('title', 'Untertopics unter diesem');
  });
});

describe('Change set 8: sparkline rule (X-16, mock engine)', () => {
  const sparkOf = async (topic: string) => {
    await engine().revealTopic('plant', topic);
    const s = await engine().treeSlice('plant', { search: topic, filters: { retainedOnly: false, changedWithinMin: null, payloadContains: '', qos: null, hasValue: false } }, 0, 200);
    return s.rows.find((r) => r.path === topic)!.spark;
  };

  it('a bare number draws by itself, JSON draws nothing until picked, a second pick replaces the first, the pick survives a reload', async () => {
    await boot('demo');
    await act(async () => connect('plant'));
    expect((await sparkOf('building/f2/r204/humidity')).length).toBeGreaterThan(1);
    expect(await sparkOf('building/f2/r205/temperature')).toEqual([]);
    expect((await engine().topicDetail('plant', 'building/f2/r205/temperature')).defaultField).toBeNull();
    await act(async () => setSparklinePath('plant', 'building/f2/r205/temperature', '$.value'));
    expect((await sparkOf('building/f2/r205/temperature')).length).toBeGreaterThan(1);
    await act(async () => setSparklinePath('plant', STATE, '$.supply.fan'));
    expect((await engine().topicDetail('plant', STATE)).defaultField).toBe('$.supply.fan');
    const reloaded = (await engine().listProfiles()).find((p) => p.id === 'plant')!;
    expect(reloaded.topics['building/f2/r205/temperature']).toEqual({ sparklinePath: '$.value' });
    expect(reloaded.topics[STATE].sparklinePath).toBe('$.supply.fan');
    expect(reloaded.topics[STATE].chartFields!.map((f) => f.path)).toEqual(['$.supply.temp', '$.supply.fan']);
  });
});

describe('Change set 8: payload click menu (X-17)', () => {
  it('a click on a value opens Use as sparkline, Add … to chart…, Copy path, Copy value; no hover icon', async () => {
    await detail();
    expect(document.querySelector('.charticon')).toBeNull();
    const json = screen.getByTestId('payload-json');
    await act(async () => fireEvent.click(json.querySelector('[data-path="$.supply.fan"]:not(.k)') ?? json.querySelectorAll('[data-path="$.supply.fan"]')[1]));
    expect(menuLabels()).toEqual(['Use as sparkline', 'Add supply.fan to chart…', 'Show supply.fan in History', 'Copy path · $.supply.fan', expect.stringMatching(/^Copy value · \d+/)]);
    await act(async () => fireEvent.click(within(screen.getByTestId('json-menu')).getByText('Use as sparkline')));
    expect(screen.queryByTestId('json-menu')).toBeNull();
    await waitFor(() => expect(useApp.getState().profiles.find((p) => p.id === 'plant')!.topics[STATE].sparklinePath).toBe('$.supply.fan'));
  });

  it('a key opens the same menu; a text field shows the plot items disabled; right-click too; Add … to chart opens the picker with it checked', async () => {
    await detail();
    const json = screen.getByTestId('payload-json');
    await act(async () => fireEvent.click(json.querySelector('.k[data-path="$.mode"]')!));
    const items = screen.getByTestId('json-menu').querySelectorAll('[role=menuitem]');
    expect(items[0]).toHaveAttribute('aria-disabled', 'true');
    expect(items[0].textContent).toBe('Use as sparkline · text, not plottable');
    expect(items[1]).toHaveAttribute('aria-disabled', 'true');
    expect(items[2]).not.toHaveAttribute('aria-disabled');
    expect(items[2].textContent).toBe('Show mode in History');
    fireEvent.keyDown(document, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByTestId('json-menu')).toBeNull());
    await act(async () => fireEvent.contextMenu(json.querySelector('.k[data-path="$.supply.temp"]')!));
    await act(async () => fireEvent.click(within(screen.getByTestId('json-menu')).getByText('Add supply.temp to chart…')));
    expect(useCharts.getState().picker).toMatchObject({ connId: 'plant', topic: STATE, preselect: '$.supply.temp', mode: 'topic' });
    useCharts.getState().closePicker();
  });
});

describe('Change set 8: tabs, diff to previous, History (X-18, X-20)', () => {
  it('Payload and Properties only; diff on by default with the baseline time; off shows the plain payload', async () => {
    await detail();
    expect(screen.getAllByRole('tab').map((t) => t.getAttribute('data-testid'))).toEqual(['tab-payload', 'tab-properties']);
    const toggle = screen.getByTestId('diff-toggle');
    expect(toggle).toHaveAttribute('aria-checked', 'true');
    await waitFor(() => expect(screen.getByTestId('diff-baseline').textContent).toMatch(/^vs \d\d:\d\d:\d\d$/));
    await waitFor(() => expect(screen.getByTestId('payload-json').querySelector('[data-kind]')).not.toBeNull());
    await act(async () => fireEvent.click(toggle));
    expect(useExplorer.getState().get('plant').diff).toBe(false);
    await waitFor(() => expect(screen.getByTestId('payload-json').querySelector('[data-kind]')).toBeNull());
    expect(screen.queryByTestId('diff-baseline')).toBeNull();
    await act(async () => fireEvent.click(toggle));
  });

  it('History lists every buffered message; selecting a row shows it, diffed against its predecessor; the buffer line has Raise…', async () => {
    await detail();
    const h = await engine().history('plant', STATE);
    await waitFor(() => expect(screen.getAllByTestId('history-row')).toHaveLength(h.length));
    expect(h.length).toBeGreaterThan(4);
    expect(screen.getByText(`History · ${h.length}`)).toBeInTheDocument();
    const rows = screen.getAllByTestId('history-row');
    await act(async () => fireEvent.click(rows[2]));
    expect(useExplorer.getState().get('plant').viewSeq).toBe(h[2].seq);
    await screen.findByText('Back to latest');
    const pred = new Date(h[3].receivedAt);
    const hh = (n: number) => String(n).padStart(2, '0');
    await waitFor(() => expect(screen.getByTestId('diff-baseline').textContent).toBe(`vs ${hh(pred.getHours())}:${hh(pred.getMinutes())}:${hh(pred.getSeconds())}`));
    expect(screen.getByTestId('buffer-line').textContent).toMatch(/Keeps the last [\d  ]+ messages for this topic\. · Raise…/);
    await act(async () => fireEvent.click(screen.getByTestId('buffer-raise')));
    expect(screen.getByRole('dialog')).toHaveTextContent('Buffer for this topic');
  });
});

describe('Change set 8: topic chart under the payload (X-19)', () => {
  it('shows the saved fields as chips; ✕ removes one and saves the profile', async () => {
    await detail();
    expect(screen.getAllByTestId('chart-field').map((c) => c.getAttribute('data-path'))).toEqual(['$.supply.temp', '$.supply.fan']);
    expect(screen.getByTestId('topic-chart')).toBeInTheDocument();
    await act(async () => fireEvent.click(screen.getByLabelText('Remove supply.fan from this chart')));
    await waitFor(() => expect(useApp.getState().profiles.find((p) => p.id === 'plant')!.topics[STATE].chartFields!.map((f) => f.path)).toEqual(['$.supply.temp']));
  });

  it('a topic without fields draws nothing; + Add field… opens the picker on this topic', async () => {
    await detail('building/f2/r205/temperature');
    expect(screen.queryByTestId('chart-field')).toBeNull();
    expect(screen.queryByTestId('topic-chart')).toBeNull();
    await act(async () => fireEvent.click(screen.getByTestId('add-field')));
    expect(useCharts.getState().picker).toMatchObject({ topic: 'building/f2/r205/temperature', mode: 'topic', preselect: null });
    useCharts.getState().closePicker();
  });

  const fields: FieldInfo[] = [
    { path: '$.mode', label: 'mode', lastValue: '"cooling"', cast: 'text', plottable: false },
    { path: '$.setpoint', label: 'setpoint', lastValue: '21.0', cast: 'number', plottable: true },
    { path: '$.supply.temp', label: 'supply.temp', lastValue: '18.2', cast: 'number', plottable: true },
    { path: '$.supply.fan', label: 'supply.fan', lastValue: '72', cast: 'number', plottable: true },
  ];

  it('picker from the Explorer: this chart’s fields come checked, unchecking removes, "Also add to" lists None, the charts and New chart', async () => {
    await boot('empty');
    let applied: unknown = null;
    render(
      <FieldPickerView
        req={{ connId: 'x', topic: STATE, preselect: null, chartId: null, mode: 'topic' }}
        fields={fields}
        current={['$.supply.temp', '$.supply.fan']}
        charts={[{ id: 'c1', name: 'Room 204' }]}
        onAdd={() => undefined}
        onApply={(items, also) => (applied = { items: items.map((i) => i.path), also })}
        onClose={() => undefined}
      />,
    );
    expect(screen.getByTestId('field-picker')).toHaveTextContent('Fields on this chart · building/f2/ahu01/state');
    expect(screen.getAllByRole('checkbox', { checked: true }).map((c) => c.getAttribute('aria-label'))).toEqual(['supply.temp', 'supply.fan']);
    const also = screen.getByTestId('picker-also') as HTMLSelectElement;
    expect(Array.from(also.options).map((o) => o.textContent)).toEqual(['None', 'Room 204', 'New chart']);
    expect(also.value).toBe('');
    await act(async () => fireEvent.click(screen.getByTestId('picker-check-supply.fan')));
    await act(async () => fireEvent.click(screen.getByTestId('picker-check-setpoint')));
    fireEvent.change(also, { target: { value: 'c1' } });
    expect(screen.getByTestId('picker-add')).toHaveTextContent('Apply');
    await act(async () => fireEvent.click(screen.getByTestId('picker-add')));
    expect(applied).toEqual({ items: ['$.setpoint', '$.supply.temp'], also: 'c1' });
  });

  it('Apply saves the topic chart and, with "New chart", creates a dashboard chart with the checked fields', async () => {
    await boot('demo');
    await act(async () => connect('plant'));
    useCharts.setState({ charts: [], activeId: null, picker: null });
    render(<FieldPicker />);
    await act(async () => useCharts.getState().openPicker({ connId: 'plant', topic: STATE, preselect: '$.setpoint', chartId: null, mode: 'topic' }));
    await screen.findByTestId('field-picker');
    expect(screen.getAllByRole('checkbox', { checked: true }).map((c) => c.getAttribute('aria-label'))).toEqual(['setpoint', 'supply.temp', 'supply.fan']);
    fireEvent.change(screen.getByTestId('picker-also'), { target: { value: 'new' } });
    await act(async () => fireEvent.click(screen.getByTestId('picker-add')));
    await waitFor(() => expect(useApp.getState().profiles.find((p) => p.id === 'plant')!.topics[STATE].chartFields!.map((f) => f.path)).toEqual(['$.supply.temp', '$.supply.fan', '$.setpoint']));
    const charts = useCharts.getState().charts;
    expect(charts).toHaveLength(1);
    expect(charts[0].series.map((s) => s.path)).toEqual(['$.setpoint', '$.supply.temp', '$.supply.fan']);
    // the saved fields survive a profile reload
    const reloaded = (await engine().listProfiles()).find((p) => p.id === 'plant')!;
    expect(reloaded.topics[STATE].chartFields).toHaveLength(3);
  });
});

describe('Change set 8: detail header and quick publish (X-21)', () => {
  it('star toggles Pin with its tooltip, the Publish icon publishes here, the quick publish row is a box with separators', async () => {
    await detail('building/f2/r204/humidity');
    const pin = screen.getByTestId('pin-btn');
    expect(pin.textContent).toBe('☆');
    expect(pin).toHaveAttribute('title', 'Pin');
    expect(pin).toHaveAttribute('aria-pressed', 'false');
    await act(async () => fireEvent.click(pin));
    await waitFor(() => expect(screen.getByTestId('pin-btn').textContent).toBe('★'));
    expect(screen.getByTestId('pin-btn')).toHaveAttribute('title', 'Pinned');
    expect(screen.getByTestId('pin-btn')).toHaveAttribute('aria-pressed', 'true');
    const pub = screen.getByTestId('publish-here');
    expect(pub.querySelector('svg')).not.toBeNull();
    expect(pub).toHaveAttribute('title', 'Publish here');
    const qp = screen.getByTestId('quick-publish');
    expect(qp.className).toBe('qp');
    expect(qp).toHaveAttribute('aria-label', 'Quick publish');
    expect(qp.querySelectorAll('.sep')).toHaveLength(4);
    expect(qp.textContent).not.toContain('Quick publish');
    for (const id of ['qp-payload', 'qp-format', 'qp-qos', 'qp-retain', 'qp-send']) expect(within(qp).getByTestId(id)).toBeInTheDocument();
    await act(async () => fireEvent.click(pub));
    await waitFor(() => expect(useApp.getState().screen).toBe('publish'));
  });
});

describe('Change set 8: Charts screen, edited in place (D-24)', () => {
  async function charts() {
    await boot('demo');
    await act(async () => connect('plant'));
    useCharts.setState({ charts: [], activeId: null, picker: null });
    const id = useCharts.getState().createChart('plant', 'Room 204');
    useCharts.getState().addSeries(id, [
      { topic: 'building/f2/r204/temperature', path: '$.value', name: 'temperature', step: false },
      { topic: 'building/f2/r204/occupancy', path: '$', name: 'occupancy', step: true },
    ]);
    render(<ChartsScreen />);
    await screen.findAllByTestId('series-card');
    return id;
  }

  it('no edit mode: cards show switch, colour, name, field and cast; the chevron opens style, axis, unit and Remove', async () => {
    const id = await charts();
    expect(screen.queryByText('Edit series')).toBeNull();
    expect(screen.queryByText('Editing series')).toBeNull();
    expect(screen.queryByText('Done')).toBeNull();
    const card = screen.getAllByTestId('series-card')[0];
    expect(within(card).getByRole('switch')).toBeInTheDocument();
    expect(within(card).getByTestId('series-colour')).toBeInTheDocument();
    expect(within(card).getByTestId('series-name')).toHaveValue('temperature');
    expect(within(card).getByTestId('series-path')).toHaveValue('$.value');
    expect(within(card).getByTestId('series-cast')).toBeInTheDocument();
    expect(within(card).queryByTestId('series-details')).toBeNull();
    // name and field save as typed (debounced) and on blur
    fireEvent.change(within(card).getByTestId('series-name'), { target: { value: 'room temp' } });
    fireEvent.blur(within(card).getByTestId('series-name'));
    expect(useCharts.getState().charts.find((c) => c.id === id)!.series[0].name).toBe('room temp');
    fireEvent.change(within(card).getByTestId('series-path'), { target: { value: 'unit' } });
    await waitFor(() => expect(useCharts.getState().charts.find((c) => c.id === id)!.series[0].path).toBe('$.unit'), { timeout: 2000 });
    // colour from the palette popover
    await act(async () => fireEvent.click(within(card).getByTestId('series-colour')));
    await act(async () => fireEvent.click(within(screen.getByTestId('series-palette')).getByLabelText('Slate')));
    expect(useCharts.getState().charts.find((c) => c.id === id)!.series[0].color).toBe('slate');
    // cast rule
    await act(async () => fireEvent.click(within(within(card).getByTestId('series-cast')).getByText('bool')));
    expect(useCharts.getState().charts.find((c) => c.id === id)!.series[0]).toMatchObject({ cast: 'boolean', style: 'step' });
    // the chevron opens the rest
    await act(async () => fireEvent.click(within(card).getByTestId('series-more')));
    const more = within(card).getByTestId('series-details');
    expect(within(more).getByText('Style')).toBeInTheDocument();
    expect(within(more).getByText('Axis')).toBeInTheDocument();
    expect(within(more).getByText('Unit')).toBeInTheDocument();
    await act(async () => fireEvent.click(within(more).getByTestId('series-remove')));
    expect(useCharts.getState().charts.find((c) => c.id === id)!.series).toHaveLength(1);
  });

  it('the chart title is renamed in place: Enter and focus loss save, Escape cancels; the series stay', async () => {
    const id = await charts();
    const chart = () => useCharts.getState().charts.find((c) => c.id === id)!;
    await act(async () => fireEvent.click(screen.getByTestId('chart-title')));
    let input = screen.getByTestId('chart-title') as HTMLInputElement;
    expect(input.tagName).toBe('INPUT');
    fireEvent.change(input, { target: { value: 'Room 204 · comfort' } });
    await act(async () => fireEvent.keyDown(input, { key: 'Enter' }));
    expect(chart().name).toBe('Room 204 · comfort');
    expect(screen.getByTestId('chart-title').tagName).toBe('BUTTON');
    await act(async () => fireEvent.click(screen.getByTestId('chart-title')));
    input = screen.getByTestId('chart-title') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'nope' } });
    await act(async () => fireEvent.keyDown(input, { key: 'Escape' }));
    expect(chart().name).toBe('Room 204 · comfort');
    await act(async () => fireEvent.click(screen.getByTestId('chart-title')));
    input = screen.getByTestId('chart-title') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'Comfort' } });
    await act(async () => fireEvent.blur(input));
    expect(chart().name).toBe('Comfort');
    expect(chart().series).toHaveLength(2);
  });
});

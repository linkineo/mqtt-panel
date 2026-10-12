import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import type { FieldInfo } from '@mqttpanel/engine-client';
import { boot } from './util';
import { FieldPickerView } from '../screens/explorer/FieldPicker';

const fields: FieldInfo[] = [
  { path: '$.mode', label: 'mode', lastValue: '"cooling"', cast: 'text', plottable: false },
  { path: '$.setpoint', label: 'setpoint', lastValue: '21.0', cast: 'number', plottable: true },
  { path: '$.supply', label: 'supply', lastValue: '{…}', cast: 'object', plottable: false },
  { path: '$.supply.temp', label: 'supply.temp', lastValue: '18.2', cast: 'number', plottable: true },
  { path: '$.supply.fan', label: 'supply.fan', lastValue: '72', cast: 'number', plottable: true },
  { path: '$.alarm.active', label: 'alarm.active', lastValue: 'false', cast: 'boolean', plottable: true },
  { path: '$.ts', label: 'ts', lastValue: '"2026-10-12T09:02:14Z"', cast: 'text', plottable: false },
];

describe('Field picker (D-16)', () => {
  it('lists plottable fields checkable, text greyed, nothing preselected', async () => {
    await boot('empty');
    const onAdd = vi.fn();
    render(<FieldPickerView req={{ connId: 'x', topic: 'building/f2/ahu01/state', preselect: null, chartId: null }} fields={fields} charts={[]} onAdd={onAdd} onClose={() => undefined} />);
    const rows = screen.getAllByTestId('picker-row');
    expect(rows.map((r) => r.children[1].textContent)).toEqual(['setpoint', 'supply.temp', 'supply.fan', 'alarm.active', 'mode', 'ts']); // plottable first (mockup 10b)
    for (const r of rows) {
      const plottable = r.getAttribute('data-plottable') === 'true';
      expect(r.querySelector('[role=checkbox]') !== null).toBe(plottable);
      if (!plottable) {
        expect(r.className).toContain('dim');
        expect(r.textContent).toContain('text, not plottable');
      }
    }
    expect(screen.getByText('boolean → 0/1')).toBeInTheDocument();
    expect(screen.queryAllByRole('checkbox', { checked: true })).toHaveLength(0);
    expect(screen.getByTestId('picker-add')).toBeDisabled();
    await act(async () => fireEvent.click(screen.getByTestId('picker-check-supply.temp')));
    await act(async () => fireEvent.click(screen.getByTestId('picker-check-supply.fan')));
    expect(screen.getByTestId('picker-add').textContent).toBe('Add 2 series');
    // a typed path for a field absent from the sample
    fireEvent.change(screen.getByTestId('picker-other'), { target: { value: '$.supply.pressure' } });
    await act(async () => fireEvent.click(screen.getByText('Add')));
    expect(screen.getByTestId('picker-add').textContent).toBe('Add 3 series');
    await act(async () => fireEvent.click(screen.getByTestId('picker-add')));
    expect(onAdd).toHaveBeenCalledWith('new', [
      { path: '$.supply.temp', label: 'supply.temp', step: false },
      { path: '$.supply.fan', label: 'supply.fan', step: false },
      { path: '$.supply.pressure', label: 'supply.pressure', step: false },
    ]);
  });

  it('preselects the field it was opened from (D-17)', async () => {
    await boot('empty');
    render(<FieldPickerView req={{ connId: 'x', topic: 't', preselect: '$.supply.temp', chartId: null }} fields={fields} charts={[]} onAdd={() => undefined} onClose={() => undefined} />);
    expect(screen.getAllByRole('checkbox', { checked: true })).toHaveLength(1);
    expect(screen.getByTestId('picker-check-supply.temp')).toHaveAttribute('aria-checked', 'true');
  });
});

import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import type { TreeRow } from '@mqttpanel/engine-client';
import { boot } from './util';
import { TreeRowView, FLASH_MS } from '../screens/explorer/Tree';
import { Sparkline } from '../components/ui';

function row(p: Partial<TreeRow> = {}): TreeRow {
  return {
    path: 'building/f2/r203',
    name: 'r203',
    depth: 2,
    hasChildren: true,
    expanded: false,
    topicCount: 12,
    messages: 1012,
    isTopic: false,
    value: null,
    retained: false,
    lastReceivedAt: 1000,
    active: true,
    rate: 4,
    spark: [],
    sparkTimes: [],
    formatHint: null,
    pinned: false,
    ...p,
  };
}

describe('Tree row highlight (X-04)', () => {
  afterEach(() => vi.useRealTimers());

  it('flashes .new for 1.2 s when lastReceivedAt changes, on a collapsed ancestor too', async () => {
    await boot('empty');
    vi.useFakeTimers();
    const props = { selected: false, cursor: false, reducedMotion: false, valuesInTree: true, now: 5000 };
    const { rerender } = render(<TreeRowView row={row()} {...props} />);
    const el = () => screen.getByTestId('tree-row');
    expect(el().className).not.toContain('new');
    rerender(<TreeRowView row={row({ lastReceivedAt: 2000 })} {...props} />);
    expect(el().className).toContain('new');
    act(() => vi.advanceTimersByTime(FLASH_MS - 50));
    expect(el().className).toContain('new');
    act(() => vi.advanceTimersByTime(100));
    expect(el().className).not.toContain('new');
    expect(FLASH_MS).toBe(1200);
  });

  it('shows the activity dot only, without the flash, under reduced motion', async () => {
    await boot('empty');
    vi.useFakeTimers();
    const props = { selected: false, cursor: false, reducedMotion: true, valuesInTree: true, now: 5000 };
    const leaf = row({ path: 'a/b', name: 'b', hasChildren: false, isTopic: true, value: '1' });
    const { rerender } = render(<TreeRowView row={leaf} {...props} />);
    rerender(<TreeRowView row={{ ...leaf, lastReceivedAt: 3000 }} {...props} />);
    expect(screen.getByTestId('tree-row').className).not.toContain('new');
    expect(screen.getByTestId('activity-dot').className).toBe('act');
  });

  it('shows count, messages since connect and rate on folders', async () => {
    await boot('empty');
    render(<TreeRowView row={row()} selected={false} cursor={false} reducedMotion={false} valuesInTree now={5000} />);
    const t = screen.getByTestId('tree-row').textContent!;
    expect(t).toContain('r203');
    expect(t).toContain('12');
    expect(t).toContain('1 012');
    expect(t).toContain('4 msg/s');
  });
});

describe('Sparkline (D-22 structural)', () => {
  it('draws exactly the last 10 values', () => {
    const values = Array.from({ length: 15 }, (_, i) => i * 1.5);
    const { container } = render(<Sparkline values={values} />);
    const svg = container.querySelector('svg')!;
    expect(svg.getAttribute('data-values')).toBe(values.slice(-10).join(','));
    expect(svg.querySelector('polyline')!.getAttribute('points')!.split(' ')).toHaveLength(10);
  });

  it('a tree row uses the row spark values', async () => {
    await boot('empty');
    const spark = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    render(<TreeRowView row={row({ hasChildren: false, isTopic: true, value: '10', spark, sparkTimes: spark.map((x) => x * 1000) })} selected={false} cursor={false} reducedMotion={false} valuesInTree now={5000} />);
    expect(screen.getByTestId('tree-row').querySelector('svg.spark')!.getAttribute('data-values')).toBe(spark.join(','));
  });
});

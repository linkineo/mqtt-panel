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

  it('under reduced motion the highlight switches on and off without a fade; no activity dot (change set 8)', async () => {
    await boot('empty');
    vi.useFakeTimers();
    const props = { selected: false, cursor: false, reducedMotion: true, valuesInTree: true, now: 5000 };
    const leaf = row({ path: 'a/b', name: 'b', hasChildren: false, isTopic: true, value: '1' });
    const { rerender } = render(<TreeRowView row={leaf} {...props} />);
    rerender(<TreeRowView row={{ ...leaf, lastReceivedAt: 3000 }} {...props} />);
    expect(screen.getByTestId('tree-row').className).toContain('new still');
    expect(screen.queryByTestId('activity-dot')).toBeNull();
    act(() => vi.advanceTimersByTime(FLASH_MS + 10));
    expect(screen.getByTestId('tree-row').className).not.toContain('new');
  });

  it('shows messages since connect, throughput and subtopics on folders, with tooltips (X-15)', async () => {
    await boot('empty');
    render(<TreeRowView row={row()} selected={false} cursor={false} reducedMotion={false} valuesInTree now={5000} />);
    const r = screen.getByTestId('tree-row');
    expect(r.textContent).toContain('r203');
    expect(screen.getByTestId('tree-msgs').textContent!.replace(/\s/g, ' ')).toBe('1 012');
    expect(screen.getByTestId('tree-msgs')).toHaveAttribute('title', 'Messages since the connection opened');
    expect(screen.getByTestId('tree-rate')).toHaveTextContent('4 msg/s');
    expect(screen.getByTestId('tree-rate')).toHaveAttribute('title', 'Messages per second');
    expect(screen.getByTestId('tree-subtopics')).toHaveTextContent('12');
    expect(screen.getByTestId('tree-subtopics')).toHaveAttribute('title', 'Subtopics below this one');
    // the subtopics counter is the last thing on the row, after the throughput
    expect(r.querySelector('.v')!.lastElementChild!.firstElementChild).toBe(screen.getByTestId('tree-subtopics'));
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

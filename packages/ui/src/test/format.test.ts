import { describe, expect, it } from 'vitest';
import { displayValue } from '../lib/format';

describe('Table value cell (mockup 05)', () => {
  it('reads {"value", "unit"} objects as "value unit"', () => {
    expect(displayValue('{"value":22.4,"unit":"°C"}')).toBe('22.4 °C');
    expect(displayValue('{"value":21.5}')).toBe('21.5');
    expect(displayValue('{"value":true}')).toBe('true');
  });
  it('leaves everything else as received', () => {
    expect(displayValue('running')).toBe('running');
    expect(displayValue('{"cmd":"restart","delay":5}')).toBe('{"cmd":"restart","delay":5}');
    expect(displayValue('{"value":{"a":1}}')).toBe('{"value":{"a":1}}');
    expect(displayValue('{broken')).toBe('{broken');
    expect(displayValue(null)).toBe('');
  });
});

import { fmtBytes } from '../lib/format';
describe('fmtBytes', () => {
  it('never shows four digits before a unit', () => {
    expect(fmtBytes(999)).toBe('999 B');
    expect(fmtBytes(1000)).toBe('1.0 KB');
    expect(fmtBytes(1020 * 1024)).toBe('1.0 MB');
  });
});

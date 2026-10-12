import '@testing-library/jest-dom/vitest';

// jsdom lacks a few browser APIs the UI touches.
class RO {
  observe() {}
  unobserve() {}
  disconnect() {}
}
const g = globalThis as unknown as Record<string, unknown>;
if (!g.ResizeObserver) g.ResizeObserver = RO;
if (typeof window !== 'undefined' && !window.matchMedia) {
  window.matchMedia = ((q: string) => ({
    matches: false,
    media: q,
    onchange: null,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
    dispatchEvent: () => false,
  })) as unknown as typeof window.matchMedia;
}
if (typeof Element !== 'undefined' && !Element.prototype.scrollTo) Element.prototype.scrollTo = () => {};

import { afterEach } from 'vitest';
import { cleanup } from '@testing-library/react';
afterEach(() => cleanup());

// Browser implementation of Platform for tests and the dev server: DOM context menu, window drops, clipboard.
import type { ContextMenuItem, Platform } from '../client';

export interface MockPlatform extends Platform {
  simulateDrop(paths: string[], x: number, y: number): void;
  lastCopied(): string | null;
}

export function createMockPlatform(): MockPlatform {
  const dropHandlers = new Set<(paths: string[], x: number, y: number) => void>();
  let copied: string | null = null;
  let closeCurrent: (() => void) | null = null;
  let listening = false;

  const onDragOver = (e: DragEvent) => e.preventDefault();
  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    const files = e.dataTransfer ? Array.from(e.dataTransfer.files) : [];
    if (files.length === 0) return;
    const paths = files.map((f) => '/dropped/' + f.name);
    dropHandlers.forEach((h) => h(paths, e.clientX, e.clientY));
  };

  return {
    showContextMenu(items: ContextMenuItem[], x: number, y: number): Promise<string | null> {
      return new Promise((resolve) => {
        if (typeof document === 'undefined') {
          resolve(null);
          return;
        }
        if (closeCurrent) closeCurrent();
        const prevFocus = document.activeElement as HTMLElement | null;
        const menu = document.createElement('div');
        menu.className = 'ctx';
        menu.setAttribute('role', 'menu');
        menu.setAttribute('data-testid', 'context-menu');
        menu.style.position = 'fixed';
        menu.style.zIndex = '1000';
        const entries: HTMLElement[] = [];
        for (const it of items) {
          if (it.separator) {
            menu.appendChild(document.createElement('hr'));
            continue;
          }
          const el = document.createElement('div');
          el.className = 'ctx-item' + (it.danger ? ' danger' : '') + (it.enabled === false ? ' disabled' : '');
          el.setAttribute('role', 'menuitem');
          el.tabIndex = -1;
          el.dataset.id = it.id;
          if (it.enabled === false) el.setAttribute('aria-disabled', 'true');
          const label = document.createElement('span');
          label.className = 'ctx-label';
          label.textContent = it.label;
          el.appendChild(label);
          if (it.accelerator) {
            const k = document.createElement('span');
            k.className = 'kbd';
            k.textContent = it.accelerator;
            el.appendChild(k);
          }
          el.addEventListener('click', (ev) => {
            ev.stopPropagation();
            if (it.enabled === false) return;
            finish(it.id);
          });
          el.addEventListener('mouseenter', () => {
            if (it.enabled !== false) el.focus();
          });
          menu.appendChild(el);
          if (it.enabled !== false) entries.push(el);
        }
        document.body.appendChild(menu);
        const vw = window.innerWidth || 1280;
        const vh = window.innerHeight || 800;
        const r = menu.getBoundingClientRect();
        menu.style.left = `${Math.max(4, Math.min(x, vw - r.width - 4))}px`;
        menu.style.top = `${Math.max(4, Math.min(y, vh - r.height - 4))}px`;
        let idx = 0;
        entries[0]?.focus();
        const onKey = (e: KeyboardEvent) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            finish(null);
          } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
            e.preventDefault();
            if (!entries.length) return;
            const cur = entries.indexOf(document.activeElement as HTMLElement);
            idx = cur < 0 ? 0 : (cur + (e.key === 'ArrowDown' ? 1 : entries.length - 1)) % entries.length;
            entries[idx].focus();
          } else if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            const cur = document.activeElement as HTMLElement;
            if (cur && entries.includes(cur)) finish(cur.dataset.id ?? null);
          }
          e.stopPropagation();
        };
        const onDown = (e: MouseEvent) => {
          if (!menu.contains(e.target as globalThis.Node)) finish(null);
        };
        const onBlur = () => finish(null);
        document.addEventListener('keydown', onKey, true);
        setTimeout(() => document.addEventListener('mousedown', onDown, true), 0);
        window.addEventListener('blur', onBlur);
        let done = false;
        function finish(id: string | null) {
          if (done) return;
          done = true;
          document.removeEventListener('keydown', onKey, true);
          document.removeEventListener('mousedown', onDown, true);
          window.removeEventListener('blur', onBlur);
          menu.remove();
          closeCurrent = null;
          if (prevFocus && typeof prevFocus.focus === 'function') prevFocus.focus();
          resolve(id);
        }
        closeCurrent = () => finish(null);
      });
    },
    onFileDrop(handler) {
      dropHandlers.add(handler);
      if (!listening && typeof window !== 'undefined') {
        window.addEventListener('dragover', onDragOver);
        window.addEventListener('drop', onDrop);
        listening = true;
      }
      return () => {
        dropHandlers.delete(handler);
        if (dropHandlers.size === 0 && listening && typeof window !== 'undefined') {
          window.removeEventListener('dragover', onDragOver);
          window.removeEventListener('drop', onDrop);
          listening = false;
        }
      };
    },
    async copyText(text: string) {
      copied = text;
      try {
        if (typeof navigator !== 'undefined' && navigator.clipboard) await navigator.clipboard.writeText(text);
      } catch {
        // clipboard may be unavailable (tests, insecure context)
      }
    },
    prefersReducedMotion() {
      try {
        return typeof window !== 'undefined' && typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
      } catch {
        return false;
      }
    },
    simulateDrop(paths, x, y) {
      dropHandlers.forEach((h) => h(paths, x, y));
    },
    lastCopied() {
      return copied;
    },
  };
}

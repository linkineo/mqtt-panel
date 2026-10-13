// Browser implementation of Platform for tests and the dev server: DOM context menu, window drops, clipboard.
import type { ContextMenuItem, Platform } from '../client';

export interface MockPlatform extends Platform {
  simulateDrop(paths: string[], x: number, y: number): void;
  lastCopied(): string | null;
  /** Language of the (simulated) native menu bar. */
  menuLanguage(): string | null;
}

export function createMockPlatform(): MockPlatform {
  const dropHandlers = new Set<(paths: string[], x: number, y: number) => void>();
  let copied: string | null = null;
  let menuLang: string | null = null;
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
        const menus: HTMLElement[] = [];
        /** Builds one menu level; items with children open a submenu (hover, click or arrow right). */
        const build = (list: ContextMenuItem[], level: number): { el: HTMLElement; entries: HTMLElement[] } => {
          const menu = document.createElement('div');
          menu.className = 'ctx';
          menu.setAttribute('role', 'menu');
          menu.setAttribute('data-testid', level === 0 ? 'context-menu' : 'context-submenu');
          menu.style.position = 'fixed';
          menu.style.zIndex = String(1000 + level);
          const entries: HTMLElement[] = [];
          for (const it of list) {
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
            if (it.items) {
              el.setAttribute('aria-haspopup', 'menu');
              const k = document.createElement('span');
              k.className = 'kbd';
              k.textContent = '▸';
              el.appendChild(k);
            } else if (it.accelerator) {
              const k = document.createElement('span');
              k.className = 'kbd';
              k.textContent = it.accelerator;
              el.appendChild(k);
            }
            const openSub = (focus: boolean) => {
              if (!it.items || it.enabled === false) return;
              closeFrom(level + 1);
              el.classList.add('on');
              const sub = build(it.items, level + 1);
              document.body.appendChild(sub.el);
              menus.push(sub.el);
              const r = el.getBoundingClientRect();
              const sr = sub.el.getBoundingClientRect();
              const vw = window.innerWidth || 1280;
              const vh = window.innerHeight || 800;
              const left = r.right + sr.width + 4 <= vw ? r.right - 2 : r.left - sr.width + 2;
              sub.el.style.left = `${Math.max(4, left)}px`;
              sub.el.style.top = `${Math.max(4, Math.min(r.top - 4, vh - sr.height - 4))}px`;
              if (focus) sub.entries[0]?.focus();
            };
            el.addEventListener('click', (ev) => {
              ev.stopPropagation();
              if (it.enabled === false) return;
              if (it.items) openSub(true);
              else finish(it.id);
            });
            el.addEventListener('mouseenter', () => {
              if (it.enabled === false) return;
              el.focus();
              if (it.items) openSub(false);
              else closeFrom(level + 1);
            });
            (el as HTMLElement & { openSub?: (f: boolean) => void }).openSub = openSub;
            menu.appendChild(el);
            if (it.enabled !== false) entries.push(el);
          }
          return { el: menu, entries };
        };
        const closeFrom = (level: number) => {
          while (menus.length > level) {
            const m = menus.pop()!;
            m.remove();
          }
          menus[level - 1]?.querySelectorAll('.ctx-item.on').forEach((x) => x.classList.remove('on'));
        };
        const root = build(items, 0);
        const menu = root.el;
        menus.push(menu);
        document.body.appendChild(menu);
        const vw = window.innerWidth || 1280;
        const vh = window.innerHeight || 800;
        const r = menu.getBoundingClientRect();
        menu.style.left = `${Math.max(4, Math.min(x, vw - r.width - 4))}px`;
        menu.style.top = `${Math.max(4, Math.min(y, vh - r.height - 4))}px`;
        root.entries[0]?.focus();
        const levelEntries = (): HTMLElement[] => {
          const cur = document.activeElement as HTMLElement | null;
          const m = (cur && menus.find((x) => x.contains(cur))) ?? menus[menus.length - 1];
          return Array.from(m.querySelectorAll<HTMLElement>('.ctx-item:not(.disabled)'));
        };
        const onKey = (e: KeyboardEvent) => {
          const cur = document.activeElement as HTMLElement | null;
          const level = cur ? menus.findIndex((m) => m.contains(cur)) : -1;
          if (e.key === 'Escape') {
            e.preventDefault();
            finish(null);
          } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
            e.preventDefault();
            const entries = levelEntries();
            if (!entries.length) return;
            const i = entries.indexOf(cur as HTMLElement);
            const idx = i < 0 ? 0 : (i + (e.key === 'ArrowDown' ? 1 : entries.length - 1)) % entries.length;
            entries[idx].focus();
          } else if (e.key === 'ArrowRight') {
            e.preventDefault();
            (cur as HTMLElement & { openSub?: (f: boolean) => void })?.openSub?.(true);
          } else if (e.key === 'ArrowLeft') {
            e.preventDefault();
            if (level > 0) {
              const parent = menus[level - 1].querySelector<HTMLElement>('.ctx-item.on');
              closeFrom(level);
              parent?.focus();
            }
          } else if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            if (cur && cur.classList.contains('ctx-item') && !cur.classList.contains('disabled')) {
              const withSub = cur as HTMLElement & { openSub?: (f: boolean) => void };
              if (cur.getAttribute('aria-haspopup')) withSub.openSub?.(true);
              else finish(cur.dataset.id ?? null);
            }
          }
          e.stopPropagation();
        };
        const onDown = (e: MouseEvent) => {
          if (!menus.some((m) => m.contains(e.target as globalThis.Node))) finish(null);
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
          menus.forEach((m) => m.remove());
          menus.length = 0;
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
    async setMenuLanguage(lang) {
      menuLang = lang;
    },
    menuLanguage() {
      return menuLang;
    },
    simulateDrop(paths, x, y) {
      dropHandlers.forEach((h) => h(paths, x, y));
    },
    lastCopied() {
      return copied;
    },
  };
}

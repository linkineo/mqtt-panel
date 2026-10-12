import type { AppInfo } from '@mqttpanel/engine-client';
import { isMac } from '../store/app';

/** Formats a shortcut written as "mod+shift+L" for the platform: "⌘⇧L" on macOS, "Ctrl+Shift+L" elsewhere. */
export function shortcut(info: AppInfo, spec: string): string {
  const mac = isMac(info);
  const parts = spec.split('+');
  const key = parts.pop() ?? '';
  const mods = parts.map((p) => {
    if (p === 'mod') return mac ? '⌘' : 'Ctrl';
    if (p === 'shift') return mac ? '⇧' : 'Shift';
    if (p === 'alt') return mac ? '⌥' : 'Alt';
    return p;
  });
  const k = key === 'enter' ? '↵' : key;
  if (mac) return mods.join('') + k;
  return [...mods, k === '↵' ? 'Enter' : k].join('+');
}

/** True when the primary modifier of the platform is held. */
export function modHeld(info: AppInfo, e: { metaKey: boolean; ctrlKey: boolean }): boolean {
  return isMac(info) ? e.metaKey : e.ctrlKey;
}

/** True when focus is in a text field, where single-letter shortcuts must not fire. */
export function typingTarget(e: Event): boolean {
  const el = e.target as HTMLElement | null;
  if (!el || !el.tagName) return false;
  const tag = el.tagName.toLowerCase();
  return tag === 'input' || tag === 'textarea' || el.isContentEditable || !!el.closest?.('.cm-editor');
}

import { describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { boot } from './util';
import { App } from '../App';
import { useApp } from '../store/app';
import { resources } from '../i18n';

// Owner, 25 Oct 2026: Microsoft Store and Mac App Store only; the stores update the app (TESTPLAN S-04).
const LANGS = ['en', 'fr', 'de'] as const;
const UPDATES = {
  en: 'Updates come from the store this copy was installed from.',
  fr: 'Les mises à jour viennent de la boutique d’applications depuis laquelle cette copie a été installée.',
  de: 'Updates kommen aus dem Store, aus dem du diese Kopie installiert hast.',
};
const LICENCE = {
  en: 'Apache 2.0 with the Commons Clause · free to use, not for sale.',
  fr: 'Apache 2.0 avec la Commons Clause · libre d’utilisation, vente interdite.',
  de: 'Apache 2.0 mit der Commons Clause · frei nutzbar, nicht zum Verkauf.',
};
const RELEASES = {
  en: 'Release notes and source code on GitHub',
  fr: 'Notes de version et code source sur GitHub',
  de: 'Versionshinweise und Quellcode auf GitHub',
};

describe('Settings, Updates and licence (25 Oct 2026)', () => {
  for (const lang of LANGS) {
    it(`Updates is the store sentence and a link to GitHub, with no switch, channel or check (${lang})`, async () => {
      cleanup();
      await boot('demo', lang);
      useApp.setState({ screen: 'settings' });
      render(<App />);
      const sec = screen.getByTestId('settings-updates');
      expect(screen.getByTestId('updates-store').textContent).toBe(UPDATES[lang]);
      expect(sec.querySelector('[role=switch], .sw, .seg, input')).toBeNull();
      expect(sec.querySelectorAll('button')).toHaveLength(1);
      const link = screen.getByTestId('updates-release-notes');
      expect(link.textContent).toBe(RELEASES[lang]);
      const { platform } = await import('../services');
      const before = (platform() as unknown as { releaseNotesOpened(): number }).releaseNotesOpened();
      fireEvent.click(link);
      await waitFor(() => expect((platform() as unknown as { releaseNotesOpened(): number }).releaseNotesOpened()).toBe(before + 1));
      expect(screen.getByTestId('settings-about').textContent).toContain(LICENCE[lang]);
    });
  }

  // The direct macOS download has its own Updates panel (docs/UPDATER.md, keys updates.*, test updates.test.tsx);
  // the store build keeps none of the removed settings.* switches.
  it('no update feed, channel or "Check now" string is left among the settings.* keys', () => {
    for (const lang of LANGS) {
      const ui = (resources as unknown as Record<string, { ui: Record<string, string> }>)[lang].ui;
      for (const k of ['checkAuto', 'checkHint', 'channel', 'stable', 'beta', 'current', 'version', 'checkNow', 'noUpdates']) {
        expect(ui[`settings.${k}`], `${lang} settings.${k}`).toBeUndefined();
      }
      expect(ui['settings.privacyText']).not.toMatch(/feed|flux|Feed|release/);
    }
    const s = useApp.getState().settings as unknown as Record<string, unknown>;
    expect(s.updateCheck).toBeUndefined();
    expect(s.updateChannel).toBeUndefined();
  });
});

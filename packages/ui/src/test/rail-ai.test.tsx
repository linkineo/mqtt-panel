import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { boot } from './util';
import { useApp } from '../store/app';
import { App } from '../App';
import { resources } from '../i18n';

// Owner request, 20 Oct 2026: a rail item "AI" after Broker opens Settings on the Assistant page (TESTPLAN M-18).
describe('Rail item AI (M-18)', () => {
  it('sits right after Broker, with the sparkle icon and its label', async () => {
    await boot('demo');
    render(<App />);
    const ids = Array.from(document.querySelectorAll('.rail button')).map((b) => b.getAttribute('data-testid'));
    expect(ids).toEqual(['rail-connect', 'rail-explore', 'rail-publish', 'rail-charts', 'rail-broker', 'rail-ai', 'rail-settings']);
    const ai = screen.getByTestId('rail-ai');
    expect(ai.textContent).toBe('AI');
    expect(ai.getAttribute('aria-label')).toBe('AI assistant');
    expect(ai.tagName).toBe('BUTTON');
    expect(ai.querySelector('svg path')).not.toBeNull();
    expect(ai.querySelector('.badge, .cnt')).toBeNull();
    expect(ai.getAttribute('aria-current')).toBeNull();
  });

  it('opens Settings on Assistant; aria-current on AI only there, Settings lit on any other page', async () => {
    await boot('demo');
    render(<App />);
    fireEvent.click(screen.getByTestId('rail-ai'));
    expect(useApp.getState().screen).toBe('settings');
    expect(useApp.getState().settingsPage).toBe('assistant');
    expect(screen.getByTestId('settings-assistant')).toBeTruthy();
    expect(screen.getByTestId('settings-nav-assistant').getAttribute('aria-selected')).toBe('true');
    expect(screen.getByTestId('rail-ai').getAttribute('aria-current')).toBe('page');
    expect(screen.getByTestId('rail-ai').className).toBe('on');
    expect(screen.getByTestId('rail-settings').getAttribute('aria-current')).toBeNull();
    // another Settings page: AI goes off, Settings lights
    fireEvent.click(screen.getByTestId('settings-nav-security'));
    expect(screen.getByTestId('rail-ai').getAttribute('aria-current')).toBeNull();
    expect(screen.getByTestId('rail-settings').getAttribute('aria-current')).toBe('page');
    // another screen: neither
    fireEvent.click(screen.getByTestId('rail-broker'));
    expect(screen.getByTestId('rail-ai').getAttribute('aria-current')).toBeNull();
    expect(screen.getByTestId('rail-settings').getAttribute('aria-current')).toBeNull();
    // Settings keeps its own behaviour: it opens on the last page chosen
    fireEvent.click(screen.getByTestId('rail-settings'));
    expect(useApp.getState().settingsPage).toBe('security');
    expect(screen.getByTestId('rail-settings').getAttribute('aria-current')).toBe('page');
  });

  it('openSettings bumps the nonce so Settings returns to the page when it is already selected', async () => {
    await boot('demo');
    const n = useApp.getState().settingsNonce;
    act(() => useApp.getState().openSettings('assistant'));
    act(() => useApp.getState().openSettings('assistant'));
    expect(useApp.getState().settingsNonce).toBe(n + 2);
    expect(useApp.getState().screen).toBe('settings');
  });

  it('labels in FR and DE', () => {
    const ui = (l: 'en' | 'fr' | 'de') => (resources[l] as Record<string, Record<string, string>>).ui;
    expect([ui('en')['rail.ai'], ui('fr')['rail.ai'], ui('de')['rail.ai']]).toEqual(['AI', 'IA', 'KI']);
    expect([ui('en')['rail.aiLabel'], ui('fr')['rail.aiLabel'], ui('de')['rail.aiLabel']]).toEqual(['AI assistant', 'Assistant IA', 'KI-Assistent']);
  });
});

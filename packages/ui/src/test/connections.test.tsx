import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { boot } from './util';
import { engine } from '../services';
import { useApp } from '../store/app';
import { saveProfile } from '../lib/actions';
import { ProfileForm } from '../screens/connections/ProfileForm';

describe('Connections form (C-08)', () => {
  it('starts with "# · QoS 0" meaning everything, and warns when the last chip is removed', async () => {
    await boot('empty');
    const p = await engine().newProfile();
    const saved = await saveProfile({ ...p, name: 'Test' });
    useApp.setState({ selectedProfileId: saved.id, folds: { security: false, advanced: true, brokerAdvanced: false } });
    render(<ProfileForm profileId={saved.id} />);
    const subs = screen.getByTestId('subscriptions');
    const chips = within(subs).getAllByTestId('sub-chip');
    expect(chips).toHaveLength(1);
    expect(chips[0].textContent).toContain('# · QoS 0');
    expect(within(subs).getByText('everything')).toBeInTheDocument();
    expect(screen.queryByTestId('no-subs')).toBeNull();
    await act(async () => fireEvent.click(within(subs).getByTestId('sub-remove')));
    expect(within(subs).queryAllByTestId('sub-chip')).toHaveLength(0);
    expect(screen.getByTestId('no-subs').textContent).toBe('No subscriptions: you will see nothing until you add one.');
    // the notice stays visible with the Advanced fold closed, and the summary says it
    await act(async () => fireEvent.click(screen.getByTestId('fold-advanced')));
    expect(screen.queryByTestId('subscriptions')).toBeNull();
    expect(screen.getByTestId('no-subs')).toBeInTheDocument();
    expect(screen.getByTestId('fold-advanced-summary').textContent).toBe('keep-alive 60 s · no subscriptions');
  });

  it('adds a subscription with the + Add popover', async () => {
    await boot('empty');
    const p = await engine().newProfile();
    const saved = await saveProfile({ ...p, name: 'Test', subscriptions: [] });
    useApp.setState({ folds: { security: false, advanced: true, brokerAdvanced: false } });
    render(<ProfileForm profileId={saved.id} />);
    await act(async () => fireEvent.click(screen.getByTestId('sub-add')));
    fireEvent.change(screen.getByTestId('sub-filter'), { target: { value: 'building/#' } });
    await act(async () => fireEvent.click(screen.getByTestId('sub-add-ok')));
    expect(screen.getAllByTestId('sub-chip')[0].textContent).toContain('building/# · QoS 0');
    expect(screen.queryByTestId('no-subs')).toBeNull();
  });
});

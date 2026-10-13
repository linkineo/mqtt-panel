import { describe, expect, it } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import { boot } from './util';
import { engine } from '../services';
import { useApp } from '../store/app';
import { BrokerScreen } from '../screens/broker/BrokerScreen';
import { checkPort } from '../screens/broker/BrokerDialogs';
import { renderMessage } from '../i18n';
import i18next from 'i18next';

describe('Broker anonymous gating (12b)', () => {
  it('greys the users table, the client-certificate panel and Advanced security', async () => {
    await boot('demo');
    const cfg = await engine().setAnonymous(true);
    useApp.setState({ brokerConfig: cfg, screen: 'broker', folds: { security: false, advanced: false, brokerAdvanced: true } });
    render(<BrokerScreen />);
    await waitFor(() => expect(screen.getAllByTestId('issued-row').length).toBeGreaterThan(0));
    // change set 4: one sentence
    expect(screen.getByTestId('anon-warning').textContent).toBe('Anonymous access is on: anyone who can reach a listener can connect.');
    expect(screen.getByTestId('users-table').parentElement!.className).toContain('gated');
    expect(screen.getByTestId('add-user')).toBeDisabled();
    expect(screen.getByTestId('anonymous')).not.toBeDisabled();
    expect(screen.getByTestId('client-certs').className).toContain('gated');
    expect(screen.getByTestId('issue-cert')).toBeDisabled();
    expect(screen.getByTestId('import-csr')).toBeDisabled();
    // the requirement and Verify against live in the "Advanced security" fold, gated with it
    expect(screen.getByTestId('advanced-security').contains(screen.getByTestId('verify-against'))).toBe(true);
    expect(within(screen.getByTestId('require-cert-seg')).getAllByRole('radio').every((b) => (b as HTMLButtonElement).disabled)).toBe(true);
    expect(screen.getByTestId('fold-brokerAdvanced-summary').textContent).toBe('not checked while Anonymous is on');
    expect(screen.queryAllByTestId('listener-cert')).toHaveLength(0);
  });

  it('restores the previous setup when Anonymous is off', async () => {
    await boot('demo');
    const cfg = await engine().setAnonymous(false);
    useApp.setState({ brokerConfig: cfg });
    render(<BrokerScreen />);
    await waitFor(() => expect(screen.getAllByTestId('issued-row').length).toBeGreaterThan(0));
    expect(screen.queryByTestId('anon-warning')).toBeNull();
    expect(screen.getByTestId('users-table').parentElement!.className).not.toContain('gated');
    expect(screen.getByTestId('client-certs').className).not.toContain('gated');
    // no permanent network explanation (change set 4): it lives in the ⓘ next to the reachability choice
    expect(screen.queryByTestId('network-warning')).toBeNull();
    expect(screen.getByTestId('fold-brokerAdvanced-summary').textContent).toContain('client certificates required · local CA');
    await act(async () => undefined);
  });
});

describe('Port validation (B-03, UI side)', () => {
  it('uses the exact messages', async () => {
    await boot('demo');
    const t = i18next.t.bind(i18next);
    expect(renderMessage(t, await checkPort(443, 'tls', null, true))).toBe('Needs admin rights below 1024. Try 8883.');
    expect(renderMessage(t, await checkPort(443, 'tcp', null, true))).toBe('Needs admin rights below 1024. Try 1883.');
    expect(renderMessage(t, await checkPort(443, 'tls', 'l-tls', false))).toBe('Ports below 1024 need administrator rights; choose 1024 or higher.');
    expect(renderMessage(t, await checkPort(70000, 'tls', null, true))).toBe('Choose a port between 1024 and 65535.');
    expect(renderMessage(t, await checkPort(1883, 'tcp', null, true))).toBe('Another listener already uses port 1883.');
    expect(await checkPort(9883, 'tls', null, true)).toBeNull();
  });
});

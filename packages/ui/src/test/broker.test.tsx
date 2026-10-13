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
  it('greys users and client certificates and shows "not checked"', async () => {
    await boot('demo');
    const cfg = await engine().setAnonymous(true);
    useApp.setState({ brokerConfig: cfg, screen: 'broker' });
    render(<BrokerScreen />);
    await waitFor(() => expect(screen.getAllByTestId('issued-row').length).toBeGreaterThan(0));
    expect(screen.getByTestId('anon-warning').textContent).toContain('Anonymous access is on.');
    expect(screen.getByTestId('users').className).toContain('gated');
    // change set 2: "On encrypted listeners" and "Verify against" live under Client certificates, gated with it
    expect(screen.getByTestId('client-certs').contains(screen.getByTestId('verify-against'))).toBe(true);
    expect(within(screen.getByTestId('require-cert-seg')).getAllByRole('radio').every((b) => (b as HTMLButtonElement).disabled)).toBe(true);
    expect(screen.getByTestId('client-certs').className).toContain('gated');
    expect(screen.getByTestId('issue-cert')).toBeDisabled();
    expect(screen.queryAllByTestId('listener-login')).toHaveLength(0);
    const tls = screen.getAllByTestId('listener-row').find((r) => r.getAttribute('data-listener') === 'l-tls')!;
    expect(within(tls).getByTestId('listener-cert').textContent).toBe('not checked');
  });

  it('restores the previous setup when Anonymous is off', async () => {
    await boot('demo');
    const cfg = await engine().setAnonymous(false);
    useApp.setState({ brokerConfig: cfg });
    render(<BrokerScreen />);
    await waitFor(() => expect(screen.getAllByTestId('issued-row').length).toBeGreaterThan(0));
    expect(screen.queryByTestId('anon-warning')).toBeNull();
    expect(screen.getByTestId('users').className).not.toContain('gated');
    const tls = screen.getAllByTestId('listener-row').find((r) => r.getAttribute('data-listener') === 'l-tls')!;
    expect(within(tls).getByTestId('listener-cert').textContent).toBe('required');
    expect(screen.getByTestId('network-warning').textContent).toContain('Reachable from the network.');
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

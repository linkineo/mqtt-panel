import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { boot } from './util';
import { engine } from '../services';
import { useApp } from '../store/app';
import { BrokerScreen } from '../screens/broker/BrokerScreen';
import { ConnectionsScreen } from '../screens/connections/ConnectionsScreen';
import { ProfileForm } from '../screens/connections/ProfileForm';
import { AppBrokerDialog, rankedListeners } from '../screens/connections/AppBrokerDialog';
import { IssueCertDialog } from '../screens/broker/BrokerDialogs';
import { abbreviate } from '../components/Secret';
import type { Listener } from '@mqttpanel/engine-client';

const L = (id: string, kind: Listener['kind'], enabled = true): Listener => ({
  id,
  kind,
  port: 1000,
  enabled,
  preset: 'plain',
});

describe('Change set 1: "This app\'s broker" (02b)', () => {
  it('preselects mutual TLS, then TLS, then secure WebSocket, then TCP, then WebSocket', () => {
    const ls = [L('ws', 'ws'), L('tcp', 'tcp'), L('wss', 'wss'), L('tls', 'tls'), L('off', 'tls', false)];
    expect(rankedListeners(ls, false).map((l) => l.id)).toEqual(['tls', 'wss', 'tcp', 'ws']);
    // the client certificate requirement is broker-wide: with Required, TLS and WSS are mutual TLS
    expect(rankedListeners(ls, true).map((l) => l.id)).toEqual(['tls', 'wss', 'tcp', 'ws']);
  });

  it('C-12: "New connection" is a menu; "New broker…" opens the form without a Target row', async () => {
    const { platform } = await boot('demo');
    render(<ConnectionsScreen />);
    await act(async () => fireEvent.click(screen.getByTestId('new-profile')));
    const menu = await screen.findByTestId('context-menu');
    expect(Array.from(menu.querySelectorAll('.ctx-label')).map((x) => x.textContent)).toEqual(['New broker…', 'Built-in broker']);
    const app = within(menu).getByText('Built-in broker').closest('.ctx-item') as HTMLElement;
    await act(async () => fireEvent.mouseEnter(app));
    const sub = await screen.findByTestId('context-submenu');
    expect(Array.from(sub.querySelectorAll('.ctx-label')).map((x) => x.textContent)).toEqual(['Use an existing listener', 'Add a listener…']);
    const before = useApp.getState().profiles.length;
    await act(async () => fireEvent.click(within(menu).getByText('New broker…')));
    await waitFor(() => expect(useApp.getState().profiles.length).toBe(before + 1));
    expect(screen.getByTestId('profile-form')).toBeInTheDocument();
    expect(screen.queryByTestId('target-app')).toBeNull();
    expect(screen.queryByText('Target')).toBeNull();
    expect(screen.getByTestId('try-connect').textContent).toBe('Test');
    void platform;
  });

  it('runs the steps, creates "mqtt-panel" and opens the Explorer on "Local broker"', async () => {
    await boot('empty');
    const closed: boolean[] = [];
    render(<AppBrokerDialog start="pick" onClose={() => closed.push(true)} />);
    const cards = await screen.findAllByTestId('appb-listener');
    // empty scenario: TCP, TLS and WS on, WSS off; TLS is the most secure enabled one
    expect(cards[0].getAttribute('data-listener')).toBe('l-tls');
    expect(cards[0].getAttribute('aria-checked')).toBe('true');
    expect(screen.getByTestId('appb-what').textContent).toContain('mqtt-panel');
    await act(async () => fireEvent.click(screen.getByTestId('appb-connect')));
    await waitFor(() => expect(closed).toEqual([true]), { timeout: 4000 });
    const st = useApp.getState();
    expect(st.screen).toBe('explore');
    const local = st.profiles.filter((p) => p.isLocalBroker);
    expect(local).toHaveLength(1);
    expect(local[0].username).toBe('mqtt-panel');
    expect(local[0].scheme).toBe('mqtts');
    const cfg = await engine().brokerConfig();
    expect(cfg.users.map((u) => [u.username, u.system])).toEqual([['mqtt-panel', true]]);
  });

  it('with Anonymous on, creates no user and says so', async () => {
    await boot('empty');
    await engine().setAnonymous(true);
    render(<AppBrokerDialog start="pick" onClose={() => undefined} />);
    await screen.findAllByTestId('appb-listener');
    expect(screen.getByTestId('appb-what').textContent).toContain('Anonymous is on');
    await act(async () => fireEvent.click(screen.getByTestId('appb-connect')));
    await waitFor(() => expect(useApp.getState().screen).toBe('explore'), { timeout: 4000 });
    expect((await engine().brokerConfig()).users).toHaveLength(0);
  });
});

describe('Change set 1: Broker screen (12, 12c)', () => {
  it('has no "Connect Explorer here", puts Users above Client certificates and lists users in a table', async () => {
    await boot('demo');
    useApp.setState({ brokerConfig: await engine().brokerConfig(), screen: 'broker' });
    render(<BrokerScreen />);
    await waitFor(() => expect(screen.getAllByTestId('issued-row').length).toBeGreaterThan(0));
    expect(screen.queryByTestId('connect-here')).toBeNull();
    expect(screen.queryByText('Connect Explorer here')).toBeNull();
    const users = screen.getByTestId('users');
    const certs = screen.getByTestId('client-certs');
    expect(users.compareDocumentPosition(certs) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const rows = screen.getAllByTestId('user-row');
    expect(rows.map((r) => r.getAttribute('data-user'))).toEqual(['mqtt-panel', 'commissioning', 'edge01', 'dashboard']);
    // change set 4: a muted "Local broker" tag instead of the sentence
    expect(within(rows[0]).getByTestId('user-local-tag').textContent).toBe('Local broker');
    expect(rows[0].textContent).not.toContain('created for the profile');
    expect(screen.getAllByRole('columnheader').map((h) => h.textContent)).not.toContain('Login');
  });

  it('B-16: the eye reveals a user password and the copy icon copies it', async () => {
    const { platform } = await boot('demo');
    useApp.setState({ brokerConfig: await engine().brokerConfig(), screen: 'broker' });
    render(<BrokerScreen />);
    await waitFor(() => expect(screen.getAllByTestId('user-row').length).toBe(4));
    const value = screen.getByTestId('user-pw-mqtt-panel-value');
    expect(value.textContent).toBe('••••••••••••');
    const eye = screen.getByTestId('user-pw-mqtt-panel-eye');
    expect(eye.getAttribute('title')).toBe('Show password');
    await act(async () => fireEvent.click(eye));
    await waitFor(() => expect(value.textContent).toBe('Vq7m…kR2p'));
    expect(eye.getAttribute('title')).toBe('Hide password');
    expect(screen.getByTestId('user-pw-mqtt-panel-copy').getAttribute('title')).toBe('Copy');
    await act(async () => fireEvent.click(screen.getByTestId('user-pw-edge01-copy')));
    await waitFor(() => expect(platform.lastCopied()).toBe('edge01-Hx4q9'));
    await act(async () => fireEvent.click(eye));
    expect(value.textContent).toBe('••••••••••••');
  });
});

describe('Change set 1: eye and copy on passwords the app owns (B-16)', () => {
  it('shows the generated PKCS#12 passphrase and copies it', async () => {
    const { platform } = await boot('demo');
    render(<IssueCertDialog onClose={() => undefined} onIssued={() => undefined} />);
    await act(async () => fireEvent.click(screen.getByText('Generate')));
    const input = screen.getByTestId('issue-passphrase') as HTMLInputElement;
    expect(input.type).toBe('password');
    expect(input.value).toHaveLength(20);
    await act(async () => fireEvent.click(screen.getByTestId('issue-passphrase-eye')));
    expect(input.type).toBe('text');
    await act(async () => fireEvent.click(screen.getByTestId('issue-passphrase-copy')));
    await waitFor(() => expect(platform.lastCopied()).toBe(input.value));
  });

  it('copies the "Local broker" profile password; the eye of other profiles reveals theirs (change set 11)', async () => {
    const { platform } = await boot('demo');
    render(<ProfileForm profileId="local" />);
    await act(async () => fireEvent.click(screen.getByTestId('profile-pw-copy')));
    await waitFor(() => expect(platform.lastCopied()).toBe('Vq7mZ3xLwT9bN4cHs8kR2p'));
    // change set 11: the eye of the password field reveals the stored password of any profile (no copy icon there)
    expect(await engine().profilePassword('plant')).toBe('site-2026-commissioning');
  });

  it('abbreviates long secrets to the first and last four characters', () => {
    expect(abbreviate('Vq7mZ3xLwT9bN4cHs8kR2p')).toBe('Vq7m…kR2p');
    expect(abbreviate('short')).toBe('short');
  });
});

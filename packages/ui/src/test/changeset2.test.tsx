import { describe, expect, it } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { boot, connect } from './util';
import { engine } from '../services';
import { useApp } from '../store/app';
import { App } from '../App';
import { ConnectionsScreen } from '../screens/connections/ConnectionsScreen';
import { BrokerScreen } from '../screens/broker/BrokerScreen';
import { PublishScreen } from '../screens/publish/PublishScreen';

const menuLabels = () => Array.from(screen.getByTestId('context-menu').querySelectorAll('.ctx-label')).map((x) => x.textContent);
const card = (id: string) => screen.getAllByTestId('profile-card').find((c) => c.getAttribute('data-profile') === id)!;
const sync = async () => {
  useApp.setState({ connections: await engine().listConnections(), profiles: await engine().listProfiles() });
};

describe('Change set 2: Connections (C-14)', () => {
  it('fills a connected card with the accent colour, outlines a reconnecting one, chip on the address line', async () => {
    const booted = await boot('demo');
    await connect('local');
    await connect('plant');
    booted.engine.setConnectionState('plant', 'reconnecting');
    await sync();
    render(<ConnectionsScreen />);
    const local = card('local');
    expect(local.className).toContain('live');
    expect(within(local).getByTestId('card-state').textContent).toBe('connected');
    const plant = card('plant');
    expect(plant.className).toContain('retry');
    expect(within(plant).getByTestId('card-state').textContent).toBe('reconnecting');
    const lab = card('lab');
    expect(lab.className).not.toContain('live');
    expect(within(lab).queryByTestId('card-state')).toBeNull();
    // change set 6: the protocol chip on its own line, "host:port" under it
    const chip = within(local).getByTestId('card-chip');
    expect(chip.textContent).toBe('mqtts');
    expect(within(local).getByTestId('card-address').textContent).toBe('localhost:8883');
  });

  it('card menu: Connect or Disconnect by state, Test, Duplicate, Delete… with a confirmation naming the profile', async () => {
    await boot('demo');
    await connect('local');
    render(<ConnectionsScreen />);
    await act(async () => fireEvent.contextMenu(card('local'), { clientX: 10, clientY: 10 }));
    expect(menuLabels()).toEqual(['Disconnect', 'Test', 'Duplicate', 'Move up', 'Move down', 'Delete…']);
    await act(async () => fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' }));
    await act(async () => fireEvent.contextMenu(card('lab'), { clientX: 10, clientY: 10 }));
    expect(menuLabels()[0]).toBe('Connect');
    // Duplicate
    const before = useApp.getState().profiles.length;
    await act(async () => fireEvent.click(within(screen.getByTestId('context-menu')).getByText('Duplicate')));
    await waitFor(() => expect(useApp.getState().profiles.length).toBe(before + 1));
    // Delete… on the connected profile: confirmation, disconnects first, removes
    await act(async () => fireEvent.contextMenu(card('local'), { clientX: 10, clientY: 10 }));
    await act(async () => fireEvent.click(within(screen.getByTestId('context-menu')).getByText('Delete…')));
    const dlg = await screen.findByTestId('delete-profile');
    expect(dlg.textContent).toContain('Delete the connection "Local broker"?');
    expect(within(dlg).getByTestId('confirm-ok').textContent).toBe('Delete');
    expect(within(dlg).getByText('Cancel')).toBeInTheDocument();
    await act(async () => fireEvent.click(within(dlg).getByTestId('confirm-ok')));
    await waitFor(() => expect(useApp.getState().profiles.some((p) => p.id === 'local')).toBe(false));
    expect((await engine().listConnections()).some((c) => c.id === 'local')).toBe(false);
  });

  it('form header: Delete outlined in brick, then Test and Connect; no Duplicate button', async () => {
    await boot('demo');
    useApp.setState({ selectedProfileId: 'plant' });
    render(<ConnectionsScreen />);
    const form = screen.getByTestId('profile-form');
    const head = form.querySelector('.ph .row')!;
    const labels = Array.from(head.querySelectorAll('button')).map((b) => b.textContent);
    expect(labels).toEqual(['Delete', 'Test', 'Connect']);
    expect(within(form).getByTestId('delete-profile-btn').className).toContain('danger out');
    expect(within(form).getByTestId('delete-profile-btn').className).not.toContain(' sm');
    expect(within(form).queryByText('Duplicate')).toBeNull();
    expect(within(form).queryByText('Delete connection…')).toBeNull();
    await act(async () => fireEvent.click(within(form).getByTestId('delete-profile-btn')));
    expect((await screen.findByTestId('delete-profile')).textContent).toContain('Delete the connection "Plant broker"?');
  });
});

describe('Change set 2: connection bar (C-15)', () => {
  it('appears with the first open connection, one tab each, click opens the Explorer, ✕ disconnects, hidden when none', async () => {
    await boot('demo');
    useApp.setState({ broker: await engine().brokerStatus(), screen: 'connect' });
    render(<App />);
    // change set 8: the bar stays, empty, with its hint
    expect(screen.getByTestId('cbar')).toHaveAttribute('data-empty', 'true');
    expect(screen.queryAllByTestId('cbar-tab')).toHaveLength(0);
    expect(screen.queryByTestId('pill')).toBeNull();
    expect(screen.queryByTestId('rail-connect-count')).toBeNull();
    await act(async () => {
      await connect('plant');
      await connect('local');
    });
    const tabs = screen.getAllByTestId('cbar-tab');
    expect(tabs.map((t) => t.getAttribute('data-conn'))).toEqual(['plant', 'local']);
    expect(tabs[0].textContent).toMatch(/^Plant broker· \d[\d  ]* topics · [\d.,]+ msg\/s · MQTT 5✕$/);
    expect(tabs[1].getAttribute('aria-selected')).toBe('true');
    expect(screen.getByTestId('rail-connect-count').textContent).toBe('2');
    useApp.setState({ screen: 'connect' });
    await act(async () => fireEvent.click(tabs[0]));
    expect(useApp.getState().screen).toBe('explore');
    expect(useApp.getState().activeConnId).toBe('plant');
    expect(screen.getAllByTestId('cbar-tab')[0].getAttribute('aria-selected')).toBe('true');
    await act(async () => fireEvent.click(within(screen.getAllByTestId('cbar-tab')[0]).getByTestId('cbar-close')));
    await act(async () => sync());
    expect(screen.getAllByTestId('cbar-tab').map((t) => t.getAttribute('data-conn'))).toEqual(['local']);
    expect(useApp.getState().activeConnId).toBe('local');
    await act(async () => fireEvent.click(screen.getByTestId('cbar-close')));
    await act(async () => sync());
    // change set 8: the bar stays, empty, with its hint
    expect(screen.getByTestId('cbar')).toHaveAttribute('data-empty', 'true');
    expect(screen.queryAllByTestId('cbar-tab')).toHaveLength(0);
    expect(screen.queryByTestId('rail-connect-count')).toBeNull();
  });

});

describe('Change set 2: Publish (P-11)', () => {
  it('"Send ⌘↵" is top right in the composer header; Send and clear, repeat and Delete retained stay at the bottom', async () => {
    await boot('demo');
    await connect('plant');
    render(<PublishScreen />);
    const composer = screen.getByTestId('composer');
    const head = composer.querySelector(':scope > .ph')!;
    const send = within(composer).getByTestId('send');
    expect(head.contains(send)).toBe(true);
    const buttons = Array.from(head.querySelectorAll('button'));
    expect(buttons[buttons.length - 1]).toBe(send);
    expect(send.className).toContain('pri');
    for (const id of ['send-clear', 'repeat-s', 'delete-retained-btn']) expect(head.contains(within(composer).getByTestId(id))).toBe(false);
  });
});

describe('Change set 2: Broker (B-18, B-19)', () => {
  it('B-18: one broker-wide control, in "Advanced security" (change set 4); listener rows never show it', async () => {
    await boot('demo');
    useApp.setState({ brokerConfig: await engine().brokerConfig(), broker: await engine().brokerStatus(), screen: 'broker' });
    render(<BrokerScreen />);
    await waitFor(() => expect(screen.getAllByTestId('issued-row').length).toBeGreaterThan(0));
    expect(screen.queryByTestId('advanced-security')).toBeNull();
    await act(async () => fireEvent.click(screen.getByTestId('fold-brokerAdvanced')));
    const adv = screen.getByTestId('advanced-security');
    const policy = within(adv).getByTestId('require-cert');
    expect(policy.textContent).toContain('Client certificates');
    expect(policy.textContent).toContain('on encrypted listeners');
    expect(within(adv).getByTestId('verify-against').textContent).toContain('Verify against');
    const seg = within(policy).getByTestId('require-cert-seg');
    expect(within(seg).getAllByRole('radio').map((b) => b.textContent)).toEqual(['Optional', 'Required']);
    expect(screen.queryByTestId('require-cert-hint')).toBeNull();
    expect(screen.queryAllByTestId('listener-cert')).toHaveLength(0);
    expect(screen.getByTestId('fold-brokerAdvanced-summary').textContent).toContain('client certificates required');
    await act(async () => fireEvent.click(within(seg).getByText('Optional')));
    await waitFor(() => expect(screen.getByTestId('fold-brokerAdvanced-summary').textContent).toContain('client certificates optional'));
    // applied at once: no prompt (change set 7)
    expect(screen.queryByTestId('restart-prompt')).toBeNull();
    expect((await engine().brokerConfig()).requireClientCert).toBe(false);
  });

  it('B-18: the "Encrypted + client certificates" preset sets Required; step 2 asks kind and port only', async () => {
    await boot('demo');
    await engine().setRequireClientCert(false);
    useApp.setState({ brokerConfig: await engine().brokerConfig(), broker: await engine().brokerStatus(), screen: 'broker' });
    render(<BrokerScreen />);
    await waitFor(() => expect(screen.getAllByTestId('issued-row').length).toBeGreaterThan(0));
    await act(async () => fireEvent.click(screen.getByTestId('add-listener')));
    const step1 = await screen.findByTestId('add-listener-1');
    await act(async () => fireEvent.click(within(step1).getByText('Encrypted + client certificates')));
    expect(screen.getByTestId('preset-explain').textContent).toContain('It sets client certificates to Required for every encrypted listener (one setting, under Advanced security) and uses the local CA.');
    await act(async () => fireEvent.click(within(step1).getByText('Next')));
    const step2 = await screen.findByTestId('add-listener-2');
    expect(within(step2).queryByTestId('addl-client-cert')).toBeNull();
    expect(Array.from(step2.querySelectorAll('.f > .lbl, .f > label')).map((l) => l.textContent)).toEqual(['Kind', 'Port']);
    fireEvent.change(screen.getByTestId('addl-port'), { target: { value: '9884' } });
    await act(async () => fireEvent.click(screen.getByTestId('addl-add')));
    await waitFor(async () => expect((await engine().brokerConfig()).requireClientCert).toBe(true));
  });

  it('B-19: clients and log update live with no Refresh; the log opens collapsed and expands on click, remembered', async () => {
    const booted = await boot('demo');
    useApp.setState({ brokerConfig: await engine().brokerConfig(), broker: await engine().brokerStatus(), screen: 'broker' });
    const { unmount } = render(<BrokerScreen />);
    await waitFor(() => expect(screen.getAllByTestId('issued-row').length).toBeGreaterThan(0));
    const side = screen.getByTestId('broker-side');
    expect(within(side).queryByText('Refresh')).toBeNull();
    expect(screen.queryByTestId('broker-log')).toBeNull();
    const strip = screen.getByTestId('broker-log-strip');
    // change set 4: no "live", no "click to expand"
    await waitFor(() => expect(strip.textContent).toBe('▸Log· 2 warnings'));
    // live: a client disconnected elsewhere leaves the table without a refresh
    const rows = () => within(screen.getByTestId('clients')).getAllByRole('row').length;
    const n = rows();
    await act(async () => {
      await booted.engine.disconnectClient('cli-sub-21');
      useApp.setState({ broker: await engine().brokerStatus() });
    });
    await waitFor(() => expect(rows()).toBe(n - 1));
    await act(async () => fireEvent.click(screen.getByTestId('log-toggle')));
    expect(screen.getByTestId('broker-log')).toBeInTheDocument();
    expect(useApp.getState().settings.brokerLogOpen).toBe(true);
    expect((await engine().getSettings()).brokerLogOpen).toBe(true);
    unmount();
    render(<BrokerScreen />);
    await waitFor(() => expect(screen.getByTestId('broker-log')).toBeInTheDocument());
    await act(async () => fireEvent.click(screen.getByTestId('log-toggle')));
    expect(screen.queryByTestId('broker-log')).toBeNull();
  });
});

describe('Change set 2: the profile "Local broker" follows the broker (C-16, mock engine)', () => {
  it('port, client-certificate requirement, mqtt-panel password and Anonymous update the profile at once', async () => {
    await boot('demo');
    const local = () => engine().listProfiles().then((ps) => ps.find((p) => p.isLocalBroker)!);
    await connect('local');
    const l = (await engine().brokerConfig()).listeners.find((x) => x.id === 'l-tls')!;
    await engine().updateListener({ ...l, port: 9993 });
    expect((await local()).port).toBe(9993);
    await engine().setRequireClientCert(false);
    expect((await local()).tls.clientCert).toBeNull();
    await engine().setRequireClientCert(true);
    expect((await local()).tls.clientCert).not.toBeNull();
    await engine().setUserPassword('mqtt-panel', 'new-password-123');
    expect(await engine().profilePassword('local')).toBe('new-password-123');
    await engine().setAnonymous(true);
    expect((await local()).username).toBe('');
    await engine().setAnonymous(false);
    expect((await local()).username).toBe('mqtt-panel');
    // nothing restarts (change set 7): the connection is open again after each change of its own settings
    expect((await engine().listConnections()).find((c) => c.id === 'local')!.state).toBe('connected');
  });
});

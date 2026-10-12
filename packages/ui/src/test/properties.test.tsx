import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { MessageView } from '@mqttpanel/engine-client';
import { boot } from './util';
import { PropertiesView } from '../screens/explorer/Detail';

const base: MessageView = {
  seq: 1,
  topic: 'building/f2/ahu01/cmd',
  receivedAt: Date.now(),
  qos: 2,
  retain: false,
  dup: false,
  size: 27,
  truncated: false,
  decoded: { format: 'json', detected: 'json', json: { cmd: 'restart' }, pretty: null, text: null, hex: null, base64: null, imageDataUrl: null, error: null, paged: false },
  properties: null,
};

describe('Properties tab (X-13)', () => {
  it('shows the protocol first and no MQTT 5 block on 3.1.1', async () => {
    await boot('empty');
    render(<PropertiesView message={base} negotiated="3.1.1" requested="auto" />);
    const dts = Array.from(screen.getByTestId('properties').querySelectorAll('dt')).map((d) => d.textContent);
    expect(dts[0]).toBe('Protocol');
    expect(screen.getByTestId('properties').textContent).toContain('MQTT 3.1.1');
    expect(screen.queryByTestId('mqtt5-props')).toBeNull();
    expect(screen.queryByText('MQTT 5 properties')).toBeNull();
  });

  it('shows the MQTT 5 block when properties are present', async () => {
    await boot('empty');
    const m: MessageView = {
      ...base,
      properties: {
        contentType: 'application/json',
        payloadFormat: 1,
        responseTopic: 'building/f2/ahu01/cmd/ack',
        correlationData: 'c-1041',
        messageExpiry: 30,
        userProperties: [['operator', 'commissioning']],
        subscriptionIds: [],
        topicAlias: null,
      },
    };
    render(<PropertiesView message={m} negotiated="5" requested="auto" />);
    expect(screen.getByTestId('mqtt5-props')).toBeInTheDocument();
    expect(screen.getByText('negotiated; profile asked "auto"')).toBeInTheDocument();
    expect(screen.getByText('application/json')).toBeInTheDocument();
    expect(screen.getByText('operator = commissioning')).toBeInTheDocument();
  });
});

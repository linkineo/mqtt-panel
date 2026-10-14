import { describe, expect, it } from 'vitest';
import { applyAddressText, applyScheme, formatAddress, isPartialPrefix, parseAddress, type AddressFields } from '../lib/address';

const lab: AddressFields = { scheme: 'mqtt', host: '10.20.4.17', port: 1883, wsPath: '/mqtt' };

describe('address prefiller: formatting (change set 5)', () => {
  it('prefix, host and port', () => {
    expect(formatAddress(lab)).toBe('mqtt://10.20.4.17:1883');
    expect(formatAddress({ ...lab, scheme: 'wss', host: 'demo.emqx.example', port: 8084 })).toBe('wss://demo.emqx.example:8084');
  });
  it('an empty host shows the prefix only', () => {
    expect(formatAddress({ ...lab, host: '' })).toBe('mqtt://');
  });
  it('IPv6 literals keep their brackets', () => {
    expect(formatAddress({ ...lab, host: '::1' })).toBe('mqtt://[::1]:1883');
    expect(formatAddress({ ...lab, host: '[::1]' })).toBe('mqtt://[::1]:1883');
  });
});

describe('address prefiller: parsing (change set 5)', () => {
  it('reads every prefix spelling, case-insensitive', () => {
    expect(parseAddress('mqtt://h').scheme).toBe('mqtt');
    expect(parseAddress('MQTTS://h').scheme).toBe('mqtts');
    expect(parseAddress('Ws://h').scheme).toBe('ws');
    expect(parseAddress('wss://h').scheme).toBe('wss');
    expect(parseAddress('tcp://h').scheme).toBe('mqtt');
    expect(parseAddress('ssl://h').scheme).toBe('mqtts');
    expect(parseAddress('TLS://h').scheme).toBe('mqtts');
    expect(parseAddress('h').scheme).toBeNull();
    expect(parseAddress('http://h')).toEqual({ scheme: null, host: 'h', port: null, path: null });
  });
  it('host, port and path', () => {
    expect(parseAddress('wss://demo.emqx.example:8084/mqtt')).toEqual({ scheme: 'wss', host: 'demo.emqx.example', port: 8084, path: '/mqtt' });
    expect(parseAddress('broker.example:1884')).toEqual({ scheme: null, host: 'broker.example', port: 1884, path: null });
    expect(parseAddress('broker.example:')).toEqual({ scheme: null, host: 'broker.example', port: null, path: null });
    expect(parseAddress('broker.example:99999').port).toBeNull();
    expect(parseAddress('broker.example:0').port).toBeNull();
    expect(parseAddress('  mqtt://h:1  ').port).toBe(1);
  });
  it('IPv6, bracketed with a port or bare', () => {
    expect(parseAddress('[::1]:1883')).toEqual({ scheme: null, host: '::1', port: 1883, path: null });
    expect(parseAddress('mqtt://[fe80::1]')).toEqual({ scheme: 'mqtt', host: 'fe80::1', port: null, path: null });
    expect(parseAddress('::1')).toEqual({ scheme: null, host: '::1', port: null, path: null });
    expect(parseAddress('[::1').host).toBe('::1');
  });
  it('a prefix being typed is not read yet', () => {
    for (const s of ['ws:', 'ws:/', 'mqtt:', 'MQTTS:/', 'tcp:/']) expect(isPartialPrefix(s), s).toBe(true);
    for (const s of ['', 'ws', 'mqtt://', 'localhost:', 'broker.example']) expect(isPartialPrefix(s), s).toBe(false);
  });
});

describe('address prefiller: applying to the profile (change set 5)', () => {
  it('a typed prefix selects the protocol; the port follows when it held the old default', () => {
    expect(applyAddressText(lab, 'ws://10.20.4.17')).toEqual({ ...lab, scheme: 'ws', port: 8083 });
    expect(applyAddressText({ ...lab, port: 1999 }, 'ws://10.20.4.17')).toEqual({ ...lab, scheme: 'ws', port: 1999 });
    expect(applyAddressText(lab, 'tls://10.20.4.17')).toEqual({ ...lab, scheme: 'mqtts', port: 8883 });
  });
  it('a deleted prefix keeps the protocol', () => {
    expect(applyAddressText({ ...lab, scheme: 'mqtts', port: 8883 }, '10.20.4.17:8883')).toEqual({ ...lab, scheme: 'mqtts', port: 8883 });
  });
  it('":port" after the host sets the port', () => {
    expect(applyAddressText(lab, 'mqtt://10.20.4.17:1884')).toEqual({ ...lab, port: 1884 });
    expect(applyAddressText(lab, 'mqtt://10.20.4.17:')).toEqual(lab);
  });
  it('a pasted full address sets protocol, host, port and the WebSocket path', () => {
    expect(applyAddressText(lab, 'wss://demo.emqx.example:8084/ws')).toEqual({ scheme: 'wss', host: 'demo.emqx.example', port: 8084, wsPath: '/ws' });
    // the path is for ws and wss only
    expect(applyAddressText(lab, 'mqtts://b.example:8883/x')).toEqual({ scheme: 'mqtts', host: 'b.example', port: 8883, wsPath: '/mqtt' });
  });
  it('a partial prefix leaves the profile as it is; an empty field is an empty host', () => {
    expect(applyAddressText(lab, 'ws:/')).toBe(lab);
    expect(applyAddressText(lab, '')).toEqual({ ...lab, host: '' });
  });
  it('is stable on its own output', () => {
    for (const text of ['wss://demo.emqx.example:8084/mqtt', 'tcp://[::1]:1883', 'h', 'ws:']) {
      const once = applyAddressText(lab, text);
      expect(applyAddressText(once, text)).toEqual(once);
      expect(applyAddressText(once, formatAddress(once))).toEqual(once);
    }
  });
  it('choosing a protocol switches a default port and keeps any other', () => {
    expect(applyScheme(lab, 'ws')).toEqual({ ...lab, scheme: 'ws', port: 8083 });
    expect(applyScheme({ ...lab, scheme: 'ws', port: 8083 }, 'wss')).toEqual({ ...lab, scheme: 'wss', port: 8084 });
    expect(applyScheme({ ...lab, port: 1884 }, 'mqtts')).toEqual({ ...lab, scheme: 'mqtts', port: 1884 });
  });
});

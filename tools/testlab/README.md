# Test lab

Two command-line scripts to test MQTT Panel by hand against the official mosquitto broker and
the official Eclipse Paho client. Nothing here runs in CI.

| Script | Direction | Use |
|---|---|---|
| `mosquitto_broker.py` | app → mosquitto | Start a mosquitto broker in one of five security modes; the script generates the certificates and prints what to enter and which files to pick in the app |
| `load_client.py` | Paho → app's broker | Connect standard clients to the embedded broker with the certificates exported from the app; generate 500 messages/s over 50 topics for throughput, chart and export tests; report rates |

## Setup, once

```
brew install mosquitto                          # macOS; Linux: apt install mosquitto mosquitto-clients
python3 -m venv tools/testlab/.venv
tools/testlab/.venv/bin/pip install paho-mqtt cryptography
```

Below, `py` stands for `tools/testlab/.venv/bin/python`.

## A. Connect the app to mosquitto

```
py tools/testlab/mosquitto_broker.py plain                   # port 1883, no TLS
py tools/testlab/mosquitto_broker.py tls                     # port 8883, private CA
py tools/testlab/mosquitto_broker.py tls-password            # + username and password
py tools/testlab/mosquitto_broker.py mtls --p12-password x   # + client certificate (PEM and .p12)
py tools/testlab/mosquitto_broker.py self-signed             # no CA: trust by fingerprint
```

Options: `--port`, `--ws-port` (adds a WebSocket listener), `--host <name or IP>` (extra names in the
server certificate, for another machine), `--user/--password`, `--seed` (a few retained demo topics),
`--keep` (reuse the certificates), `--verbose`. Files land in `tools/testlab/out/<mode>/` (git-ignored).

The script prints the connection values for "New connection > Another broker" and the files to pick with
Browse, plus a `mosquitto_sub` command to cross-check from a terminal. Ctrl-C stops the broker.

## B. Connect standard clients to the app's broker

Start the broker in the app, then:

```
py tools/testlab/load_client.py --check                                   # plain listener, round trip
py tools/testlab/load_client.py --port 8883 --cafile ~/Downloads/CA.pem   # encrypted listener
py tools/testlab/load_client.py --port 8883 --cafile CA.pem --user mqtt-panel --password '...'
py tools/testlab/load_client.py --port 8884 --cafile CA.pem --cert device.pem --key device-key.pem
py tools/testlab/load_client.py --port 8884 --cafile CA.pem --p12 device.p12 --p12-password '...'
```

Export the files from the app: Broker > Broker certificates > Export CA.pem; Broker > Client
certificates > Export (PEM or PKCS#12). `--insecure` skips certificate checks (lab only).

Load options: `--rate 500` messages/s in total, `--topics 50`, `--skew 1.0` (Zipf exponent, 0 = even),
`--duration 60`, `--report 5`, `--no-sub`, `--mqtt3`, `--ws`, `--prefix mqttpanel-lab`.

Every report line shows published and received rate, totals, lag (published minus received) and the
mean delivery latency of JSON messages. The subscriber uses QoS 1 on `<prefix>/#`.

Topic kinds (every 6th topic, 50 topics by default):

| Kind | Topic | Payload | Charts |
|---|---|---|---|
| climate | `sensors/<room>/climate` | JSON: temperature sine, humidity random walk, online bool, mode text | temperature, humidity, online (0/1) |
| energy | `meters/<n>/energy` | JSON: power sawtooth, total counter, phase text; QoS 1 | power_w, total_kwh |
| number | `machines/<n>/speed` | plain number, random walk | yes |
| onoff | `relays/<n>/state` | `on` / `off` text | 0/1 step |
| status | `devices/<n>/status` | `ok` / `warning` / `error` | not plottable |
| telemetry | `devices/<n>/telemetry` | nested JSON with counters; retained | counters.sent, device.rssi |

Retained topics stay on the broker after a run: `py tools/testlab/load_client.py --clear-retained`.

## Public brokers

Never point `load_client.py` at a public broker with the default rate. The project rule for public
brokers is a throwaway prefix, at most 50 messages and no retained messages left behind.

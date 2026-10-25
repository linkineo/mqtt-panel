# Notes for App Review

Sent to App Store Connect, App Review Information, Notes, by `scripts/asc/metadata.py` (everything below the marker, English only). Keep it under 1500 characters.

<!-- notes -->
MQTT Panel needs no account. To see it work with an external broker, use the public EMQX broker: in the sidebar choose Connect, then New connection, New broker…, host broker.emqx.io, port 1883, protocol mqtt (or port 8883 with mqtts, which verifies with the system certificates), Connect. Topics appear in Explore within seconds; publish to a topic such as mqttpanel-review/hello from Publish and see it under that topic.

To see the built-in broker: Broker, Start, then Connect, New connection, Built-in broker, Connect.

The AI assistant (beta) needs an MCP host such as Claude Desktop and can be ignored for the review.

MQTT Panel collects no data and makes no network calls except to the brokers the user configures.

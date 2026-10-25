# App Privacy (Mac App Store)

The App Store Connect API cannot set the privacy labels ("nutrition labels"). The owner declares them once in the web UI: App Store Connect, MQTT Panel, App Privacy, Get Started (or Edit).

## What to declare

1. "Do you or your third-party partners collect data from this app?" → **No, we do not collect data from this app.**
2. Publish. The listing then shows **Data Not Collected**, which covers every category: Contact Info, Health and Fitness, Financial Info, Location, Sensitive Info, Contacts, User Content, Browsing History, Search History, Identifiers, Purchases, Usage Data, Diagnostics, Other Data.
3. Privacy Policy URL (App Information, per language): https://mqtt-panel.com/privacy, https://mqtt-panel.com/fr/privacy, https://mqtt-panel.com/de/privacy (already set through the API).

## Why "Data Not Collected" is correct

- No telemetry, no analytics, no crash reporting, no advertising, no third-party SDK that sends data.
- No accounts and no sign-in.
Messages, profiles and history stay on the user's computer.
- Passwords, private keys and passphrases are stored in the macOS keychain, on the computer.
- The AI assistant feature (beta) runs as a local MCP server for an assistant the user installs; MQTT Panel itself sends nothing to any AI service.

Apple's definition: "collect" means transmitting data off the device in a way that allows the developer or partners to access it. MQTT Panel never does, so no category applies.

# Mac App Store submission

State of the MQTT Panel listing in App Store Connect (app id 6824290610, bundle id com.remora-labs.mqttpanel, macOS), how to update it, and what only the owner can do. The texts below are copies of `scripts/asc/texts.py`, which is the source the scripts send.

## Set through the API

- macOS App Store version **1.0.0** (state Prepare for Submission).
- App information per language (en-US, fr-FR, de-DE): name, subtitle, privacy policy URL.
- Version information per language: description, keywords, promotional text, support URL and marketing URL (https://mqtt-panel.com).
- Screenshots: 8 per language, display type APP_DESKTOP, 2560 x 1600 JPEG from `website/assets/media` (order in `texts.SCREENSHOTS`).
- Categories: Developer Tools (primary), Utilities (secondary).
- Age rating: no content flags (4+).
- Content rights: the app does not use third-party content.
- Price: free (price schedule with the 0.00 price point, base territory United States); available in all territories and in new ones.
- App Review information: notes from `REVIEW_NOTES.md`, no demo account (no sign-in).
- Build: the newest VALID build of the 1.0.0 train attached to the version.

"What's New" is refused by App Store Connect on an app's first version; the texts stay in `texts.WHATS_NEW` for the next one.

## Still manual (web UI, owner)

1. **App Privacy**: declare "Data Not Collected", see `APP_PRIVACY.md`.
2. **App Review contact**: first name, last name, phone and e-mail (App Review Information). Not set by the scripts on purpose.
3. **Copyright** line (version page), or set `ASC_COPYRIGHT` in `.env.local` and rerun `metadata.py`.
4. Check the price shows Free and availability covers the intended countries (set through the API; check once).
5. **Add for Review / Submit**: only the owner submits.

## Rerun

The scripts use only the Python standard library and `xcrun swift` for the token. They read `ASC_KEY_ID` and `ASC_ISSUER_ID` from the git-ignored `.env.local` at the repository root; the private key is read from `~/.appstoreconnect/private_keys/AuthKey_<key id>.p8` and never copied. None of them submits for review.

```
python3 scripts/asc/texts.py        # checks the lengths (subtitle 30, keywords 100, promo 170, description 4000)
python3 scripts/asc/metadata.py     # texts, URLs, categories, age rating, content rights, review notes, price, availability
python3 scripts/asc/screenshots.py  # replaces the APP_DESKTOP screenshots in the three languages
python3 scripts/asc/build.py [id]   # attaches the newest VALID 1.0.0 build (or the given build id)
python3 scripts/asc/status.py       # read-only: what is set and what still blocks submission
```

If `xcode-select` points to the Command Line Tools, prefix the commands with `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`. For a new version, change `VERSION_STRING` in `scripts/asc/ids.py`.

## Texts

### English (en-US)

- Name: MQTT Panel
- Subtitle: MQTT client, built-in broker
- Keywords: broker,iot,client,tls,sparkplug,charts,publish,subscribe,websocket,mcp,protobuf,topics,sensor
- Promotional text: See what is happening on your MQTT brokers: topics, values, history, charts, publishing, and a broker built in.
- What's New (from the second version): First release.
- Support URL and marketing URL: https://mqtt-panel.com
- Privacy policy URL: https://mqtt-panel.com/privacy

Description:

```text
See what is happening on your MQTT brokers: topics, values, history, charts, publishing, and a broker built in. Your AI assistant can drive it too: Claude and ChatGPT connect through MCP to set up brokers, certificates and charts for you.

EXPLORER
See every topic and value live, and what changed in each message. Browse thousands of topics as a tree or a table, search with + # or a regular expression, and read JSON, Sparkplug B, CBOR, MessagePack and Protobuf decoded.

SEVERAL BROKERS
Every open connection gets a tab with its live figures; the cards show which profiles are connected.

CHARTS
Chart any field of any payload as it arrives. Pick fields from a payload and plot them on a dashboard, with two axes when the units differ and one time range for every chart.

PUBLISH
Send test messages, repeat them, and see the replies. Templates with placeholders, QoS and retain, MQTT 5 request and response, and a repeat to simulate a sensor.

BUILT-IN BROKER
A broker built in, for tests on your desk or devices on your network. TCP, TLS and WebSocket listeners, users and client certificates on one screen. Every change applies at once.

SECURITY
Proper TLS, and nothing leaves your computer. Your own CA, client certificates and certificate pinning. Passwords and keys are stored encrypted. No telemetry, no account.

WORKS WITH YOUR AI ASSISTANT (BETA)
Ask in plain words. Watch it happen in the app. MQTT Panel is an MCP server: Claude and ChatGPT can set up connections and certificates, configure the built-in broker, build charts and extract data, all through the running app. Every change shows in the window and is listed in the Assistant log. You accept once that the assistant may change anything in MQTT Panel. Secrets stay in the app: no tool ever returns a password or a key.
Beta: the tools and their texts may still change; tell us what you need.

YOUR DATA STAYS YOURS
MQTT Panel collects nothing. It talks only to the brokers you configure.

LICENCE
Source available, free to use. MQTT Panel is licensed under the Apache License 2.0 with the Commons Clause. You may use it for free, at home or at work, including for your own commercial activities.

Available in English, French and German.
```

### French (fr-FR)

- Name: MQTT Panel
- Subtitle: Client MQTT et broker intégré
- Keywords: broker,iot,client,tls,sparkplug,courbes,publier,abonnement,websocket,mcp,protobuf,topics,capteur
- Promotional text: Voyez ce qui se passe sur vos brokers MQTT : topics, valeurs, historique, courbes, publication, et un broker intégré.
- What's New (from the second version): Première version.
- Support URL and marketing URL: https://mqtt-panel.com
- Privacy policy URL: https://mqtt-panel.com/fr/privacy

Description:

```text
Voyez ce qui se passe sur vos brokers MQTT : topics, valeurs, historique, courbes, publication, et un broker intégré. Votre assistant IA peut aussi piloter l’app : Claude et ChatGPT s’y connectent par MCP et préparent brokers, certificats et courbes à votre place.

EXPLORER
Tous les topics et leurs valeurs en direct, avec ce qui a changé à chaque message. Parcourez des milliers de topics en arbre ou en tableau, cherchez avec + # ou une expression régulière, et lisez JSON, Sparkplug B, CBOR, MessagePack et Protobuf décodés.

PLUSIEURS BROKERS
Chaque connexion ouverte a son onglet et ses chiffres en direct ; les cartes montrent les profils connectés.

COURBES
Tracez n’importe quel champ d’un message, au fil de son arrivée. Choisissez des champs d’un message et tracez-les sur un tableau de bord, avec deux axes quand les unités diffèrent et une même plage de temps pour toutes les courbes.

PUBLIER
Envoyez des messages de test, répétez-les et voyez les réponses. Modèles avec variables, QoS et retain, requête et réponse MQTT 5, et répétition pour simuler un capteur.

BROKER INTÉGRÉ
Un broker intégré, pour tester sur votre poste ou servir les appareils du réseau. Points d’écoute TCP, TLS et WebSocket, utilisateurs et certificats clients sur un seul écran. Chaque changement s’applique aussitôt.

SÉCURITÉ
Un TLS soigné, et rien ne quitte votre ordinateur. Votre propre AC, des certificats clients et l’épinglage de certificat. Mots de passe et clés sont stockés chiffrés. Ni télémétrie, ni compte.

FONCTIONNE AVEC VOTRE ASSISTANT IA (BÊTA)
Demandez avec vos mots, et voyez le résultat dans l’app. MQTT Panel est un serveur MCP : Claude et ChatGPT peuvent créer les connexions et les certificats, configurer le broker intégré, construire des courbes et extraire des données, toujours par l’app ouverte. Chaque changement apparaît dans la fenêtre et s’inscrit dans le journal de l’assistant. Vous acceptez une fois que l’assistant puisse tout modifier dans MQTT Panel. Les secrets restent dans l’app : aucun outil ne renvoie de mot de passe ni de clé.
Bêta : les outils et leurs textes peuvent encore changer ; dites-nous ce dont vous avez besoin.

VOS DONNÉES RESTENT LES VÔTRES
MQTT Panel ne collecte rien. L’app ne parle qu’aux brokers que vous configurez.

LICENCE
Code source disponible, utilisation gratuite. MQTT Panel est sous licence Apache 2.0 assortie de la Commons Clause. Vous pouvez l’utiliser gratuitement, chez vous ou au travail, y compris pour vos propres activités commerciales.

Disponible en français, en anglais et en allemand.
```

### German (de-DE)

- Name: MQTT Panel
- Subtitle: MQTT-Client mit eigenem Broker
- Keywords: broker,iot,client,tls,sparkplug,kurven,senden,abonnieren,websocket,mcp,protobuf,topics,sensor
- Promotional text: Sieh, was auf deinen MQTT-Brokern passiert: Topics, Werte, Verlauf, Kurven, Senden und ein eingebauter Broker.
- What's New (from the second version): Erste Version.
- Support URL and marketing URL: https://mqtt-panel.com
- Privacy policy URL: https://mqtt-panel.com/de/privacy

Description:

```text
Sieh, was auf deinen MQTT-Brokern passiert: Topics, Werte, Verlauf, Kurven, Senden und ein eingebauter Broker. Auch dein KI-Assistent kann die App steuern: Claude und ChatGPT verbinden sich über MCP und richten Broker, Zertifikate und Kurven für dich ein.

ERKUNDEN
Alle Topics und Werte live, und was sich mit jeder Nachricht geändert hat. Durchsuche Tausende Topics als Baum oder Tabelle, filtere mit + # oder einem regulären Ausdruck und lies JSON, Sparkplug B, CBOR, MessagePack und Protobuf dekodiert.

MEHRERE BROKER
Jede offene Verbindung hat einen Tab mit ihren Live-Werten; die Karten zeigen, welche Profile verbunden sind.

KURVEN
Zeichne jedes Feld jeder Nachricht, während sie ankommt. Wähl Felder aus einer Nachricht und zeichne sie in ein Dashboard, mit zwei Achsen bei unterschiedlichen Einheiten und einem Zeitbereich für alle Kurven.

SENDEN
Schick Testnachrichten, wiederhole sie und sieh die Antworten. Vorlagen mit Platzhaltern, QoS und Retain, Anfrage und Antwort mit MQTT 5 und Wiederholen, um einen Sensor zu simulieren.

EINGEBAUTER BROKER
Ein eingebauter Broker, für Tests am Schreibtisch oder Geräte im Netzwerk. TCP-, TLS- und WebSocket-Listener, Benutzer und Client-Zertifikate auf einem Bildschirm. Jede Änderung gilt sofort.

SICHERHEIT
Sauberes TLS, und nichts verlässt deinen Computer. Eigene CA, Client-Zertifikate und Zertifikat-Pinning. Passwörter und Schlüssel werden verschlüsselt gespeichert. Keine Telemetrie, kein Konto.

FUNKTIONIERT MIT DEINEM KI-ASSISTENTEN (BETA)
Sag in eigenen Worten, was du brauchst, und sieh zu, wie es in der App passiert. MQTT Panel ist ein MCP-Server: Claude und ChatGPT können Verbindungen und Zertifikate einrichten, den eingebauten Broker konfigurieren, Kurven anlegen und Daten exportieren, immer über die laufende App. Jede Änderung siehst du im Fenster, und sie steht im Assistenten-Protokoll. Du bestätigst einmal, dass der Assistent in MQTT Panel alles ändern darf. Geheimnisse bleiben in der App: Kein Werkzeug gibt je ein Passwort oder einen Schlüssel zurück.
Beta: Die Werkzeuge und ihre Texte können sich noch ändern. Sag uns, was du brauchst.

DEINE DATEN BLEIBEN DEINE
MQTT Panel sammelt nichts. Die App spricht nur mit den Brokern, die du einrichtest.

LIZENZ
Quellcode offen einsehbar, kostenlos nutzbar. MQTT Panel steht unter der Apache License 2.0 mit der Commons Clause. Du darfst es kostenlos nutzen, zu Hause oder bei der Arbeit, auch für deine eigenen geschäftlichen Zwecke.

Verfügbar auf Deutsch, Englisch und Französisch.
```

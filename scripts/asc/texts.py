"""Mac App Store listing texts for MQTT Panel (EN, FR, DE). Wording comes from website/build.py.
French uses non-breaking spaces before : ; ? and inside « », German uses "du"."""

SITE = "https://mqtt-panel.com"
N = " "  # non-breaking space (French typography)

NAME = "MQTT Panel"

SUBTITLE = {
    "en-US": "MQTT client, built-in broker",
    "fr-FR": "Client MQTT et broker intégré",
    "de-DE": "MQTT-Client mit eigenem Broker",
}

PRIVACY_URL = {"en-US": f"{SITE}/privacy", "fr-FR": f"{SITE}/fr/privacy", "de-DE": f"{SITE}/de/privacy"}

KEYWORDS = {
    "en-US": "broker,iot,client,tls,sparkplug,charts,publish,subscribe,websocket,mcp,protobuf,topics,sensor",
    "fr-FR": "broker,iot,client,tls,sparkplug,courbes,publier,abonnement,websocket,mcp,protobuf,topics,capteur",
    "de-DE": "broker,iot,client,tls,sparkplug,kurven,senden,abonnieren,websocket,mcp,protobuf,topics,sensor",
}

PROMO = {
    "en-US": "See what is happening on your MQTT brokers: topics, values, history, charts, publishing, and a broker built in.",
    "fr-FR": f"Voyez ce qui se passe sur vos brokers MQTT{N}: topics, valeurs, historique, courbes, publication, et un broker intégré.",
    "de-DE": "Sieh, was auf deinen MQTT-Brokern passiert: Topics, Werte, Verlauf, Kurven, Senden und ein eingebauter Broker.",
}

WHATS_NEW = {"en-US": "First release.", "fr-FR": "Première version.", "de-DE": "Erste Version."}

DESCRIPTION = {
"en-US": """See what is happening on your MQTT brokers: topics, values, history, charts, publishing, and a broker built in. Your AI assistant can drive it too: Claude and ChatGPT connect through MCP to set up brokers, certificates and charts for you.

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

Available in English, French and German.""",

"fr-FR": f"""Voyez ce qui se passe sur vos brokers MQTT{N}: topics, valeurs, historique, courbes, publication, et un broker intégré. Votre assistant IA peut aussi piloter l’app{N}: Claude et ChatGPT s’y connectent par MCP et préparent brokers, certificats et courbes à votre place.

EXPLORER
Tous les topics et leurs valeurs en direct, avec ce qui a changé à chaque message. Parcourez des milliers de topics en arbre ou en tableau, cherchez avec + # ou une expression régulière, et lisez JSON, Sparkplug B, CBOR, MessagePack et Protobuf décodés.

PLUSIEURS BROKERS
Chaque connexion ouverte a son onglet et ses chiffres en direct{N}; les cartes montrent les profils connectés.

COURBES
Tracez n’importe quel champ d’un message, au fil de son arrivée. Choisissez des champs d’un message et tracez-les sur un tableau de bord, avec deux axes quand les unités diffèrent et une même plage de temps pour toutes les courbes.

PUBLIER
Envoyez des messages de test, répétez-les et voyez les réponses. Modèles avec variables, QoS et retain, requête et réponse MQTT 5, et répétition pour simuler un capteur.

BROKER INTÉGRÉ
Un broker intégré, pour tester sur votre poste ou servir les appareils du réseau. Points d’écoute TCP, TLS et WebSocket, utilisateurs et certificats clients sur un seul écran. Chaque changement s’applique aussitôt.

SÉCURITÉ
Un TLS soigné, et rien ne quitte votre ordinateur. Votre propre AC, des certificats clients et l’épinglage de certificat. Mots de passe et clés sont stockés chiffrés. Ni télémétrie, ni compte.

FONCTIONNE AVEC VOTRE ASSISTANT IA (BÊTA)
Demandez avec vos mots, et voyez le résultat dans l’app. MQTT Panel est un serveur MCP{N}: Claude et ChatGPT peuvent créer les connexions et les certificats, configurer le broker intégré, construire des courbes et extraire des données, toujours par l’app ouverte. Chaque changement apparaît dans la fenêtre et s’inscrit dans le journal de l’assistant. Vous acceptez une fois que l’assistant puisse tout modifier dans MQTT Panel. Les secrets restent dans l’app{N}: aucun outil ne renvoie de mot de passe ni de clé.
Bêta{N}: les outils et leurs textes peuvent encore changer{N}; dites-nous ce dont vous avez besoin.

VOS DONNÉES RESTENT LES VÔTRES
MQTT Panel ne collecte rien. L’app ne parle qu’aux brokers que vous configurez.

LICENCE
Code source disponible, utilisation gratuite. MQTT Panel est sous licence Apache 2.0 assortie de la Commons Clause. Vous pouvez l’utiliser gratuitement, chez vous ou au travail, y compris pour vos propres activités commerciales.

Disponible en français, en anglais et en allemand.""",

"de-DE": """Sieh, was auf deinen MQTT-Brokern passiert: Topics, Werte, Verlauf, Kurven, Senden und ein eingebauter Broker. Auch dein KI-Assistent kann die App steuern: Claude und ChatGPT verbinden sich über MCP und richten Broker, Zertifikate und Kurven für dich ein.

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

Verfügbar auf Deutsch, Englisch und Französisch.""",
}

# Screenshots per locale, in store order (light first, dark at the end). Files: website/assets/media/<base>.<lang>.<theme>.jpg
SCREENSHOTS = [
    ("look-connections", "light"), ("look-tree", "light"), ("look-history", "light"), ("shot-charts", "light"),
    ("shot-publish", "light"), ("shot-broker", "light"), ("shot-security", "light"), ("look-tree", "dark"),
]

def check():
    for loc in DESCRIPTION:
        assert len(SUBTITLE[loc]) <= 30, (loc, len(SUBTITLE[loc]))
        assert len(KEYWORDS[loc]) <= 100, (loc, len(KEYWORDS[loc]))
        assert len(PROMO[loc]) <= 170, (loc, len(PROMO[loc]))
        assert len(DESCRIPTION[loc]) <= 4000, (loc, len(DESCRIPTION[loc]))

if __name__ == "__main__":
    check()
    for loc in DESCRIPTION:
        print(loc, len(SUBTITLE[loc]), len(KEYWORDS[loc]), len(PROMO[loc]), len(DESCRIPTION[loc]))

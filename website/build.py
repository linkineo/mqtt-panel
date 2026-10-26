"""Builds the MQTT Panel website (EN at /, FR at /fr/, DE at /de/) into website/public/.

Run:  python3 website/build.py              build the site
      python3 website/build.py --licences   refresh third-party.json from Cargo and npm metadata, then build
Deploy: see website/README.md. Static files only: no cookies, no trackers, nothing loaded from third parties.
"""
import html
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys

# ---- The owner fills the store links when the listings exist (the GitHub repository is public). Empty: the link goes nowhere (href="#", aria-disabled) and
# shows "coming soon". They appear in the Download section at the bottom of the home page. The app is distributed
# through the two stores and, for macOS, as a signed and notarized DMG attached to the GitHub release (owner,
# 26 Oct 2026); the asset name never carries the version so the link below always points at the latest release.
MAC_APP_STORE_URL = ""
MICROSOFT_STORE_URL = ""
GITHUB_URL = "https://github.com/linkineo/mqtt-panel"
MAC_DMG_URL = GITHUB_URL + "/releases/latest/download/MQTT-Panel-macOS.dmg"

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parent
OUT = ROOT / "public"
SITE = "https://mqtt-panel.com"
LANGS = ["en", "fr", "de"]
PREFIX = {"en": "", "fr": "/fr", "de": "/de"}
ICONS = REPO / "apps" / "desktop" / "src-tauri" / "icons"
YEAR = 2026

# Words that must never appear on the site: the private pattern list of scripts/check-forbidden-words.sh
# (.forbidden-words.local at the repository root, git-ignored; the prefixes name:/brand: are dropped here).
# The site is only built and deployed by the maintainer, so a missing list fails the build.
FORBIDDEN_LIST = pathlib.Path(os.environ.get("MQTTPANEL_FORBIDDEN_WORDS", REPO / ".forbidden-words.local"))


def forbidden_patterns():
    if not FORBIDDEN_LIST.is_file():
        sys.exit(f"forbidden-words list not found: {FORBIDDEN_LIST} (private to the maintainer)")
    pats = []
    for line in FORBIDDEN_LIST.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        pats.append(line.split(":", 1)[1] if line.startswith(("name:", "brand:")) else line)
    return pats


def e(s):
    return html.escape(s, quote=True)


def fr_typo(s):
    """French typography: no-break space before the colon, narrow no-break space before ; ! ? and inside « »."""
    s = re.sub(r" ([:])", " \\1", s)
    s = re.sub(r" ([;!?])", " \\1", s)
    s = s.replace("« ", "« ").replace(" »", " »")
    return s


# ------------------------------------------------------------------------------------------------ texts

T = {
    "en": dict(
        title="MQTT Panel: MQTT client and built-in broker for Windows and macOS",
        meta="See what is happening on your MQTT brokers: topics, values, history, charts, publishing, and a broker built in.",
        nav_features="Features", nav_privacy="Privacy", lang_label="Language",
        lead="See what is happening on your MQTT brokers: topics, values, history, charts, publishing, and a broker built in. Your AI assistant can drive it too: Claude and ChatGPT connect through MCP to set up brokers, certificates and charts for you.",
        dl=dict(button="Download", h="Download", soon="coming soon", mas="Download on the Mac App Store",
                ms="Get it from Microsoft", stores="Installed and updated by the stores.",
                dmg="Download for macOS", dmg_note="Universal, macOS 11 or later, notarized. Updates itself.",
                gh="Build it yourself", gh_note="Source code and release notes."),
        demo_h="A closer look",
        demo_p="Screens from the app, with demo data.",
        looks=[
            ("look-connections", "Several brokers", "Every open connection gets a tab with its live figures; the cards show which profiles are connected.", "The Connect screen with three connected profiles and their tabs."),
            ("look-table", "Table view", "The same topics as a flat table, sortable by any column.", "The Explorer as a table of topics."),
        ],
        hero=dict(alt='MQTT Panel live with demo data: the topic tree, a JSON payload shown as a tree, and its chart.', hint='Live demo with sample data. Use the menu on the left to switch screens.'),
        live=dict(pause='Pause the live view', play='Play the live view'),
        theme=dict(label='Appearance', light='Light', dark='Dark'),
        features_h="What it does",
        features=[
            ('explorer', 'look-history', 'Explorer', 'A temperature topic with its history list and the change from the previous message.', 'See every topic and value live, and what changed in each message.', 'Browse thousands of topics as a tree or a table, search with + # or a regular expression, and read JSON, Sparkplug B, CBOR, MessagePack and Protobuf decoded.'),
            ('charts', 'shot-charts', 'Charts', 'A dashboard chart with three series from one topic.', 'Chart any field of any payload as it arrives.', 'Pick fields from a payload and plot them on a dashboard, with two axes when the units differ and one time range for every chart.'),
            ('publish', 'shot-publish', 'Publish', 'The Publish screen with templates, the composer and the list of sent messages with a reply.', 'Send test messages, repeat them, and see the replies.', 'Templates with placeholders, QoS and retain, MQTT 5 request and response, and a repeat to simulate a sensor.'),
            ('broker', 'shot-broker', 'Built-in broker', 'The built-in broker with its listeners, users, client certificates and connected clients.', 'A broker built in, for tests on your desk or devices on your network.', 'TCP, TLS and WebSocket listeners, users and client certificates on one screen. Every change applies at once.'),
            ('security', 'shot-security', 'Security', 'A connection profile with its security settings: CA, client certificate and key.', 'Proper TLS, and nothing leaves your computer.', 'Your own CA, client certificates and certificate pinning. Passwords and keys are stored encrypted. No telemetry, no account.'),
            ('cli', '', 'Command line', '', 'The same profiles and secrets, from the terminal.', 'mqttpanel sub, pub and broker use the same profiles as the app, with text, JSON or NDJSON output.'),
        ],
        ai={'title': 'Works with your AI assistant (beta)', 'beta': 'Beta: the tools and their texts may still change; tell us what you need.', 'pitch': 'Ask in plain words. Watch it happen in the app.', 'body': 'MQTT Panel is an MCP server: Claude and ChatGPT can set up connections and certificates, configure the built-in broker, build charts and extract data, all through the running app. Every change shows in the window and is listed in the Assistant log. You accept once that the assistant may change anything in MQTT Panel. Secrets stay in the app: no tool ever returns a password or a key.', 'bullets': ['Connect to a broker: profile, TLS, CA and client certificate, test, connect, and plain answers when it fails.', 'Set up the built-in broker: listeners, users and client certificates for your devices.', 'Find topics, read values and history, add charts and export data to a file you pick.', 'Passwords and keys can be typed in MQTT Panel itself, so they never pass through the chat.', 'One button in Settings, Assistant adds MQTT Panel to Claude or to the ChatGPT desktop app.'], 'works': 'Works with', 'chat_label': 'A chat with an AI assistant that sets up MQTT Panel', 'chat_head': 'Your AI assistant', 'you': 'You', 'bot': 'Assistant', 'prompt': 'Connect MQTT Panel to broker.plant.example over TLS with my client certificate, then chart the temperature of room 204.', 'reply': ['Created the connection, imported the certificate, connected.', 'Topic chart added for building/f2/r204/temperature.', 'It is open in MQTT Panel.']},
        cli_comment=["# watch a building, with a saved profile", "# send one message", "# run the broker on this computer"],
        privacy_h="Your data stays yours",
        privacy_p="MQTT Panel collects nothing. It talks only to the brokers you configure.",
        privacy_link="Read the privacy statement",
        footer_privacy="Privacy", footer_legal="Legal notice", footer_licences="Licences",
        back="Back to the home page",
        privacy_title="Privacy",
        privacy_lead="The short version: MQTT Panel collects nothing, and this website sets no cookies.",
        privacy_sections=[
            ("The app", [
                "MQTT Panel has no telemetry, no crash reporting, no analytics and no account.",
                "The app makes no network connections except to the brokers you configure. Copies from the Microsoft Store or the Mac App Store are updated by the store.",
                "The macOS download from this website also checks for updates: once a day at most, plus when you choose Check now, it asks mqtt-panel.com whether a newer version exists. The request carries only the app version, the system and the processor type: no identifier, no cookie. The website keeps nothing and sends the app on to the release file on GitHub, which sees your IP address as for any download. You can switch the check off in Settings, Updates; nothing is installed until you ask.",
                "Profiles, templates, views and the message history stay on your computer.",
            ]),
            ("Secrets", [
                "Passwords, private keys and PKCS#12 passphrases are stored encrypted on your computer. The key that unlocks them is kept in the system's secret store (the keychain on macOS). They are never written to profile files and never leave the device.",
            ]),
            ("This website", [
                "This website sets no cookies, runs no analytics and loads nothing from third parties: no fonts, no scripts, no images from other sites.",
                "If you pick light or dark with the switch at the top, your browser keeps that choice on your device (local storage). It is never sent anywhere.",
                "It is served by Cloudflare, which processes the technical data any web server needs to deliver a page, such as your IP address, for that purpose.",
            ]),
            ("Questions", [
                "If something here is unclear, see the legal notice for how to reach the operator.",
            ]),
        ],
        legal_title="Legal notice and licences",
        legal_lead="Who runs this website, and the licences of MQTT Panel and the software it includes.",
        legal_notice_h="Legal notice",
        operator="Operator details: to be completed by the owner.",
        licence_h="MQTT Panel licence",
        licence_p=["MQTT Panel is source-available software, licensed under the Apache License 2.0 with the Commons Clause.",
                   "You may use it for free, at home or at work, including for your own commercial activities, change it and build it yourself. You may not sell MQTT Panel, nor sell a product or service whose value comes substantially from it.",
                   "This is a summary: the licence text below is the authoritative one."],
        licence_cc="Licence condition (Commons Clause)", licence_apache="Apache License, Version 2.0",
        licence_link="Licence text as a plain file",
        third_h="Third-party software",
        third_p="MQTT Panel includes the following open-source components, each under its own licence:",
        third_rust="Engine and command line (Rust crates)",
        third_npm="Interface (npm packages)",
        not_found="Page not found",
    ),
    "fr": dict(
        title="MQTT Panel : client MQTT et broker intégré pour Windows et macOS",
        meta="Voyez ce qui se passe sur vos brokers MQTT : topics, valeurs, historique, courbes, publication, et un broker intégré.",
        nav_features="Fonctions", nav_privacy="Confidentialité", lang_label="Langue",
        lead="Voyez ce qui se passe sur vos brokers MQTT : topics, valeurs, historique, courbes, publication, et un broker intégré. Votre assistant IA peut aussi piloter l’app : Claude et ChatGPT s’y connectent par MCP et préparent brokers, certificats et courbes à votre place.",
        dl=dict(button="Télécharger", h="Télécharger", soon="bientôt disponible", mas="Télécharger dans le Mac App Store",
                ms="Obtenir auprès de Microsoft", stores="Installé et mis à jour par les boutiques.",
                dmg="Télécharger pour macOS", dmg_note="Universel, macOS 11 ou plus récent, notarisé. Se met à jour tout seul.",
                gh="Compilez-le vous-même", gh_note="Code source et notes de version."),
        demo_h="Vu de plus près",
        demo_p="Des écrans de l’app, avec des données de démonstration.",
        looks=[
            ("look-connections", "Plusieurs brokers", "Chaque connexion ouverte a son onglet et ses chiffres en direct ; les cartes montrent les profils connectés.", "L’écran Connexion avec trois profils connectés et leurs onglets."),
            ("look-table", "Vue tableau", "Les mêmes topics dans un tableau, triable sur chaque colonne.", "L’écran Explorer en tableau de topics."),
        ],
        hero=dict(alt='MQTT Panel en direct avec des données de démonstration : l’arbre des topics, un message JSON en arbre et sa courbe.', hint='Démo en direct avec des données d’exemple. Changez d’écran avec le menu de gauche.'),
        live=dict(pause='Mettre la vue en direct en pause', play='Relancer la vue en direct'),
        theme=dict(label='Apparence', light='Clair', dark='Sombre'),
        features_h="Ce que fait l’app",
        features=[
            ('explorer', 'look-history', 'Explorer', 'Un topic de température avec son historique et le changement par rapport au message précédent.', 'Tous les topics et leurs valeurs en direct, avec ce qui a changé à chaque message.', 'Parcourez des milliers de topics en arbre ou en tableau, cherchez avec + # ou une expression régulière, et lisez JSON, Sparkplug B, CBOR, MessagePack et Protobuf décodés.'),
            ('charts', 'shot-charts', 'Courbes', 'Une courbe du tableau de bord à trois séries issues d’un même topic.', 'Tracez n’importe quel champ d’un message, au fil de son arrivée.', 'Choisissez des champs d’un message et tracez-les sur un tableau de bord, avec deux axes quand les unités diffèrent et une même plage de temps pour toutes les courbes.'),
            ('publish', 'shot-publish', 'Publier', 'L’écran Publier avec les modèles, l’éditeur et la liste des messages envoyés avec une réponse.', 'Envoyez des messages de test, répétez-les et voyez les réponses.', 'Modèles avec variables, QoS et retain, requête et réponse MQTT 5, et répétition pour simuler un capteur.'),
            ('broker', 'shot-broker', 'Broker intégré', 'Le broker intégré avec ses points d’écoute, ses utilisateurs, ses certificats clients et les clients connectés.', 'Un broker intégré, pour tester sur votre poste ou servir les appareils du réseau.', 'Points d’écoute TCP, TLS et WebSocket, utilisateurs et certificats clients sur un seul écran. Chaque changement s’applique aussitôt.'),
            ('security', 'shot-security', 'Sécurité', 'Un profil de connexion avec ses réglages de sécurité : AC, certificat client et clé.', 'Un TLS soigné, et rien ne quitte votre ordinateur.', 'Votre propre AC, des certificats clients et l’épinglage de certificat. Mots de passe et clés sont stockés chiffrés. Ni télémétrie, ni compte.'),
            ('cli', '', 'Ligne de commande', '', 'Les mêmes profils et secrets, depuis le terminal.', 'mqttpanel sub, pub et broker utilisent les mêmes profils que l’app, avec une sortie texte, JSON ou NDJSON.'),
        ],
        ai={'title': 'Fonctionne avec votre assistant IA (bêta)', 'beta': 'Bêta : les outils et leurs textes peuvent encore changer ; dites-nous ce dont vous avez besoin.', 'pitch': 'Demandez avec vos mots, et voyez le résultat dans l’app.', 'body': 'MQTT Panel est un serveur MCP : Claude et ChatGPT peuvent créer les connexions et les certificats, configurer le broker intégré, construire des courbes et extraire des données, toujours par l’app ouverte. Chaque changement apparaît dans la fenêtre et s’inscrit dans le journal de l’assistant. Vous acceptez une fois que l’assistant puisse tout modifier dans MQTT Panel. Les secrets restent dans l’app : aucun outil ne renvoie de mot de passe ni de clé.', 'bullets': ['Se connecter à un broker : profil, TLS, AC et certificat client, test, connexion, et une explication claire en cas d’échec.', 'Préparer le broker intégré : points d’écoute, utilisateurs et certificats clients pour vos appareils.', 'Trouver des topics, lire valeurs et historique, ajouter des courbes et exporter les données dans le fichier de votre choix.', 'Mots de passe et clés peuvent se saisir dans MQTT Panel même : ils ne passent jamais par la conversation.', 'Un bouton dans Réglages, Assistant ajoute MQTT Panel à Claude ou à l’app de bureau ChatGPT.'], 'works': 'Fonctionne avec', 'chat_label': 'Une conversation avec un assistant IA qui configure MQTT Panel', 'chat_head': 'Votre assistant IA', 'you': 'Vous', 'bot': 'Assistant', 'prompt': 'Connectez MQTT Panel à broker.plant.example en TLS avec mon certificat client, puis tracez la température de la salle 204.', 'reply': ['Connexion créée, certificat importé, MQTT Panel est connecté.', 'Courbe ajoutée pour le topic building/f2/r204/temperature.', 'Tout est ouvert dans MQTT Panel.']},
        cli_comment=["# suivre un bâtiment avec un profil enregistré", "# envoyer un message", "# lancer le broker sur cet ordinateur"],
        privacy_h="Vos données restent les vôtres",
        privacy_p="MQTT Panel ne collecte rien. L’app ne parle qu’aux brokers que vous configurez.",
        privacy_link="Lire la déclaration de confidentialité",
        footer_privacy="Confidentialité", footer_legal="Mentions légales", footer_licences="Licences",
        back="Retour à l’accueil",
        privacy_title="Confidentialité",
        privacy_lead="En bref : MQTT Panel ne collecte rien, et ce site ne dépose aucun cookie.",
        privacy_sections=[
            ("L’app", [
                "MQTT Panel n’a ni télémétrie, ni rapport de plantage, ni mesure d’audience, ni compte.",
                "L’app ne se connecte qu’aux brokers que vous configurez. Les copies du Microsoft Store ou du Mac App Store sont mises à jour par la boutique.",
                "La version macOS téléchargée sur ce site vérifie aussi les mises à jour : au plus une fois par jour, et quand vous choisissez Vérifier maintenant, elle demande à mqtt-panel.com si une version plus récente existe. La requête ne contient que la version de l’app, le système et le type de processeur : aucun identifiant, aucun cookie. Le site ne conserve rien et renvoie l’app vers le fichier de la version sur GitHub, qui voit votre adresse IP comme pour tout téléchargement. Vous pouvez désactiver la vérification dans Réglages, Mises à jour ; rien n’est installé sans votre accord.",
                "Profils, modèles, vues et historique des messages restent sur votre ordinateur.",
            ]),
            ("Secrets", [
                "Mots de passe, clés privées et phrases secrètes PKCS#12 sont stockés chiffrés sur votre ordinateur. La clé qui les déverrouille est conservée dans le coffre de secrets du système (le trousseau sur macOS). Ils ne sont jamais écrits dans les fichiers de profil et ne quittent jamais l’appareil.",
            ]),
            ("Ce site", [
                "Ce site ne dépose aucun cookie, ne mesure pas l’audience et ne charge rien depuis des tiers : ni polices, ni scripts, ni images venant d’autres sites.",
                "Si vous choisissez le mode clair ou sombre avec le sélecteur en haut, votre navigateur garde ce choix sur votre appareil (stockage local). Il n’est envoyé nulle part.",
                "Il est servi par Cloudflare, qui traite les données techniques nécessaires à tout serveur web pour livrer une page, comme votre adresse IP, dans ce seul but.",
            ]),
            ("Questions", [
                "Si un point n’est pas clair, les mentions légales indiquent comment joindre l’exploitant.",
            ]),
        ],
        legal_title="Mentions légales et licences",
        legal_lead="Qui exploite ce site, et les licences de MQTT Panel et des logiciels qu’il inclut.",
        legal_notice_h="Mentions légales",
        operator="Coordonnées de l’exploitant : à compléter par le propriétaire.",
        licence_h="Licence de MQTT Panel",
        licence_p=["MQTT Panel est un logiciel à code source disponible (« source available »), sous licence Apache 2.0 assortie de la Commons Clause.",
                   "Vous pouvez l’utiliser gratuitement, chez vous ou au travail, y compris pour vos propres activités commerciales, le modifier et le compiler vous-même. Vous ne pouvez pas vendre MQTT Panel, ni vendre un produit ou un service dont la valeur repose pour l’essentiel sur lui.",
                   "Ceci n’est qu’un résumé : seul le texte de la licence ci-dessous, en anglais, fait foi."],
        licence_cc="Condition de licence (Commons Clause, en anglais)", licence_apache="Apache License, version 2.0 (en anglais)",
        licence_link="Texte de la licence en fichier texte",
        third_h="Logiciels tiers",
        third_p="MQTT Panel inclut les composants open source suivants, chacun sous sa propre licence :",
        third_rust="Moteur et ligne de commande (crates Rust)",
        third_npm="Interface (paquets npm)",
        not_found="Page introuvable",
    ),
    "de": dict(
        title="MQTT Panel: MQTT-Client und eingebauter Broker für Windows und macOS",
        meta="Sieh, was auf deinen MQTT-Brokern passiert: Topics, Werte, Verlauf, Kurven, Senden und ein eingebauter Broker.",
        nav_features="Funktionen", nav_privacy="Datenschutz", lang_label="Sprache",
        lead="Sieh, was auf deinen MQTT-Brokern passiert: Topics, Werte, Verlauf, Kurven, Senden und ein eingebauter Broker. Auch dein KI-Assistent kann die App steuern: Claude und ChatGPT verbinden sich über MCP und richten Broker, Zertifikate und Kurven für dich ein.",
        dl=dict(button="Herunterladen", h="Herunterladen", soon="bald verfügbar", mas="Laden im Mac App Store",
                ms="Bei Microsoft herunterladen", stores="Installiert und aktualisiert über die Stores.",
                dmg="Für macOS herunterladen", dmg_note="Universal, macOS 11 oder neuer, notarisiert. Aktualisiert sich selbst.",
                gh="Selbst bauen", gh_note="Quellcode und Versionshinweise."),
        demo_h="Genauer hingeschaut",
        demo_p="Bildschirme aus der App, mit Demodaten.",
        looks=[
            ("look-connections", "Mehrere Broker", "Jede offene Verbindung hat einen Tab mit ihren Live-Werten; die Karten zeigen, welche Profile verbunden sind.", "Der Bereich Verbinden mit drei verbundenen Profilen und ihren Tabs."),
            ("look-table", "Tabellenansicht", "Dieselben Topics als Tabelle, nach jeder Spalte sortierbar.", "Erkunden als Tabelle der Topics."),
        ],
        hero=dict(alt='MQTT Panel live mit Demodaten: der Topic-Baum, eine JSON-Nachricht als Baum und ihre Kurve.', hint='Live-Demo mit Beispieldaten. Wechsle den Bildschirm über das Menü links.'),
        live=dict(pause='Live-Ansicht anhalten', play='Live-Ansicht fortsetzen'),
        theme=dict(label='Darstellung', light='Hell', dark='Dunkel'),
        features_h="Was die App kann",
        features=[
            ('explorer', 'look-history', 'Erkunden', 'Ein Temperatur-Topic mit Verlauf und der Änderung zur vorherigen Nachricht.', 'Alle Topics und Werte live, und was sich mit jeder Nachricht geändert hat.', 'Durchsuche Tausende Topics als Baum oder Tabelle, filtere mit + # oder einem regulären Ausdruck und lies JSON, Sparkplug B, CBOR, MessagePack und Protobuf dekodiert.'),
            ('charts', 'shot-charts', 'Kurven', 'Eine Dashboard-Kurve mit drei Reihen aus einem Topic.', 'Zeichne jedes Feld jeder Nachricht, während sie ankommt.', 'Wähl Felder aus einer Nachricht und zeichne sie in ein Dashboard, mit zwei Achsen bei unterschiedlichen Einheiten und einem Zeitbereich für alle Kurven.'),
            ('publish', 'shot-publish', 'Senden', 'Der Bereich Senden mit Vorlagen, Editor und der Liste gesendeter Nachrichten mit einer Antwort.', 'Schick Testnachrichten, wiederhole sie und sieh die Antworten.', 'Vorlagen mit Platzhaltern, QoS und Retain, Anfrage und Antwort mit MQTT 5 und Wiederholen, um einen Sensor zu simulieren.'),
            ('broker', 'shot-broker', 'Eingebauter Broker', 'Der eingebaute Broker mit Listenern, Benutzern, Client-Zertifikaten und verbundenen Clients.', 'Ein eingebauter Broker, für Tests am Schreibtisch oder Geräte im Netzwerk.', 'TCP-, TLS- und WebSocket-Listener, Benutzer und Client-Zertifikate auf einem Bildschirm. Jede Änderung gilt sofort.'),
            ('security', 'shot-security', 'Sicherheit', 'Ein Verbindungsprofil mit seinen Sicherheitseinstellungen: CA, Client-Zertifikat und Schlüssel.', 'Sauberes TLS, und nichts verlässt deinen Computer.', 'Eigene CA, Client-Zertifikate und Zertifikat-Pinning. Passwörter und Schlüssel werden verschlüsselt gespeichert. Keine Telemetrie, kein Konto.'),
            ('cli', '', 'Kommandozeile', '', 'Dieselben Profile und Geheimnisse im Terminal.', 'mqttpanel sub, pub und broker nutzen dieselben Profile wie die App, mit Ausgabe als Text, JSON oder NDJSON.'),
        ],
        ai={'title': 'Funktioniert mit deinem KI-Assistenten (Beta)', 'beta': 'Beta: Die Werkzeuge und ihre Texte können sich noch ändern. Sag uns, was du brauchst.', 'pitch': 'Sag in eigenen Worten, was du brauchst, und sieh zu, wie es in der App passiert.', 'body': 'MQTT Panel ist ein MCP-Server: Claude und ChatGPT können Verbindungen und Zertifikate einrichten, den eingebauten Broker konfigurieren, Kurven anlegen und Daten exportieren, immer über die laufende App. Jede Änderung siehst du im Fenster, und sie steht im Assistenten-Protokoll. Du bestätigst einmal, dass der Assistent in MQTT Panel alles ändern darf. Geheimnisse bleiben in der App: Kein Werkzeug gibt je ein Passwort oder einen Schlüssel zurück.', 'bullets': ['Mit einem Broker verbinden: Profil, TLS, CA und Client-Zertifikat, testen, verbinden und klare Antworten, wenn etwas nicht klappt.', 'Den eingebauten Broker einrichten: Listener, Benutzer und Client-Zertifikate für deine Geräte.', 'Topics finden, Werte und Verlauf lesen, Kurven anlegen und Daten in eine Datei deiner Wahl exportieren.', 'Passwörter und Schlüssel kannst du direkt in MQTT Panel eingeben, sie gehen dann nie durch den Chat.', 'Ein Klick in Einstellungen, Assistent fügt MQTT Panel zu Claude oder zur ChatGPT-Desktop-App hinzu.'], 'works': 'Funktioniert mit', 'chat_label': 'Ein Chat mit einem KI-Assistenten, der MQTT Panel einrichtet', 'chat_head': 'Dein KI-Assistent', 'you': 'Du', 'bot': 'Assistent', 'prompt': 'Verbinde MQTT Panel per TLS und mit meinem Client-Zertifikat mit broker.plant.example und zeig mir dann die Temperatur von Raum 204 als Kurve.', 'reply': ['Verbindung angelegt, Zertifikat importiert, verbunden.', 'Topic-Kurve für building/f2/r204/temperature hinzugefügt.', 'Sie ist in MQTT Panel geöffnet.']},
        cli_comment=["# ein Gebäude beobachten, mit gespeichertem Profil", "# eine Nachricht senden", "# den Broker auf diesem Computer starten"],
        privacy_h="Deine Daten bleiben deine",
        privacy_p="MQTT Panel sammelt nichts. Die App spricht nur mit den Brokern, die du einrichtest.",
        privacy_link="Datenschutzerklärung lesen",
        footer_privacy="Datenschutz", footer_legal="Impressum", footer_licences="Lizenzen",
        back="Zurück zur Startseite",
        privacy_title="Datenschutz",
        privacy_lead="Kurz gesagt: MQTT Panel sammelt nichts, und diese Website setzt keine Cookies.",
        privacy_sections=[
            ("Die App", [
                "MQTT Panel hat keine Telemetrie, keine Absturzberichte, keine Nutzungsstatistik und kein Konto.",
                "Die App verbindet sich nur mit den Brokern, die du einrichtest. Kopien aus dem Microsoft Store oder dem Mac App Store aktualisiert der Store.",
                "Der macOS-Download von dieser Website sucht außerdem nach Updates: höchstens einmal am Tag und wenn du Jetzt prüfen wählst, fragt er bei mqtt-panel.com nach, ob es eine neuere Version gibt. Die Anfrage enthält nur die App-Version, das System und den Prozessortyp: keine Kennung, kein Cookie. Die Website speichert nichts und leitet die App zur Release-Datei auf GitHub weiter, das deine IP-Adresse wie bei jedem Download sieht. Du kannst die Prüfung unter Einstellungen, Updates abschalten; installiert wird nur, wenn du es möchtest.",
                "Profile, Vorlagen, Ansichten und der Nachrichtenverlauf bleiben auf deinem Computer.",
            ]),
            ("Geheimnisse", [
                "Passwörter, private Schlüssel und PKCS#12-Passphrasen liegen verschlüsselt auf deinem Computer. Der Schlüssel dazu liegt im Geheimnisspeicher des Systems (unter macOS im Schlüsselbund). Sie landen nie in Profildateien und verlassen nie das Gerät.",
            ]),
            ("Diese Website", [
                "Diese Website setzt keine Cookies, misst keine Besuche und lädt nichts von Dritten: keine Schriften, keine Skripte, keine Bilder von anderen Websites.",
                "Wenn du oben hell oder dunkel wählst, merkt sich dein Browser diese Wahl auf deinem Gerät (lokaler Speicher). Sie wird nirgendwohin gesendet.",
                "Sie wird von Cloudflare ausgeliefert. Cloudflare verarbeitet dabei die technischen Daten, die jeder Webserver zum Ausliefern einer Seite braucht, etwa deine IP-Adresse, und nur dafür.",
            ]),
            ("Fragen", [
                "Wenn etwas unklar ist: Im Impressum steht, wie du den Betreiber erreichst.",
            ]),
        ],
        legal_title="Impressum und Lizenzen",
        legal_lead="Wer diese Website betreibt, und die Lizenzen von MQTT Panel und der enthaltenen Software.",
        legal_notice_h="Impressum",
        operator="Angaben zum Betreiber: vom Inhaber zu ergänzen.",
        licence_h="Lizenz von MQTT Panel",
        licence_p=["MQTT Panel ist Software mit offengelegtem Quellcode (Source Available), lizenziert unter der Apache License 2.0 mit der Commons Clause.",
                   "Du darfst es kostenlos nutzen, zu Hause oder bei der Arbeit, auch für deine eigenen geschäftlichen Zwecke, es ändern und selbst bauen. Du darfst MQTT Panel nicht verkaufen und auch kein Produkt und keine Dienstleistung, deren Wert im Wesentlichen auf MQTT Panel beruht.",
                   "Dies ist nur eine Zusammenfassung: Maßgeblich ist der Lizenztext unten (auf Englisch)."],
        licence_cc="Lizenzbedingung (Commons Clause, Englisch)", licence_apache="Apache License, Version 2.0 (Englisch)",
        licence_link="Lizenztext als Textdatei",
        third_h="Software von Dritten",
        third_p="MQTT Panel enthält die folgenden Open-Source-Komponenten, jeweils unter ihrer eigenen Lizenz:",
        third_rust="Engine und Kommandozeile (Rust-Crates)",
        third_npm="Oberfläche (npm-Pakete)",
        not_found="Seite nicht gefunden",
    ),
}

CLI_LINES = [
    "mqttpanel sub --profile \"Plant broker\" -t 'building/f2/#'",
    "mqttpanel pub -h localhost -t demo/hello -m 'hi'",
    "mqttpanel broker --bind 127.0.0.1",
]


def apply_fr(obj):
    if isinstance(obj, str):
        return fr_typo(obj)
    if isinstance(obj, list):
        return [apply_fr(x) for x in obj]
    if isinstance(obj, tuple):
        return tuple(apply_fr(x) for x in obj)
    if isinstance(obj, dict):
        return {k: apply_fr(v) for k, v in obj.items()}
    return obj


T["fr"] = apply_fr(T["fr"])

# ------------------------------------------------------------------------------------------------ pieces

MEDIA = "/assets/media"
CUR = ' aria-current="page"'


def head(t, lang, page, title, desc):
    path = "" if page == "" else page
    alts = "".join(f'<link rel="alternate" hreflang="{l}" href="{SITE}{PREFIX[l]}/{path}">' for l in LANGS)
    alts += f'<link rel="alternate" hreflang="x-default" href="{SITE}/{path}">'
    return f"""<!doctype html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>{e(title)}</title>
<meta name="description" content="{e(desc)}">
<meta name="color-scheme" content="light dark">
<meta name="theme-color" content="#F3F1EC" media="(prefers-color-scheme: light)">
<meta name="theme-color" content="#161615" media="(prefers-color-scheme: dark)">
<link rel="canonical" href="{SITE}{PREFIX[lang]}/{path}">{alts}
<meta property="og:title" content="{e(title)}"><meta property="og:description" content="{e(desc)}">
<meta property="og:image" content="{SITE}{MEDIA}/look-tree.{lang}.light.jpg">
<link rel="icon" href="/assets/favicon.png" type="image/png"><link rel="apple-touch-icon" href="/assets/icon-256.png">
<script src="/theme.js"></script>
<link rel="stylesheet" href="/style.css">
<script src="/site.js" defer></script>
</head>
<body>
<div class="wrap">"""


ICON = {
    "light": '<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="3"/><path d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.4 1.4M11.6 11.6L13 13M3 13l1.4-1.4M11.6 4.4L13 3"/></svg>',
    "dark": '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M13 9.5A5.5 5.5 0 1 1 6.5 3a4.5 4.5 0 0 0 6.5 6.5z"/></svg>',
}


def theme_switch(t):
    """Light or dark. Until the visitor picks one, the system setting is the one shown as selected (site.js)."""
    th = t["theme"]
    btns = "".join(
        f'<button type="button" data-theme-set="{k}" aria-pressed="false" aria-label="{e(th[k])}" title="{e(th[k])}">{ICON[k]}</button>'
        for k in ("light", "dark")
    )
    return f'<span class="theme" role="group" aria-label="{e(th["label"])}">{btns}</span>'


BADGES = ROOT / "assets" / "badges"
# Apple's badge is only available from Apple's marketing tools (Marketing Resources and Identity Guidelines,
# "Download on the Mac App Store"), after the owner accepts Apple's licence. Save the official black badge, unaltered,
# as website/assets/badges/mac-app-store.svg: the build then shows it instead of the neutral placeholder button.
MAS_BADGE = "mac-app-store.svg"


def dl_link(t, url, inner, cls, note=""):
    """A download link; without a URL it goes nowhere and says "coming soon" under it. An optional caption follows."""
    note = f'<small class="dlcap">{e(note)}</small>' if note else ""
    if url:
        return f'<li class="dlitem"><a class="{cls}" href="{e(url)}">{inner}</a>{note}</li>'
    return (f'<li class="dlitem"><a class="{cls}" href="#" aria-disabled="true">{inner}</a>'
            f'<small class="soon">{e(t["dl"]["soon"])}</small>{note}</li>')


GH_MARK = '<img src="/assets/badges/github-mark-white.svg" alt="" width="25" height="24">'


def download_section(t):
    """The Download band at the bottom of the home page: the official store badges (black, never recoloured), the
    sentence that the stores install and update the app, then the direct macOS download (signed and notarized DMG
    on the GitHub release) and the GitHub button for people who build it themselves."""
    d = t["dl"]
    if (BADGES / MAS_BADGE).exists():
        mas = f'<img src="/assets/badges/{MAS_BADGE}" alt="{e(d["mas"])}" height="48">'
        mas_cls = "badge"
    else:
        mas = 'Mac App Store'  # neutral placeholder: no Apple logo until the official badge is in place
        mas_cls = "badge plain"
    ms = f'<img src="/assets/badges/microsoft-store.svg" alt="{e(d["ms"])}" width="175" height="48">'
    gh = f'{GH_MARK}<span>{e(d["gh"])}</span>'
    # The direct macOS download sits with the store badges; the GitHub button stays alone under the rule.
    dmg = dl_link(t, MAC_DMG_URL, f'<span>{e(d["dmg"])}</span>', "badge plain", d["dmg_note"])
    badges = dl_link(t, MAC_APP_STORE_URL, mas, mas_cls) + dl_link(t, MICROSOFT_STORE_URL, ms, "badge") + dmg
    build = dl_link(t, GITHUB_URL, gh, "badge plain gh", d["gh_note"])
    return f"""<section class="dlband" id="download" aria-labelledby="download-h">
  <h2 id="download-h">{e(d["h"])}</h2>
  <ul class="badges">{badges}</ul>
  <p class="dlnote">{e(d["stores"])}</p>
  <ul class="build">{build}</ul>
</section>"""


def header(t, lang, page):
    p = PREFIX[lang]
    langs = "".join(
        f'<a href="{PREFIX[l]}/{page}" hreflang="{l}" lang="{l}"{CUR if l == lang else ""}>{l.upper()}</a>'
        for l in LANGS
    )
    return f"""<header class="top">
  <a class="brand" href="{p}/"><img src="/assets/icon-64.png" alt="" width="32" height="32"><span>MQTT Panel</span></a>
  <nav aria-label="Site">
    <a class="navl" href="{p}/#features">{e(t["nav_features"])}</a><a class="navl" href="{p}/privacy">{e(t["nav_privacy"])}</a>
    <span class="lang" role="group" aria-label="{e(t["lang_label"])}">{langs}</span>
    {theme_switch(t)}
    <a class="cta" href="{p}/#download">{e(t["dl"]["button"])}</a>
  </nav>
</header>"""


def footer(t, lang, page):
    p = PREFIX[lang]
    langs = "".join(
        f'<a href="{PREFIX[l]}/{page}" hreflang="{l}" lang="{l}"{CUR if l == lang else ""}>{l.upper()}</a>'
        for l in LANGS
    )
    return f"""<footer>
  <p>© {YEAR} mqtt-panel.com · <a href="{p}/privacy">{e(t["footer_privacy"])}</a> · <a href="{p}/legal">{e(t["footer_legal"])}</a> · <a href="{p}/legal#licences">{e(t["footer_licences"])}</a></p>
  <span class="lang" role="group" aria-label="{e(t["lang_label"])}">{langs}</span>
</footer>
</div>
</body>
</html>
"""


def shot(id_, lang, alt, cls=""):
    """One screenshot in both themes; CSS shows the one matching the page theme. The other is display:none and,
    being lazy, is not fetched."""
    c = f' class="{cls}"' if cls else ""
    return (
        f'<img class="tl" src="{MEDIA}/{id_}.{lang}.light.jpg" alt="{e(alt)}" width="1280" height="800" loading="lazy"{c}>'
        f'<img class="td" src="{MEDIA}/{id_}.{lang}.dark.jpg" alt="{e(alt)}" width="1280" height="800" loading="lazy"{c}>'
    )


# Live views: the app's own interface from /app/ (demo mode, mock data, nothing leaves the page), set up by a few
# steps and shown scaled, without interaction; the screenshot stays as the poster and the fallback (no script,
# reduced motion, small screens, off screen). Owner, 18 Oct 2026.
ROW = '[data-testid=tree-row][data-path="{}"]'
SCENES = {
    # the hero is the interactive demo: a dashboard chart is prepared first, so the Charts screen has one
    "hero": ("mock=demo&connect=plant,local&screen=explore", [
        ["fill", "[data-testid=search]", "building/f2/ahu01"], ["click", ROW.format("building/f2/ahu01/state")], ["wait", 500],
        ["click", "[data-testid=add-field]"], ["clickText", "[data-testid=picker-row]", "setpoint", 0],
        ["select", "[data-testid=picker-also]", "new"], ["click", "[data-testid=picker-add]"], ["wait", 300],
        ["click", "[data-testid=rail-charts]"], ["clickText", "[data-testid=chart-pane] .ph button", "15 min", 0], ["wait", 300],
        ["click", "[data-testid=rail-explore]"], ["fill", "[data-testid=search]", "building/f2"], ["wait", 300],
        ["click", ROW.format("building/f2/ahu01/state")], ["wait", 600], ["click", "[data-testid=payload-form-tree]"]]),
    "look-history": ("mock=demo&connect=plant,local&screen=explore", [
        ["fill", "[data-testid=search]", "building/f2/r204"], ["click", ROW.format("building/f2/r204/temperature")], ["wait", 1500],
        ["clickText", "[data-testid=history-row]", "value", 1]]),
    "shot-charts": ("mock=demo&connect=plant,local&screen=explore", [
        ["fill", "[data-testid=search]", "building/f2/ahu01"], ["click", ROW.format("building/f2/ahu01/state")], ["wait", 500],
        ["click", "[data-testid=add-field]"], ["clickText", "[data-testid=picker-row]", "setpoint", 0],
        ["select", "[data-testid=picker-also]", "new"], ["click", "[data-testid=picker-add]"], ["wait", 300],
        ["click", "[data-testid=rail-charts]"], ["clickText", "[data-testid=chart-pane] .ph button", "15 min", 0]]),
    "shot-publish": ("mock=demo&connect=plant,local&screen=publish", [
        ["clickNth", "[data-testid=template]", 2], ["click", "[data-testid=send]"]]),
    "shot-broker": ("mock=demo&screen=broker", []),
    "shot-security": ("mock=demo&connect=plant,local&screen=connect", [["click", "[data-testid=fold-security]"]]),
    "look-connections": ("mock=demo&connect=plant,local&screen=connect", [["click", "[data-profile=lab] [data-testid=card-play]"]]),
    "look-table": ("mock=demo&connect=plant,local&screen=explore", [
        ["click", "[data-testid=layout-table]"], ["fill", "[data-testid=search]", "building/f2"]]),
}


def live(t, lang, scene, poster, alt, hero=False):
    query, steps = SCENES[scene]
    L = t["live"]
    return (
        f'<div class="live frame{" hero-live" if hero else ""}"{" data-interactive" if hero else ""} data-query="{e(query)}&amp;lang={lang}" data-steps="{e(json.dumps(steps))}">'
        f'{shot(poster, lang, alt)}'
        f'<button type="button" class="lpause" data-pause="{e(L["pause"])}" data-play="{e(L["play"])}" aria-label="{e(L["pause"])}" hidden><span aria-hidden="true">❚❚</span></button>'
        f'</div>'
    )


# "A closer look": one still per tab, the first shown; without script every panel shows, one under the other.
def looks(t, lang):
    tabs, panels = [], []
    for i, (id_, label, caption, alt) in enumerate(t["looks"]):
        sel = "true" if i == 0 else "false"
        tabindex = "" if i == 0 else ' tabindex="-1"'
        tabs.append(
            f'<button type="button" role="tab" id="tab-{id_}" aria-controls="panel-{id_}" aria-selected="{sel}"'
            f'{tabindex}>{e(label)}</button>'
        )
        off = "" if i == 0 else " data-off"
        panels.append(
            f'<figure class="look" role="tabpanel" id="panel-{id_}" aria-labelledby="tab-{id_}"{off}>'
            f'{live(t, lang, id_, id_, alt)}<figcaption>{e(caption)}</figcaption></figure>'
        )
    return f'<div class="looks"><div class="ltabs" role="tablist">{"".join(tabs)}</div>{"".join(panels)}</div>'


# "What it does" as cards (owner, 18 Oct 2026): the feature name, a headline, a short text, then the view itself.
WIDE = {"explorer", "broker"}


def feature(t, lang, i, f):
    anchor, img, title, alt, pitch, body = f
    if anchor == "cli":
        lines = "".join(
            f'<span class="c">{e(c)}</span>\n<span class="ln"><span class="p">$</span> <span class="cmd">{e(cmd)}</span></span>\n'
            for c, cmd in zip(t["cli_comment"], CLI_LINES)
        )
        visual = f'<pre class="term" data-type aria-label="{e(title)}"><code>{lines.rstrip()}</code></pre>'
    else:
        visual = live(t, lang, img, img, alt)
    wide = " wide" if anchor in WIDE else ""
    return f"""<article class="card{wide}" id="{e(anchor)}">
  <div class="ctext"><p class="eyebrow">{e(title)}</p><h3>{e(pitch)}</h3><p class="body">{e(body)}</p></div>
  <div class="cvisual">{visual}</div>
</article>"""


# "Works with your AI assistant" (owner, 20 Oct 2026): a wide block after the others, so the two-column grid has no
# gap (two wide blocks and four half blocks). The visual is a mock chat drawn in HTML, no vendor interface. The
# Claude logo is Anthropic's file from its press kit, unaltered; ChatGPT is a plain text mark (see website/README.md).
CLAUDE_LOGO = ('<img class="tl" src="/assets/brands/claude-logo-slate.svg" alt="Claude" width="92" height="20">'
               '<img class="td" src="/assets/brands/claude-logo-ivory.svg" alt="Claude" width="92" height="20">')


def assistant(t):
    a = t["ai"]
    bullets = "".join(f"<li>{e(b)}</li>" for b in a["bullets"])
    works = (f'<ul class="works">'
             f'<li><span class="wlabel">{e(a["works"])}</span><span class="wmark">{CLAUDE_LOGO}</span></li>'
             f'<li><span class="wlabel">{e(a["works"])}</span><span class="wmark wtext">ChatGPT</span></li></ul>')
    reply = "".join(f"<p>{e(r)}</p>" for r in a["reply"])
    chat = (f'<figure class="chat" aria-label="{e(a["chat_label"])}">'
            f'<div class="chathead" aria-hidden="true"><span class="dot"></span>{e(a["chat_head"])}</div>'
            f'<div class="msgs">'
            f'<div class="msg me"><span class="who">{e(a["you"])}</span><div class="bubble">{e(a["prompt"])}</div></div>'
            f'<div class="msg bot"><span class="who">{e(a["bot"])}</span><div class="bubble">{reply}</div></div>'
            f'</div></figure>')
    return f"""<article class="card wide ai" id="assistant">
  <div class="ctext"><p class="eyebrow">{e(a["title"])}</p><h3>{e(a["pitch"])}</h3><p class="body">{e(a["body"])}</p><p class="body beta">{e(a["beta"])}</p>
  <ul class="ticks">{bullets}</ul>{works}</div>
  <div class="cvisual">{chat}</div>
</article>"""


def index(t, lang):
    feats = "".join(feature(t, lang, i, f) for i, f in enumerate(t["features"])) + assistant(t)
    p = PREFIX[lang]
    return head(t, lang, "", t["title"], t["meta"]) + header(t, lang, "") + f"""
<main>
<section class="hero">
  <h1>MQTT Panel</h1>
  <p class="lead">{e(t["lead"])}</p>
  {live(t, lang, "hero", "look-tree", t["hero"]["alt"], hero=True)}
  <p class="hint">{e(t["hero"]["hint"])}</p>
</section>
<section class="block" id="features">
  <div class="bhead"><h2>{e(t["features_h"])}</h2></div>
  <div class="cards">{feats}</div>
</section>
<section class="block" id="demo">
  <div class="bhead"><h2>{e(t["demo_h"])}</h2><p>{e(t["demo_p"])}</p></div>
  {looks(t, lang)}
</section>
<section class="block teaser" id="privacy">
  <h2>{e(t["privacy_h"])}</h2>
  <p>{e(t["privacy_p"])}</p>
  <p><a class="more" href="{p}/privacy">{e(t["privacy_link"])} →</a></p>
</section>
{download_section(t)}
</main>""" + footer(t, lang, "")


def doc_page(t, lang, page, title, lead, body):
    return head(t, lang, page, f"{title} · MQTT Panel", lead) + header(t, lang, page) + f"""
<main class="doc">
  <h1>{e(title)}</h1>
  <p class="lead">{e(lead)}</p>
  {body}
  <p><a class="more" href="{PREFIX[lang]}/">← {e(t["back"])}</a></p>
</main>""" + footer(t, lang, page)


def privacy(t, lang):
    body = "".join(
        f'<section><h2>{e(h)}</h2>{"".join(f"<p>{e(x)}</p>" for x in ps)}</section>' for h, ps in t["privacy_sections"]
    )
    return doc_page(t, lang, "privacy", t["privacy_title"], t["privacy_lead"], body)


def licence_list(items):
    by = {}
    for it in items:
        by.setdefault(it["license"] or "unknown", []).append(f'{it["name"]} {it["version"]}')
    out = []
    for lic in sorted(by, key=lambda k: (-len(by[k]), k)):
        names = ", ".join(sorted(by[lic], key=str.lower))
        out.append(f"<dt>{e(lic)}</dt><dd>{e(names)}</dd>")
    return "<dl class=\"lic\">" + "".join(out) + "</dl>"


# The licence of MQTT Panel (owner, 25 Oct 2026): the Commons Clause header as published at https://commonsclause.com/
# with its three fields filled in, then the Apache License 2.0 as published at
# https://www.apache.org/licenses/LICENSE-2.0.txt. Both unaltered otherwise; LICENSE.txt is the two joined.
LIC_CC = (ROOT / "licence" / "commons-clause.txt").read_text(encoding="utf-8").strip()
LIC_APACHE = (ROOT / "licence" / "apache-2.0.txt").read_text(encoding="utf-8").strip("\n")


def legal(t, lang, third):
    body = f"""<section><h2>{e(t["legal_notice_h"])}</h2><p class="placeholder">{e(t["operator"])}</p></section>
<section id="licence"><h2>{e(t["licence_h"])}</h2>{"".join(f"<p>{e(x)}</p>" for x in t["licence_p"])}
<h3>{e(t["licence_cc"])}</h3><pre class="lictext" lang="en">{e(LIC_CC)}</pre>
<h3>{e(t["licence_apache"])}</h3><pre class="lictext" lang="en">{e(LIC_APACHE)}</pre>
<p><a class="more" href="/LICENSE.txt">{e(t["licence_link"])} →</a></p></section>
<section id="licences"><h2>{e(t["third_h"])}</h2><p>{e(t["third_p"])}</p>
<h3>{e(t["third_rust"])}</h3>{licence_list(third["rust"])}
<h3>{e(t["third_npm"])}</h3>{licence_list(third["npm"])}</section>"""
    return doc_page(t, lang, "legal", t["legal_title"], t["legal_lead"], body)


def not_found():
    t = T["en"]
    msg = " · ".join(T[l]["not_found"] for l in LANGS)
    return head(t, "en", "404", f"{t['not_found']} · MQTT Panel", t["not_found"]) + header(t, "en", "") + f"""
<main class="doc"><h1>{e(msg)}</h1><p><a class="more" href="/">← MQTT Panel</a></p></main>""" + footer(t, "en", "")


# ------------------------------------------------------------------------------------------------ licences

def refresh_licences():
    """Writes third-party.json: the licences of the Rust crates the workspace ships (normal dependencies of the
    workspace members, all platforms) and of the npm packages the interface ships (non-dev entries of the lockfile)."""
    meta = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--locked", "--offline"], cwd=REPO))
    pkgs = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    members = set(meta["workspace_members"])
    seen, stack = set(), list(members)
    while stack:
        nid = stack.pop()
        for d in nodes[nid]["deps"]:
            if any(k["kind"] is None for k in d["dep_kinds"]) and d["pkg"] not in seen:
                seen.add(d["pkg"])
                stack.append(d["pkg"])
    rust = sorted(
        ({"name": pkgs[i]["name"], "version": pkgs[i]["version"], "license": pkgs[i].get("license") or ""} for i in seen if i not in members),
        key=lambda x: (x["name"], x["version"]),
    )
    lock = json.loads((REPO / "package-lock.json").read_text())
    npm = []
    for path, p in lock.get("packages", {}).items():
        if not path or "node_modules/" not in path or p.get("dev") or p.get("devOptional") or p.get("link"):
            continue
        name = path.rsplit("node_modules/", 1)[1]
        if name.startswith("@mqttpanel/"):
            continue
        npm.append({"name": name, "version": p.get("version", ""), "license": p.get("license") or ""})
    npm = sorted({(x["name"], x["version"]): x for x in npm}.values(), key=lambda x: (x["name"], x["version"]))
    (ROOT / "third-party.json").write_text(json.dumps({"rust": rust, "npm": npm}, indent=1, ensure_ascii=False) + "\n")
    print(f"third-party.json: {len(rust)} crates, {len(npm)} npm packages")


# ------------------------------------------------------------------------------------------------ build

def check_forbidden():
    pats = [re.compile(p, re.I) for p in forbidden_patterns()]
    hits = []
    for f in OUT.rglob("*"):
        if f.suffix not in (".html", ".txt", ".xml", ".js", ".css", ".json"):
            continue
        text = f.read_text(encoding="utf-8", errors="replace")
        for p in pats:
            if p.search(text):
                hits.append(f"{f.relative_to(OUT)}: {p.pattern}")
    if hits:
        print("forbidden words on the site:\n  " + "\n  ".join(hits))
        sys.exit(1)


def build():
    third_path = ROOT / "third-party.json"
    if not third_path.exists():
        sys.exit("third-party.json missing: run python3 website/build.py --licences")
    third = json.loads(third_path.read_text())
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir()
    shutil.copytree(ROOT / "assets", OUT / "assets")
    shutil.copy(ICONS / "128x128@2x.png", OUT / "assets" / "icon-256.png")
    shutil.copy(ICONS / "64x64.png", OUT / "assets" / "icon-64.png")
    shutil.copy(ICONS / "32x32.png", OUT / "assets" / "favicon.png")
    shutil.copy(ROOT / "style.css", OUT / "style.css")
    shutil.copy(ROOT / "site.js", OUT / "site.js")
    shutil.copy(ROOT / "theme.js", OUT / "theme.js")
    app = REPO / "packages" / "ui" / "dist-site"
    if not (app / "index.html").exists():
        sys.exit("packages/ui/dist-site missing: run npm run build:site -w @mqttpanel/ui")
    shutil.copytree(app, OUT / "app")
    (OUT / "LICENSE.txt").write_text(LIC_CC + "\n\n" + "-" * 75 + "\n\n\n" + LIC_APACHE + "\n", encoding="utf-8")
    for lang in LANGS:
        t = T[lang]
        d = OUT / PREFIX[lang].strip("/")
        d.mkdir(exist_ok=True)
        (d / "index.html").write_text(index(t, lang), encoding="utf-8")
        (d / "privacy.html").write_text(privacy(t, lang), encoding="utf-8")
        (d / "legal.html").write_text(legal(t, lang, third), encoding="utf-8")
    (OUT / "404.html").write_text(not_found(), encoding="utf-8")
    (OUT / "robots.txt").write_text(f"User-agent: *\nAllow: /\nDisallow: /app/\nSitemap: {SITE}/sitemap.xml\n")
    urls = "".join(f"<url><loc>{SITE}{PREFIX[l]}/{p}</loc></url>" for l in LANGS for p in ("", "privacy", "legal"))
    (OUT / "sitemap.xml").write_text(
        f'<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">{urls}</urlset>\n'
    )
    check_forbidden()
    print("built", sum(1 for _ in OUT.rglob("*.html")), "pages into", OUT.relative_to(REPO))


if __name__ == "__main__":
    if "--licences" in sys.argv:
        refresh_licences()
    build()

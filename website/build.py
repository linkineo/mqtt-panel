"""Builds the MQTT Panel website (EN at /, FR at /fr/, DE at /de/) into website/public/.

Run:  python3 website/build.py              build the site
      python3 website/build.py --licences   refresh third-party.json from Cargo and npm metadata, then build
Deploy with website/deploy.sh. Static files only: no cookies, no trackers, nothing loaded from third parties.
"""
import html
import json
import pathlib
import re
import shutil
import subprocess
import sys

# ---- The owner fills these when the listings exist. Empty: the button shows "… · coming soon", disabled. ----
MAC_APP_STORE_URL = ""
MICROSOFT_STORE_URL = ""
GITHUB_URL = ""

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parent
OUT = ROOT / "public"
SITE = "https://mqtt-panel.com"
LANGS = ["en", "fr", "de"]
PREFIX = {"en": "", "fr": "/fr", "de": "/de"}
ICONS = REPO / "apps" / "desktop" / "src-tauri" / "icons"
YEAR = 2026

# Words that must never appear on the site: the patterns of scripts/check-forbidden-words.sh (split so the
# repository guard does not trip on this file) plus the former company name.
FORBIDDEN = [
    # (pattern removed)
    # (pattern removed)
    # (pattern removed)
    # (pattern removed)
    # (pattern removed)
    # (pattern removed)
    # (pattern removed)
    # (pattern removed)
    "mqtt-panel[ -]labs",
]


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
        title="MQTT Panel: MQTT client and built-in broker for Windows, macOS and Linux",
        meta="See what is happening on your MQTT brokers: topics, values, history, charts, publishing, and a broker built in.",
        nav_features="Features", nav_privacy="Privacy", lang_label="Language",
        lead="See what is happening on your MQTT brokers: topics, values, history, charts, publishing, and a broker built in.",
        platforms="Windows · macOS · Linux · open source, Apache 2.0",
        store=["Mac App Store", "Microsoft Store", "Source code on GitHub"], soon="coming soon",
        icon_alt="MQTT Panel app icon",
        demo_h="See it working",
        demo_p="Short clips recorded from the app itself, with demo data.",
        clips=[
            ("clip-connections", "Several brokers at once", "Every open connection gets a tab with its live figures; the cards show which profiles are connected."),
            ("clip-explorer", "A broker with many topics", "Unfold the tree, follow a value in its sparkline, open the payload with its history and the diff to the previous message."),
            ("clip-charts", "Charts", "Pick fields from a payload for the topic chart, then send them to a dashboard chart with several series."),
            ("clip-broker", "The built-in broker", "Listeners, users and client certificates on one screen. A port change applies at once."),
        ],
        features_h="What it does",
        features=[
            ("shot-explorer", "Explorer", "The topic tree and the topic table on the Explorer screen, with the payload of a selected topic, its diff and its chart.", [
                "Tree or table: thousands of topics stay fast to browse and search, with + # wildcards or a regular expression.",
                "Every message shows its payload, with the changes from the previous one highlighted.",
                "Sparklines next to each topic show how a value moved over its last messages.",
                "A history list per topic, filterable by one field of a JSON payload.",
                "JSON, text, binary, images, Sparkplug B, CBOR, MessagePack and Protobuf are decoded.",
            ]),
            ("shot-charts", "Charts", "A dashboard chart with three series from one topic.", [
                "A series is one topic plus one field of its payload.",
                "Numbers are plotted as they are, numeric text is read as a number, true and false become 1 and 0.",
                "Two axes when units differ, for example °C on the left and % on the right.",
                "A dashboard holds several charts with a shared time range, pause and hover values.",
            ]),
            ("shot-publish", "Publish", "The Publish screen with templates, the composer and the list of sent messages with a reply.", [
                "A composer with topic, QoS, retain and the payload format (JSON, text, hex, Base64).",
                "Templates for the messages you send often, with placeholders such as {room}.",
                "Request and response on MQTT 5: set a response topic and see the reply next to the request.",
                "Repeat a message every few seconds to simulate a sensor.",
            ]),
            ("shot-broker", "Built-in broker", "The built-in broker with its listeners, users, client certificates and connected clients.", [
                "TCP, TLS, WebSocket and secure WebSocket listeners, switched on and off one by one.",
                "Users with passwords, shared by all listeners.",
                "Client certificates issued in one click, exported as PEM or PKCS#12.",
                "A local certificate authority is created on first start.",
                "Every change applies at once, without restarting the broker.",
            ]),
            ("shot-security", "Security", "A connection profile with its security settings: CA, client certificate and key.", [
                "TLS with your own CA, client certificates and keys (PEM or PKCS#12).",
                "A broker with a self-signed certificate can be trusted by pinning its certificate.",
                "Passwords, keys and passphrases are stored encrypted on your computer, never in profile files.",
                "Nothing leaves your computer: no telemetry, no account, no crash reports.",
            ]),
            ("cli", "Command line", "", [
                "mqttpanel sub and mqttpanel pub, with the same profiles and secrets as the app.",
                "mqttpanel broker starts the built-in broker without a window.",
                "Text, JSON, NDJSON or raw output for scripts.",
            ]),
        ],
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
                "The app makes no network connections except to the brokers you configure and, if you turn it on, to the update feed that tells you about new versions.",
                "Profiles, templates, views and the message history stay on your computer.",
            ]),
            ("Secrets", [
                "Passwords, private keys and PKCS#12 passphrases are stored encrypted on your computer. The key that unlocks them is kept in the system's secret store (the keychain on macOS). They are never written to profile files and never leave the device.",
            ]),
            ("This website", [
                "This website sets no cookies, runs no analytics and loads nothing from third parties: no fonts, no scripts, no images from other sites.",
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
        licence_p="MQTT Panel is open-source software, licensed under the Apache License, Version 2.0.",
        licence_link="Read the licence text",
        third_h="Third-party software",
        third_p="MQTT Panel includes the following open-source components, each under its own licence:",
        third_rust="Engine and command line (Rust crates)",
        third_npm="Interface (npm packages)",
        not_found="Page not found",
    ),
    "fr": dict(
        title="MQTT Panel : client MQTT et broker intégré pour Windows, macOS et Linux",
        meta="Voyez ce qui se passe sur vos brokers MQTT : topics, valeurs, historique, courbes, publication, et un broker intégré.",
        nav_features="Fonctions", nav_privacy="Confidentialité", lang_label="Langue",
        lead="Voyez ce qui se passe sur vos brokers MQTT : topics, valeurs, historique, courbes, publication, et un broker intégré.",
        platforms="Windows · macOS · Linux · open source, Apache 2.0",
        store=["Mac App Store", "Microsoft Store", "Code source sur GitHub"], soon="bientôt disponible",
        icon_alt="Icône de l’app MQTT Panel",
        demo_h="En action",
        demo_p="De courtes séquences enregistrées dans l’app elle-même, avec des données de démonstration.",
        clips=[
            ("clip-connections", "Plusieurs brokers à la fois", "Chaque connexion ouverte a son onglet et ses chiffres en direct ; les cartes montrent les profils connectés."),
            ("clip-explorer", "Un broker aux nombreux topics", "Dépliez l’arbre, suivez une valeur dans sa mini-courbe, ouvrez le message avec son historique et les écarts avec le précédent."),
            ("clip-charts", "Courbes", "Choisissez des champs du message pour la courbe du topic, puis envoyez-les vers une courbe du tableau de bord à plusieurs séries."),
            ("clip-broker", "Le broker intégré", "Points d’écoute, utilisateurs et certificats clients sur un seul écran. Un changement de port s’applique aussitôt."),
        ],
        features_h="Ce que fait l’app",
        features=[
            ("shot-explorer", "Explorer", "L’arbre des topics sur l’écran Explorer, avec le message du topic choisi, ses écarts et sa courbe.", [
                "Arbre ou tableau : des milliers de topics restent rapides à parcourir et à chercher, avec les jokers + # ou une expression régulière.",
                "Chaque message s’affiche avec les changements par rapport au précédent mis en évidence.",
                "À côté de chaque topic, une mini-courbe montre l’évolution de la valeur sur les derniers messages.",
                "Un historique par topic, filtrable sur un champ d’un message JSON.",
                "JSON, texte, binaire, images, Sparkplug B, CBOR, MessagePack et Protobuf sont décodés.",
            ]),
            ("shot-charts", "Courbes", "Une courbe du tableau de bord avec trois séries issues d’un même topic.", [
                "Une série, c’est un topic et un champ de son message.",
                "Les nombres sont tracés tels quels, un texte numérique est lu comme un nombre, true et false deviennent 1 et 0.",
                "Deux axes quand les unités diffèrent, par exemple °C à gauche et % à droite.",
                "Un tableau de bord réunit plusieurs courbes, avec une même plage de temps, une pause et les valeurs au survol.",
            ]),
            ("shot-publish", "Publier", "L’écran Publier avec les modèles, l’éditeur et la liste des messages envoyés avec une réponse.", [
                "Un éditeur avec topic, QoS, retain et format du message (JSON, texte, hexa, Base64).",
                "Des modèles pour les messages envoyés souvent, avec des variables comme {room}.",
                "Requête et réponse en MQTT 5 : indiquez un topic de réponse et voyez la réponse à côté de la requête.",
                "Répétez un message toutes les quelques secondes pour simuler un capteur.",
            ]),
            ("shot-broker", "Broker intégré", "Le broker intégré avec ses points d’écoute, ses utilisateurs, ses certificats clients et les clients connectés.", [
                "Points d’écoute TCP, TLS, WebSocket et WebSocket sécurisé, activés un par un.",
                "Des utilisateurs avec mot de passe, communs à tous les points d’écoute.",
                "Des certificats clients émis en un clic, exportés en PEM ou PKCS#12.",
                "Une autorité de certification locale est créée au premier démarrage.",
                "Chaque changement s’applique aussitôt, sans redémarrer le broker.",
            ]),
            ("shot-security", "Sécurité", "Un profil de connexion avec ses réglages de sécurité : CA, certificat client et clé.", [
                "TLS avec votre propre CA, certificats clients et clés (PEM ou PKCS#12).",
                "Un broker au certificat auto-signé peut être approuvé en épinglant son certificat.",
                "Mots de passe, clés et phrases secrètes sont stockés chiffrés sur votre ordinateur, jamais dans les fichiers de profil.",
                "Rien ne quitte votre ordinateur : ni télémétrie, ni compte, ni rapport de plantage.",
            ]),
            ("cli", "Ligne de commande", "", [
                "mqttpanel sub et mqttpanel pub, avec les mêmes profils et secrets que l’app.",
                "mqttpanel broker lance le broker intégré sans fenêtre.",
                "Sortie en texte, JSON, NDJSON ou brute pour vos scripts.",
            ]),
        ],
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
                "L’app ne se connecte qu’aux brokers que vous configurez et, si vous l’activez, au flux de mises à jour qui annonce les nouvelles versions.",
                "Profils, modèles, vues et historique des messages restent sur votre ordinateur.",
            ]),
            ("Secrets", [
                "Mots de passe, clés privées et phrases secrètes PKCS#12 sont stockés chiffrés sur votre ordinateur. La clé qui les déverrouille est conservée dans le coffre de secrets du système (le trousseau sur macOS). Ils ne sont jamais écrits dans les fichiers de profil et ne quittent jamais l’appareil.",
            ]),
            ("Ce site", [
                "Ce site ne dépose aucun cookie, ne mesure pas l’audience et ne charge rien depuis des tiers : ni polices, ni scripts, ni images venant d’autres sites.",
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
        licence_p="MQTT Panel est un logiciel open source, sous licence Apache, version 2.0.",
        licence_link="Lire le texte de la licence (en anglais)",
        third_h="Logiciels tiers",
        third_p="MQTT Panel inclut les composants open source suivants, chacun sous sa propre licence :",
        third_rust="Moteur et ligne de commande (crates Rust)",
        third_npm="Interface (paquets npm)",
        not_found="Page introuvable",
    ),
    "de": dict(
        title="MQTT Panel: MQTT-Client und eingebauter Broker für Windows, macOS und Linux",
        meta="Sieh, was auf deinen MQTT-Brokern passiert: Topics, Werte, Verlauf, Kurven, Senden und ein eingebauter Broker.",
        nav_features="Funktionen", nav_privacy="Datenschutz", lang_label="Sprache",
        lead="Sieh, was auf deinen MQTT-Brokern passiert: Topics, Werte, Verlauf, Kurven, Senden und ein eingebauter Broker.",
        platforms="Windows · macOS · Linux · Open Source, Apache 2.0",
        store=["Mac App Store", "Microsoft Store", "Quellcode auf GitHub"], soon="bald verfügbar",
        icon_alt="App-Symbol von MQTT Panel",
        demo_h="So sieht es aus",
        demo_p="Kurze Aufnahmen direkt aus der App, mit Demodaten.",
        clips=[
            ("clip-connections", "Mehrere Broker gleichzeitig", "Jede offene Verbindung hat einen Tab mit ihren Live-Werten; die Karten zeigen, welche Profile verbunden sind."),
            ("clip-explorer", "Ein Broker mit vielen Topics", "Klapp den Baum auf, verfolge einen Wert in seiner Mini-Kurve, öffne die Nachricht mit Verlauf und den Unterschieden zur vorherigen."),
            ("clip-charts", "Kurven", "Wähl Felder aus der Nachricht für die Kurve des Topics und schick sie an eine Dashboard-Kurve mit mehreren Reihen."),
            ("clip-broker", "Der eingebaute Broker", "Listener, Benutzer und Client-Zertifikate auf einem Bildschirm. Ein geänderter Port gilt sofort."),
        ],
        features_h="Was die App kann",
        features=[
            ("shot-explorer", "Erkunden", "Der Topic-Baum im Bereich Erkunden, mit der Nachricht des gewählten Topics, ihren Unterschieden und ihrer Kurve.", [
                "Baum oder Tabelle: Auch Tausende Topics lassen sich schnell durchsehen und durchsuchen, mit + # oder einem regulären Ausdruck.",
                "Jede Nachricht zeigt, was sich gegenüber der vorherigen geändert hat.",
                "Neben jedem Topic zeigt eine Mini-Kurve, wie sich der Wert zuletzt bewegt hat.",
                "Ein Verlauf pro Topic, filterbar nach einem Feld einer JSON-Nachricht.",
                "JSON, Text, Binärdaten, Bilder, Sparkplug B, CBOR, MessagePack und Protobuf werden dekodiert.",
            ]),
            ("shot-charts", "Kurven", "Eine Dashboard-Kurve mit drei Reihen aus einem Topic.", [
                "Eine Reihe ist ein Topic plus ein Feld seiner Nachricht.",
                "Zahlen werden direkt gezeichnet, Zahlen als Text werden gelesen, true und false werden zu 1 und 0.",
                "Zwei Achsen, wenn die Einheiten verschieden sind, etwa °C links und % rechts.",
                "Ein Dashboard fasst mehrere Kurven zusammen, mit gemeinsamem Zeitraum, Pause und Werten beim Darüberfahren.",
            ]),
            ("shot-publish", "Senden", "Der Bereich Senden mit Vorlagen, Editor und der Liste gesendeter Nachrichten samt Antwort.", [
                "Ein Editor mit Topic, QoS, Retain und Format der Nachricht (JSON, Text, Hex, Base64).",
                "Vorlagen für Nachrichten, die du oft sendest, mit Platzhaltern wie {room}.",
                "Anfrage und Antwort mit MQTT 5: Gib ein Antwort-Topic an und sieh die Antwort direkt neben der Anfrage.",
                "Wiederhol eine Nachricht alle paar Sekunden, um einen Sensor zu simulieren.",
            ]),
            ("shot-broker", "Eingebauter Broker", "Der eingebaute Broker mit Listenern, Benutzern, Client-Zertifikaten und verbundenen Clients.", [
                "Listener für TCP, TLS, WebSocket und sicheres WebSocket, einzeln ein- und ausschaltbar.",
                "Benutzer mit Passwort, gemeinsam für alle Listener.",
                "Client-Zertifikate mit einem Klick ausstellen und als PEM oder PKCS#12 exportieren.",
                "Beim ersten Start entsteht eine lokale Zertifizierungsstelle.",
                "Jede Änderung gilt sofort, ohne den Broker neu zu starten.",
            ]),
            ("shot-security", "Sicherheit", "Ein Verbindungsprofil mit seinen Sicherheitseinstellungen: CA, Client-Zertifikat und Schlüssel.", [
                "TLS mit deiner eigenen CA, Client-Zertifikaten und Schlüsseln (PEM oder PKCS#12).",
                "Einem Broker mit selbstsigniertem Zertifikat kannst du vertrauen, indem du sein Zertifikat festlegst.",
                "Passwörter, Schlüssel und Passphrasen liegen verschlüsselt auf deinem Computer, nie in Profildateien.",
                "Nichts verlässt deinen Computer: keine Telemetrie, kein Konto, keine Absturzberichte.",
            ]),
            ("cli", "Kommandozeile", "", [
                "mqttpanel sub und mqttpanel pub, mit denselben Profilen und Geheimnissen wie die App.",
                "mqttpanel broker startet den eingebauten Broker ohne Fenster.",
                "Ausgabe als Text, JSON, NDJSON oder roh für Skripte.",
            ]),
        ],
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
                "Die App verbindet sich nur mit den Brokern, die du einrichtest, und, wenn du es einschaltest, mit dem Update-Feed, der neue Versionen meldet.",
                "Profile, Vorlagen, Ansichten und der Nachrichtenverlauf bleiben auf deinem Computer.",
            ]),
            ("Geheimnisse", [
                "Passwörter, private Schlüssel und PKCS#12-Passphrasen liegen verschlüsselt auf deinem Computer. Der Schlüssel dazu liegt im Geheimnisspeicher des Systems (unter macOS im Schlüsselbund). Sie landen nie in Profildateien und verlassen nie das Gerät.",
            ]),
            ("Diese Website", [
                "Diese Website setzt keine Cookies, misst keine Besuche und lädt nichts von Dritten: keine Schriften, keine Skripte, keine Bilder von anderen Websites.",
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
        licence_p="MQTT Panel ist Open-Source-Software unter der Apache License, Version 2.0.",
        licence_link="Lizenztext lesen (Englisch)",
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
<meta property="og:image" content="{SITE}{MEDIA}/shot-explorer.jpg">
<link rel="icon" href="/assets/favicon.png" type="image/png"><link rel="apple-touch-icon" href="/assets/icon-256.png">
<link rel="stylesheet" href="/style.css">
<script src="/site.js" defer></script>
</head>
<body>
<div class="wrap">"""


def header(t, lang, page):
    p = PREFIX[lang]
    langs = "".join(
        f'<a href="{PREFIX[l]}/{page}" hreflang="{l}" lang="{l}"{CUR if l == lang else ""}>{l.upper()}</a>'
        for l in LANGS
    )
    return f"""<header class="top">
  <a class="brand" href="{p}/"><img src="/assets/icon-64.png" alt="" width="32" height="32">MQTT Panel</a>
  <nav aria-label="Site">
    <a href="{p}/#features">{e(t["nav_features"])}</a><a href="{p}/privacy">{e(t["nav_privacy"])}</a>
    <span class="lang" role="group" aria-label="{e(t["lang_label"])}">{langs}</span>
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


def store_buttons(t):
    out = []
    for i, (label, url) in enumerate(zip(t["store"], (MAC_APP_STORE_URL, MICROSOFT_STORE_URL, GITHUB_URL))):
        cls = "btn primary" if i == 0 else "btn"
        if url:
            out.append(f'<a class="{cls}" href="{e(url)}">{e(label)}</a>')
        else:
            out.append(f'<a class="{cls}" href="#" aria-disabled="true">{e(label)} · {e(t["soon"])}</a>')
    return "".join(out)


def clip(id_, title, caption):
    return f"""<figure class="clip">
  <div class="frame"><video muted loop playsinline preload="none" poster="{MEDIA}/{id_}.jpg" width="1280" height="800" aria-label="{e(title)}">
    <source src="{MEDIA}/{id_}.webm" type="video/webm"><source src="{MEDIA}/{id_}.mp4" type="video/mp4">
  </video></div>
  <figcaption><b>{e(title)}</b> {e(caption)}</figcaption>
</figure>"""


def feature(t, i, f):
    id_, title, alt, bullets = f
    lis = "".join(f"<li>{e(b)}</li>" for b in bullets)
    if id_ == "cli":
        lines = "".join(
            f'<span class="c">{e(c)}</span>\n<span class="p">$</span> {e(cmd)}\n' for c, cmd in zip(t["cli_comment"], CLI_LINES)
        )
        visual = f'<pre class="term" aria-label="{e(title)}"><code>{lines.rstrip()}</code></pre>'
    else:
        visual = f'<div class="frame"><img src="{MEDIA}/{id_}.jpg" alt="{e(alt)}" width="1280" height="800" loading="lazy"></div>'
    return f"""<section class="feature" id="{e(id_.replace("shot-", ""))}">
  <div class="ftext"><p class="n">{i + 1:02d}</p><h3>{e(title)}</h3><ul>{lis}</ul></div>
  <div class="fvisual">{visual}</div>
</section>"""


def index(t, lang):
    clips = "".join(clip(*c) for c in t["clips"])
    feats = "".join(feature(t, i, f) for i, f in enumerate(t["features"]))
    p = PREFIX[lang]
    return head(t, lang, "", t["title"], t["meta"]) + header(t, lang, "") + f"""
<main>
<section class="hero">
  <div class="htext">
    <h1>MQTT Panel</h1>
    <p class="lead">{e(t["lead"])}</p>
    <div class="cta">{store_buttons(t)}</div>
    <p class="meta">{e(t["platforms"])}</p>
  </div>
  <img class="appicon" src="/assets/icon-256.png" alt="{e(t["icon_alt"])}" width="256" height="256">
</section>
<section class="block" id="demo">
  <div class="bhead"><h2>{e(t["demo_h"])}</h2><p>{e(t["demo_p"])}</p></div>
  <div class="clips">{clips}</div>
</section>
<section class="block" id="features">
  <div class="bhead"><h2>{e(t["features_h"])}</h2></div>
  {feats}
</section>
<section class="block teaser" id="privacy">
  <h2>{e(t["privacy_h"])}</h2>
  <p>{e(t["privacy_p"])}</p>
  <p><a class="more" href="{p}/privacy">{e(t["privacy_link"])} →</a></p>
</section>
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


def legal(t, lang, third):
    body = f"""<section><h2>{e(t["legal_notice_h"])}</h2><p class="placeholder">{e(t["operator"])}</p></section>
<section><h2>{e(t["licence_h"])}</h2><p>{e(t["licence_p"])}</p><p><a class="more" href="/LICENSE.txt">{e(t["licence_link"])} →</a></p></section>
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
    pats = [re.compile(p, re.I) for p in FORBIDDEN]
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
    shutil.copy(REPO / "LICENSE", OUT / "LICENSE.txt")
    for lang in LANGS:
        t = T[lang]
        d = OUT / PREFIX[lang].strip("/")
        d.mkdir(exist_ok=True)
        (d / "index.html").write_text(index(t, lang), encoding="utf-8")
        (d / "privacy.html").write_text(privacy(t, lang), encoding="utf-8")
        (d / "legal.html").write_text(legal(t, lang, third), encoding="utf-8")
    (OUT / "404.html").write_text(not_found(), encoding="utf-8")
    (OUT / "robots.txt").write_text(f"User-agent: *\nAllow: /\nSitemap: {SITE}/sitemap.xml\n")
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

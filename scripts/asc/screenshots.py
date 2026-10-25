"""Uploads the Mac App Store screenshots (2560x1600 JPEG from website/assets/media) for en-US, fr-FR and de-DE,
in the order of texts.SCREENSHOTS. Replaces the existing APP_DESKTOP set. Never submits.
Run: python3 scripts/asc/screenshots.py"""
import hashlib, pathlib, urllib.request
from asc import *
from ids import LOCALES, version_id
import texts

MEDIA = pathlib.Path(__file__).resolve().parents[2] / "website/assets/media"
DISPLAY = "APP_DESKTOP"

def upload(set_id, path):
    data = path.read_bytes()
    r = post("/v1/appScreenshots", {"data": {"type": "appScreenshots", "attributes": {"fileName": path.name, "fileSize": len(data)},
             "relationships": {"appScreenshotSet": {"data": {"type": "appScreenshotSets", "id": set_id}}}}})["data"]
    for op in r["attributes"]["uploadOperations"]:
        req = urllib.request.Request(op["url"], data=data[op["offset"]: op["offset"] + op["length"]], method=op["method"],
                                     headers={h["name"]: h["value"] for h in op["requestHeaders"]})
        urllib.request.urlopen(req).read()
    patch(f"/v1/appScreenshots/{r['id']}", {"data": {"type": "appScreenshots", "id": r["id"],
          "attributes": {"uploaded": True, "sourceFileChecksum": hashlib.md5(data).hexdigest()}}})

def main():
    V = version_id(get, post, patch)
    for loc in get(f"/v1/appStoreVersions/{V}/appStoreVersionLocalizations")["data"]:
        locale = loc["attributes"]["locale"]
        if locale not in LOCALES: continue
        lang = locale[:2]
        files = [MEDIA / f"{base}.{lang}.{theme}.jpg" for base, theme in texts.SCREENSHOTS]
        missing = [f.name for f in files if not f.exists()]
        if missing: print(locale, "missing", missing); continue
        sets = {s["attributes"]["screenshotDisplayType"]: s["id"] for s in get(f"/v1/appStoreVersionLocalizations/{loc['id']}/appScreenshotSets")["data"]}
        if DISPLAY in sets:
            set_id = sets[DISPLAY]
            for shot in get(f"/v1/appScreenshotSets/{set_id}/appScreenshots")["data"]:
                call("DELETE", f"/v1/appScreenshots/{shot['id']}")
        else:
            set_id = post("/v1/appScreenshotSets", {"data": {"type": "appScreenshotSets", "attributes": {"screenshotDisplayType": DISPLAY},
                          "relationships": {"appStoreVersionLocalization": {"data": {"type": "appStoreVersionLocalizations", "id": loc["id"]}}}}})["data"]["id"]
        for f in files: upload(set_id, f)
        print(locale, len(files), "uploaded")

if __name__ == "__main__":
    main()

"""Mac App Store listing of MQTT Panel: localisations, categories, age rating, content rights, review details, free price.
Run: python3 scripts/asc/metadata.py   (updates App Store Connect; never submits for review)"""
import os, pathlib
from asc import *
from ids import APP, INFO, LOCALES, version_id
import texts

REVIEW_NOTES = (pathlib.Path(__file__).resolve().parents[2] / "docs/store/REVIEW_NOTES.md").read_text(encoding="utf-8")
REVIEW_NOTES = REVIEW_NOTES.split("<!-- notes -->", 1)[1].strip()

def step(label, fn):
    try:
        fn(); print(label, "ok")
    except RuntimeError as e:
        print(label, "FAILED", e)

def upsert(list_path, typ, parent_rel, parent_type, parent_id, loc, attrs):
    existing = {x["attributes"]["locale"]: x["id"] for x in get(list_path)["data"]}
    if loc in existing:
        patch(f"/v1/{typ}/{existing[loc]}", {"data": {"type": typ, "id": existing[loc], "attributes": attrs}})
    else:
        post(f"/v1/{typ}", {"data": {"type": typ, "attributes": {"locale": loc, **attrs},
             "relationships": {parent_rel: {"data": {"type": parent_type, "id": parent_id}}}}})

def main():
    texts.check()
    V = version_id(get, post, patch); print("version", V)
    for loc in LOCALES:
        step(f"app info {loc}", lambda: upsert(f"/v1/appInfos/{INFO}/appInfoLocalizations", "appInfoLocalizations", "appInfo", "appInfos", INFO, loc,
             {"name": texts.NAME, "subtitle": texts.SUBTITLE[loc], "privacyPolicyUrl": texts.PRIVACY_URL[loc]}))
        attrs = {"description": texts.DESCRIPTION[loc], "keywords": texts.KEYWORDS[loc], "promotionalText": texts.PROMO[loc],
                 "supportUrl": texts.SITE, "marketingUrl": texts.SITE}
        # "What's new" is refused on an app's very first version; it is tried separately.
        step(f"version {loc}", lambda: upsert(f"/v1/appStoreVersions/{V}/appStoreVersionLocalizations", "appStoreVersionLocalizations",
             "appStoreVersion", "appStoreVersions", V, loc, attrs))
        step(f"what's new {loc}", lambda: upsert(f"/v1/appStoreVersions/{V}/appStoreVersionLocalizations", "appStoreVersionLocalizations",
             "appStoreVersion", "appStoreVersions", V, loc, {"whatsNew": texts.WHATS_NEW[loc]}))

    step("categories", lambda: patch(f"/v1/appInfos/{INFO}", {"data": {"type": "appInfos", "id": INFO, "relationships": {
        "primaryCategory": {"data": {"type": "appCategories", "id": "DEVELOPER_TOOLS"}},
        "secondaryCategory": {"data": {"type": "appCategories", "id": "UTILITIES"}}}}}))

    step("content rights", lambda: patch(f"/v1/apps/{APP}", {"data": {"type": "apps", "id": APP,
        "attributes": {"contentRightsDeclaration": "DOES_NOT_USE_THIRD_PARTY_CONTENT"}}}))

    # Age rating: no content flags at all, which gives 4+.
    none = "NONE"
    age = {k: none for k in ["alcoholTobaccoOrDrugUseOrReferences", "contests", "gamblingSimulated", "gunsOrOtherWeapons",
           "medicalOrTreatmentInformation", "profanityOrCrudeHumor", "sexualContentGraphicAndNudity", "sexualContentOrNudity",
           "horrorOrFearThemes", "matureOrSuggestiveThemes", "violenceCartoonOrFantasy",
           "violenceRealisticProlongedGraphicOrSadistic", "violenceRealistic"]}
    age.update({k: False for k in ["advertising", "gambling", "healthOrWellnessTopics", "lootBox", "messagingAndChat",
           "parentalControls", "ageAssurance", "socialMedia", "unrestrictedWebAccess", "userGeneratedContent"]})
    step("age rating", lambda: patch(f"/v1/ageRatingDeclarations/{INFO}", {"data": {"type": "ageRatingDeclarations", "id": INFO, "attributes": age}}))

    # Review details: contact name, phone and e-mail are left for the owner (never invented here).
    def review():
        cur = get(f"/v1/appStoreVersions/{V}/appStoreReviewDetail")["data"]
        attrs = {"notes": REVIEW_NOTES, "demoAccountRequired": False}
        if cur:
            patch(f"/v1/appStoreReviewDetails/{cur['id']}", {"data": {"type": "appStoreReviewDetails", "id": cur["id"], "attributes": attrs}})
        else:
            post("/v1/appStoreReviewDetails", {"data": {"type": "appStoreReviewDetails", "attributes": attrs,
                 "relationships": {"appStoreVersion": {"data": {"type": "appStoreVersions", "id": V}}}}})
    step("review details", review)

    # Free in every territory: a price schedule with the 0.0 price point, base territory USA.
    def price():
        free = next(p["id"] for p in get(f"/v1/apps/{APP}/appPricePoints?filter[territory]=USA&limit=5")["data"]
                    if float(p["attributes"]["customerPrice"]) == 0)
        post("/v1/appPriceSchedules", {"data": {"type": "appPriceSchedules", "relationships": {
                "app": {"data": {"type": "apps", "id": APP}},
                "baseTerritory": {"data": {"type": "territories", "id": "USA"}},
                "manualPrices": {"data": [{"type": "appPrices", "id": "${free}"}]}}},
              "included": [{"type": "appPrices", "id": "${free}", "attributes": {"startDate": None},
                "relationships": {"appPricePoint": {"data": {"type": "appPricePoints", "id": free}}}}]})
    step("price free", price)

    # Availability in every territory, also future ones.
    def availability():
        terr = []
        url = "/v1/territories?limit=200"
        while url:
            r = get(url); terr += [t["id"] for t in r["data"]]
            url = r.get("links", {}).get("next", "").replace(BASE, "") or None
        post("/v2/appAvailabilities", {"data": {"type": "appAvailabilities", "attributes": {"availableInNewTerritories": True},
                "relationships": {"app": {"data": {"type": "apps", "id": APP}},
                    "territoryAvailabilities": {"data": [{"type": "territoryAvailabilities", "id": f"${{{t}}}"} for t in terr]}}},
              "included": [{"type": "territoryAvailabilities", "id": f"${{{t}}}", "attributes": {"available": True},
                "relationships": {"territory": {"data": {"type": "territories", "id": t}}}} for t in terr]})
        print("territories", len(terr))
    step("availability", availability)

    # Copyright line: a legal statement chosen by the owner, set only when ASC_COPYRIGHT is given (e.g. in .env.local).
    if os.environ.get("ASC_COPYRIGHT"):
        step("copyright", lambda: patch(f"/v1/appStoreVersions/{V}", {"data": {"type": "appStoreVersions", "id": V,
             "attributes": {"copyright": os.environ["ASC_COPYRIGHT"]}}}))

if __name__ == "__main__":
    main()

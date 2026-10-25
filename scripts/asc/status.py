"""Read-only: prints what the macOS App Store version has, and what still blocks submission.
Run: python3 scripts/asc/status.py"""
from asc import *
from ids import APP, INFO, LOCALES

def main():
    v = next(x for x in get(f"/v1/apps/{APP}/appStoreVersions?filter[platform]=MAC_OS")["data"]
             if x["attributes"]["appStoreState"] != "READY_FOR_SALE")
    V, a = v["id"], v["attributes"]
    print("version", V, a["versionString"], a["appStoreState"], "copyright:", a["copyright"])
    todo = []
    if not a["copyright"]: todo.append("copyright line (version attribute)")
    locs = {l["attributes"]["locale"]: l for l in get(f"/v1/appStoreVersions/{V}/appStoreVersionLocalizations")["data"]}
    infos = {l["attributes"]["locale"]: l["attributes"] for l in get(f"/v1/appInfos/{INFO}/appInfoLocalizations")["data"]}
    for loc in LOCALES:
        l = locs.get(loc)
        if not l: todo.append(f"{loc}: no version localisation"); continue
        la = l["attributes"]
        sets = get(f"/v1/appStoreVersionLocalizations/{l['id']}/appScreenshotSets")["data"]
        n = sum(len(get(f"/v1/appScreenshotSets/{s['id']}/appScreenshots")["data"]) for s in sets)
        i = infos.get(loc, {})
        print(f"  {loc}: name={i.get('name')!r} subtitle={i.get('subtitle')!r} description={len(la['description'] or '')} chars, "
              f"keywords={bool(la['keywords'])} support={la['supportUrl']} privacy={i.get('privacyPolicyUrl')} screenshots={n}")
        for k in ("description", "keywords", "supportUrl"):
            if not la[k]: todo.append(f"{loc}: {k}")
        if not i.get("privacyPolicyUrl"): todo.append(f"{loc}: privacy policy URL")
        if n == 0: todo.append(f"{loc}: screenshots")
    cat = get(f"/v1/appInfos/{INFO}/primaryCategory")["data"]
    print("  primary category:", cat and cat["id"], " secondary:", (get(f"/v1/appInfos/{INFO}/secondaryCategory")["data"] or {}).get("id"))
    if not cat: todo.append("primary category")
    b = get(f"/v1/appStoreVersions/{V}/build")["data"]
    if b:
        ba = b["attributes"]
        print("  build:", b["id"], ba["version"], ba["processingState"], "usesNonExemptEncryption:", ba.get("usesNonExemptEncryption"))
        if ba.get("usesNonExemptEncryption") is None: todo.append("export compliance on the build")
    else:
        todo.append("build attached")
    r = get(f"/v1/appStoreVersions/{V}/appStoreReviewDetail")["data"]
    ra = r["attributes"] if r else {}
    print("  review notes:", len(ra.get("notes") or ""), "chars")
    for k in ("contactFirstName", "contactLastName", "contactPhone", "contactEmail"):
        if not ra.get(k): todo.append(f"review contact: {k} (owner, web UI)")
    age = get(f"/v1/appInfos/{INFO}/ageRatingDeclaration")["data"]["attributes"]
    if any(v is None for k, v in age.items() if k in ("violenceRealistic", "gambling", "unrestrictedWebAccess")): todo.append("age rating")
    ps = get(f"/v1/appPriceSchedules/{APP}/manualPrices?include=appPricePoint")
    print("  manual prices:", len(ps["data"]), [p["attributes"].get("customerPrice") for p in ps.get("included", [])])
    if not ps["data"]: todo.append("price")
    print("\nstill open:" if todo else "\nnothing open that the API can see")
    for t in todo: print("  -", t)
    print("  - App Privacy labels (web UI only)\n  - Submit for review (owner)")

if __name__ == "__main__":
    main()

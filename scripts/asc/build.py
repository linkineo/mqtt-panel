"""Attaches the newest VALID build of train ids.VERSION_STRING to the macOS App Store version. Never submits.
Run: python3 scripts/asc/build.py [build-id]"""
import sys
from asc import *
from ids import APP, VERSION_STRING, version_id

def main():
    V = version_id(get, post, patch)
    if len(sys.argv) > 1:
        bid = sys.argv[1]
    else:
        trains = [p["id"] for p in get(f"/v1/preReleaseVersions?filter[app]={APP}&filter[platform]=MAC_OS&filter[version]={VERSION_STRING}")["data"]]
        builds = [b for t in trains for b in get(f"/v1/preReleaseVersions/{t}/builds?limit=200")["data"]
                  if b["attributes"]["processingState"] == "VALID" and not b["attributes"]["expired"]]
        if not builds: sys.exit(f"no VALID build in train {VERSION_STRING} yet")
        bid = max(builds, key=lambda b: b["attributes"]["uploadedDate"])["id"]
    patch(f"/v1/appStoreVersions/{V}/relationships/build", {"data": {"type": "builds", "id": bid}})
    b = get(f"/v1/builds/{bid}")["data"]["attributes"]
    print("attached", bid, b["version"], b["processingState"], "usesNonExemptEncryption:", b.get("usesNonExemptEncryption"))

if __name__ == "__main__":
    main()

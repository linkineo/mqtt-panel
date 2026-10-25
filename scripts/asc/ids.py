"""App Store Connect identifiers of MQTT Panel (macOS). Not secret."""
APP = "6824290610"          # app id
INFO = "720a71da-b520-4b22-a151-9b18c15123a9"   # appInfos id (also the ageRatingDeclarations id)
VERSION_STRING = "1.0.0"
LOCALES = ["en-US", "fr-FR", "de-DE"]

def version_id(get, post, patch):
    """Returns the editable macOS appStoreVersion id; renames a PREPARE_FOR_SUBMISSION version to VERSION_STRING or creates one."""
    vs = get(f"/v1/apps/{APP}/appStoreVersions?filter[platform]=MAC_OS")["data"]
    for v in vs:
        if v["attributes"]["appStoreState"] in ("PREPARE_FOR_SUBMISSION", "DEVELOPER_REJECTED", "REJECTED", "METADATA_REJECTED"):
            if v["attributes"]["versionString"] != VERSION_STRING:
                patch(f"/v1/appStoreVersions/{v['id']}", {"data": {"type": "appStoreVersions", "id": v["id"],
                      "attributes": {"versionString": VERSION_STRING}}})
            return v["id"]
    return post("/v1/appStoreVersions", {"data": {"type": "appStoreVersions",
                "attributes": {"platform": "MAC_OS", "versionString": VERSION_STRING},
                "relationships": {"app": {"data": {"type": "apps", "id": APP}}}}})["data"]["id"]

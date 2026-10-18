# Signing material (local only)

Everything in this folder except this file is git-ignored. `scripts/release-mac-testflight.sh` fills it from
`.env.local` and `~/.appstoreconnect` before a build: the Mac App Store provisioning profile as
`embedded.provisionprofile`, and `Entitlements.plist` rendered from `../Entitlements.mas.plist` with the team identifier.
Provisioning profiles name the account holder, so they never enter the repository.

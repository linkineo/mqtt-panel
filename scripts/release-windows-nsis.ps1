<#
.SYNOPSIS
  Builds the Windows installer of MQTT Panel for the GitHub release. See docs/WINDOWS_BUILD.md, section 9.

.DESCRIPTION
  Tauri NSIS bundle with the overlay apps\desktop\src-tauri\tauri.nsis.conf.json: per-user install (no administrator
  rights), installer texts in English, French and German following the system language, WebView2 downloaded only when
  missing. The installer is not code-signed: Windows SmartScreen asks once before it runs.

  Like the macOS DMG, this variant carries the updater (docs/UPDATER.md): it is built with the cargo feature
  self-update, the installer is signed with the updater's minisign key (TAURI_SIGNING_PRIVATE_KEY_PATH and
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD from .env.local, see .env.example), and the windows-x86_64 entries are merged
  into the release's latest.json, keeping the macOS entries.

  Release assets, in target\release-nsis: MQTT-Panel-Windows-setup.exe, MQTT-Panel-Windows-setup.exe.sha256,
  MQTT-Panel-Windows-setup.exe.sig and latest.json (fixed names: the download links and the updater point at
  releases/latest/download). Upload them with gh release upload v<version> ... --clobber.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\release-windows-nsis.ps1

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\release-windows-nsis.ps1 -SkipBuild
#>
[CmdletBinding()]
param(
  [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location $Root

function Step([string]$text) { Write-Host "==> $text" -ForegroundColor Cyan }

function Invoke-Native([string]$exe, [string[]]$arguments) {
  & $exe @arguments
  if ($LASTEXITCODE -ne 0) { throw "$exe failed with exit code $LASTEXITCODE" }
}

$conf = Get-Content (Join-Path $Root 'apps\desktop\src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
$Version = $conf.version
$Src = Join-Path $Root "target\release\bundle\nsis\$($conf.productName)_${Version}_x64-setup.exe"
$Out = Join-Path $Root 'target\release-nsis'
$Asset = 'MQTT-Panel-Windows-setup.exe'   # fixed asset name: the download links point at releases/latest/download
$Overlay = Join-Path $Root 'apps\desktop\src-tauri\tauri.nsis.conf.json'   # absolute: npm runs the CLI in apps\desktop

# --- 0. Updater signing key (.env.local, the same key as the DMG) ----------------------------------------------
$envFile = Join-Path $Root '.env.local'
if (-not (Test-Path $envFile)) { throw 'missing .env.local (copy .env.example): the updater signing key is needed' }
$vars = @{}
foreach ($line in Get-Content $envFile) {
  if ($line -match '^\s*([A-Z_][A-Z0-9_]*)\s*=\s*(.*?)\s*$') { $vars[$Matches[1]] = $Matches[2].Trim('"') }
}
if (-not $vars.ContainsKey('TAURI_SIGNING_PRIVATE_KEY_PATH')) { throw '.env.local has no TAURI_SIGNING_PRIVATE_KEY_PATH' }
if (-not $vars.ContainsKey('TAURI_SIGNING_PRIVATE_KEY_PASSWORD')) { throw '.env.local has no TAURI_SIGNING_PRIVATE_KEY_PASSWORD' }
$keyPath = $vars['TAURI_SIGNING_PRIVATE_KEY_PATH'] -replace '^~', $HOME
if (-not (Test-Path $keyPath)) { throw "updater signing key not found: $keyPath" }
# The CLI signs the updater artifact with TAURI_SIGNING_PRIVATE_KEY (a key string or the path of the key file).
$env:TAURI_SIGNING_PRIVATE_KEY = (Resolve-Path $keyPath).Path
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $vars['TAURI_SIGNING_PRIVATE_KEY_PASSWORD']

# --- 1. Build ----------------------------------------------------------------------------------------------------
if (-not $SkipBuild) {
  Step "NSIS installer $Version with the updater (Tauri, overlay tauri.nsis.conf.json)"
  foreach ($f in @($Src, "$Src.sig")) { if (Test-Path $f) { Remove-Item $f -Force } }
  Invoke-Native 'npm.cmd' @('run', 'tauri', '--', 'build', '--bundles', 'nsis', '--features', 'self-update', '--config', $Overlay)
}
if (-not (Test-Path $Src)) { throw "No installer at $Src" }
if (-not (Test-Path "$Src.sig")) { throw "No updater signature at $Src.sig" }

# --- 2. Checks ---------------------------------------------------------------------------------------------------
Step 'Checks'
$exe = Get-ChildItem (Join-Path $Root 'target\release') -Filter '*.exe' |
  Where-Object { $_.Name -in @('mqtt-panel.exe', 'MQTT Panel.exe') } |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $exe) { throw 'No release executable under target\release' }
$fileVersion = (Get-Item $exe.FullName).VersionInfo.ProductVersion
if ($fileVersion -notlike "$Version*") { throw "Executable version '$fileVersion' does not match tauri.conf.json $Version" }
if (-not (Select-String -Path $exe.FullName -Pattern 'mqtt-panel.com/update/' -SimpleMatch -Quiet)) {
  throw 'The executable carries no updater endpoint; build with the self-update feature and the overlay'
}
Write-Host "    executable $($exe.Name) $fileVersion, updater on"

# --- 3. Assets ---------------------------------------------------------------------------------------------------
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$dest = Join-Path $Out $Asset
Copy-Item $Src $dest -Force
Copy-Item "$Src.sig" "$dest.sig" -Force
$utf8 = New-Object System.Text.UTF8Encoding($false)
$hash = (Get-FileHash $dest -Algorithm SHA256).Hash.ToLowerInvariant()
# Same form as the macOS .sha256 (shasum): "<hash>  <file name>", LF, no BOM.
[System.IO.File]::WriteAllText("$dest.sha256", "$hash  $Asset`n", $utf8)

# latest.json (docs/UPDATER.md): the plugin looks up windows-x86_64-nsis, then windows-x86_64. The macOS entries of
# the same version are kept, so either release script may run first.
Step 'latest.json'
$manifestPath = Join-Path $Out 'latest.json'
if (Test-Path $manifestPath) { Remove-Item $manifestPath -Force }
& gh release download "v$Version" --pattern latest.json --dir $Out 2>$null
if ($LASTEXITCODE -ne 0 -and (Test-Path $manifestPath)) { Remove-Item $manifestPath -Force }
$manifest = $null
if (Test-Path $manifestPath) {
  $manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
  if ($manifest.version -ne $Version) { Write-Host "    the release's latest.json is for $($manifest.version): starting a new one"; $manifest = $null }
}
if (-not $manifest) {
  $manifest = [pscustomobject]@{
    version   = $Version
    notes     = "Release notes: https://github.com/linkineo/mqtt-panel/releases/tag/v$Version"
    pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'")
    platforms = [pscustomobject]@{}
  }
}
$entry = [pscustomobject]@{
  signature = (Get-Content "$dest.sig" -Raw).Trim()
  url       = "https://github.com/linkineo/mqtt-panel/releases/download/v$Version/$Asset"
}
foreach ($key in @('windows-x86_64-nsis', 'windows-x86_64')) {
  $manifest.platforms | Add-Member -NotePropertyName $key -NotePropertyValue $entry -Force
}
[System.IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 5) + "`n", $utf8)
Write-Host "    platforms: $(($manifest.platforms.PSObject.Properties.Name) -join ', ')"

Write-Host "release assets in ${Out}:" -ForegroundColor Green
Get-ChildItem $Out | Where-Object { $_.Name -like "$Asset*" -or $_.Name -eq 'latest.json' } | Format-Table Name, Length -AutoSize

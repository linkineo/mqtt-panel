<#
.SYNOPSIS
  Builds the Microsoft Store package (MSIX) of MQTT Panel on a Windows machine. See docs/WINDOWS_BUILD.md.

.DESCRIPTION
  1. Runs scripts/check-forbidden-words.sh through Git Bash.
  2. Builds the release executable with the Tauri CLI (no Tauri bundle: the Store wants an MSIX, not MSI or NSIS).
  3. Lays out target\msix\package: mqtt-panel.exe, Assets\*.png, AppxManifest.xml filled from
     apps\desktop\windows\AppxManifest.xml.
  4. Runs makepri (resources.pri) and makeappx pack. The result, target\msix\MQTTPanel_<version>_x64.msix, is
     UNSIGNED: that is the file to upload to Partner Center, where the Store signs it.
  5. With -TestSign: signs a separate copy (..._test-signed.msix) with a self-signed certificate whose subject equals
     the Publisher, for local installation only. With -Install: installs that copy with Add-AppxPackage.

  Identity values are parameters (from Partner Center, "Product identity"); they are never written into the repository.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\build-msix.ps1 `
    -IdentityName "<Package/Identity/Name>" -Publisher "CN=<publisher id>" -PublisherDisplayName "<publisher display name>"

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\build-msix.ps1 -IdentityName ... -Publisher ... -PublisherDisplayName ... -SkipBuild -TestSign -Install
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$IdentityName,
  [Parameter(Mandatory = $true)][string]$Publisher,
  [Parameter(Mandatory = $true)][string]$PublisherDisplayName,
  [switch]$SkipBuild,
  [switch]$TestSign,
  [switch]$Install
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

function Find-SdkTool([string]$name) {
  $cmd = Get-Command $name -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
  $hit = Get-ChildItem $kits -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^10\.\d+\.\d+\.\d+$' } |
    Sort-Object { [version]$_.Name } -Descending |
    ForEach-Object { Join-Path $_.FullName "x64\$name" } |
    Where-Object { Test-Path $_ } |
    Select-Object -First 1
  if (-not $hit) { throw "$name not found: install the Windows SDK (docs/WINDOWS_BUILD.md, prerequisites)" }
  return $hit
}

function Find-GitBash {
  foreach ($p in @("$env:ProgramFiles\Git\bin\bash.exe", "${env:ProgramFiles(x86)}\Git\bin\bash.exe", "$env:LOCALAPPDATA\Programs\Git\bin\bash.exe")) {
    if ($p -and (Test-Path $p)) { return $p }
  }
  # Never fall back to C:\Windows\System32\bash.exe: that is WSL, not Git Bash.
  throw 'Git Bash not found: install Git for Windows (docs/WINDOWS_BUILD.md, prerequisites)'
}

# --- 0. Inputs ---------------------------------------------------------------------------------------------------
if ($Publisher -notmatch '^CN=') { throw 'Publisher must be the full subject from Partner Center, starting with "CN="' }
if ($Install -and -not $TestSign) { throw '-Install needs -TestSign (Windows installs only signed packages)' }

$conf = Get-Content (Join-Path $Root 'apps\desktop\src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
if ($conf.version -notmatch '^\d+\.\d+\.\d+$') { throw "tauri.conf.json version '$($conf.version)' is not MAJOR.MINOR.PATCH" }
$Version = "$($conf.version).0"   # the Store requires four parts with the last one 0

$Out = Join-Path $Root 'target\msix'
$Pkg = Join-Path $Out 'package'
$Msix = Join-Path $Out "MQTTPanel_${Version}_x64.msix"
$TestMsix = Join-Path $Out "MQTTPanel_${Version}_x64_test-signed.msix"

# --- 1. Guard ----------------------------------------------------------------------------------------------------
Step 'Forbidden-words guard'
$bash = Find-GitBash
Invoke-Native $bash @('scripts/check-forbidden-words.sh')

# --- 2. Build ----------------------------------------------------------------------------------------------------
if (-not $SkipBuild) {
  Step 'Release build (Tauri, no bundle)'
  Invoke-Native 'npm.cmd' @('run', 'tauri', '--', 'build', '--no-bundle')
}
$exe = @('mqtt-panel.exe', 'MQTT Panel.exe') |
  ForEach-Object { Join-Path $Root "target\release\$_" } |
  Where-Object { Test-Path $_ } |
  Sort-Object { (Get-Item $_).LastWriteTime } -Descending |
  Select-Object -First 1
if (-not $exe) { throw 'No release executable under target\release (expected mqtt-panel.exe or "MQTT Panel.exe")' }
Write-Host "    executable: $exe"

# --- 3. Layout ---------------------------------------------------------------------------------------------------
Step "Package layout in $Pkg"
if (Test-Path $Pkg) { Remove-Item $Pkg -Recurse -Force }
New-Item -ItemType Directory -Force -Path (Join-Path $Pkg 'Assets') | Out-Null
Copy-Item $exe (Join-Path $Pkg 'mqtt-panel.exe')
$icons = Join-Path $Root 'apps\desktop\src-tauri\icons'
foreach ($icon in @('StoreLogo.png', 'Square44x44Logo.png', 'Square71x71Logo.png', 'Square150x150Logo.png', 'Square310x310Logo.png')) {
  Copy-Item (Join-Path $icons $icon) (Join-Path $Pkg "Assets\$icon")
}
$escape = { param($s) [System.Security.SecurityElement]::Escape($s) }
$manifest = Get-Content (Join-Path $Root 'apps\desktop\windows\AppxManifest.xml') -Raw
$manifest = $manifest.Replace('{{IDENTITY_NAME}}', (& $escape $IdentityName))
$manifest = $manifest.Replace('{{PUBLISHER_DISPLAY_NAME}}', (& $escape $PublisherDisplayName))
$manifest = $manifest.Replace('{{PUBLISHER}}', (& $escape $Publisher))
$manifest = $manifest.Replace('{{VERSION}}', $Version)
if ($manifest -match '\{\{') { throw 'A placeholder is left in AppxManifest.xml' }
$null = [xml]$manifest   # fails on malformed XML
$manifestPath = Join-Path $Pkg 'AppxManifest.xml'
[System.IO.File]::WriteAllText($manifestPath, $manifest, (New-Object System.Text.UTF8Encoding($false)))

# --- 4. makepri and makeappx -------------------------------------------------------------------------------------
$makepri = Find-SdkTool 'makepri.exe'
$makeappx = Find-SdkTool 'makeappx.exe'
Step 'makepri'
$priConfig = Join-Path $Out 'priconfig.xml'
Invoke-Native $makepri @('createconfig', '/cf', $priConfig, '/dq', 'en-US_fr-FR_de-DE', '/pv', '10.0.0', '/o')
Invoke-Native $makepri @('new', '/pr', $Pkg, '/cf', $priConfig, '/mn', $manifestPath, '/of', (Join-Path $Pkg 'resources.pri'), '/o')
Step 'makeappx pack'
Invoke-Native $makeappx @('pack', '/d', $Pkg, '/p', $Msix, '/o')
Write-Host "    unsigned package for Partner Center: $Msix" -ForegroundColor Green

# --- 5. Local test signing and install ---------------------------------------------------------------------------
if ($TestSign) {
  Step 'Test signing (local only; never upload this copy)'
  $cert = Get-ChildItem Cert:\CurrentUser\My |
    Where-Object { $_.Subject -eq $Publisher -and $_.FriendlyName -eq 'MQTT Panel local test' -and $_.NotAfter -gt (Get-Date) } |
    Select-Object -First 1
  if (-not $cert) {
    $cert = New-SelfSignedCertificate -Type Custom -Subject $Publisher -KeyUsage DigitalSignature `
      -FriendlyName 'MQTT Panel local test' -CertStoreLocation 'Cert:\CurrentUser\My' `
      -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
  }
  $cer = Join-Path $Out 'test-signing.cer'
  Export-Certificate -Cert $cert -FilePath $cer | Out-Null
  Copy-Item $Msix $TestMsix -Force
  $signtool = Find-SdkTool 'signtool.exe'
  Invoke-Native $signtool @('sign', '/fd', 'SHA256', '/sha1', $cert.Thumbprint, '/s', 'My', $TestMsix)
  Write-Host "    test-signed copy: $TestMsix"
  $trusted = Get-ChildItem Cert:\LocalMachine\TrustedPeople | Where-Object { $_.Thumbprint -eq $cert.Thumbprint }
  if (-not $trusted) {
    $admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if ($admin) {
      Import-Certificate -FilePath $cer -CertStoreLocation 'Cert:\LocalMachine\TrustedPeople' | Out-Null
    } else {
      Write-Warning "Trust the test certificate once from an elevated PowerShell:`n  Import-Certificate -FilePath `"$cer`" -CertStoreLocation Cert:\LocalMachine\TrustedPeople"
      if ($Install) { throw 'Cannot install before the test certificate is trusted' }
    }
  }
}

if ($Install) {
  Step 'Add-AppxPackage'
  Add-AppxPackage -Path $TestMsix -ForceUpdateFromAnyVersion
  Write-Host '    installed; start "MQTT Panel" from the Start menu' -ForegroundColor Green
}

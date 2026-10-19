# Downloads the pinned pdfium build (bblanchon/pdfium-binaries) into vendor/pdfium and verifies its SHA-256.
$ErrorActionPreference = 'Stop'
$Tag = 'chromium/8066'
$Asset = 'pdfium-win-x64.tgz'
$Sha256 = '739a57d597d864297909cc40a2411eba728490c76a0fa25e3ea299c7f6b07020'

$Root = Split-Path -Parent $PSScriptRoot
$Vendor = Join-Path $Root 'vendor\pdfium'
$Lib = Join-Path $Vendor 'pdfium.dll'
if (Test-Path $Lib) { Write-Host "pdfium already present: $Lib"; exit 0 }

New-Item -ItemType Directory -Force $Vendor | Out-Null
$Archive = Join-Path $Vendor $Asset
Invoke-WebRequest -Uri "https://github.com/bblanchon/pdfium-binaries/releases/download/$Tag/$Asset" -OutFile $Archive
$Actual = (Get-FileHash -Algorithm SHA256 $Archive).Hash.ToLower()
if ($Actual -ne $Sha256) {
    Remove-Item $Archive
    throw "SHA-256 mismatch for ${Asset}: $Actual"
}
# Relative paths: GNU tar (e.g. from Git for Windows) treats `C:` as a remote host.
Push-Location $Vendor
try { tar -xzf $Asset; if ($LASTEXITCODE -ne 0) { throw 'tar extraction failed' } } finally { Pop-Location }
Copy-Item (Join-Path $Vendor 'bin\pdfium.dll') $Lib
Remove-Item $Archive
Write-Host "pdfium ready: $Lib"

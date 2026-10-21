<#
.SYNOPSIS
  Regenerates THIRD-PARTY-NOTICES.md (Rust crates, npm production packages, pdfium).

.DESCRIPTION
  Requirements: `cargo install cargo-about --locked --features cli`, Node.js, and an installed
  node_modules (npm ci) for the npm license texts. For the pdfium section, run
  scripts/fetch-pdfium.ps1 first so vendor/pdfium/{LICENSE,licenses} exist.
  Generation fails if a Rust crate uses a license that is not listed in scripts/licenses/about.toml.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/third-party-notices.ps1
  powershell -ExecutionPolicy Bypass -File scripts/third-party-notices.ps1 -NodeModules C:\path\to\node_modules
#>
param([string]$NodeModules = 'node_modules', [string]$PdfiumDir = 'vendor/pdfium')
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$utf8 = New-Object System.Text.UTF8Encoding($false)
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) 'omnimark-rust-notices.md'

cargo about generate -m src-tauri/Cargo.toml -c scripts/licenses/about.toml scripts/licenses/about.hbs -o $tmp
if ($LASTEXITCODE -ne 0) { throw 'cargo about failed' }
$npm = (node scripts/licenses/npm-notices.mjs $NodeModules) -join "`n"
if ($LASTEXITCODE -ne 0) { throw 'npm notices failed' }

$sb = New-Object System.Text.StringBuilder
[void]$sb.AppendLine([System.IO.File]::ReadAllText((Join-Path $root 'scripts/licenses/header.md'), $utf8).TrimEnd())
[void]$sb.AppendLine('')
[void]$sb.AppendLine([System.IO.File]::ReadAllText($tmp, $utf8).TrimEnd())
[void]$sb.AppendLine('')
[void]$sb.AppendLine($npm.TrimEnd())
[void]$sb.AppendLine('')
[void]$sb.AppendLine('## pdfium')
[void]$sb.AppendLine('')
[void]$sb.AppendLine("The app ships a prebuilt pdfium library from [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries) (MIT, below), which packages Google's [PDFium](https://pdfium.googlesource.com/pdfium/) (BSD-3-Clause) together with the third-party libraries whose licenses follow.")
[void]$sb.AppendLine('')
$files = @()
if (Test-Path "$PdfiumDir/LICENSE") { $files += Get-Item "$PdfiumDir/LICENSE" }
if (Test-Path "$PdfiumDir/licenses") { $files += Get-ChildItem "$PdfiumDir/licenses" -File | Sort-Object Name }
if ($files.Count -eq 0) { throw "pdfium licenses not found in $PdfiumDir (run scripts/fetch-pdfium.ps1)" }
foreach ($f in $files) {
  $title = if ($f.Name -eq 'LICENSE') { 'pdfium-binaries (MIT)' } else { $f.Name }
  [void]$sb.AppendLine("### $title")
  [void]$sb.AppendLine('')
  [void]$sb.AppendLine('```text')
  [void]$sb.AppendLine([System.IO.File]::ReadAllText($f.FullName, $utf8).Trim())
  [void]$sb.AppendLine('```')
  [void]$sb.AppendLine('')
}
[System.IO.File]::WriteAllText((Join-Path $root 'THIRD-PARTY-NOTICES.md'), ($sb.ToString() -replace "`r`n", "`n"), $utf8)
Write-Host 'Wrote THIRD-PARTY-NOTICES.md'

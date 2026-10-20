# Optional (Windows only): puts a project-local Tesseract into vendor/tesseract for the OCR tests.
# macOS/Linux: use the system package (brew install tesseract tesseract-lang / apt-get install tesseract-ocr tesseract-ocr-tur).
# The installer is run silently into a temporary folder, only the needed files are copied, then it is removed.
$ErrorActionPreference = 'Stop'
$SetupUrl = 'https://github.com/UB-Mannheim/tesseract/releases/download/v5.4.0.20240606/tesseract-ocr-w64-setup-5.4.0.20240606.exe'
$SetupSize = 50175248
$SetupSha256 = 'C885FFF6998E0608BA4BB8AB51436E1C6775C2BAFC2559A19B423E18678B60C9'
$EngSha256 = '7D4322BD2A7749724879683FC3912CB542F19906C83BCC1A52132556427170B2'
$TurUrl = 'https://github.com/tesseract-ocr/tessdata/raw/ced78752cc61322fb554c280d13360b35b8684e4/tur.traineddata'
$TurSize = 18750612
$TurSha256 = '489B9504E80D7184ED1AC9A1976647884EE71149DA231FF3C2C1DC15370F2F3D'
$Dlls = @(
    'libLerc.dll', 'libarchive-13.dll', 'libb2-1.dll', 'libbz2-1.dll', 'libcrypto-3-x64.dll', 'libdeflate.dll',
    'libexpat-1.dll', 'libgcc_s_seh-1.dll', 'libgif-7.dll', 'libiconv-2.dll', 'libjbig-0.dll', 'libjpeg-8.dll',
    'libleptonica-6.dll', 'liblz4.dll', 'liblzma-5.dll', 'libopenjp2-7.dll', 'libpng16-16.dll', 'libsharpyuv-0.dll',
    'libstdc++-6.dll', 'libtesseract-5.dll', 'libtiff-6.dll', 'libwebp-7.dll', 'libwebpmux-3.dll',
    'libwinpthread-1.dll', 'libzstd.dll', 'zlib1.dll'
)

$Root = Split-Path -Parent $PSScriptRoot
$Vendor = Join-Path $Root 'vendor\tesseract'
$Exe = Join-Path $Vendor 'tesseract.exe'
$ProgramFilesDir = Join-Path $env:ProgramFiles 'Tesseract-OCR'

function Get-Langs($TesseractExe) {
    # stdout, or stderr when stdout is empty (old versions); no `2>&1` (it raises under 'Stop' on PS 5.1).
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $TesseractExe
    $psi.Arguments = '--list-langs'
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $proc = [System.Diagnostics.Process]::Start($psi)
    $errTask = $proc.StandardError.ReadToEndAsync()
    $out = $proc.StandardOutput.ReadToEnd()
    $err = $errTask.Result
    $proc.WaitForExit()
    $text = if ($out.Trim()) { $out } else { $err }
    @($text -split "\r?\n" | ForEach-Object { $_.Trim() } | Where-Object { $_ -and $_ -notlike 'List of available languages*' })
}

function Assert-Langs($TesseractExe) {
    $langs = Get-Langs $TesseractExe
    foreach ($code in 'eng', 'osd', 'tur') {
        if ($langs -notcontains $code) { throw "language '$code' missing from --list-langs of ${TesseractExe}: $($langs -join ', ')" }
    }
}

if (Test-Path $Exe) {
    Assert-Langs $Exe   # an incomplete folder fails here instead of being accepted
    Write-Host "tesseract already present: $Exe"
    exit 0
}
if (Test-Path $Vendor) { throw "$Vendor exists but has no tesseract.exe; delete it and run again" }

function Get-Verified($Url, $Path, $Size, $Sha256) {
    Invoke-WebRequest -Uri $Url -OutFile $Path
    $actual = (Get-FileHash -Algorithm SHA256 $Path).Hash
    if ((Get-Item $Path).Length -ne $Size -or $actual -ne $Sha256) {
        Remove-Item $Path
        throw "Size or SHA-256 mismatch for ${Url}: $actual"
    }
}

function Assert-Hash($Path, $Sha256) {
    $actual = (Get-FileHash -Algorithm SHA256 $Path).Hash
    if ($actual -ne $Sha256) { throw "SHA-256 mismatch for ${Path}: $actual" }
}

# Snapshot of what the installer must not change.
$PathUserBefore = [Environment]::GetEnvironmentVariable('Path', 'User')
$PathMachineBefore = [Environment]::GetEnvironmentVariable('Path', 'Machine')
$ProgramFilesBefore = Test-Path $ProgramFilesDir

$Work = Join-Path ([IO.Path]::GetTempPath()) ("omnimark-tesseract-" + [Guid]::NewGuid().ToString('N'))
$Install = Join-Path $Work 'install'
$Stage = Join-Path $Work 'stage'
New-Item -ItemType Directory -Force $Work | Out-Null
try {
    try {
        # The Mannheim certificate expired on 2023-12-10, so the signature is not checked; SHA-256 is what counts.
        $Setup = Join-Path $Work 'setup.exe'
        Get-Verified $SetupUrl $Setup $SetupSize $SetupSha256
        # `/D=` must be last and unquoted; Start-Process -ArgumentList would quote it and install to C:\Program Files.
        cmd /c "start `"`" /wait `"$Setup`" /S /D=$Install"
        if (-not (Test-Path (Join-Path $Install 'tesseract.exe'))) { throw 'silent install did not produce tesseract.exe' }

        # Everything is staged and verified under $Work first.
        New-Item -ItemType Directory -Force (Join-Path $Stage 'tessdata') | Out-Null
        Copy-Item (Join-Path $Install 'tesseract.exe') $Stage
        foreach ($dll in $Dlls) { Copy-Item (Join-Path $Install $dll) $Stage }
        $Eng = Join-Path $Install 'tessdata\eng.traineddata'
        Assert-Hash $Eng $EngSha256
        Copy-Item $Eng (Join-Path $Stage 'tessdata')
        Copy-Item (Join-Path $Install 'tessdata\osd.traineddata') (Join-Path $Stage 'tessdata')
        $Configs = Join-Path $Install 'tessdata\configs'
        if (Test-Path $Configs) { Copy-Item $Configs (Join-Path $Stage 'tessdata') -Recurse }
        Get-Verified $TurUrl (Join-Path $Stage 'tessdata\tur.traineddata') $TurSize $TurSha256
        Assert-Langs (Join-Path $Stage 'tesseract.exe')
    } finally {
        $Uninstall = Join-Path $Install 'tesseract-uninstall.exe'
        if (Test-Path $Uninstall) { cmd /c "start `"`" /wait `"$Uninstall`" /S" }
        # If the installer ignored /D, remove the system-wide copy it created.
        $SystemUninstall = Join-Path $ProgramFilesDir 'tesseract-uninstall.exe'
        if ((-not $ProgramFilesBefore) -and (Test-Path $SystemUninstall)) { cmd /c "start `"`" /wait `"$SystemUninstall`" /S" }
    }

    $Leftovers = @()
    if ((-not $ProgramFilesBefore) -and (Test-Path $ProgramFilesDir)) { $Leftovers += "folder $ProgramFilesDir" }
    if ([Environment]::GetEnvironmentVariable('Path', 'User') -ne $PathUserBefore) { $Leftovers += 'changed User PATH' }
    if ([Environment]::GetEnvironmentVariable('Path', 'Machine') -ne $PathMachineBefore) { $Leftovers += 'changed Machine PATH' }
    if ($Leftovers.Count -gt 0) { throw "The installer left system-wide changes behind: $($Leftovers -join '; '). Remove them manually." }

    # Publish in one step: copy next to the target, then rename.
    $Partial = Join-Path $Root 'vendor\tesseract.partial'
    if (Test-Path $Partial) { Remove-Item -Recurse -Force $Partial }
    New-Item -ItemType Directory -Force (Join-Path $Root 'vendor') | Out-Null
    try {
        Copy-Item $Stage $Partial -Recurse
        Rename-Item $Partial 'tesseract'
    } catch {
        if (Test-Path $Partial) { Remove-Item -Recurse -Force $Partial }
        throw
    }
} finally {
    Start-Sleep -Seconds 2
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}

Assert-Langs $Exe
Write-Host "tesseract ready: $Exe"

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
if (Test-Path $Exe) { Write-Host "tesseract already present: $Exe"; exit 0 }

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

$Work = Join-Path ([IO.Path]::GetTempPath()) ("omnimark-tesseract-" + [Guid]::NewGuid().ToString('N'))
$Install = Join-Path $Work 'install'
New-Item -ItemType Directory -Force $Work | Out-Null
try {
    # The Mannheim certificate expired on 2023-12-10, so the signature is not checked; SHA-256 is what counts.
    $Setup = Join-Path $Work 'setup.exe'
    Get-Verified $SetupUrl $Setup $SetupSize $SetupSha256
    # `/D=` must be last and unquoted; Start-Process -ArgumentList would quote it and install to C:\Program Files.
    cmd /c "start `"`" /wait `"$Setup`" /S /D=$Install"
    if (-not (Test-Path (Join-Path $Install 'tesseract.exe'))) { throw 'silent install did not produce tesseract.exe' }

    New-Item -ItemType Directory -Force (Join-Path $Vendor 'tessdata') | Out-Null
    Copy-Item (Join-Path $Install 'tesseract.exe') $Vendor
    foreach ($dll in $Dlls) { Copy-Item (Join-Path $Install $dll) $Vendor }
    $Eng = Join-Path $Install 'tessdata\eng.traineddata'
    Assert-Hash $Eng $EngSha256
    Copy-Item $Eng (Join-Path $Vendor 'tessdata')
    Copy-Item (Join-Path $Install 'tessdata\osd.traineddata') (Join-Path $Vendor 'tessdata')
    $Configs = Join-Path $Install 'tessdata\configs'
    if (Test-Path $Configs) { Copy-Item $Configs (Join-Path $Vendor 'tessdata') -Recurse }
    Get-Verified $TurUrl (Join-Path $Vendor 'tessdata\tur.traineddata') $TurSize $TurSha256

    $Uninstall = Join-Path $Install 'tesseract-uninstall.exe'
    if (Test-Path $Uninstall) { cmd /c "start `"`" /wait `"$Uninstall`" /S" }
} finally {
    Start-Sleep -Seconds 2
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}

$langs = (& $Exe --list-langs 2>&1) -join ' '
foreach ($code in 'eng', 'osd', 'tur') {
    if ($langs -notmatch "\b$code\b") { throw "language '$code' missing from --list-langs: $langs" }
}
Write-Host "tesseract ready: $Exe"

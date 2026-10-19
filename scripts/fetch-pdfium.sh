#!/usr/bin/env bash
# Downloads the pinned pdfium build (bblanchon/pdfium-binaries) into vendor/pdfium and verifies its SHA-256.
set -euo pipefail
TAG="chromium/8066"
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  ASSET=pdfium-mac-arm64.tgz;  SHA=336219e80580b93c6523f44db7dc1de59cc497b13a7390ddac84223f68ca162b; LIB=libpdfium.dylib ;;
  Darwin-x86_64) ASSET=pdfium-mac-x64.tgz;    SHA=841ecac278cdd46288dd065873522cf72f3996560d8978f473d336f01d59942c; LIB=libpdfium.dylib ;;
  Linux-x86_64)  ASSET=pdfium-linux-x64.tgz;  SHA=0b43f405477cf2cfc4dbff06905093c3309756c6bca1fb9da99234a2ca97fed2; LIB=libpdfium.so ;;
  *) echo "unsupported platform: $(uname -s)-$(uname -m) (Windows: use fetch-pdfium.ps1)" >&2; exit 1 ;;
esac
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VENDOR="$ROOT/vendor/pdfium"
if [ -f "$VENDOR/$LIB" ]; then echo "pdfium already present: $VENDOR/$LIB"; exit 0; fi
mkdir -p "$VENDOR"
curl -fsSL -o "$VENDOR/$ASSET" "https://github.com/bblanchon/pdfium-binaries/releases/download/$TAG/$ASSET"
if command -v sha256sum >/dev/null; then ACTUAL=$(sha256sum "$VENDOR/$ASSET" | cut -d' ' -f1); else ACTUAL=$(shasum -a 256 "$VENDOR/$ASSET" | cut -d' ' -f1); fi
if [ "$ACTUAL" != "$SHA" ]; then rm -f "$VENDOR/$ASSET"; echo "SHA-256 mismatch for $ASSET: $ACTUAL" >&2; exit 1; fi
tar -xzf "$VENDOR/$ASSET" -C "$VENDOR"
cp "$VENDOR/lib/$LIB" "$VENDOR/$LIB"
rm "$VENDOR/$ASSET"
echo "pdfium ready: $VENDOR/$LIB"

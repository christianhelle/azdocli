#!/bin/bash
set -e

VERSION="$1"
WIN_X64_SHA="$2"
WIN_ARM64_SHA="$3"

if [ -z "$VERSION" ] || [ -z "$WIN_X64_SHA" ] || [ -z "$WIN_ARM64_SHA" ]; then
    echo "Usage: $0 <version> <win-x64-sha256> <win-arm64-sha256>"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$REPO_ROOT/dist/scoop"
OUTPUT_DIR="${OUTPUT_DIR:-$REPO_ROOT/output}/scoop"
MANIFEST="$OUTPUT_DIR/azdocli.json"

mkdir -p "$OUTPUT_DIR"

# Create Scoop manifest
cp "$DIST_DIR/azdocli.json" "$MANIFEST"
sed -i.bak "s/VERSION_PLACEHOLDER/$VERSION/g" "$MANIFEST"
rm -f "$MANIFEST".bak
sed -i.bak "s/SHA256_X64_PLACEHOLDER/$WIN_X64_SHA/g" "$MANIFEST"
rm -f "$MANIFEST".bak
sed -i.bak "s/SHA256_ARM64_PLACEHOLDER/$WIN_ARM64_SHA/g" "$MANIFEST"
rm -f "$MANIFEST".bak

echo "Scoop manifest created: $MANIFEST"
cat "$MANIFEST"

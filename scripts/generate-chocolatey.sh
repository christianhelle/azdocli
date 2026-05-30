#!/bin/bash
set -e

VERSION="$1"
WIN_X64_SHA="$2"

if [ -z "$VERSION" ] || [ -z "$WIN_X64_SHA" ]; then
    echo "Usage: $0 <version> <win-x64-sha256>"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$REPO_ROOT/dist/chocolatey"
OUTPUT_DIR="${OUTPUT_DIR:-$REPO_ROOT/output}/chocolatey"

rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"

# Create Chocolatey package
cp -r "$DIST_DIR/." "$OUTPUT_DIR/"

# Update version and SHA256
sed -i "s/VERSION_PLACEHOLDER/$VERSION/g" "$OUTPUT_DIR/azdocli.nuspec"
sed -i "s/VERSION_PLACEHOLDER/$VERSION/g" "$OUTPUT_DIR/tools/chocolateyinstall.ps1"
sed -i "s/SHA256_X64_PLACEHOLDER/$WIN_X64_SHA/g" "$OUTPUT_DIR/tools/chocolateyinstall.ps1"

echo "Chocolatey package created in $OUTPUT_DIR/"
ls -la "$OUTPUT_DIR/"

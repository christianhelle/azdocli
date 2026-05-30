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
DIST_DIR="$REPO_ROOT/dist/winget"
OUTPUT_DIR="${OUTPUT_DIR:-$REPO_ROOT/output}/winget-$VERSION"

rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"

# Create WinGet manifests
cp "$DIST_DIR"/*.yaml "$OUTPUT_DIR/"

# Update version and SHA256 in all manifests
for file in "$OUTPUT_DIR"/*.yaml; do
    sed -i "s/VERSION_PLACEHOLDER/$VERSION/g" "$file"
    sed -i "s/SHA256_X64_PLACEHOLDER/$WIN_X64_SHA/g" "$file"
    sed -i "s/SHA256_ARM64_PLACEHOLDER/$WIN_ARM64_SHA/g" "$file"
done

echo "WinGet manifests created in $OUTPUT_DIR/"
ls -la "$OUTPUT_DIR/"

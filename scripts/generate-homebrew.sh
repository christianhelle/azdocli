#!/bin/bash
set -e

VERSION="$1"
MACOS_X64_SHA="$2"
MACOS_ARM64_SHA="$3"

if [ -z "$VERSION" ] || [ -z "$MACOS_X64_SHA" ] || [ -z "$MACOS_ARM64_SHA" ]; then
    echo "Usage: $0 <version> <macos-x64-sha256> <macos-arm64-sha256>"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$REPO_ROOT/dist/homebrew"
OUTPUT_DIR="${OUTPUT_DIR:-$REPO_ROOT/output}/homebrew"

mkdir -p "$OUTPUT_DIR"

# Create Homebrew formula
cp "$DIST_DIR/azdocli.rb" "$OUTPUT_DIR/azdocli-$VERSION.rb"
sed -i "s/VERSION_PLACEHOLDER/$VERSION/g" "$OUTPUT_DIR/azdocli-$VERSION.rb"
sed -i "s/SHA256_X64_PLACEHOLDER/$MACOS_X64_SHA/g" "$OUTPUT_DIR/azdocli-$VERSION.rb"
sed -i "s/SHA256_ARM64_PLACEHOLDER/$MACOS_ARM64_SHA/g" "$OUTPUT_DIR/azdocli-$VERSION.rb"

echo "Homebrew formula created: $OUTPUT_DIR/azdocli-$VERSION.rb"
cat "$OUTPUT_DIR/azdocli-$VERSION.rb"

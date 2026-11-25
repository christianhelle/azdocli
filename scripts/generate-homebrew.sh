#!/bin/bash
set -e

VERSION="$1"
MACOS_X64_SHA="$2"
MACOS_ARM64_SHA="$3"
LINUX_X64_SHA="$4"
LINUX_ARM64_SHA="$5"

if [ -z "$VERSION" ] || [ -z "$MACOS_X64_SHA" ] || [ -z "$MACOS_ARM64_SHA" ] || [ -z "$LINUX_X64_SHA" ] || [ -z "$LINUX_ARM64_SHA" ]; then
    echo "Usage: $0 <version> <macos-x64-sha256> <macos-arm64-sha256> <linux-x64-sha256> <linux-arm64-sha256>"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$REPO_ROOT/dist/homebrew"
OUTPUT_DIR="${OUTPUT_DIR:-$REPO_ROOT/output}/homebrew"
FORMULA="$OUTPUT_DIR/azdocli.rb"

mkdir -p "$OUTPUT_DIR"

# Create Homebrew formula (file name must match the class name)
cp "$DIST_DIR/azdocli.rb" "$FORMULA"
sed -i.bak "s/VERSION_PLACEHOLDER/$VERSION/g" "$FORMULA"
rm -f "$FORMULA".bak
sed -i.bak "s/SHA256_MACOS_X64_PLACEHOLDER/$MACOS_X64_SHA/g" "$FORMULA"
rm -f "$FORMULA".bak
sed -i.bak "s/SHA256_MACOS_ARM64_PLACEHOLDER/$MACOS_ARM64_SHA/g" "$FORMULA"
rm -f "$FORMULA".bak
sed -i.bak "s/SHA256_LINUX_X64_PLACEHOLDER/$LINUX_X64_SHA/g" "$FORMULA"
rm -f "$FORMULA".bak
sed -i.bak "s/SHA256_LINUX_ARM64_PLACEHOLDER/$LINUX_ARM64_SHA/g" "$FORMULA"
rm -f "$FORMULA".bak

echo "Homebrew formula created: $FORMULA"
cat "$FORMULA"

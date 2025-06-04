#!/bin/bash
set -e

VERSION="$1"
MACOS_X64_SHA="$2"
MACOS_ARM64_SHA="$3"

if [ -z "$VERSION" ] || [ -z "$MACOS_X64_SHA" ] || [ -z "$MACOS_ARM64_SHA" ]; then
    echo "Usage: $0 <version> <macos-x64-sha256> <macos-arm64-sha256>"
    exit 1
fi

# Create Homebrew formula
cp homebrew/azdocli.rb homebrew/azdocli-$VERSION.rb
sed -i "s/VERSION_PLACEHOLDER/$VERSION/g" homebrew/azdocli-$VERSION.rb
sed -i "s/SHA256_X64_PLACEHOLDER/$MACOS_X64_SHA/g" homebrew/azdocli-$VERSION.rb
sed -i "s/SHA256_ARM64_PLACEHOLDER/$MACOS_ARM64_SHA/g" homebrew/azdocli-$VERSION.rb

echo "Homebrew formula created: homebrew/azdocli-$VERSION.rb"
cat homebrew/azdocli-$VERSION.rb
#!/bin/bash
set -e

VERSION="$1"
WIN_X64_SHA="$2"
WIN_ARM64_SHA="$3"

if [ -z "$VERSION" ] || [ -z "$WIN_X64_SHA" ] || [ -z "$WIN_ARM64_SHA" ]; then
    echo "Usage: $0 <version> <win-x64-sha256> <win-arm64-sha256>"
    exit 1
fi

# Create WinGet manifests
mkdir -p winget-$VERSION
cp winget/*.yaml winget-$VERSION/

# Update version and SHA256 in all manifests
for file in winget-$VERSION/*.yaml; do
    sed -i "s/VERSION_PLACEHOLDER/$VERSION/g" "$file"
    sed -i "s/SHA256_X64_PLACEHOLDER/$WIN_X64_SHA/g" "$file"
    sed -i "s/SHA256_ARM64_PLACEHOLDER/$WIN_ARM64_SHA/g" "$file"
done

echo "WinGet manifests created in winget-$VERSION/"
ls -la winget-$VERSION/
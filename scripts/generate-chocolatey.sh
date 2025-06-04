#!/bin/bash
set -e

VERSION="$1"
WIN_X64_SHA="$2"

if [ -z "$VERSION" ] || [ -z "$WIN_X64_SHA" ]; then
    echo "Usage: $0 <version> <win-x64-sha256>"
    exit 1
fi

# Create Chocolatey package
mkdir -p chocolatey-package-$VERSION
cp -r chocolatey/* chocolatey-package-$VERSION/

# Update version and SHA256
sed -i "s/VERSION_PLACEHOLDER/$VERSION/g" chocolatey-package-$VERSION/azdocli.nuspec
sed -i "s/VERSION_PLACEHOLDER/$VERSION/g" chocolatey-package-$VERSION/tools/chocolateyinstall.ps1
sed -i "s/SHA256_X64_PLACEHOLDER/$WIN_X64_SHA/g" chocolatey-package-$VERSION/tools/chocolateyinstall.ps1

echo "Chocolatey package created in chocolatey-package-$VERSION/"
ls -la chocolatey-package-$VERSION/
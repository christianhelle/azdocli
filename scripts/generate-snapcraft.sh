#!/bin/bash
set -e

VERSION="$1"

if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    exit 1
fi

# Create Snapcraft package
mkdir -p snap-package-$VERSION
cp -r snap/* snap-package-$VERSION/

# Update version
sed -i "s/version: git/version: '$VERSION'/g" snap-package-$VERSION/snapcraft.yaml

echo "Snapcraft package created in snap-package-$VERSION/"
cat snap-package-$VERSION/snapcraft.yaml
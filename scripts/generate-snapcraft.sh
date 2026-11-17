#!/bin/bash
set -e

VERSION="$1"

if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SOURCE_FILE="$REPO_ROOT/snapcraft.yaml"
OUTPUT_DIR="${OUTPUT_DIR:-$REPO_ROOT/output}/snap"

rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"

# Copy snapcraft.yaml from the repo root
cp "$SOURCE_FILE" "$OUTPUT_DIR/snapcraft.yaml"

# Update version (handles quoted and unquoted values)
sed -i -E "s/^version:.*/version: '$VERSION'/" "$OUTPUT_DIR/snapcraft.yaml"

if ! grep -q "^version: '$VERSION'$" "$OUTPUT_DIR/snapcraft.yaml"; then
    echo "Failed to set version in $OUTPUT_DIR/snapcraft.yaml" >&2
    exit 1
fi

echo "Snapcraft config created in $OUTPUT_DIR/"
cat "$OUTPUT_DIR/snapcraft.yaml"

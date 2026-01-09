#!/usr/bin/env bash
#
# print-artifacts.sh - List build artifacts with sizes
#
# Usage: ./scripts/print-artifacts.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUNDLE_DIR="$PROJECT_ROOT/src-tauri/target/release/bundle"

echo "=============================================="
echo "CADalytix Installer - Build Artifacts"
echo "=============================================="
echo ""

if [ ! -d "$BUNDLE_DIR" ]; then
    echo "Bundle directory not found: $BUNDLE_DIR"
    echo "Run 'cargo tauri build' first."
    exit 1
fi

echo "Artifacts in: $BUNDLE_DIR"
echo ""

# List all files with human-readable sizes
find "$BUNDLE_DIR" -type f \( -name "*.deb" -o -name "*.rpm" -o -name "*.AppImage" -o -name "*.dmg" -o -name "*.msi" -o -name "*.exe" \) -exec ls -lh {} \; 2>/dev/null | awk '{print $5, $9}'

echo ""
echo "Full listing:"
find "$BUNDLE_DIR" -type f -exec ls -lh {} \; 2>/dev/null | awk '{print $5, $9}' | sort -k2

echo ""
echo "SHA256 checksums:"
find "$BUNDLE_DIR" -type f \( -name "*.deb" -o -name "*.rpm" -o -name "*.AppImage" -o -name "*.msi" -o -name "*.exe" \) -print0 \
  | xargs -0 sha256sum 2>/dev/null || true

echo ""
echo "Done."


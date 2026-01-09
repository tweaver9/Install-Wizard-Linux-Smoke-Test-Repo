#!/usr/bin/env bash
#
# build-linux.sh - Build CADalytix Installer for Linux
#
# Usage: ./scripts/build-linux.sh
#
# This script prepares the frontend build and prints instructions
# for the final Tauri build step.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=============================================="
echo "CADalytix Installer - Linux Build"
echo "=============================================="
echo ""

# Print environment versions
echo "Environment:"
echo "  rustc:  $(rustc --version 2>/dev/null || echo 'NOT FOUND')"
echo "  cargo:  $(cargo --version 2>/dev/null || echo 'NOT FOUND')"
echo "  node:   $(node --version 2>/dev/null || echo 'NOT FOUND')"
echo "  npm:    $(npm --version 2>/dev/null || echo 'NOT FOUND')"
echo ""

# Check for required tools
if ! command -v rustc &> /dev/null; then
    echo "ERROR: rustc not found. Install Rust: curl https://sh.rustup.rs -sSf | sh"
    exit 1
fi

if ! command -v node &> /dev/null; then
    echo "ERROR: node not found. Install Node.js 18+"
    exit 1
fi

# Build frontend
echo "Building frontend..."
cd "$PROJECT_ROOT/frontend"
if [ -f package-lock.json ]; then
  npm ci
else
  npm install
fi
npm run build
echo "✓ Frontend build complete"
echo ""

# Print next steps
echo "=============================================="
echo "Next: Run the Tauri build"
echo "=============================================="
echo ""
echo "  cd $PROJECT_ROOT/src-tauri"
echo "  cargo tauri build"
echo ""
echo "Expected bundle outputs:"
echo "  src-tauri/target/release/bundle/deb/*.deb"
echo "  src-tauri/target/release/bundle/rpm/*.rpm"
echo "  src-tauri/target/release/bundle/appimage/*.AppImage"
echo ""


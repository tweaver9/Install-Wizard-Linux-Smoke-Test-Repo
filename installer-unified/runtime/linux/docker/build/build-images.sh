#!/usr/bin/env bash
# CADalytix Docker Image Build Script
# Builds Docker images and exports them as .tar files for offline installation.
#
# Usage:
#   ./build-images.sh [--push] [--tag VERSION]
#
# Options:
#   --push      Push images to registry after building
#   --tag       Specify version tag (default: latest)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUNTIME_DIR="$(dirname "$SCRIPT_DIR")"
IMAGES_DIR="$RUNTIME_DIR/images"
SOURCE_DIR="${CADALYTIX_SOURCE_DIR:-$(dirname "$(dirname "$(dirname "$(dirname "$RUNTIME_DIR")")")")"

# Configuration
REGISTRY="${CADALYTIX_REGISTRY:-cadalytix}"
VERSION="${1:-latest}"
PUSH=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --push)
            PUSH=true
            shift
            ;;
        --tag)
            VERSION="$2"
            shift 2
            ;;
        *)
            shift
            ;;
    esac
done

echo "=============================================="
echo "CADalytix Docker Image Build"
echo "=============================================="
echo "Registry:    $REGISTRY"
echo "Version:     $VERSION"
echo "Push:        $PUSH"
echo "Source Dir:  $SOURCE_DIR"
echo "Output Dir:  $IMAGES_DIR"
echo "=============================================="

# Ensure output directory exists
mkdir -p "$IMAGES_DIR"

# Build web image
echo ""
echo "[1/4] Building web image..."
docker build \
    -f "$SCRIPT_DIR/Dockerfile.web" \
    -t "$REGISTRY/web:$VERSION" \
    -t "$REGISTRY/web:latest" \
    "$SOURCE_DIR"

# Build worker image
echo ""
echo "[2/4] Building worker image..."
docker build \
    -f "$SCRIPT_DIR/Dockerfile.worker" \
    -t "$REGISTRY/worker:$VERSION" \
    -t "$REGISTRY/worker:latest" \
    "$SOURCE_DIR"

# Export images to tar files
echo ""
echo "[3/4] Exporting images to tar files..."

echo "  - Saving web image..."
docker save "$REGISTRY/web:$VERSION" -o "$IMAGES_DIR/cadalytix-web-$VERSION.tar"

echo "  - Saving worker image..."
docker save "$REGISTRY/worker:$VERSION" -o "$IMAGES_DIR/cadalytix-worker-$VERSION.tar"

# Calculate checksums
echo ""
echo "[4/4] Generating checksums..."
cd "$IMAGES_DIR"
sha256sum *.tar > SHA256SUMS.txt
cat SHA256SUMS.txt

# Push if requested
if [ "$PUSH" = true ]; then
    echo ""
    echo "Pushing images to registry..."
    docker push "$REGISTRY/web:$VERSION"
    docker push "$REGISTRY/web:latest"
    docker push "$REGISTRY/worker:$VERSION"
    docker push "$REGISTRY/worker:latest"
fi

echo ""
echo "=============================================="
echo "Build complete!"
echo "=============================================="
echo "Images exported to: $IMAGES_DIR"
ls -lh "$IMAGES_DIR"


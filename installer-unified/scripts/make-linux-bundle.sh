#!/usr/bin/env bash
#
# make-linux-bundle.sh — Assemble LINUX_BUNDLE from build outputs
# Run from installer-unified/ directory
#
set -euo pipefail

# ─────────────────────────────────────────────────────────────────────────────
# Configuration
# ─────────────────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BUNDLE_DIR="${PROJECT_ROOT}/LINUX_BUNDLE"
DIST_DIR="${PROJECT_ROOT}/dist"

# Build output locations (try both possible paths)
BUILD_OUTPUT_1="${PROJECT_ROOT}/target/release/bundle"
BUILD_OUTPUT_2="${PROJECT_ROOT}/src-tauri/target/release/bundle"

# ─────────────────────────────────────────────────────────────────────────────
# Helper Functions
# ─────────────────────────────────────────────────────────────────────────────
log() { echo "[make-linux-bundle] $*"; }
error() { echo "[ERROR] $*" >&2; exit 1; }

get_version() {
    local version=""
    
    # Try command line argument first
    if [[ -n "${1:-}" ]]; then
        version="$1"
    # Try tauri.conf.json
    elif [[ -f "${PROJECT_ROOT}/src-tauri/tauri.conf.json" ]]; then
        version=$(grep -o '"version": *"[^"]*"' "${PROJECT_ROOT}/src-tauri/tauri.conf.json" | head -1 | cut -d'"' -f4)
    fi
    
    if [[ -z "${version}" ]]; then
        version="0.0.0"
    fi
    
    echo "${version}"
}

find_build_output() {
    if [[ -d "${BUILD_OUTPUT_1}" ]]; then
        echo "${BUILD_OUTPUT_1}"
    elif [[ -d "${BUILD_OUTPUT_2}" ]]; then
        echo "${BUILD_OUTPUT_2}"
    else
        error "No build output found. Run 'cargo tauri build' first."
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# Main
# ─────────────────────────────────────────────────────────────────────────────
main() {
    local version
    version=$(get_version "${1:-}")
    log "Building Linux bundle version ${version}"
    
    local build_output
    build_output=$(find_build_output)
    log "Using build output from: ${build_output}"
    
    # Clean and prepare directories
    log "Preparing bundle directories..."
    rm -rf "${BUNDLE_DIR}/artifacts"/*
    rm -rf "${BUNDLE_DIR}/checksums"/*
    mkdir -p "${BUNDLE_DIR}/artifacts"
    mkdir -p "${BUNDLE_DIR}/checksums"
    mkdir -p "${BUNDLE_DIR}/logs"
    mkdir -p "${BUNDLE_DIR}/tui"
    mkdir -p "${DIST_DIR}"
    
    # Copy artifacts
    log "Copying artifacts..."
    local count=0
    
    if [[ -d "${build_output}/deb" ]]; then
        cp -v "${build_output}/deb"/*.deb "${BUNDLE_DIR}/artifacts/" 2>/dev/null && ((count++)) || true
    fi
    
    if [[ -d "${build_output}/rpm" ]]; then
        cp -v "${build_output}/rpm"/*.rpm "${BUNDLE_DIR}/artifacts/" 2>/dev/null && ((count++)) || true
    fi
    
    if [[ -d "${build_output}/appimage" ]]; then
        cp -v "${build_output}/appimage"/*.AppImage "${BUNDLE_DIR}/artifacts/" 2>/dev/null && ((count++)) || true
    fi
    
    if [[ ${count} -eq 0 ]]; then
        error "No artifacts found to bundle!"
    fi
    log "Copied ${count} artifact type(s)"
    
    # Write VERSION.txt
    echo "${version}" > "${BUNDLE_DIR}/VERSION.txt"
    log "Wrote VERSION.txt: ${version}"
    
    # Generate checksums
    log "Generating checksums..."
    (
        cd "${BUNDLE_DIR}/artifacts"
        sha256sum * > "${BUNDLE_DIR}/checksums/SHA256SUMS.txt" 2>/dev/null || true
    )
    log "Checksums written to checksums/SHA256SUMS.txt"
    
    # Ensure INSTALL is executable
    chmod +x "${BUNDLE_DIR}/INSTALL"
    log "Made INSTALL executable"
    
    # Create tarball
    local archive_name="CADalytix_Linux_Bundle_${version}.tar.gz"
    log "Creating archive: ${archive_name}"
    (
        cd "${PROJECT_ROOT}"
        tar -czvf "${DIST_DIR}/${archive_name}" LINUX_BUNDLE/
    )
    
    # Generate archive checksum
    (
        cd "${DIST_DIR}"
        sha256sum "${archive_name}" > "${archive_name}.sha256"
    )
    
    log "─────────────────────────────────────────────────────"
    log "Bundle created successfully!"
    log "Archive: ${DIST_DIR}/${archive_name}"
    log "Checksum: ${DIST_DIR}/${archive_name}.sha256"
    log "─────────────────────────────────────────────────────"
    
    # List contents
    log "Bundle contents:"
    ls -la "${BUNDLE_DIR}/artifacts/"
}

main "$@"


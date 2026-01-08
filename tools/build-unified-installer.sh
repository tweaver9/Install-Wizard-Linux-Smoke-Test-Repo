#!/bin/bash
# CADalytix Unified Installer - Phase 7 Build + Packaging Script (Linux)
# Builds release binary, runs smoke gate, creates CADALYTIX_INSTALLER/ bundle.
#
# Generated: 2026-01-07
# Usage: ./build-unified-installer.sh [--no-build] [--no-smoke] [--no-manifest] [--no-clean]

set -e

# Parse flags
NO_BUILD=false
NO_SMOKE=false
NO_MANIFEST=false
CLEAN=true

for arg in "$@"; do
    case $arg in
        --no-build) NO_BUILD=true ;;
        --no-smoke) NO_SMOKE=true ;;
        --no-manifest) NO_MANIFEST=true ;;
        --no-clean) CLEAN=false ;;
    esac
done

# Paths (same resolution as smoke-test-unified-installer.sh)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROD_INSTALL_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROD_INSTALL_DIR/.." && pwd)"
INSTALLER_ROOT="$PROD_INSTALL_DIR/installer-unified"
SRC_TAURI="$INSTALLER_ROOT/src-tauri"
LOG_DIR="$REPO_ROOT/Prod_Wizard_Log"
OUTPUT_ROOT="$REPO_ROOT/CADALYTIX_INSTALLER"
BUILD_LOG="$LOG_DIR/P7_build_linux.log"
EXE="$INSTALLER_ROOT/target/release/installer-unified"
SMOKE_SCRIPT="$SCRIPT_DIR/smoke-test-unified-installer.sh"

ALL_PASSED=true

# Ensure log directory exists
mkdir -p "$LOG_DIR"

# Clear build log
> "$BUILD_LOG"

log() {
    local message="$1"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    local line="[$timestamp] $message"
    echo "$line"
    echo "$line" >> "$BUILD_LOG"
}

log_color() {
    local color="$1"
    local message="$2"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    local line="[$timestamp] $message"
    case $color in
        green)  echo -e "\033[32m$line\033[0m" ;;
        red)    echo -e "\033[31m$line\033[0m" ;;
        cyan)   echo -e "\033[36m$line\033[0m" ;;
        *)      echo "$line" ;;
    esac
    echo "$line" >> "$BUILD_LOG"
}

log "========================================"
log "Phase 7 Build + Package (Linux)"
log "========================================"
log "Repo Root: $REPO_ROOT"
log "Output:    $OUTPUT_ROOT"
log "Flags:     NO_BUILD=$NO_BUILD NO_SMOKE=$NO_SMOKE NO_MANIFEST=$NO_MANIFEST CLEAN=$CLEAN"
log ""

# Step A: Build
if [ "$NO_BUILD" = false ]; then
    log_color cyan "=== Build --release --locked ==="
    pushd "$SRC_TAURI" > /dev/null
    if cargo build --release --locked; then
        log_color green "  [PASS] Build"
    else
        log_color red "  [FAIL] Build"
        ALL_PASSED=false
    fi
    popd > /dev/null
else
    log "Skipping build (--no-build specified)"
fi

# Verify binary exists
if [ ! -f "$EXE" ]; then
    log_color red "[FATAL] Binary not found: $EXE"
    log "ExitCode=1"
    exit 1
fi

# Step B: Smoke gate
if [ "$NO_SMOKE" = false ]; then
    log_color cyan "=== Smoke Gate (--no-build mode) ==="
    if "$SMOKE_SCRIPT" --no-build; then
        log_color green "  [PASS] Smoke Gate"
    else
        log_color red "  [FAIL] Smoke Gate"
        ALL_PASSED=false
    fi
else
    log "Skipping smoke gate (--no-smoke specified)"
fi

# Step C: Clean and create bundle structure
if [ "$CLEAN" = true ] && [ -d "$OUTPUT_ROOT" ]; then
    log "Cleaning previous bundle: $OUTPUT_ROOT"
    rm -rf "$OUTPUT_ROOT"
fi

mkdir -p "$OUTPUT_ROOT/INSTALLER/windows"
mkdir -p "$OUTPUT_ROOT/INSTALLER/linux"
mkdir -p "$OUTPUT_ROOT/TOOLS"
mkdir -p "$OUTPUT_ROOT/DOCS"
mkdir -p "$OUTPUT_ROOT/VERIFY/PROOFS"
log "Created bundle structure"

# Step D: Copy binaries
cp "$EXE" "$OUTPUT_ROOT/INSTALLER/linux/installer-unified"
chmod +x "$OUTPUT_ROOT/INSTALLER/linux/installer-unified"
log "Copied Linux binary (chmod +x)"

WIN_EXE="$INSTALLER_ROOT/target/release/installer-unified.exe"
if [ -f "$WIN_EXE" ]; then
    cp "$WIN_EXE" "$OUTPUT_ROOT/INSTALLER/windows/installer-unified.exe"
    log "Copied Windows binary"
else
    log "Windows binary not present on Linux build host (expected)"
fi

# Step E: Copy tools
cp "$SCRIPT_DIR/smoke-test-unified-installer.ps1" "$OUTPUT_ROOT/TOOLS/"
cp "$SCRIPT_DIR/smoke-test-unified-installer.sh" "$OUTPUT_ROOT/TOOLS/"
chmod +x "$OUTPUT_ROOT/TOOLS/smoke-test-unified-installer.sh"
log "Copied smoke test scripts to TOOLS/"

# Step F: Generate DOCS
cat > "$OUTPUT_ROOT/DOCS/README.md" << 'EOF'
# CADalytix Unified Installer

Cross-platform installer for CADalytix platform (Windows + Linux).

## Quick Start

### Windows
```powershell
.\INSTALLER\windows\installer-unified.exe --help
```

### Linux
```bash
chmod +x ./INSTALLER/linux/installer-unified
./INSTALLER/linux/installer-unified --help
```

## Verification

Check VERIFY/MANIFEST.sha256 to verify file integrity:
```bash
cd VERIFY && sha256sum -c MANIFEST.sha256
```
EOF

cat > "$OUTPUT_ROOT/DOCS/QUICK_START.md" << 'EOF'
# Quick Start Guide

## 1. Pre-flight Check
- Ensure database is accessible (PostgreSQL or SQL Server)
- Have connection credentials ready
- Linux: Have sudo access for systemd service setup

## 2. Run Installer
Linux: ./INSTALLER/linux/installer-unified

## 3. Follow Wizard Steps
1. Accept license agreement
2. Choose installation directory
3. Configure database connection
4. Set retention/archive policies
5. Map source columns to CADalytix schema
6. Begin installation
EOF

cat > "$OUTPUT_ROOT/DOCS/SYSTEM_REQUIREMENTS.md" << 'EOF'
# System Requirements

## Linux
- Ubuntu 20.04+, Debian 11+, RHEL 8+, or compatible
- glibc 2.31+
- 4GB RAM minimum, 8GB recommended
- 500MB disk space for installer
- systemd for service management

## Database
- PostgreSQL 12+ OR SQL Server 2016+
- Network connectivity from installer host
EOF

cat > "$OUTPUT_ROOT/DOCS/TROUBLESHOOTING.md" << 'EOF'
# Troubleshooting

## Connection Errors
- Verify network connectivity: ping <db-host>
- Check firewall rules allow DB port (5432/1433)
- Verify credentials are correct

## Service Won't Start
- Check: journalctl -u cadalytix-installer
- Verify database is accessible after install
EOF

# Copy detailed docs from repo
INSTALL_GUIDE="$PROD_INSTALL_DIR/docs/INSTALLATION_GUIDE.md"
if [ -f "$INSTALL_GUIDE" ]; then
    cp "$INSTALL_GUIDE" "$OUTPUT_ROOT/DOCS/INSTALLATION_GUIDE.md"
fi
log "Generated DOCS/"

# Step G: Generate VERSIONS.txt
GIT_HASH=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
RUSTC_VER=$(rustc -V 2>/dev/null || echo "unknown")
CARGO_VER=$(cargo -V 2>/dev/null || echo "unknown")
NODE_VER=$(node -v 2>/dev/null || echo "unknown")
NPM_VER=$(npm -v 2>/dev/null || echo "unknown")

cat > "$OUTPUT_ROOT/VERIFY/VERSIONS.txt" << EOF
# CADalytix Unified Installer - Build Info
Generated: $(date '+%Y-%m-%d %H:%M:%S')
Git Commit: $GIT_HASH
Platform: Linux

# Toolchain Versions
$RUSTC_VER
$CARGO_VER
node $NODE_VER
npm $NPM_VER
EOF

log "Generated VERIFY/VERSIONS.txt"

# Step H: Copy proof logs (BEFORE manifest so they're included)
PROOF_DIR="$OUTPUT_ROOT/VERIFY/PROOFS"
PROOF_PATTERNS=(
    "P6_smoke_windows.log"
    "P6_smoke_linux.log"
    "P6_unit_tests*.log"
    "P6_connection_failure_deterministic.log"
    "P7_*.log"
    "P8_*.log"
    "B1_*.log"
    "B2_*.log"
    "B3_*.log"
    "D2_*.log"
)

COPIED_COUNT=0
for pattern in "${PROOF_PATTERNS[@]}"; do
    for file in $LOG_DIR/$pattern; do
        if [ -f "$file" ]; then
            cp "$file" "$PROOF_DIR/"
            ((COPIED_COUNT++)) || true
        fi
    done
done

log "Copied $COPIED_COUNT proof logs to VERIFY/PROOFS/"

# Step I: Generate MANIFEST.sha256 (LAST, after all files are in place)
if [ "$NO_MANIFEST" = false ]; then
    MANIFEST_PATH="$OUTPUT_ROOT/VERIFY/MANIFEST.sha256"
    > "$MANIFEST_PATH"

    # Find all files, exclude MANIFEST itself, sort, compute SHA256
    # Includes VERIFY/PROOFS/ since they were copied first
    find "$OUTPUT_ROOT" -type f ! -name "MANIFEST.sha256" | sort | while read -r file; do
        hash=$(sha256sum "$file" | awk '{print $1}')
        rel_path="${file#$OUTPUT_ROOT/}"
        echo "$hash  $rel_path" >> "$MANIFEST_PATH"
    done

    FILE_COUNT=$(wc -l < "$MANIFEST_PATH")
    log "Generated VERIFY/MANIFEST.sha256 ($FILE_COUNT files)"
else
    log "Skipping manifest generation (--no-manifest specified)"
fi

# Final summary
log ""
log "========================================"
if [ "$ALL_PASSED" = true ]; then
    log_color green "PHASE 7 BUILD COMPLETE"
    log "Bundle: $OUTPUT_ROOT"
    log "ExitCode=0"
    exit 0
else
    log_color red "PHASE 7 BUILD FAILED"
    log "ExitCode=1"
    exit 1
fi


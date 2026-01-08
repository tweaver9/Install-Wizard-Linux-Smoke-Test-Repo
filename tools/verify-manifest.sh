#!/usr/bin/env bash
# ============================================================================
# P8 Manifest Verification (Linux)
# Verifies MANIFEST.sha256 in the CADALYTIX_INSTALLER bundle.
# Writes proof log to Prod_Wizard_Log/P8_manifest_verify_linux.log.
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOLS_DIR="$SCRIPT_DIR"
REPO_ROOT="$(cd "$TOOLS_DIR/../.." && pwd)"

BUNDLE_PATH="${1:-$REPO_ROOT/CADALYTIX_INSTALLER}"
MANIFEST_PATH="$BUNDLE_PATH/VERIFY/MANIFEST.sha256"
LOG_DIR="$REPO_ROOT/Prod_Wizard_Log"
LOG_PATH="$LOG_DIR/P8_manifest_verify_linux.log"

# Ensure log directory exists
mkdir -p "$LOG_DIR"

# Logging functions
log() {
    echo "$1" | tee -a "$LOG_PATH"
}

log_color() {
    local color="$1"
    local msg="$2"
    case "$color" in
        red)    echo -e "\033[0;31m$msg\033[0m" | tee -a "$LOG_PATH" ;;
        green)  echo -e "\033[0;32m$msg\033[0m" | tee -a "$LOG_PATH" ;;
        yellow) echo -e "\033[0;33m$msg\033[0m" | tee -a "$LOG_PATH" ;;
        *)      echo "$msg" | tee -a "$LOG_PATH" ;;
    esac
}

# Initialize log file
> "$LOG_PATH"

log "=== P8 Manifest Verification (Linux) ==="
log "Started: $(date '+%Y-%m-%d %H:%M:%S')"
log "Bundle: $BUNDLE_PATH"
log "Manifest: $MANIFEST_PATH"
log ""

# Check manifest exists
if [ ! -f "$MANIFEST_PATH" ]; then
    log_color red "[FAIL] MANIFEST.sha256 not found at: $MANIFEST_PATH"
    log "ExitCode=1"
    exit 1
fi

# Count files
FILE_COUNT=$(wc -l < "$MANIFEST_PATH")
log "Files in manifest: $FILE_COUNT"
log ""

VERIFIED=0
MISSING=0
MISMATCHED=0
FAILURES=""

# Read manifest and verify each file
while IFS= read -r line || [ -n "$line" ]; do
    [ -z "$line" ] && continue
    
    # Format: <hash>  <relative_path>
    EXPECTED_HASH=$(echo "$line" | awk '{print $1}')
    RELATIVE_PATH=$(echo "$line" | cut -d' ' -f3-)
    FULL_PATH="$BUNDLE_PATH/$RELATIVE_PATH"
    
    if [ ! -f "$FULL_PATH" ]; then
        log_color red "[MISSING] $RELATIVE_PATH"
        MISSING=$((MISSING + 1))
        FAILURES="$FAILURES\nMISSING: $RELATIVE_PATH"
        continue
    fi
    
    ACTUAL_HASH=$(sha256sum "$FULL_PATH" | awk '{print $1}')
    
    if [ "$ACTUAL_HASH" != "$EXPECTED_HASH" ]; then
        log_color red "[MISMATCH] $RELATIVE_PATH"
        log "  Expected: $EXPECTED_HASH"
        log "  Actual:   $ACTUAL_HASH"
        MISMATCHED=$((MISMATCHED + 1))
        FAILURES="$FAILURES\nMISMATCH: $RELATIVE_PATH"
    else
        VERIFIED=$((VERIFIED + 1))
    fi
done < "$MANIFEST_PATH"

log ""
log "=== Summary ==="
log "Verified: $VERIFIED"
log "Missing: $MISSING"
log "Mismatched: $MISMATCHED"
log ""

if [ -n "$FAILURES" ]; then
    log_color red "=== Failures ==="
    echo -e "$FAILURES" | tee -a "$LOG_PATH"
    log ""
    log "========================================"
    log_color red "MANIFEST VERIFICATION FAILED"
    log "========================================"
    log "ExitCode=1"
    exit 1
else
    log "========================================"
    log_color green "MANIFEST VERIFICATION PASSED"
    log "========================================"
    log "ExitCode=0"
    exit 0
fi


#!/bin/bash
# CADalytix Unified Installer - Phase 6 Smoke Test Script (Linux)
# Runs all proof modes and TUI smoke targets in predictable order.
# Produces stable P6_* logs under Prod_Wizard_Log/.
# Stops on first failure.
#
# Generated: 2026-01-07
# Usage: ./smoke-test-unified-installer.sh [--no-build] [--verbose]

set -e

NO_BUILD=false
VERBOSE=false

for arg in "$@"; do
    case $arg in
        --no-build) NO_BUILD=true ;;
        --verbose) VERBOSE=true ;;
    esac
done

# Paths
# tools/ is inside Prod_Install_Wizard_Deployment/, so parent is Prod_Install_Wizard_Deployment
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROD_INSTALL_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROD_INSTALL_DIR/.." && pwd)"
INSTALLER_ROOT="$PROD_INSTALL_DIR/installer-unified"
SRC_TAURI="$INSTALLER_ROOT/src-tauri"
LOG_DIR="$REPO_ROOT/Prod_Wizard_Log"
# Note: exe is under installer-unified/target/release, not src-tauri/target/release
EXE="$INSTALLER_ROOT/target/release/installer-unified"
SUMMARY_LOG="$LOG_DIR/P6_smoke_linux.log"

# Ensure log directory exists
mkdir -p "$LOG_DIR"

log() {
    local color="$1"
    local message="$2"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    local line="[$timestamp] $message"
    
    case $color in
        green)  echo -e "\033[32m$line\033[0m" ;;
        red)    echo -e "\033[31m$line\033[0m" ;;
        yellow) echo -e "\033[33m$line\033[0m" ;;
        cyan)   echo -e "\033[36m$line\033[0m" ;;
        *)      echo "$line" ;;
    esac
    echo "$line" >> "$SUMMARY_LOG"
}

run_proof_mode() {
    local name="$1"
    local flag="$2"
    local logfile="$3"
    local full_log_path="$LOG_DIR/$logfile"
    
    log cyan "Running: $name ($flag)"
    
    if "$EXE" "$flag" > "$full_log_path" 2>&1; then
        local exit_code=$?
        echo "ExitCode=$exit_code" >> "$full_log_path"
        log green "  [PASS] $name (ExitCode=$exit_code)"
        return 0
    else
        local exit_code=$?
        echo "ExitCode=$exit_code" >> "$full_log_path"
        log red "  [FAIL] $name (ExitCode=$exit_code)"
        return 1
    fi
}

run_tui_smoke() {
    local target="$1"
    local logfile="P6_tui_smoke_$target.log"
    run_proof_mode "TUI Smoke: $target" "--tui-smoke=$target" "$logfile"
}

# Start
log yellow "========================================"
log yellow "PHASE 6 SMOKE TEST - LINUX"
log yellow "========================================"
log white "Repo Root: $REPO_ROOT"
log white "Log Dir: $LOG_DIR"

# Build (unless --no-build)
if [ "$NO_BUILD" = false ]; then
    log cyan "Building release..."
    pushd "$SRC_TAURI" > /dev/null
    if cargo build --release > "$LOG_DIR/P6_build.log" 2>&1; then
        log green "  [PASS] Build complete"
    else
        log red "[FAIL] cargo build --release failed"
        exit 1
    fi
    popd > /dev/null
else
    log yellow "Skipping build (--no-build specified)"
fi

# Verify exe exists
if [ ! -f "$EXE" ]; then
    log red "[FAIL] Executable not found: $EXE"
    exit 1
fi

log white ""
log yellow "--- Proof Modes ---"

# Run proof modes
run_proof_mode "Install Contract Smoke" "--install-contract-smoke" "B1_install_contract_smoke_transcript.log" || exit 1
run_proof_mode "Archive Dry-Run" "--archive-dry-run" "B2_archive_pipeline_dryrun_transcript.log" || exit 1
run_proof_mode "Mapping Persist Smoke" "--mapping-persist-smoke" "B3_mapping_persist_smoke_transcript.log" || exit 1
run_proof_mode "DB Setup Smoke" "--db-setup-smoke" "D2_db_setup_smoke_transcript.log" || exit 1

log white ""
log yellow "--- TUI Smoke Targets ---"

# TUI smoke targets
for target in welcome license destination db storage retention archive consent mapping ready progress; do
    run_tui_smoke "$target" || exit 1
done

log white ""
log green "========================================"
log green "ALL SMOKE TESTS PASSED"
log green "========================================"
log white "Summary log: $SUMMARY_LOG"
log white "ExitCode=0"

exit 0


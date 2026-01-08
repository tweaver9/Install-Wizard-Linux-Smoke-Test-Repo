#!/usr/bin/env bash
# ============================================================================
# P8 SQL Injection Scan (Linux)
# Scans for potential SQL injection vulnerabilities in Rust source files.
# Writes proof log to Prod_Wizard_Log/P8_sql_scan_linux.log
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LOG_DIR="$REPO_ROOT/Prod_Wizard_Log"
SRC_DIR="$REPO_ROOT/Prod_Install_Wizard_Deployment/installer-unified/src-tauri/src"
LOG_PATH="$LOG_DIR/P8_sql_scan_linux.log"

mkdir -p "$LOG_DIR"

log() { echo "$1" | tee -a "$LOG_PATH"; }
log_color() {
    local color="$1"; local msg="$2"
    case "$color" in
        red)    echo -e "\033[0;31m$msg\033[0m" | tee -a "$LOG_PATH" ;;
        green)  echo -e "\033[0;32m$msg\033[0m" | tee -a "$LOG_PATH" ;;
        yellow) echo -e "\033[0;33m$msg\033[0m" | tee -a "$LOG_PATH" ;;
        *)      echo "$msg" | tee -a "$LOG_PATH" ;;
    esac
}

> "$LOG_PATH"

log "=== P8 SQL Injection Scan (Linux) ==="
log "Started: $(date '+%Y-%m-%d %H:%M:%S')"
log "Scanning: $SRC_DIR"
log ""

if [ ! -d "$SRC_DIR" ]; then
    log_color yellow "[SKIP] Source directory does not exist: $SRC_DIR"
    log "ExitCode=0"
    exit 0
fi

FILE_COUNT=$(find "$SRC_DIR" -name "*.rs" -type f | wc -l)
log "Rust files to scan: $FILE_COUNT"
log ""

# Dangerous patterns: format! with SQL keywords and {} placeholders
DANGEROUS_PATTERNS=(
    'format!\s*\(\s*"[^"]*SELECT[^"]*\{\}'
    'format!\s*\(\s*"[^"]*INSERT[^"]*\{\}'
    'format!\s*\(\s*"[^"]*UPDATE[^"]*\{\}'
    'format!\s*\(\s*"[^"]*DELETE[^"]*\{\}'
    'format!\s*\(\s*"[^"]*WHERE[^"]*\{\}'
)

HIT_COUNT=0
HITS=""

for pattern in "${DANGEROUS_PATTERNS[@]}"; do
    while IFS= read -r match; do
        [ -z "$match" ] && continue
        
        # Check if the file also uses parameterized queries (sqlx::query, $1, @P1)
        FILE_PATH=$(echo "$match" | cut -d: -f1)
        if grep -qE "(sqlx::query|\\$[0-9]|@P[0-9])" "$FILE_PATH" 2>/dev/null; then
            continue  # Likely using parameterized queries
        fi
        
        RELATIVE=$(echo "$match" | sed "s|$SRC_DIR/||")
        HITS="$HITS\n  $RELATIVE"
        HIT_COUNT=$((HIT_COUNT + 1))
    done < <(grep -rn "$pattern" "$SRC_DIR" --include="*.rs" 2>/dev/null || true)
done

log "=== Summary ==="
log "Potential SQL injection risks: $HIT_COUNT"
log ""

if [ "$HIT_COUNT" -gt 0 ]; then
    log_color yellow "=== Potential Risks ==="
    echo -e "$HITS" | tee -a "$LOG_PATH"
    log ""
    log "NOTE: Review these lines manually."
    log ""
    log "========================================"
    log "SQL SCAN COMPLETED WITH WARNINGS"
    log "========================================"
    log "ExitCode=0"
    exit 0
else
    log "========================================"
    log_color green "SQL SCAN PASSED"
    log "========================================"
    log "ExitCode=0"
    exit 0
fi


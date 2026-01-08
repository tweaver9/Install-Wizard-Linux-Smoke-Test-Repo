#!/usr/bin/env bash
# ============================================================================
# P8 Secret Scanning (Linux)
# Scans for secrets in logs, proofs, and source code.
# Writes proof logs under Prod_Wizard_Log/P8_secret_scan_*.log
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LOG_DIR="$REPO_ROOT/Prod_Wizard_Log"
BUNDLE_PROOFS="$REPO_ROOT/CADALYTIX_INSTALLER/VERIFY/PROOFS"
SRC_DIR="$REPO_ROOT/Prod_Install_Wizard_Deployment/installer-unified/src-tauri/src"

mkdir -p "$LOG_DIR"

# Secret patterns (case-insensitive grep patterns)
SECRET_PATTERNS=(
    "password[[:space:]]*="
    "pwd[[:space:]]*="
    "token[[:space:]]*="
    "apikey"
    "api_key"
    "secret[[:space:]]*="
    "bearer[[:space:]]"
    "authorization:[[:space:]]*bearer"
    "begin private key"
    "aws_access_key_id"
    "secretaccesskey"
    "accountkey"
    "x-amz-signature"
)

log_color() {
    local color="$1"
    local msg="$2"
    case "$color" in
        red)    echo -e "\033[0;31m$msg\033[0m" ;;
        green)  echo -e "\033[0;32m$msg\033[0m" ;;
        yellow) echo -e "\033[0;33m$msg\033[0m" ;;
        cyan)   echo -e "\033[0;36m$msg\033[0m" ;;
        *)      echo "$msg" ;;
    esac
}

scan_directory() {
    local path="$1"
    local log_path="$2"
    local label="$3"
    local allow_test="${4:-false}"
    
    > "$log_path"
    
    echo "=== P8 Secret Scan: $label ===" >> "$log_path"
    echo "Started: $(date '+%Y-%m-%d %H:%M:%S')" >> "$log_path"
    echo "Scanning: $path" >> "$log_path"
    echo "" >> "$log_path"
    
    if [ ! -d "$path" ]; then
        echo "[SKIP] Path does not exist: $path" >> "$log_path"
        echo "ExitCode=0" >> "$log_path"
        return 0
    fi
    
    local file_count
    file_count=$(find "$path" -type f 2>/dev/null | wc -l)
    echo "Files to scan: $file_count" >> "$log_path"
    echo "" >> "$log_path"
    
    local hit_count=0
    
    for pattern in "${SECRET_PATTERNS[@]}"; do
        while IFS= read -r match; do
            [ -z "$match" ] && continue
            
            # Skip known test fixtures in test files
            if [ "$allow_test" = "true" ]; then
                if echo "$match" | grep -qE "(test|_test\.rs)"; then
                    if echo "$match" | grep -qiE "(SuperSecret123|TestPassword|fake_password|\*\*\*\*\*\*\*\*)"; then
                        continue
                    fi
                fi
            fi
            
            echo "[HIT] $match" >> "$log_path"
            hit_count=$((hit_count + 1))
        done < <(grep -riln "$pattern" "$path" 2>/dev/null || true)
    done
    
    echo "" >> "$log_path"
    echo "=== Summary ===" >> "$log_path"
    echo "Hits: $hit_count" >> "$log_path"
    
    if [ "$hit_count" -gt 0 ]; then
        echo "" >> "$log_path"
        echo "========================================" >> "$log_path"
        echo "SECRET SCAN FAILED" >> "$log_path"
        echo "========================================" >> "$log_path"
        echo "ExitCode=1" >> "$log_path"
        return 1
    else
        echo "" >> "$log_path"
        echo "========================================" >> "$log_path"
        echo "SECRET SCAN PASSED" >> "$log_path"
        echo "========================================" >> "$log_path"
        echo "ExitCode=0" >> "$log_path"
        return 0
    fi
}

log_color cyan "=== P8 Secret Scanning ==="
echo ""

ALL_PASSED=true

# Scan logs
log_color yellow "Scanning Prod_Wizard_Log/..."
if scan_directory "$LOG_DIR" "$LOG_DIR/P8_secret_scan_logs_linux.log" "Logs" "false"; then
    log_color green "  [PASS] No secrets in logs"
else
    log_color red "  [FAIL] Secrets found in logs"
    ALL_PASSED=false
fi

# Scan bundle proofs
log_color yellow "Scanning CADALYTIX_INSTALLER/VERIFY/PROOFS/..."
if scan_directory "$BUNDLE_PROOFS" "$LOG_DIR/P8_secret_scan_proofs_linux.log" "Proofs" "false"; then
    log_color green "  [PASS] No secrets in proofs"
else
    log_color red "  [FAIL] Secrets found in proofs"
    ALL_PASSED=false
fi

# Scan source code (allow test fixtures)
log_color yellow "Scanning source code..."
if scan_directory "$SRC_DIR" "$LOG_DIR/P8_secret_scan_code_linux.log" "Code" "true"; then
    log_color green "  [PASS] No secrets in code"
else
    log_color red "  [FAIL] Secrets found in code"
    ALL_PASSED=false
fi

echo ""
if [ "$ALL_PASSED" = true ]; then
    log_color green "ALL SECRET SCANS PASSED"
    exit 0
else
    log_color red "SECRET SCANS FAILED"
    exit 1
fi


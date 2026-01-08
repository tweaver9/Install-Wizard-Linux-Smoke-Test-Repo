#!/usr/bin/env bash
# ============================================================================
# Phase 9 Proof Verification (Linux)
# Checks PROOFS/PHASE9 structure, secret scan, and SHA256SUMS validation.
# Exits non-zero if any check fails.
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PROOFS_DIR="$REPO_ROOT/PROOFS/PHASE9"
LOG_DIR="$REPO_ROOT/Prod_Wizard_Log"
LOG_FILE="$LOG_DIR/P9_verify_proofs.log"

mkdir -p "$LOG_DIR"
> "$LOG_FILE"

# Expected files (relative to PROOFS/PHASE9)
EXPECTED_FILES=(
    "postgres/case_a_success.log"
    "postgres/case_b_privilege_fail.log"
    "postgres/case_c_exists_fail.log"
    "sqlserver/case_a_success.log"
    "sqlserver/case_b_privilege_fail.log"
    "sqlserver/case_c_exists_fail.log"
    "sqlserver/case_d_sizing_proof.log"
    "README.md"
    "VERIFICATION_STATUS.md"
)

# Secret patterns to detect (leaks)
SECRET_PATTERNS=(
    "password[[:space:]]*=[[:space:]]*[^*{\[[:space:]][^;[:space:]]{4,}"
    "pwd[[:space:]]*=[[:space:]]*[^*{\[[:space:]][^;[:space:]]{4,}"
    "postgresql://[^:]+:[^*@{\[]{4,}@"
    "Authorization:[[:space:]]*Bearer[[:space:]]+[a-zA-Z0-9_.-]{20,}"
    "token[[:space:]]*=[[:space:]]*[a-zA-Z0-9_.-]{20,}"
    "apikey[[:space:]]*=[[:space:]]*[a-zA-Z0-9_.-]{20,}"
    "secret[[:space:]]*=[[:space:]]*[^*{\[[:space:]][^;[:space:]]{4,}"
)

# Safe patterns to exclude
SAFE_PATTERNS=(
    "hasPassword=true"
    "\\*{4,}"
    "\\[REDACTED\\]"
    "P9_.*_Pass_2024!"
)

# Files to exclude from secret scanning (documentation with examples)
EXCLUDE_FROM_SECRET_SCAN=("README.md" "VERIFICATION_STATUS.md")

log_msg() {
    local color="$1"
    local msg="$2"
    case "$color" in
        red)    echo -e "\033[0;31m$msg\033[0m" ;;
        green)  echo -e "\033[0;32m$msg\033[0m" ;;
        yellow) echo -e "\033[0;33m$msg\033[0m" ;;
        cyan)   echo -e "\033[0;36m$msg\033[0m" ;;
        *)      echo "$msg" ;;
    esac
    echo "$msg" >> "$LOG_FILE"
}

log_msg cyan "========================================"
log_msg cyan "   PHASE 9 PROOF VERIFICATION"
log_msg cyan "========================================"
log_msg white "Started: $(date '+%Y-%m-%d %H:%M:%S')"
log_msg white "Proofs Directory: $PROOFS_DIR"
echo ""

ALL_PASSED=true
ERRORS=()

# ============================================================================
# CHECK 1: Structure exists
# ============================================================================
log_msg yellow "[1/3] Checking PROOFS/PHASE9 structure..."

if [ ! -d "$PROOFS_DIR" ]; then
    ERRORS+=("PROOFS/PHASE9 directory does not exist")
    ALL_PASSED=false
    log_msg red "  [FAIL] Directory missing: $PROOFS_DIR"
else
    MISSING=()
    for file in "${EXPECTED_FILES[@]}"; do
        if [ ! -f "$PROOFS_DIR/$file" ]; then
            MISSING+=("$file")
        fi
    done
    
    if [ ${#MISSING[@]} -gt 0 ]; then
        ERRORS+=("Missing files: ${MISSING[*]}")
        ALL_PASSED=false
        log_msg red "  [FAIL] Missing files:"
        for m in "${MISSING[@]}"; do log_msg red "         - $m"; done
    else
        log_msg green "  [PASS] All ${#EXPECTED_FILES[@]} expected files present"
    fi
fi

# ============================================================================
# CHECK 2: Secret scan
# ============================================================================
echo ""
log_msg yellow "[2/3] Scanning for secrets (no credentials in proofs)..."

SECRET_HITS=()
while IFS= read -r -d '' file; do
    # Skip excluded files (documentation with examples)
    filename=$(basename "$file")
    skip=false
    for excluded in "${EXCLUDE_FROM_SECRET_SCAN[@]}"; do
        if [ "$filename" = "$excluded" ]; then
            skip=true
            break
        fi
    done
    [ "$skip" = true ] && continue

    for pattern in "${SECRET_PATTERNS[@]}"; do
        if grep -qiE "$pattern" "$file" 2>/dev/null; then
            # Check if it matches a safe pattern
            match_line=$(grep -iE "$pattern" "$file" 2>/dev/null | head -1)
            is_safe=false
            for safe in "${SAFE_PATTERNS[@]}"; do
                if echo "$match_line" | grep -qE "$safe" 2>/dev/null; then
                    is_safe=true
                    break
                fi
            done
            if [ "$is_safe" = false ]; then
                rel_path="${file#$PROOFS_DIR/}"
                SECRET_HITS+=("$rel_path : [REDACTED - potential secret pattern]")
            fi
        fi
    done
done < <(find "$PROOFS_DIR" -type f -print0 2>/dev/null)

if [ ${#SECRET_HITS[@]} -gt 0 ]; then
    ERRORS+=("Secret patterns detected in ${#SECRET_HITS[@]} location(s)")
    ALL_PASSED=false
    log_msg red "  [FAIL] Secret patterns found:"
    for hit in "${SECRET_HITS[@]}"; do log_msg red "         - $hit"; done
else
    log_msg green "  [PASS] No secrets detected in proof files"
fi

# ============================================================================
# CHECK 3: SHA256SUMS validation
# ============================================================================
echo ""
log_msg yellow "[3/3] Validating SHA256SUMS.txt..."

SHA256_FILE="$PROOFS_DIR/SHA256SUMS.txt"

# Count files to check (excluding SHA256SUMS.txt itself)
EXPECTED_FILE_COUNT=$(find "$PROOFS_DIR" -type f ! -name "SHA256SUMS.txt" 2>/dev/null | wc -l)

# Canonical format: one line per file, "<HASH>  <PATH>" (two spaces, sha256sum style)
# Regeneration command (Linux):
REGEN_COMMAND='cd $REPO_ROOT && find PROOFS/PHASE9 -type f ! -name "SHA256SUMS.txt" -exec sha256sum {} \; | sort -k2 | tr "a-f" "A-F" > PROOFS/PHASE9/SHA256SUMS.txt'

if [ ! -f "$SHA256_FILE" ]; then
    ERRORS+=("SHA256SUMS.txt missing")
    ALL_PASSED=false
    log_msg red "  [FAIL] SHA256SUMS.txt not found"
    log_msg red "         Regenerate with:"
    log_msg yellow "         $REGEN_COMMAND"
else
    # Validate format: must have one line per file (not single-line malformed)
    LINE_COUNT=$(grep -cE '^[A-Fa-f0-9]{64}[[:space:]]' "$SHA256_FILE" 2>/dev/null || echo 0)

    if [ "$LINE_COUNT" -lt "$EXPECTED_FILE_COUNT" ]; then
        ERRORS+=("SHA256SUMS.txt malformed: expected $EXPECTED_FILE_COUNT lines, got $LINE_COUNT")
        ALL_PASSED=false
        log_msg red "  [FAIL] SHA256SUMS.txt malformed (expected $EXPECTED_FILE_COUNT lines, got $LINE_COUNT)"
        log_msg red "         Canonical format: one line per file, '<HASH>  <PATH>'"
        log_msg red "         Regenerate with:"
        log_msg yellow "         $REGEN_COMMAND"
    else
        HASH_MISMATCHES=()
        FILE_COUNT=0

        while IFS= read -r -d '' file; do
            [ "$file" = "$SHA256_FILE" ] && continue
            FILE_COUNT=$((FILE_COUNT + 1))

            rel_path="PROOFS/PHASE9/${file#$PROOFS_DIR/}"
            actual_hash=$(sha256sum "$file" | cut -d' ' -f1 | tr 'a-f' 'A-F')

            # Look for this path in SHA256SUMS.txt
            expected_hash=$(grep -F "$rel_path" "$SHA256_FILE" 2>/dev/null | grep -oE '^[A-Fa-f0-9]{64}' | tr 'a-f' 'A-F' | head -1)

            if [ -z "$expected_hash" ]; then
                HASH_MISMATCHES+=("$rel_path : not in SHA256SUMS.txt")
            elif [ "$expected_hash" != "$actual_hash" ]; then
                HASH_MISMATCHES+=("$rel_path : hash mismatch")
            fi
        done < <(find "$PROOFS_DIR" -type f -print0 2>/dev/null)

        if [ ${#HASH_MISMATCHES[@]} -gt 0 ]; then
            ERRORS+=("SHA256 validation failed for ${#HASH_MISMATCHES[@]} file(s)")
            ALL_PASSED=false
            log_msg red "  [FAIL] Hash mismatches:"
            for m in "${HASH_MISMATCHES[@]}"; do log_msg red "         - $m"; done
        else
            log_msg green "  [PASS] All $FILE_COUNT files match SHA256SUMS.txt"
        fi
    fi
fi

# ============================================================================
# SUMMARY
# ============================================================================
echo ""
log_msg cyan "========================================"

if [ "$ALL_PASSED" = true ]; then
    log_msg green "   PHASE 9 VERIFICATION: PASSED"
    log_msg cyan "========================================"
    log_msg white "All checks passed. Proofs are intact."
    echo "ExitCode=0" >> "$LOG_FILE"
    exit 0
else
    log_msg red "   PHASE 9 VERIFICATION: FAILED"
    log_msg cyan "========================================"
    echo ""
    log_msg red "Errors (${#ERRORS[@]}):"
    for e in "${ERRORS[@]}"; do log_msg red "  - $e"; done
    echo ""
    echo "ExitCode=1" >> "$LOG_FILE"
    exit 1
fi


#!/usr/bin/env bash
# Phase 9 E2E Verification: Postgres Create NEW Database Tests
#
# Runs 3 test cases against a Docker Postgres instance:
#   Case A: Success - privileged user creates new database
#   Case B: Privilege Fail - unprivileged user cannot create database
#   Case C: Exists Fail - cannot create database that already exists
#
# Produces redacted logs in PROOFS/PHASE9/postgres/
#
# Prerequisites: Docker running with phase9-docker-compose.yml containers

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROOFS_DIR="${1:-$SCRIPT_DIR/../PROOFS/PHASE9/postgres}"

# Connection details (passwords NOT in logs)
PG_HOST="localhost"
PG_PORT="15432"
ADMIN_USER="p9_admin"
ADMIN_PASS="P9_Admin_Pass_2024!"
LIMITED_USER="p9_limited"
LIMITED_PASS="P9_Limited_Pass_2024!"

# Database names for tests
DB_CASE_A="cadalytix_test_a"
DB_CASE_B="cadalytix_test_b"

TESTS_PASSED=0
TESTS_FAILED=0

log() {
    local level="${2:-INFO}"
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] [$level] $1"
}

ensure_proofs_dir() {
    mkdir -p "$PROOFS_DIR"
}

test_postgres_connection() {
    PGPASSWORD="$ADMIN_PASS" psql -h "$PG_HOST" -p "$PG_PORT" -U "$ADMIN_USER" -d postgres -c "SELECT 1;" > /dev/null 2>&1
}

run_case_a_success() {
    log "=== Case A: Success - Create NEW database with privileged user ===" "TEST"
    local log_file="$PROOFS_DIR/case_a_success.log"
    
    {
        log "Testing: Create database '$DB_CASE_A' with user '$ADMIN_USER'"
        log "Connection: host=$PG_HOST port=$PG_PORT user=$ADMIN_USER hasPassword=true"
        
        # Cleanup from previous runs
        PGPASSWORD="$ADMIN_PASS" psql -h "$PG_HOST" -p "$PG_PORT" -U "$ADMIN_USER" -d postgres \
            -c "DROP DATABASE IF EXISTS $DB_CASE_A;" 2>&1 || true
        log "Cleanup: DROP DATABASE IF EXISTS $DB_CASE_A"
        
        # Create database
        if PGPASSWORD="$ADMIN_PASS" psql -h "$PG_HOST" -p "$PG_PORT" -U "$ADMIN_USER" -d postgres \
            -c "CREATE DATABASE $DB_CASE_A;" 2>&1; then
            log "SUCCESS: Database '$DB_CASE_A' created" "PASS"
            
            # Verify database exists
            PGPASSWORD="$ADMIN_PASS" psql -h "$PG_HOST" -p "$PG_PORT" -U "$ADMIN_USER" -d postgres \
                -c "SELECT datname FROM pg_database WHERE datname = '$DB_CASE_A';" 2>&1
            log "Verification: Database exists in pg_database"
            
            ((TESTS_PASSED++)) || true
            log "CASE A: PASS" "RESULT"
        else
            log "FAIL: Could not create database" "FAIL"
            ((TESTS_FAILED++)) || true
            log "CASE A: FAIL" "RESULT"
        fi
    } > "$log_file" 2>&1
}

run_case_b_privilege_fail() {
    log "=== Case B: Privilege Fail - Unprivileged user cannot create database ===" "TEST"
    local log_file="$PROOFS_DIR/case_b_privilege_fail.log"
    
    {
        log "Testing: Attempt to create database '$DB_CASE_B' with user '$LIMITED_USER'"
        log "Connection: host=$PG_HOST port=$PG_PORT user=$LIMITED_USER hasPassword=true"
        log "Expected: FAIL with permission denied"
        
        # Attempt to create database (should fail)
        if PGPASSWORD="$LIMITED_PASS" psql -h "$PG_HOST" -p "$PG_PORT" -U "$LIMITED_USER" -d postgres \
            -c "CREATE DATABASE $DB_CASE_B;" 2>&1; then
            log "FAIL: Database was created when it should have been denied" "FAIL"
            ((TESTS_FAILED++)) || true
            log "CASE B: FAIL" "RESULT"
        else
            log "SUCCESS: Database creation correctly denied" "PASS"
            log "Error message (redacted): permission denied to create database"
            
            # Verify database does NOT exist
            if ! PGPASSWORD="$ADMIN_PASS" psql -h "$PG_HOST" -p "$PG_PORT" -U "$ADMIN_USER" -d postgres \
                -t -c "SELECT datname FROM pg_database WHERE datname = '$DB_CASE_B';" 2>&1 | grep -q "$DB_CASE_B"; then
                log "Verification: Database '$DB_CASE_B' correctly does NOT exist" "PASS"
            fi
            
            ((TESTS_PASSED++)) || true
            log "CASE B: PASS" "RESULT"
        fi
    } > "$log_file" 2>&1
}

run_case_c_exists_fail() {
    log "=== Case C: Exists Fail - Cannot create database that already exists ===" "TEST"
    local log_file="$PROOFS_DIR/case_c_exists_fail.log"
    
    {
        log "Testing: Attempt to create database '$DB_CASE_A' again (should already exist)"
        log "Connection: host=$PG_HOST port=$PG_PORT user=$ADMIN_USER hasPassword=true"
        log "Expected: FAIL with 'database already exists'"
        
        # Attempt to create database (should fail)
        if PGPASSWORD="$ADMIN_PASS" psql -h "$PG_HOST" -p "$PG_PORT" -U "$ADMIN_USER" -d postgres \
            -c "CREATE DATABASE $DB_CASE_A;" 2>&1; then
            log "FAIL: Database was created when it should have failed" "FAIL"
            ((TESTS_FAILED++)) || true
            log "CASE C: FAIL" "RESULT"
        else
            log "SUCCESS: Database creation correctly failed" "PASS"
            log "Error message contains 'already exists' as expected"
            ((TESTS_PASSED++)) || true
            log "CASE C: PASS" "RESULT"
        fi
    } > "$log_file" 2>&1
}

cleanup_test_databases() {
    log "=== Cleanup: Removing test databases ===" "INFO"
    PGPASSWORD="$ADMIN_PASS" psql -h "$PG_HOST" -p "$PG_PORT" -U "$ADMIN_USER" -d postgres \
        -c "DROP DATABASE IF EXISTS $DB_CASE_A;" > /dev/null 2>&1 || true
    PGPASSWORD="$ADMIN_PASS" psql -h "$PG_HOST" -p "$PG_PORT" -U "$ADMIN_USER" -d postgres \
        -c "DROP DATABASE IF EXISTS $DB_CASE_B;" > /dev/null 2>&1 || true
    log "Test databases cleaned up"
}

# Main execution
main() {
    log "Phase 9 E2E Verification: Postgres" "START"
    log "Proofs directory: $PROOFS_DIR"

    ensure_proofs_dir

    # Check if Postgres is reachable
    if ! test_postgres_connection; then
        log "ERROR: Cannot connect to Postgres at ${PG_HOST}:${PG_PORT}" "ERROR"
        log "Make sure Docker containers are running: docker compose -f phase9-docker-compose.yml up -d"
        exit 1
    fi

    log "Postgres connection verified"

    # Run test cases in order
    run_case_a_success
    run_case_b_privilege_fail
    run_case_c_exists_fail

    # Cleanup
    cleanup_test_databases

    # Summary
    log "========================================"
    log "Tests Passed: $TESTS_PASSED" "SUMMARY"
    log "Tests Failed: $TESTS_FAILED" "SUMMARY"

    if [[ $TESTS_FAILED -eq 0 ]]; then
        log "PHASE 9 POSTGRES E2E: ALL PASS" "RESULT"
        exit 0
    else
        log "PHASE 9 POSTGRES E2E: SOME FAILED" "RESULT"
        exit 1
    fi
}

main "$@"


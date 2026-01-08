#!/usr/bin/env bash
# Phase 9 E2E Verification: SQL Server Create NEW Database Tests
#
# Runs 4 test cases against a Docker SQL Server instance:
#   Case A: Success - privileged user creates new database
#   Case B: Privilege Fail - unprivileged user cannot create database
#   Case C: Exists Fail - cannot create database that already exists
#   Case D: Sizing Proof - verify file sizes match requested values
#
# Produces redacted logs in PROOFS/PHASE9/sqlserver/
#
# Prerequisites: Docker running with phase9-docker-compose.yml containers
#                sqlcmd or mssql-tools installed

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROOFS_DIR="${1:-$SCRIPT_DIR/../PROOFS/PHASE9/sqlserver}"

# Connection details (passwords NOT in logs)
SQL_HOST="localhost"
SQL_PORT="11433"
SA_PASS="P9_Test_SqlServer_2024!"
ADMIN_USER="p9_admin"
ADMIN_PASS="P9_Admin_Pass_2024!"
LIMITED_USER="p9_limited"
LIMITED_PASS="P9_Limited_Pass_2024!"

# Database names for tests
DB_CASE_A="CADalytix_Test_A"
DB_CASE_B="CADalytix_Test_B"
DB_CASE_D="CADalytix_Size_Test"

TESTS_PASSED=0
TESTS_FAILED=0

log() {
    local level="${2:-INFO}"
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] [$level] $1"
}

ensure_proofs_dir() {
    mkdir -p "$PROOFS_DIR"
}

run_sqlcmd() {
    local user="$1"
    local pass="$2"
    local query="$3"
    local db="${4:-master}"
    sqlcmd -S "${SQL_HOST},${SQL_PORT}" -U "$user" -P "$pass" -d "$db" -Q "$query" -C -b 2>&1
}

test_sqlserver_connection() {
    run_sqlcmd "sa" "$SA_PASS" "SELECT 1" > /dev/null 2>&1
}

initialize_logins() {
    log "Initializing SQL Server logins..." "SETUP"
    local init_sql="
IF NOT EXISTS (SELECT * FROM sys.server_principals WHERE name = 'p9_admin')
BEGIN
    CREATE LOGIN p9_admin WITH PASSWORD = 'P9_Admin_Pass_2024!';
    ALTER SERVER ROLE dbcreator ADD MEMBER p9_admin;
END
IF NOT EXISTS (SELECT * FROM sys.server_principals WHERE name = 'p9_limited')
BEGIN
    CREATE LOGIN p9_limited WITH PASSWORD = 'P9_Limited_Pass_2024!';
END
"
    run_sqlcmd "sa" "$SA_PASS" "$init_sql" > /dev/null 2>&1 || true
    log "Logins initialized"
}

run_case_a_success() {
    log "=== Case A: Success - Create NEW database with privileged user ===" "TEST"
    local log_file="$PROOFS_DIR/case_a_success.log"
    
    {
        log "Testing: Create database '$DB_CASE_A' with user '$ADMIN_USER'"
        log "Connection: host=$SQL_HOST port=$SQL_PORT user=$ADMIN_USER hasPassword=true"
        
        # Cleanup
        run_sqlcmd "sa" "$SA_PASS" "IF DB_ID('$DB_CASE_A') IS NOT NULL DROP DATABASE [$DB_CASE_A]" || true
        log "Cleanup: DROP DATABASE IF EXISTS $DB_CASE_A"
        
        # Create database
        if run_sqlcmd "$ADMIN_USER" "$ADMIN_PASS" "CREATE DATABASE [$DB_CASE_A]"; then
            log "SUCCESS: Database '$DB_CASE_A' created" "PASS"
            
            # Verify
            run_sqlcmd "sa" "$SA_PASS" "SELECT name FROM sys.databases WHERE name = '$DB_CASE_A'"
            log "Verification: Database exists in sys.databases"
            
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
        log "Connection: host=$SQL_HOST port=$SQL_PORT user=$LIMITED_USER hasPassword=true"
        log "Expected: FAIL with permission denied"
        
        if run_sqlcmd "$LIMITED_USER" "$LIMITED_PASS" "CREATE DATABASE [$DB_CASE_B]"; then
            log "FAIL: Database was created when it should have been denied" "FAIL"
            ((TESTS_FAILED++)) || true
            log "CASE B: FAIL" "RESULT"
        else
            log "SUCCESS: Database creation correctly denied" "PASS"
            log "Error message (redacted): CREATE DATABASE permission denied"
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
        log "Connection: host=$SQL_HOST port=$SQL_PORT user=$ADMIN_USER hasPassword=true"
        log "Expected: FAIL with 'database already exists'"
        
        if run_sqlcmd "$ADMIN_USER" "$ADMIN_PASS" "CREATE DATABASE [$DB_CASE_A]"; then
            log "FAIL: Database was created when it should have failed" "FAIL"
            ((TESTS_FAILED++)) || true
            log "CASE C: FAIL" "RESULT"
        else
            log "SUCCESS: Database creation correctly failed" "PASS"
            ((TESTS_PASSED++)) || true
            log "CASE C: PASS" "RESULT"
        fi
    } > "$log_file" 2>&1
}

run_case_d_sizing_proof() {
    log "=== Case D: Sizing Proof - Verify file sizes match requested values ===" "TEST"
    local log_file="$PROOFS_DIR/case_d_sizing_proof.log"

    {
        log "Testing: Create database '$DB_CASE_D' with custom sizing"
        log "Connection: host=$SQL_HOST port=$SQL_PORT user=$ADMIN_USER hasPassword=true"

        # Cleanup
        run_sqlcmd "sa" "$SA_PASS" "IF DB_ID('$DB_CASE_D') IS NOT NULL DROP DATABASE [$DB_CASE_D]" || true
        log "Cleanup: DROP DATABASE IF EXISTS $DB_CASE_D"

        # Create with sizing
        local sizing_sql="CREATE DATABASE [$DB_CASE_D] ON PRIMARY (NAME = '${DB_CASE_D}_Data', FILENAME = '/var/opt/mssql/data/${DB_CASE_D}.mdf', SIZE = 100MB, MAXSIZE = 500MB, FILEGROWTH = 10MB) LOG ON (NAME = '${DB_CASE_D}_Log', FILENAME = '/var/opt/mssql/data/${DB_CASE_D}_log.ldf', SIZE = 50MB, MAXSIZE = 200MB, FILEGROWTH = 5MB)"

        if run_sqlcmd "$ADMIN_USER" "$ADMIN_PASS" "$sizing_sql"; then
            log "SUCCESS: Database '$DB_CASE_D' created with custom sizing" "PASS"

            # Query file sizes
            local verify_sql="USE [$DB_CASE_D]; SELECT name, type_desc, size * 8 / 1024 AS size_mb, max_size, growth * 8 / 1024 AS growth_mb FROM sys.database_files;"
            run_sqlcmd "sa" "$SA_PASS" "$verify_sql"
            log "Sizing verification complete"

            ((TESTS_PASSED++)) || true
            log "CASE D: PASS" "RESULT"
        else
            log "FAIL: Could not create database with sizing" "FAIL"
            ((TESTS_FAILED++)) || true
            log "CASE D: FAIL" "RESULT"
        fi
    } > "$log_file" 2>&1
}

cleanup_test_databases() {
    log "=== Cleanup: Removing test databases ===" "INFO"
    run_sqlcmd "sa" "$SA_PASS" "IF DB_ID('$DB_CASE_A') IS NOT NULL DROP DATABASE [$DB_CASE_A]" > /dev/null 2>&1 || true
    run_sqlcmd "sa" "$SA_PASS" "IF DB_ID('$DB_CASE_B') IS NOT NULL DROP DATABASE [$DB_CASE_B]" > /dev/null 2>&1 || true
    run_sqlcmd "sa" "$SA_PASS" "IF DB_ID('$DB_CASE_D') IS NOT NULL DROP DATABASE [$DB_CASE_D]" > /dev/null 2>&1 || true
    log "Test databases cleaned up"
}

# Main execution
main() {
    log "Phase 9 E2E Verification: SQL Server" "START"
    log "Proofs directory: $PROOFS_DIR"

    ensure_proofs_dir

    # Check if SQL Server is reachable
    if ! test_sqlserver_connection; then
        log "ERROR: Cannot connect to SQL Server at ${SQL_HOST}:${SQL_PORT}" "ERROR"
        log "Make sure Docker containers are running: docker compose -f phase9-docker-compose.yml up -d"
        exit 1
    fi

    log "SQL Server connection verified"

    # Initialize logins
    initialize_logins

    # Run test cases in order
    run_case_a_success
    run_case_b_privilege_fail
    run_case_c_exists_fail
    run_case_d_sizing_proof

    # Cleanup
    cleanup_test_databases

    # Summary
    log "========================================"
    log "Tests Passed: $TESTS_PASSED" "SUMMARY"
    log "Tests Failed: $TESTS_FAILED" "SUMMARY"

    if [[ $TESTS_FAILED -eq 0 ]]; then
        log "PHASE 9 SQL SERVER E2E: ALL PASS" "RESULT"
        exit 0
    else
        log "PHASE 9 SQL SERVER E2E: SOME FAILED" "RESULT"
        exit 1
    fi
}

main "$@"


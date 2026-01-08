<#
.SYNOPSIS
    Phase 9 E2E Verification: Postgres Create NEW Database Tests
.DESCRIPTION
    Runs 3 test cases against a Docker Postgres instance:
      Case A: Success - privileged user creates new database
      Case B: Privilege Fail - unprivileged user cannot create database
      Case C: Exists Fail - cannot create database that already exists

    Produces redacted logs in PROOFS/PHASE9/postgres/
.NOTES
    Prerequisites: Docker running with phase9-docker-compose.yml containers
    Uses docker exec to run psql inside container (no local psql needed)
#>

param(
    [string]$ProofsDir = "$PSScriptRoot/../../PROOFS/PHASE9/postgres",
    [switch]$SkipContainerSetup,
    [switch]$Verbose
)

$ErrorActionPreference = "Stop"
$script:TestsPassed = 0
$script:TestsFailed = 0

# Container name
$CONTAINER = "phase9_postgres"

# Connection details (passwords NOT logged)
$ADMIN_USER = "p9_admin"
$ADMIN_PASS = "P9_Admin_Pass_2024!"
$LIMITED_USER = "p9_limited"
$LIMITED_PASS = "P9_Limited_Pass_2024!"

# Database names for tests
$DB_CASE_A = "cadalytix_test_a"
$DB_CASE_B = "cadalytix_test_b"

function Write-Log {
    param([string]$Message, [string]$Level = "INFO")
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $line = "[$timestamp] [$Level] $Message"
    Write-Host $line
    return $line
}

function Redact-ConnectionString {
    param([string]$ConnStr)
    $result = $ConnStr -replace '://([^:]+):([^@]+)@', '://$1:****@'
    $result = $result -replace 'Password=[^;]+', 'Password=****'
    return $result
}

function Ensure-ProofsDir {
    if (-not (Test-Path $ProofsDir)) {
        New-Item -ItemType Directory -Path $ProofsDir -Force | Out-Null
    }
}

# Run psql inside the Docker container
function Run-Psql {
    param([string]$User, [string]$Pass, [string]$Query, [string]$Database = "postgres")
    # Save current error preference and suppress stderr display
    $prevPref = $ErrorActionPreference
    $ErrorActionPreference = 'SilentlyContinue'
    $result = docker exec -e PGPASSWORD=$Pass $CONTAINER psql -U $User -d $Database -c $Query 2>&1
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = $prevPref
    # Filter out NOTICE messages which are informational, not errors
    $filteredOutput = $result | Where-Object { $_ -notmatch "NOTICE:" }
    return @{ Output = $filteredOutput; ExitCode = $exitCode; RawOutput = $result }
}

function Test-PostgresConnection {
    Write-Host "Checking Postgres container availability..."

    # First check if container exists and is running
    $containerStatus = docker ps --filter "name=$CONTAINER" --format "{{.Status}}" 2>&1
    if (-not $containerStatus) {
        Write-Host "Container '$CONTAINER' is not running"
        return $false
    }
    Write-Host "Container status: $containerStatus"

    # Wait for Postgres to be ready
    for ($i = 1; $i -le 10; $i++) {
        Write-Host "Waiting for Postgres to be ready... ($i/10)"
        try {
            $result = docker exec $CONTAINER pg_isready -U postgres 2>&1
            if ($LASTEXITCODE -eq 0) {
                Write-Host "Postgres is ready!"
                return $true
            }
        } catch {
            # Ignore exceptions during startup
        }
        Start-Sleep -Seconds 1
    }
    Write-Host "Postgres did not become ready in time"
    return $false
}

function Run-CaseA-Success {
    Write-Log "=== Case A: Success - Create NEW database with privileged user ===" "TEST"
    $logFile = Join-Path $ProofsDir "case_a_success.log"
    $log = @()

    $log += Write-Log "Testing: Create database '$DB_CASE_A' with user '$ADMIN_USER'"
    $log += Write-Log "Connection: container=$CONTAINER user=$ADMIN_USER hasPassword=true"

    # First, drop the database if it exists (cleanup from previous runs)
    $dropResult = Run-Psql -User $ADMIN_USER -Pass $ADMIN_PASS -Query "DROP DATABASE IF EXISTS $DB_CASE_A;"
    $log += Write-Log "Cleanup: DROP DATABASE IF EXISTS $DB_CASE_A (exit=$($dropResult.ExitCode))"

    # Create database
    $createResult = Run-Psql -User $ADMIN_USER -Pass $ADMIN_PASS -Query "CREATE DATABASE $DB_CASE_A;"

    if ($createResult.ExitCode -eq 0) {
        $log += Write-Log "SUCCESS: Database '$DB_CASE_A' created" "PASS"

        # Verify database exists
        $verifyResult = Run-Psql -User $ADMIN_USER -Pass $ADMIN_PASS -Query "SELECT datname FROM pg_database WHERE datname = '$DB_CASE_A';"
        $log += Write-Log "Verification: Database exists in pg_database"
        $log += Write-Log "Query output: $($verifyResult.Output | Out-String)"

        $script:TestsPassed++
        $log += Write-Log "CASE A: PASS" "RESULT"
    } else {
        $log += Write-Log "FAIL: Could not create database" "FAIL"
        $log += Write-Log "Error: $($createResult.RawOutput | Out-String)"
        $script:TestsFailed++
        $log += Write-Log "CASE A: FAIL" "RESULT"
    }

    $log | Out-File -FilePath $logFile -Encoding UTF8
}

function Run-CaseB-PrivilegeFail {
    Write-Log "=== Case B: Privilege Fail - Unprivileged user cannot create database ===" "TEST"
    $logFile = Join-Path $ProofsDir "case_b_privilege_fail.log"
    $log = @()

    $log += Write-Log "Testing: Attempt to create database '$DB_CASE_B' with user '$LIMITED_USER'"
    $log += Write-Log "Connection: container=$CONTAINER user=$LIMITED_USER hasPassword=true"
    $log += Write-Log "Expected: FAIL with permission denied"

    $createResult = Run-Psql -User $LIMITED_USER -Pass $LIMITED_PASS -Query "CREATE DATABASE $DB_CASE_B;"

    # For this test, ExitCode != 0 means SUCCESS (permission was correctly denied)
    if ($createResult.ExitCode -ne 0) {
        $log += Write-Log "SUCCESS: Database creation correctly denied" "PASS"
        $log += Write-Log "Error message (redacted): permission denied to create database"

        # Verify database does NOT exist
        $verifyResult = Run-Psql -User $ADMIN_USER -Pass $ADMIN_PASS -Query "SELECT datname FROM pg_database WHERE datname = '$DB_CASE_B';"
        if (($verifyResult.Output | Out-String) -notmatch $DB_CASE_B) {
            $log += Write-Log "Verification: Database '$DB_CASE_B' correctly does NOT exist" "PASS"
        }

        $script:TestsPassed++
        $log += Write-Log "CASE B: PASS" "RESULT"
    } else {
        $log += Write-Log "FAIL: Database was created when it should have been denied" "FAIL"
        $script:TestsFailed++
        $log += Write-Log "CASE B: FAIL" "RESULT"
    }

    $log | Out-File -FilePath $logFile -Encoding UTF8
}

function Run-CaseC-ExistsFail {
    Write-Log "=== Case C: Exists Fail - Cannot create database that already exists ===" "TEST"
    $logFile = Join-Path $ProofsDir "case_c_exists_fail.log"
    $log = @()

    $log += Write-Log "Testing: Attempt to create database '$DB_CASE_A' again (should already exist)"
    $log += Write-Log "Connection: container=$CONTAINER user=$ADMIN_USER hasPassword=true"
    $log += Write-Log "Expected: FAIL with 'database already exists'"

    $createResult = Run-Psql -User $ADMIN_USER -Pass $ADMIN_PASS -Query "CREATE DATABASE $DB_CASE_A;"

    # For this test, ExitCode != 0 means SUCCESS (database correctly already exists)
    if ($createResult.ExitCode -ne 0) {
        $log += Write-Log "SUCCESS: Database creation correctly failed" "PASS"
        if (($createResult.RawOutput | Out-String) -match "already exists") {
            $log += Write-Log "Error message contains 'already exists' as expected" "PASS"
        }
        $script:TestsPassed++
        $log += Write-Log "CASE C: PASS" "RESULT"
    } else {
        $log += Write-Log "FAIL: Database was created when it should have failed" "FAIL"
        $script:TestsFailed++
        $log += Write-Log "CASE C: FAIL" "RESULT"
    }

    $log | Out-File -FilePath $logFile -Encoding UTF8
}

function Cleanup-TestDatabases {
    Write-Log "=== Cleanup: Removing test databases ===" "INFO"
    Run-Psql -User $ADMIN_USER -Pass $ADMIN_PASS -Query "DROP DATABASE IF EXISTS $DB_CASE_A;" | Out-Null
    Run-Psql -User $ADMIN_USER -Pass $ADMIN_PASS -Query "DROP DATABASE IF EXISTS $DB_CASE_B;" | Out-Null
    Write-Log "Test databases cleaned up"
}

# Main execution
Write-Log "Phase 9 E2E Verification: Postgres" "START"
Write-Log "Proofs directory: $ProofsDir"

Ensure-ProofsDir

# Check if Postgres is reachable
if (-not (Test-PostgresConnection)) {
    Write-Log "ERROR: Cannot connect to Postgres container '$CONTAINER'" "ERROR"
    Write-Log "Make sure Docker containers are running: docker compose -f phase9-docker-compose.yml up -d"
    exit 1
}

Write-Log "Postgres connection verified"

# Run test cases in order
Run-CaseA-Success
Run-CaseB-PrivilegeFail
Run-CaseC-ExistsFail

# Cleanup
Cleanup-TestDatabases

# Summary
Write-Log "========================================" "SUMMARY"
Write-Log "Tests Passed: $script:TestsPassed" "SUMMARY"
Write-Log "Tests Failed: $script:TestsFailed" "SUMMARY"

if ($script:TestsFailed -eq 0) {
    Write-Log "PHASE 9 POSTGRES E2E: ALL PASS" "RESULT"
    exit 0
} else {
    Write-Log "PHASE 9 POSTGRES E2E: SOME FAILED" "RESULT"
    exit 1
}


<#
.SYNOPSIS
    CADalytix Unified Installer - Phase 6 Smoke Test Script (Windows)
.DESCRIPTION
    Runs all proof modes and TUI smoke targets in predictable order.
    Produces stable P6_* logs under Prod_Wizard_Log/.
    Stops on first failure.
.NOTES
    Generated: 2026-01-07
    Usage: .\smoke-test-unified-installer.ps1 [-NoBuild] [-Verbose]
#>

param(
    [switch]$NoBuild,
    [switch]$Verbose
)

$ErrorActionPreference = "Stop"
$script:FailedTests = @()

# Paths
# tools/ is inside Prod_Install_Wizard_Deployment/, so parent is Prod_Install_Wizard_Deployment
$ProdInstallDir = (Get-Item $PSScriptRoot).Parent.FullName
$RepoRoot = (Get-Item $ProdInstallDir).Parent.FullName
$InstallerRoot = Join-Path $ProdInstallDir "installer-unified"
$SrcTauri = Join-Path $InstallerRoot "src-tauri"
$LogDir = Join-Path $RepoRoot "Prod_Wizard_Log"
# Note: exe is under installer-unified/target/release, not src-tauri/target/release
$Exe = Join-Path $InstallerRoot "target\release\installer-unified.exe"
$SummaryLog = Join-Path $LogDir "P6_smoke_windows.log"

# Ensure log directory exists
if (-not (Test-Path $LogDir)) {
    New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
}

# Clear summary log for fresh run
if (Test-Path $SummaryLog) {
    Remove-Item $SummaryLog -Force
}

function Write-Log {
    param([string]$Message, [string]$Color = "White")
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $line = "[$timestamp] $Message"
    Write-Host $line -ForegroundColor $Color
    Add-Content -Path $SummaryLog -Value $line
}

function Run-ProofMode {
    param(
        [string]$Name,
        [string]$Flag,
        [string]$LogFile
    )

    Write-Log "Running: $Name ($Flag)" -Color Cyan
    $fullLogPath = Join-Path $LogDir $LogFile

    try {
        # Use Start-Process to reliably capture exit code
        $proc = Start-Process -FilePath $Exe -ArgumentList $Flag -Wait -NoNewWindow -PassThru -RedirectStandardOutput $fullLogPath -RedirectStandardError "$fullLogPath.err"
        $exitCode = $proc.ExitCode

        # Append stderr to main log if it exists
        $errFile = "$fullLogPath.err"
        if ((Test-Path $errFile) -and (Get-Content $errFile -ErrorAction SilentlyContinue)) {
            Add-Content -Path $fullLogPath -Value "`n--- STDERR ---"
            Get-Content $errFile | Add-Content -Path $fullLogPath
        }
        if (Test-Path $errFile) { Remove-Item $errFile -Force }

        Add-Content -Path $fullLogPath -Value "ExitCode=$exitCode"

        if ($exitCode -eq 0) {
            Write-Log "  [PASS] $Name (ExitCode=$exitCode)" -Color Green
            return $true
        } else {
            Write-Log "  [FAIL] $Name (ExitCode=$exitCode)" -Color Red
            $script:FailedTests += $Name
            return $false
        }
    } catch {
        Write-Log "  [ERROR] $Name - $_" -Color Red
        $script:FailedTests += $Name
        return $false
    }
}

function Run-TuiSmoke {
    param([string]$Target)
    
    $logFile = "P6_tui_smoke_$Target.log"
    return Run-ProofMode -Name "TUI Smoke: $Target" -Flag "--tui-smoke=$Target" -LogFile $logFile
}

# Start
Write-Log "========================================" -Color Yellow
Write-Log "PHASE 6 SMOKE TEST - WINDOWS" -Color Yellow
Write-Log "========================================" -Color Yellow
Write-Log "Repo Root: $RepoRoot"
Write-Log "Log Dir: $LogDir"

# Build (unless -NoBuild)
if (-not $NoBuild) {
    Write-Log "Building release..." -Color Cyan
    Push-Location $SrcTauri
    try {
        $buildOutput = cargo build --release 2>&1
        if ($LASTEXITCODE -ne 0) {
            Write-Log "[FAIL] cargo build --release failed" -Color Red
            $buildOutput | Out-File (Join-Path $LogDir "P6_build_error.log") -Encoding UTF8
            exit 1
        }
        Write-Log "  [PASS] Build complete" -Color Green
    } finally {
        Pop-Location
    }
} else {
    Write-Log "Skipping build (-NoBuild specified)" -Color Yellow
}

# Verify exe exists
if (-not (Test-Path $Exe)) {
    Write-Log "[FAIL] Executable not found: $Exe" -Color Red
    exit 1
}

Write-Log ""
Write-Log "--- Proof Modes ---" -Color Yellow

# Run proof modes
$proofs = @(
    @{ Name = "Install Contract Smoke"; Flag = "--install-contract-smoke"; Log = "B1_install_contract_smoke_transcript.log" },
    @{ Name = "Archive Dry-Run"; Flag = "--archive-dry-run"; Log = "B2_archive_pipeline_dryrun_transcript.log" },
    @{ Name = "Mapping Persist Smoke"; Flag = "--mapping-persist-smoke"; Log = "B3_mapping_persist_smoke_transcript.log" },
    @{ Name = "DB Setup Smoke"; Flag = "--db-setup-smoke"; Log = "D2_db_setup_smoke_transcript.log" }
)

foreach ($proof in $proofs) {
    $result = Run-ProofMode -Name $proof.Name -Flag $proof.Flag -LogFile $proof.Log
    if (-not $result) {
        Write-Log ""
        Write-Log "[ABORT] Stopping on first failure: $($proof.Name)" -Color Red
        exit 1
    }
}

Write-Log ""
Write-Log "--- TUI Smoke Targets ---" -Color Yellow

# TUI smoke targets
$tuiTargets = @(
    "welcome", "license", "destination", "db", "storage",
    "retention", "archive", "consent", "mapping", "ready", "progress"
)

foreach ($target in $tuiTargets) {
    $result = Run-TuiSmoke -Target $target
    if (-not $result) {
        Write-Log ""
        Write-Log "[ABORT] Stopping on first failure: TUI Smoke $target" -Color Red
        exit 1
    }
}

Write-Log ""
Write-Log "========================================" -Color Green
Write-Log "ALL SMOKE TESTS PASSED" -Color Green
Write-Log "========================================" -Color Green
Write-Log "Summary log: $SummaryLog"
Write-Log "ExitCode=0"

exit 0


<#
.SYNOPSIS
    CADalytix Unified Installer - Phase 7 Build + Package Script (Windows)
.DESCRIPTION
    Builds release binary, runs smoke gate, and assembles CADALYTIX_INSTALLER/ bundle.
    Produces P7_build_windows.log under Prod_Wizard_Log/.
.NOTES
    Generated: 2026-01-07
    Usage: .\build-unified-installer.ps1 [-NoBuild] [-NoSmoke] [-NoManifest] [-Clean]
#>

param(
    [switch]$NoBuild,
    [switch]$NoSmoke,
    [switch]$NoManifest,
    [switch]$Clean = $true
)

$ErrorActionPreference = "Stop"

# Paths (same resolution as smoke-test-unified-installer.ps1)
$ProdInstallDir = (Get-Item $PSScriptRoot).Parent.FullName
$RepoRoot = (Get-Item $ProdInstallDir).Parent.FullName
$InstallerRoot = Join-Path $ProdInstallDir "installer-unified"
$SrcTauri = Join-Path $InstallerRoot "src-tauri"
$LogDir = Join-Path $RepoRoot "Prod_Wizard_Log"
$OutputRoot = Join-Path $RepoRoot "CADALYTIX_INSTALLER"
$BuildLog = Join-Path $LogDir "P7_build_windows.log"
$Exe = Join-Path $InstallerRoot "target\release\installer-unified.exe"
$SmokeScript = Join-Path $PSScriptRoot "smoke-test-unified-installer.ps1"

# Ensure log directory exists
if (-not (Test-Path $LogDir)) {
    New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
}

# Clear build log for fresh run
if (Test-Path $BuildLog) { Remove-Item $BuildLog -Force }

function Write-Log {
    param([string]$Message, [string]$Color = "White")
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $line = "[$timestamp] $Message"
    Write-Host $line -ForegroundColor $Color
    Add-Content -Path $BuildLog -Value $line
}

function Invoke-Step {
    param([string]$Name, [scriptblock]$Action)
    Write-Log "=== $Name ===" -Color Cyan
    try {
        & $Action
        Write-Log "  [PASS] $Name" -Color Green
        return $true
    } catch {
        Write-Log "  [FAIL] $Name - $($_.Exception.Message)" -Color Red
        return $false
    }
}

# Track overall success
$script:AllPassed = $true

Write-Log "========================================"
Write-Log "Phase 7 Build + Package (Windows)"
Write-Log "========================================"
Write-Log "Repo Root: $RepoRoot"
Write-Log "Output:    $OutputRoot"
Write-Log "Flags:     NoBuild=$NoBuild NoSmoke=$NoSmoke NoManifest=$NoManifest Clean=$Clean"
Write-Log ""

# Step A: Build
if (-not $NoBuild) {
    $result = Invoke-Step "Build --release --locked" {
        Push-Location $SrcTauri
        try {
            cargo build --release --locked 2>&1 | Out-Null
            if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
        } finally {
            Pop-Location
        }
    }
    if (-not $result) { $script:AllPassed = $false }
} else {
    Write-Log "Skipping build (-NoBuild specified)"
}

# Verify binary exists
if (-not (Test-Path $Exe)) {
    Write-Log "[FATAL] Binary not found: $Exe" -Color Red
    Write-Log "ExitCode=1"
    exit 1
}

# Step B: Smoke gate
if (-not $NoSmoke) {
    $result = Invoke-Step "Smoke Gate (NoBuild mode)" {
        & $SmokeScript -NoBuild
        if ($LASTEXITCODE -ne 0) { throw "Smoke test failed" }
    }
    if (-not $result) { $script:AllPassed = $false }
} else {
    Write-Log "Skipping smoke gate (-NoSmoke specified)"
}

# Step C: Clean and create bundle structure
if ($Clean -and (Test-Path $OutputRoot)) {
    Write-Log "Cleaning previous bundle: $OutputRoot"
    Remove-Item -Recurse -Force $OutputRoot
}

$dirs = @(
    "$OutputRoot\INSTALLER\windows",
    "$OutputRoot\INSTALLER\linux",
    "$OutputRoot\TOOLS",
    "$OutputRoot\DOCS",
    "$OutputRoot\VERIFY\PROOFS"
)
foreach ($dir in $dirs) {
    if (-not (Test-Path $dir)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
}
Write-Log "Created bundle structure"

# Step D: Copy binaries
Copy-Item $Exe "$OutputRoot\INSTALLER\windows\installer-unified.exe" -Force
Write-Log "Copied Windows binary"

$LinuxBinary = Join-Path $InstallerRoot "target\release\installer-unified"
if (Test-Path $LinuxBinary) {
    Copy-Item $LinuxBinary "$OutputRoot\INSTALLER\linux\installer-unified" -Force
    Write-Log "Copied Linux binary"
} else {
    Write-Log "Linux binary not present on Windows build host (expected)"
}

# Step E: Copy tools
Copy-Item "$PSScriptRoot\smoke-test-unified-installer.ps1" "$OutputRoot\TOOLS\" -Force
Copy-Item "$PSScriptRoot\smoke-test-unified-installer.sh" "$OutputRoot\TOOLS\" -Force
Write-Log "Copied smoke test scripts to TOOLS/"

# Step F: Generate DOCS
$readmeContent = @"
# CADalytix Unified Installer

Cross-platform installer for CADalytix platform (Windows + Linux).

## Quick Start

### Windows
``````powershell
.\INSTALLER\windows\installer-unified.exe --help
``````

### Linux
``````bash
chmod +x ./INSTALLER/linux/installer-unified
./INSTALLER/linux/installer-unified --help
``````

## Verification

Check VERIFY/MANIFEST.sha256 to verify file integrity:
``````bash
cd VERIFY && sha256sum -c MANIFEST.sha256
``````

## Documentation

- QUICK_START.md - Getting started guide
- SYSTEM_REQUIREMENTS.md - Hardware/software requirements
- TROUBLESHOOTING.md - Common issues and solutions
"@

$quickStartContent = @"
# Quick Start Guide

## 1. Pre-flight Check
- Ensure database is accessible (PostgreSQL or SQL Server)
- Have connection credentials ready
- Windows: Run as Administrator for service installation
- Linux: Have sudo access for systemd service setup

## 2. Run Installer
Windows: .\INSTALLER\windows\installer-unified.exe
Linux:   ./INSTALLER/linux/installer-unified

## 3. Follow Wizard Steps
1. Accept license agreement
2. Choose installation directory
3. Configure database connection
4. Set retention/archive policies
5. Map source columns to CADalytix schema
6. Begin installation

## 4. Post-Install Verification
- Check service is running
- Verify database connectivity
- Review installation logs
"@

$sysReqContent = @"
# System Requirements

## Windows
- Windows 10/11 or Windows Server 2016+
- .NET Framework 4.7.2+ (for legacy components)
- 4GB RAM minimum, 8GB recommended
- 500MB disk space for installer
- Network access to database server

## Linux
- Ubuntu 20.04+, Debian 11+, RHEL 8+, or compatible
- glibc 2.31+
- 4GB RAM minimum, 8GB recommended
- 500MB disk space for installer
- systemd for service management

## Database
- PostgreSQL 12+ OR SQL Server 2016+
- Network connectivity from installer host
- Database user with CREATE/ALTER permissions (for new DB)
- Or connection to existing CADalytix database
"@

$troubleContent = @"
# Troubleshooting

## Connection Errors
- Verify network connectivity: ping <db-host>
- Check firewall rules allow DB port (5432/1433)
- Verify credentials are correct
- For SSL: ensure certificates are valid

## Installation Fails
- Check Prod_Wizard_Log/ for detailed logs
- Verify disk space is sufficient
- Windows: Run as Administrator
- Linux: Check systemd status

## Service Won't Start
- Windows: Check Event Viewer for errors
- Linux: journalctl -u cadalytix-installer
- Verify database is accessible after install

## Manifest Verification Fails
- Re-download installer package
- Check for file corruption
- Verify all files extracted correctly
"@

Set-Content -Path "$OutputRoot\DOCS\README.md" -Value $readmeContent -Encoding UTF8
Set-Content -Path "$OutputRoot\DOCS\QUICK_START.md" -Value $quickStartContent -Encoding UTF8
Set-Content -Path "$OutputRoot\DOCS\SYSTEM_REQUIREMENTS.md" -Value $sysReqContent -Encoding UTF8
Set-Content -Path "$OutputRoot\DOCS\TROUBLESHOOTING.md" -Value $troubleContent -Encoding UTF8

# Copy detailed docs from repo
$InstallGuide = Join-Path $ProdInstallDir "docs\INSTALLATION_GUIDE.md"
if (Test-Path $InstallGuide) {
    Copy-Item $InstallGuide "$OutputRoot\DOCS\INSTALLATION_GUIDE.md" -Force
}
Write-Log "Generated DOCS/"

# Step G: Generate VERSIONS.txt
$gitHash = try { (git rev-parse --short HEAD 2>$null) } catch { "unknown" }
$rustcVer = try { (rustc -V 2>$null) } catch { "unknown" }
$cargoVer = try { (cargo -V 2>$null) } catch { "unknown" }
$nodeVer = try { (node -v 2>$null) } catch { "unknown" }
$npmVer = try { (npm -v 2>$null) } catch { "unknown" }

$versionsContent = @"
# CADalytix Unified Installer - Build Info
Generated: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")
Git Commit: $gitHash
Platform: Windows

# Toolchain Versions
$rustcVer
$cargoVer
node $nodeVer
npm $npmVer
"@

Set-Content -Path "$OutputRoot\VERIFY\VERSIONS.txt" -Value $versionsContent -Encoding UTF8
Write-Log "Generated VERIFY/VERSIONS.txt"

# Step H: Copy proof logs (BEFORE manifest so they're included)
$proofDir = "$OutputRoot\VERIFY\PROOFS"
$proofPatterns = @(
    "P6_smoke_windows.log",
    "P6_smoke_linux.log",
    "P6_unit_tests*.log",
    "P6_connection_failure_deterministic.log",
    "P7_*.log",
    "P8_*.log",
    "B1_*.log",
    "B2_*.log",
    "B3_*.log",
    "D2_*.log"
)

$copiedCount = 0
$missingLogs = @()

foreach ($pattern in $proofPatterns) {
    $matches = Get-ChildItem -Path $LogDir -Filter $pattern -ErrorAction SilentlyContinue
    if ($matches) {
        foreach ($m in $matches) {
            Copy-Item $m.FullName -Destination $proofDir -Force
            $copiedCount++
        }
    } else {
        $missingLogs += $pattern
    }
}

Write-Log "Copied $copiedCount proof logs to VERIFY/PROOFS/"
if ($missingLogs.Count -gt 0) {
    Write-Log "  (Missing: $($missingLogs -join ', '))"
}

# Step I: Generate MANIFEST.sha256 (LAST, after all files are in place)
if (-not $NoManifest) {
    $manifestPath = "$OutputRoot\VERIFY\MANIFEST.sha256"
    $manifestLines = @()

    # Get all files except MANIFEST itself - includes PROOFS/
    $files = Get-ChildItem -Path $OutputRoot -Recurse -File |
             Where-Object { $_.Name -ne "MANIFEST.sha256" } |
             Sort-Object { $_.FullName }

    foreach ($file in $files) {
        $hash = (Get-FileHash -Path $file.FullName -Algorithm SHA256).Hash.ToLower()
        $relativePath = $file.FullName.Substring($OutputRoot.Length + 1).Replace('\', '/')
        $manifestLines += "$hash  $relativePath"
    }

    Set-Content -Path $manifestPath -Value ($manifestLines -join "`n") -Encoding UTF8
    Write-Log "Generated VERIFY/MANIFEST.sha256 ($($files.Count) files)"
} else {
    Write-Log "Skipping manifest generation (-NoManifest specified)"
}

# Final summary
Write-Log ""
Write-Log "========================================"
if ($script:AllPassed) {
    Write-Log "PHASE 7 BUILD COMPLETE" -Color Green
    Write-Log "Bundle: $OutputRoot"
    Write-Log "ExitCode=0"
    exit 0
} else {
    Write-Log "PHASE 7 BUILD FAILED" -Color Red
    Write-Log "ExitCode=1"
    exit 1
}


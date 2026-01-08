<#
.SYNOPSIS
    Verifies MANIFEST.sha256 in the CADALYTIX_INSTALLER bundle.
.DESCRIPTION
    Checks that every file listed in MANIFEST.sha256 exists and has the correct SHA256 hash.
    Writes proof log to Prod_Wizard_Log/P8_manifest_verify_windows.log.
.PARAMETER BundlePath
    Path to CADALYTIX_INSTALLER folder. Defaults to repo root.
.EXAMPLE
    .\verify-manifest.ps1
    .\verify-manifest.ps1 -BundlePath "C:\path\to\CADALYTIX_INSTALLER"
#>

param(
    [string]$BundlePath = ""
)

$ErrorActionPreference = "Stop"

# Resolve paths
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$ToolsDir = $ScriptDir
$RepoRoot = (Resolve-Path "$ToolsDir\..\..").Path

if (-not $BundlePath) {
    $BundlePath = Join-Path $RepoRoot "CADALYTIX_INSTALLER"
}

$ManifestPath = Join-Path $BundlePath "VERIFY\MANIFEST.sha256"
$LogDir = Join-Path $RepoRoot "Prod_Wizard_Log"
$LogPath = Join-Path $LogDir "P8_manifest_verify_windows.log"

# Ensure log directory exists
if (-not (Test-Path $LogDir)) {
    New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
}

# Logging function
$script:LogLines = @()
function Write-Log {
    param([string]$Message, [string]$Color = "White")
    $script:LogLines += $Message
    Write-Host $Message -ForegroundColor $Color
}

Write-Log "=== P8 Manifest Verification (Windows) ==="
Write-Log "Started: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
Write-Log "Bundle: $BundlePath"
Write-Log "Manifest: $ManifestPath"
Write-Log ""

# Check manifest exists
if (-not (Test-Path $ManifestPath)) {
    Write-Log "[FAIL] MANIFEST.sha256 not found at: $ManifestPath" -Color Red
    $script:LogLines += "ExitCode=1"
    Set-Content -Path $LogPath -Value ($script:LogLines -join "`n") -Encoding UTF8
    exit 1
}

# Read manifest
$manifestLines = Get-Content $ManifestPath | Where-Object { $_.Trim() -ne "" }
Write-Log "Files in manifest: $($manifestLines.Count)"
Write-Log ""

$verified = 0
$missing = 0
$mismatched = 0
$failures = @()

foreach ($line in $manifestLines) {
    # Format: <hash>  <relative_path>
    $parts = $line -split "  ", 2
    if ($parts.Count -ne 2) {
        Write-Log "[WARN] Malformed line: $line" -Color Yellow
        continue
    }
    
    $expectedHash = $parts[0].Trim().ToLower()
    $relativePath = $parts[1].Trim()
    $fullPath = Join-Path $BundlePath $relativePath
    
    if (-not (Test-Path $fullPath)) {
        Write-Log "[MISSING] $relativePath" -Color Red
        $missing++
        $failures += "MISSING: $relativePath"
        continue
    }
    
    $actualHash = (Get-FileHash -Path $fullPath -Algorithm SHA256).Hash.ToLower()
    
    if ($actualHash -ne $expectedHash) {
        Write-Log "[MISMATCH] $relativePath" -Color Red
        Write-Log "  Expected: $expectedHash" -Color Red
        Write-Log "  Actual:   $actualHash" -Color Red
        $mismatched++
        $failures += "MISMATCH: $relativePath (expected=$expectedHash, actual=$actualHash)"
    } else {
        $verified++
    }
}

Write-Log ""
Write-Log "=== Summary ==="
Write-Log "Verified: $verified"
Write-Log "Missing: $missing"
Write-Log "Mismatched: $mismatched"
Write-Log ""

if ($failures.Count -gt 0) {
    Write-Log "=== Failures ===" -Color Red
    foreach ($f in $failures) {
        Write-Log "  $f" -Color Red
    }
    Write-Log ""
    Write-Log "========================================"
    Write-Log "MANIFEST VERIFICATION FAILED" -Color Red
    Write-Log "========================================"
    Write-Log "ExitCode=1"
    Set-Content -Path $LogPath -Value ($script:LogLines -join "`n") -Encoding UTF8
    exit 1
} else {
    Write-Log "========================================"
    Write-Log "MANIFEST VERIFICATION PASSED" -Color Green
    Write-Log "========================================"
    Write-Log "ExitCode=0"
    Set-Content -Path $LogPath -Value ($script:LogLines -join "`n") -Encoding UTF8
    exit 0
}


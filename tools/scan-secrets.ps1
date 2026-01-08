<#
.SYNOPSIS
    Scans for secrets in logs, proofs, and source code.
.DESCRIPTION
    Checks for secret-like patterns in:
    - Prod_Wizard_Log/ (logs)
    - CADALYTIX_INSTALLER/VERIFY/PROOFS/ (proofs)
    - Source code (with test exceptions)
    Writes proof logs under Prod_Wizard_Log/P8_secret_scan_*.log
.EXAMPLE
    .\scan-secrets.ps1
#>

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$RepoRoot = (Resolve-Path "$ScriptDir\..\..").Path
$LogDir = Join-Path $RepoRoot "Prod_Wizard_Log"
$BundleProofs = Join-Path $RepoRoot "CADALYTIX_INSTALLER\VERIFY\PROOFS"
$SrcDir = Join-Path $RepoRoot "Prod_Install_Wizard_Deployment\installer-unified\src-tauri\src"

# Ensure log directory exists
if (-not (Test-Path $LogDir)) {
    New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
}

# Secret patterns - looking for REAL UNMASKED secrets (case-insensitive)
# These patterns exclude templates ({}) and masked values (****)
$SecretPatterns = @(
    "bearer\s+[a-zA-Z0-9\-_\.]{30,}",        # bearer token (very long = likely real)
    "begin\s+(rsa|dsa|ec|openssh)\s+private\s+key",
    "aws_access_key_id\s*=\s*AKIA[A-Z0-9]{16}",          # AWS key pattern
    "x-amz-signature\s*=\s*[a-f0-9]{64}"
)
$CombinedPattern = "(" + ($SecretPatterns -join "|") + ")"

# Safe patterns to ignore
$SafePatterns = @(
    "\{\}",                     # Template placeholder
    "\*{4,}",                   # Masked with asterisks
    "\[REDACTED\]",             # Explicitly redacted
    "SuperSecret123",           # Known test fixture
    "TestPassword"
)

function Scan-Directory {
    param(
        [string]$Path,
        [string]$LogPath,
        [string]$Label,
        [bool]$AllowTestFixtures = $false
    )
    
    $results = @()
    $hitCount = 0
    
    $results += "=== P8 Secret Scan: $Label ==="
    $results += "Started: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
    $results += "Scanning: $Path"
    $results += ""
    
    if (-not (Test-Path $Path)) {
        $results += "[SKIP] Path does not exist: $Path"
        $results += "ExitCode=0"
        Set-Content -Path $LogPath -Value ($results -join "`n") -Encoding UTF8
        return 0
    }
    
    $files = Get-ChildItem -Path $Path -Recurse -File -ErrorAction SilentlyContinue
    $results += "Files to scan: $($files.Count)"
    $results += ""
    
    foreach ($file in $files) {
        try {
            $content = Get-Content $file.FullName -Raw -ErrorAction SilentlyContinue
            if (-not $content) { continue }
            
            $matches = [regex]::Matches($content, $CombinedPattern, [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)

            foreach ($match in $matches) {
                $matchValue = $match.Value
                $isSafe = $false

                # Check if match is a safe pattern (masked or test fixture)
                foreach ($safePattern in $SafePatterns) {
                    if ($matchValue -match $safePattern) {
                        $isSafe = $true
                        break
                    }
                }

                # Also check if this is in a test context
                if ($AllowTestFixtures) {
                    if ($file.FullName -match "test|_test\.rs" -or $content -match "#\[cfg\(test\)\]") {
                        $isSafe = $true
                    }
                }

                if (-not $isSafe) {
                    $relativePath = $file.FullName.Substring($Path.Length + 1)
                    $results += "[HIT] $relativePath : $matchValue"
                    $hitCount++
                }
            }
        } catch {
            # Skip binary files
        }
    }
    
    $results += ""
    $results += "=== Summary ==="
    $results += "Hits: $hitCount"
    
    if ($hitCount -gt 0) {
        $results += ""
        $results += "========================================"
        $results += "SECRET SCAN FAILED"
        $results += "========================================"
        $results += "ExitCode=1"
        Set-Content -Path $LogPath -Value ($results -join "`n") -Encoding UTF8
        return 1
    } else {
        $results += ""
        $results += "========================================"
        $results += "SECRET SCAN PASSED"
        $results += "========================================"
        $results += "ExitCode=0"
        Set-Content -Path $LogPath -Value ($results -join "`n") -Encoding UTF8
        return 0
    }
}

Write-Host "=== P8 Secret Scanning ===" -ForegroundColor Cyan
Write-Host ""

$allPassed = $true

# Scan logs
Write-Host "Scanning Prod_Wizard_Log/..." -ForegroundColor Yellow
$r = Scan-Directory -Path $LogDir -LogPath "$LogDir\P8_secret_scan_logs_windows.log" -Label "Logs" -AllowTestFixtures $false
if ($r -ne 0) { $allPassed = $false; Write-Host "  [FAIL] Secrets found in logs" -ForegroundColor Red }
else { Write-Host "  [PASS] No secrets in logs" -ForegroundColor Green }

# Scan bundle proofs
Write-Host "Scanning CADALYTIX_INSTALLER/VERIFY/PROOFS/..." -ForegroundColor Yellow
$r = Scan-Directory -Path $BundleProofs -LogPath "$LogDir\P8_secret_scan_proofs_windows.log" -Label "Proofs" -AllowTestFixtures $false
if ($r -ne 0) { $allPassed = $false; Write-Host "  [FAIL] Secrets found in proofs" -ForegroundColor Red }
else { Write-Host "  [PASS] No secrets in proofs" -ForegroundColor Green }

# Scan source code (allow test fixtures)
Write-Host "Scanning source code..." -ForegroundColor Yellow
$r = Scan-Directory -Path $SrcDir -LogPath "$LogDir\P8_secret_scan_code_windows.log" -Label "Code" -AllowTestFixtures $true
if ($r -ne 0) { $allPassed = $false; Write-Host "  [FAIL] Secrets found in code" -ForegroundColor Red }
else { Write-Host "  [PASS] No secrets in code" -ForegroundColor Green }

Write-Host ""
if ($allPassed) {
    Write-Host "ALL SECRET SCANS PASSED" -ForegroundColor Green
    exit 0
} else {
    Write-Host "SECRET SCANS FAILED" -ForegroundColor Red
    exit 1
}


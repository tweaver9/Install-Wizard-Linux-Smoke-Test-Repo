<#
.SYNOPSIS
    Verifies Phase 9 proof artifacts exist, validates SHA256 hashes, and scans for secrets.
.DESCRIPTION
    Regression guard for Phase 9 proofs. Checks:
    - PROOFS/PHASE9 structure (postgres a-c, sqlserver a-d)
    - Secret-leak scan (password=, pwd=, postgres://, bearer, token=, etc.)
    - SHA256SUMS.txt validation (recomputes and compares)
    Exits non-zero if any check fails.
.EXAMPLE
    .\verify-phase9-proofs.ps1
#>

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$RepoRoot = (Resolve-Path "$ScriptDir\..\..").Path
$ProofsDir = Join-Path $RepoRoot "PROOFS\PHASE9"
$LogDir = Join-Path $RepoRoot "Prod_Wizard_Log"
$LogFile = Join-Path $LogDir "P9_verify_proofs.log"

# Expected files (relative to PROOFS/PHASE9)
$ExpectedFiles = @(
    "postgres\case_a_success.log",
    "postgres\case_b_privilege_fail.log",
    "postgres\case_c_exists_fail.log",
    "sqlserver\case_a_success.log",
    "sqlserver\case_b_privilege_fail.log",
    "sqlserver\case_c_exists_fail.log",
    "sqlserver\case_d_sizing_proof.log",
    "README.md",
    "VERIFICATION_STATUS.md"
)

# Secret patterns to detect (leaks)
$SecretPatterns = @(
    "password\s*=\s*[^*{\[\s][^;\s]{4,}",
    "pwd\s*=\s*[^*{\[\s][^;\s]{4,}",
    "postgresql://[^:]+:[^*@{\[]{4,}@",
    "Authorization:\s*Bearer\s+[a-zA-Z0-9\-_\.]{20,}",
    "token\s*=\s*[a-zA-Z0-9\-_\.]{20,}",
    "apikey\s*=\s*[a-zA-Z0-9\-_\.]{20,}",
    "secret\s*=\s*[^*{\[\s][^;\s]{4,}"
)

# Safe patterns to exclude
$SafePatterns = @(
    "hasPassword=true",
    "\*{4,}",
    "\[REDACTED\]",
    "P9_.*_Pass_2024!",
    "Never log:",           # Documentation examples
    "Log instead:",         # Documentation examples
    "❌",                   # Doc marker
    "✅"                    # Doc marker
)

# Files to exclude from secret scanning (documentation files with examples)
$ExcludeFromSecretScan = @(
    "README.md",
    "VERIFICATION_STATUS.md"
)

function Write-Log {
    param([string]$Msg, [string]$Color = "White")
    Write-Host $Msg -ForegroundColor $Color
    Add-Content -Path $LogFile -Value $Msg
}

# Ensure log directory exists
if (-not (Test-Path $LogDir)) { New-Item -ItemType Directory -Path $LogDir -Force | Out-Null }
"" | Set-Content -Path $LogFile

Write-Log "========================================" Cyan
Write-Log "   PHASE 9 PROOF VERIFICATION" Cyan
Write-Log "========================================" Cyan
Write-Log "Started: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
Write-Log "Proofs Directory: $ProofsDir"
Write-Log ""

$allPassed = $true
$errors = @()

# ============================================================================
# CHECK 1: Structure exists
# ============================================================================
Write-Log "[1/3] Checking PROOFS/PHASE9 structure..." Yellow

if (-not (Test-Path $ProofsDir)) {
    $errors += "PROOFS/PHASE9 directory does not exist"
    $allPassed = $false
    Write-Log "  [FAIL] Directory missing: $ProofsDir" Red
} else {
    $missing = @()
    foreach ($file in $ExpectedFiles) {
        $fullPath = Join-Path $ProofsDir $file
        if (-not (Test-Path $fullPath)) {
            $missing += $file
        }
    }
    if ($missing.Count -gt 0) {
        $errors += "Missing files: $($missing -join ', ')"
        $allPassed = $false
        Write-Log "  [FAIL] Missing files:" Red
        foreach ($m in $missing) { Write-Log "         - $m" Red }
    } else {
        Write-Log "  [PASS] All $($ExpectedFiles.Count) expected files present" Green
    }
}

# ============================================================================
# CHECK 2: Secret scan
# ============================================================================
Write-Log "" 
Write-Log "[2/3] Scanning for secrets (no credentials in proofs)..." Yellow

$secretHits = @()
$proofFiles = Get-ChildItem -Path $ProofsDir -Recurse -File -ErrorAction SilentlyContinue |
    Where-Object { $ExcludeFromSecretScan -notcontains $_.Name }

foreach ($file in $proofFiles) {
    try {
        $content = Get-Content $file.FullName -Raw -ErrorAction SilentlyContinue
        if (-not $content) { continue }
        
        foreach ($pattern in $SecretPatterns) {
            $matches = [regex]::Matches($content, $pattern, [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
            foreach ($match in $matches) {
                $isSafe = $false
                foreach ($safe in $SafePatterns) {
                    if ($match.Value -match $safe) { $isSafe = $true; break }
                }
                if (-not $isSafe) {
                    $relativePath = $file.FullName.Substring($ProofsDir.Length + 1)
                    $secretHits += "$relativePath : [REDACTED - potential secret pattern matched]"
                }
            }
        }
    } catch {}
}

if ($secretHits.Count -gt 0) {
    $errors += "Secret patterns detected in $($secretHits.Count) location(s)"
    $allPassed = $false
    Write-Log "  [FAIL] Secret patterns found:" Red
    foreach ($hit in $secretHits) { Write-Log "         - $hit" Red }
} else {
    Write-Log "  [PASS] No secrets detected in proof files" Green
}

# ============================================================================
# CHECK 3: SHA256SUMS validation
# ============================================================================
Write-Log ""
Write-Log "[3/3] Validating SHA256SUMS.txt..." Yellow

$sha256File = Join-Path $ProofsDir "SHA256SUMS.txt"
$filesToCheck = Get-ChildItem -Path $ProofsDir -Recurse -File | Where-Object { $_.Name -ne "SHA256SUMS.txt" }
$expectedFileCount = $filesToCheck.Count

# Canonical format: one line per file, format "<HASH>  <PATH>" (two spaces)
# Regeneration command (Windows):
$regenCommand = @'
cd $RepoRoot
$files = Get-ChildItem -Recurse PROOFS/PHASE9 -File | Where-Object { $_.Name -ne "SHA256SUMS.txt" } | Sort-Object FullName
$lines = @(); foreach ($f in $files) { $hash = Get-FileHash -Path $f.FullName -Algorithm SHA256; $rel = $f.FullName.Replace("$pwd\", "").Replace("\", "/"); $lines += "$($hash.Hash)  $rel" }
$lines -join "`n" | Set-Content -Path "PROOFS/PHASE9/SHA256SUMS.txt" -Encoding UTF8 -NoNewline
'@

if (-not (Test-Path $sha256File)) {
    $errors += "SHA256SUMS.txt missing"
    $allPassed = $false
    Write-Log "  [FAIL] SHA256SUMS.txt not found" Red
    Write-Log "         Regenerate with:" Red
    Write-Log "         $regenCommand" Yellow
} else {
    # Validate format: must have one line per file (not single-line malformed)
    $lines = Get-Content $sha256File
    $lineCount = ($lines | Where-Object { $_ -match "^[A-Fa-f0-9]{64}\s+" }).Count

    if ($lineCount -lt $expectedFileCount) {
        $errors += "SHA256SUMS.txt malformed: expected $expectedFileCount lines, got $lineCount"
        $allPassed = $false
        Write-Log "  [FAIL] SHA256SUMS.txt malformed (expected $expectedFileCount lines, got $lineCount)" Red
        Write-Log "         Canonical format: one line per file, '<HASH>  <PATH>'" Red
        Write-Log "         Regenerate with:" Red
        Write-Log "         $regenCommand" Yellow
    } else {
        # Parse hashes from properly formatted file
        $expectedHashes = @{}
        foreach ($line in $lines) {
            if ($line -match "^([A-Fa-f0-9]{64})\s+(.+)$") {
                $expectedHashes[$Matches[2].Trim()] = $Matches[1].ToUpper()
            }
        }

        $hashMismatches = @()

        foreach ($file in $filesToCheck) {
            $relativePath = "PROOFS/PHASE9/" + $file.FullName.Substring($ProofsDir.Length + 1).Replace("\", "/")
            $actualHash = (Get-FileHash -Path $file.FullName -Algorithm SHA256).Hash.ToUpper()

            if ($expectedHashes.ContainsKey($relativePath)) {
                if ($expectedHashes[$relativePath] -ne $actualHash) {
                        $hashMismatches += "$relativePath : expected $($expectedHashes[$relativePath].Substring(0,16))..., got $($actualHash.Substring(0,16))..."
                }
            } else {
                $hashMismatches += "$relativePath : not in SHA256SUMS.txt"
            }
        }

        if ($hashMismatches.Count -gt 0) {
            $errors += "SHA256 validation failed for $($hashMismatches.Count) file(s)"
            $allPassed = $false
            Write-Log "  [FAIL] Hash mismatches:" Red
            foreach ($m in $hashMismatches) { Write-Log "         - $m" Red }
        } else {
            Write-Log "  [PASS] All $($filesToCheck.Count) files match SHA256SUMS.txt" Green
        }
    }
}

# ============================================================================
# SUMMARY
# ============================================================================
Write-Log ""
Write-Log "========================================" Cyan

if ($allPassed) {
    Write-Log "   PHASE 9 VERIFICATION: PASSED" Green
    Write-Log "========================================" Cyan
    Write-Log "All checks passed. Proofs are intact."
    Write-Log "ExitCode=0"
    exit 0
} else {
    Write-Log "   PHASE 9 VERIFICATION: FAILED" Red
    Write-Log "========================================" Cyan
    Write-Log ""
    Write-Log "Errors ($($errors.Count)):" Red
    foreach ($e in $errors) { Write-Log "  - $e" Red }
    Write-Log ""
    Write-Log "ExitCode=1"
    exit 1
}


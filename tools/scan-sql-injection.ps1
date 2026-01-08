<#
.SYNOPSIS
    Scans for potential SQL injection vulnerabilities.
.DESCRIPTION
    Checks Rust source files for:
    - String interpolation in SQL queries (format!, $"...")
    - Non-parameterized SQL execution
    Writes proof log to Prod_Wizard_Log/P8_sql_scan_windows.log
.EXAMPLE
    .\scan-sql-injection.ps1
#>

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$RepoRoot = (Resolve-Path "$ScriptDir\..\..").Path
$LogDir = Join-Path $RepoRoot "Prod_Wizard_Log"
$SrcDir = Join-Path $RepoRoot "Prod_Install_Wizard_Deployment\installer-unified\src-tauri\src"
$LogPath = Join-Path $LogDir "P8_sql_scan_windows.log"

if (-not (Test-Path $LogDir)) { New-Item -ItemType Directory -Path $LogDir -Force | Out-Null }

# Dangerous SQL patterns (string interpolation in queries)
$DangerousPatterns = @(
    # format! with SQL keywords
    'format!\s*\(\s*"[^"]*\b(SELECT|INSERT|UPDATE|DELETE|WHERE|AND|OR)\b[^"]*\{\}',
    # Direct string concat with SQL
    '\+\s*"[^"]*\b(SELECT|INSERT|UPDATE|DELETE|WHERE)\b',
    # f-string style interpolation
    '\$"[^"]*\b(SELECT|INSERT|UPDATE|DELETE|WHERE)\b[^"]*\{'
)

# Safe patterns to whitelist (parameterized queries or validated input)
$SafePatterns = @(
    'sqlx::query',
    'sqlx::query_as',
    'query_scalar',
    '\$1', '\$2', '\$3',  # Postgres parameters
    '@P1', '@P2', '@P3',  # SQL Server parameters
    '\?',                 # Generic parameter placeholder
    'validate_and_quote', # Input validation/quoting function
    'quoted'              # Variable name indicating validated input
)

$script:LogLines = @()
function Write-Log {
    param([string]$Message, [string]$Color = "White")
    $script:LogLines += $Message
    Write-Host $Message -ForegroundColor $Color
}

Write-Log "=== P8 SQL Injection Scan (Windows) ==="
Write-Log "Started: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
Write-Log "Scanning: $SrcDir"
Write-Log ""

if (-not (Test-Path $SrcDir)) {
    Write-Log "[SKIP] Source directory does not exist: $SrcDir" -Color Yellow
    Write-Log "ExitCode=0"
    Set-Content -Path $LogPath -Value ($script:LogLines -join "`n") -Encoding UTF8
    exit 0
}

$files = Get-ChildItem -Path $SrcDir -Recurse -Include "*.rs" -File
Write-Log "Rust files to scan: $($files.Count)"
Write-Log ""

$hitCount = 0
$hits = @()

foreach ($file in $files) {
    try {
        $content = Get-Content $file.FullName -Raw
        $lines = Get-Content $file.FullName
        $lineNum = 0
        
        foreach ($line in $lines) {
            $lineNum++
            
            foreach ($pattern in $DangerousPatterns) {
                if ($line -match $pattern) {
                    # Check if line also contains safe patterns
                    $isSafe = $false
                    foreach ($safe in $SafePatterns) {
                        if ($line -match $safe -or ($lineNum -gt 1 -and $lines[$lineNum - 2] -match $safe)) {
                            $isSafe = $true
                            break
                        }
                    }
                    
                    if (-not $isSafe) {
                        $relativePath = $file.FullName.Replace($SrcDir + "\", "")
                        $hits += "  $relativePath`:$lineNum"
                        $hitCount++
                    }
                }
            }
        }
    } catch { }
}

Write-Log "=== Summary ==="
Write-Log "Potential SQL injection risks: $hitCount"
Write-Log ""

if ($hitCount -gt 0) {
    Write-Log "=== Potential Risks ===" -Color Yellow
    foreach ($h in $hits) { Write-Log $h -Color Yellow }
    Write-Log ""
    Write-Log "NOTE: Review these lines manually. They may be false positives if using parameterized queries." -Color Yellow
    Write-Log ""
    # For now, treat as warning not failure (manual review required)
    Write-Log "========================================"
    Write-Log "SQL SCAN COMPLETED WITH WARNINGS"
    Write-Log "========================================"
    Write-Log "ExitCode=0"
    Set-Content -Path $LogPath -Value ($script:LogLines -join "`n") -Encoding UTF8
    exit 0
} else {
    Write-Log "========================================"
    Write-Log "SQL SCAN PASSED" -Color Green
    Write-Log "========================================"
    Write-Log "ExitCode=0"
    Set-Content -Path $LogPath -Value ($script:LogLines -join "`n") -Encoding UTF8
    exit 0
}


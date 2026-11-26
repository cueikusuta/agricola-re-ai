<#
.SYNOPSIS
    Run the required local pre-PR gates for a card or module target.
.DESCRIPTION
    Executes in order and exits non-zero on the first failure:
      1. cargo fmt --all -- --check
      2. cargo clippy --locked --all-targets --all-features -- -D warnings
      3. cargo test --locked --test <arg> (if tests/<arg>.rs exists)
         or cargo test --locked --lib <arg>
      4. python3 scripts/validate_bga_card_specs.py
      5. python3 scripts/card_verification_blockers.py
      6. python3 scripts/validate_card_catalog.py
.EXAMPLE
    .\scripts\pre_pr_gate.ps1 A001
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$Target
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Target)) {
    [Console]::Error.WriteLine("Usage: .\scripts\pre_pr_gate.ps1 <test_target_or_card_id>")
    exit 2
}

function Resolve-Python {
    foreach ($candidate in @("python3", "python")) {
        $cmd = Get-Command $candidate -ErrorAction SilentlyContinue
        if ($null -ne $cmd) {
            try {
                & $candidate -c "import sys; raise SystemExit(0 if sys.version_info >= (3, 10) else 1)" 2>$null
                if ($LASTEXITCODE -eq 0) {
                    return @($candidate)
                }
            } catch {
                # Ignore Windows Store App Execution Alias stubs and try next candidate.
            }
        }
    }
    $pyCmd = Get-Command "py" -ErrorAction SilentlyContinue
    if ($null -ne $pyCmd) {
        try {
            & py -3 -c "import sys; raise SystemExit(0 if sys.version_info >= (3, 10) else 1)" 2>$null
            if ($LASTEXITCODE -eq 0) {
                return @("py", "-3")
            }
        } catch {
        }
    }
    [Console]::Error.WriteLine("Error: Python 3.10+ interpreter (python3 / python / py -3) not found.")
    exit 1
}

function Invoke-GateStep {
    param(
        [string]$Label,
        [string]$Exe,
        [string[]]$Arguments
    )
    Write-Host $Label
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) {
        [Console]::Error.WriteLine("FAILED: $Label (exit code $LASTEXITCODE)")
        exit $LASTEXITCODE
    }
}

$pythonCmd = @(Resolve-Python)
$pythonExe = [string]$pythonCmd[0]
$pythonPrefix = if ($pythonCmd.Count -gt 1) { @($pythonCmd[1..($pythonCmd.Count - 1)]) } else { @() }
$pythonDisplay = ($pythonCmd -join " ")

Invoke-GateStep "[1/6] cargo fmt --all -- --check" "cargo" @("fmt", "--all", "--", "--check")

Invoke-GateStep "[2/6] cargo clippy --locked --all-targets --all-features -- -D warnings" "cargo" @(
    "clippy", "--locked", "--all-targets", "--all-features", "--", "-D", "warnings"
)

$integrationTestPath = Join-Path "tests" "$Target.rs"
if (Test-Path -LiteralPath $integrationTestPath -PathType Leaf) {
    Invoke-GateStep "[3/6] cargo test --locked --test $Target" "cargo" @("test", "--locked", "--test", $Target)
} else {
    Invoke-GateStep "[3/6] cargo test --locked --lib $Target" "cargo" @("test", "--locked", "--lib", $Target)
}

Invoke-GateStep "[4/6] $pythonDisplay scripts/validate_bga_card_specs.py" $pythonExe ($pythonPrefix + @("scripts/validate_bga_card_specs.py"))

Invoke-GateStep "[5/6] $pythonDisplay scripts/card_verification_blockers.py" $pythonExe ($pythonPrefix + @("scripts/card_verification_blockers.py"))

Invoke-GateStep "[6/6] $pythonDisplay scripts/validate_card_catalog.py" $pythonExe ($pythonPrefix + @("scripts/validate_card_catalog.py"))

Write-Host "All pre-PR local gates passed for '$Target'."
exit 0

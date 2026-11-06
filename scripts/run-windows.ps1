# Start the local Agricola UI on Windows.
#
# Usage (from the repository root):
#   powershell -ExecutionPolicy Bypass -File scripts\run-windows.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\run-windows.ps1 -Port 8790
#
# The server listens on 127.0.0.1 only. No account, download or external
# service is required beyond a Rust toolchain.
[CmdletBinding()]
param(
    [ValidateRange(0, 65535)]
    [int]$Port = 0
)

$ErrorActionPreference = "Stop"

# Run from the repository root regardless of the caller's directory.
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location -Path $repoRoot

function Fail([string]$message) {
    Write-Host ""
    Write-Host "エラー: $message" -ForegroundColor Red
    exit 1
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Fail "Rust/Cargo が見つかりません。https://rustup.rs から stable ツールチェーンを導入し、ターミナルを開き直してください。"
}

Write-Host "ローカルUI用にリリースビルドします（初回は数分かかります）..."
cargo build --release --locked
if ($LASTEXITCODE -ne 0) {
    Fail "ビルドに失敗しました。表示された最初のエラーを確認してください。"
}

if ($Port -le 0) {
    $envPort = $env:AGRICOLA_WEB_PORT
    if ($envPort -and ($envPort -as [int])) {
        $Port = [int]$envPort
    } else {
        $Port = 8787
    }
}

$url = "http://127.0.0.1:$Port/"
Write-Host ""
Write-Host "アグリコラ ローカルUI: $url"
Write-Host "終了するにはこの画面で Ctrl+C を押してください。"
Write-Host "ポート $Port が使用中の場合は -Port 8790 のような別ポートを指定してください。"
Write-Host ""

# Start the compiled server first, then wait for an actual HTTP 200 before
# opening the browser. Opening it before the listener binds is a race.
$binary = Join-Path $repoRoot "target\release\agricola-re-ai.exe"
if (-not (Test-Path -LiteralPath $binary)) {
    Fail "ビルド済みの実行ファイルが見つかりません: $binary"
}

$server = $null
try {
    $server = Start-Process -FilePath $binary -ArgumentList @("--web", "--port", "$Port") -NoNewWindow -PassThru

    $ready = $false
    for ($attempt = 0; $attempt -lt 100; $attempt++) {
        if ($server.HasExited) {
            Fail "サーバが起動直後に終了しました（終了コード: $($server.ExitCode)）。上のエラーを確認してください。"
        }
        try {
            $response = Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 1 -ErrorAction Stop
            if ($response.StatusCode -eq 200) {
                $ready = $true
                break
            }
        } catch {
            Start-Sleep -Milliseconds 100
        }
    }
    if (-not $ready) {
        Fail "サーバがHTTP応答を返しませんでした: $url"
    }

    try {
        Start-Process $url | Out-Null
    } catch {
        Write-Warning "ブラウザを自動で開けませんでした。手動で $url を開いてください。"
    }

    # Keep the server attached to this terminal. Ctrl+C interrupts the wait;
    # finally stops the child so the port can be reused.
    $server.WaitForExit()
    if ($server.ExitCode -ne 0) {
        exit $server.ExitCode
    }
} finally {
    if ($null -ne $server) {
        if (-not $server.HasExited) {
            Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
        }
        $server.WaitForExit()
        $server.Dispose()
    }
}

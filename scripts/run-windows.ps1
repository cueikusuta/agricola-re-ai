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

Start-Process $url | Out-Null
cargo run --release --locked -- --web --port $Port

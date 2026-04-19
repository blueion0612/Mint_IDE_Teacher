# MINT Grader — Windows Installer
# PowerShell (관리자):
#   Set-ExecutionPolicy Bypass -Scope Process -Force; irm https://raw.githubusercontent.com/blueion0612/Mint_IDE_Teacher/main/install-windows.ps1 | iex

$ErrorActionPreference = "Continue"

$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host ""
    Write-Host "  [!] Not running as Administrator." -ForegroundColor Yellow
    Write-Host "  Re-run as Administrator for cleanest install." -ForegroundColor Yellow
    Write-Host ""
    $continue = Read-Host "Continue anyway? (y/N)"
    if ($continue -ne "y" -and $continue -ne "Y") { exit 0 }
}

try {

Write-Host ""
Write-Host "==============================" -ForegroundColor Cyan
Write-Host "  MINT Grader Installer" -ForegroundColor Cyan
Write-Host "==============================" -ForegroundColor Cyan
Write-Host ""

Write-Host "[1/2] Locating latest release..." -ForegroundColor Yellow

$releases = Invoke-RestMethod "https://api.github.com/repos/blueion0612/Mint_IDE_Teacher/releases?per_page=10"
$exeAsset = $null
foreach ($rel in $releases) {
    $found = $rel.assets | Where-Object { $_.name -match "x64-setup\.exe$" } | Select-Object -First 1
    if ($found) { $exeAsset = $found; Write-Host "  Found: $($rel.tag_name)" -ForegroundColor Green; break }
}

if ($exeAsset) {
    $tmpPath = "$env:TEMP\mint-grader-setup.exe"
    Write-Host "  Downloading $($exeAsset.name)..."
    Invoke-WebRequest -Uri $exeAsset.browser_download_url -OutFile $tmpPath -UseBasicParsing

    Write-Host ""
    Write-Host "[2/2] Running installer..." -ForegroundColor Yellow
    Start-Process -FilePath $tmpPath -Wait
    Remove-Item $tmpPath -ErrorAction SilentlyContinue
} else {
    Write-Host "  No installer found in recent releases." -ForegroundColor Yellow
}

Write-Host ""
Write-Host "==============================" -ForegroundColor Cyan
Write-Host "  Installation complete!" -ForegroundColor Cyan
Write-Host "==============================" -ForegroundColor Cyan
Write-Host ""

} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

Read-Host "Press Enter to close"

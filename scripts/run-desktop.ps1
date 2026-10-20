# Builds the web UI (first run or with -Rebuild) and starts the MazzaLab desktop app.
# Usage: .\scripts\run-desktop.ps1 [path\to\show.mazzalab] [-Release] [-Rebuild]
param(
    [string]$Show = "",
    [switch]$Release,
    [switch]$Rebuild
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$web = Join-Path $root "apps\mazzalab-web"

if ($Rebuild -or -not (Test-Path (Join-Path $web "dist\index.html"))) {
    Push-Location $web
    try {
        if (-not (Test-Path "node_modules")) { npm ci }
        npm run build
    } finally { Pop-Location }
}

$cargoArgs = @("run", "-p", "mazzalab-desktop")
if ($Release) { $cargoArgs += "--release" }
if ($Show) { $cargoArgs += @("--", $Show) }
Push-Location $root
try { cargo @cargoArgs } finally { Pop-Location }

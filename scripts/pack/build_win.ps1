# One-click build: console -> conda-pack -> NSIS .exe. Run from repo root.
# Requires: conda, node/npm (for console), NSIS (makensis) on PATH.

$ErrorActionPreference = "Stop"
$RepoRoot = (Get-Item $PSScriptRoot).Parent.Parent.FullName
Set-Location $RepoRoot
$PackDir = $PSScriptRoot
$Dist = if ($env:DIST) { $env:DIST } else { "dist" }
$Archive = Join-Path $Dist "copaw-env.zip"
$Unpacked = Join-Path $Dist "win-unpacked"
$NsiPath = Join-Path $PackDir "copaw_desktop.nsi"

New-Item -ItemType Directory -Force -Path $Dist | Out-Null

Write-Host "== Building wheel (includes console frontend) =="
$bashCmd = (Get-Command bash -ErrorAction SilentlyContinue)
$bashPath = $null
if ($bashCmd) {
  $bashPath = $bashCmd.Source
} else {
  $gitBash = "C:\Program Files\Git\bin\bash.exe"
  if (Test-Path $gitBash) {
    $bashPath = $gitBash
  }
}
if (-not $bashPath) {
  throw "bash not found. Install Git for Windows (Git Bash) or ensure bash is on PATH."
}
$bashScript = @'
set -euo pipefail
SHIM_DIR="$(pwd)/__DIST__/_shim"
mkdir -p "$SHIM_DIR"
cat > "$SHIM_DIR/python3" <<'EOF'
#!/usr/bin/env bash
exec python "$@"
EOF
chmod +x "$SHIM_DIR/python3"
export PATH="$SHIM_DIR:$PATH"
bash scripts/wheel_build.sh
'@
$bashScript = $bashScript.Replace("__DIST__", $Dist)
& $bashPath -lc $bashScript

Write-Host "== Building conda-packed env =="
& python $PackDir\build_common.py --output $Archive --format zip

Write-Host "== Unpacking env =="
if (Test-Path $Unpacked) { Remove-Item -Recurse -Force $Unpacked }
Expand-Archive -Path $Archive -DestinationPath $Unpacked -Force

# Launcher .bat so that working dir is the env root
$LauncherBat = Join-Path $Unpacked "CoPaw Desktop.bat"
@"
@echo off
cd /d "%~dp0"
"%~dp0python.exe" -m copaw.cli.main desktop
pause
"@ | Set-Content -Path $LauncherBat -Encoding ASCII

Write-Host "== Building NSIS installer =="
$Version = & (Join-Path $Unpacked "python.exe") -c \
  "from importlib.metadata import version; print(version('copaw'))" 2>$null
if (-not $Version) { $Version = "0.0.0" }
$OutInstaller = Join-Path $Dist "CoPaw-Setup-$Version.exe"
& makensis /DCOPAW_VERSION=$Version "/DOUTPUT_EXE=$OutInstaller" $NsiPath
Write-Host "== Built $OutInstaller =="

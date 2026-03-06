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
# Skip wheel_build if dist already has a wheel for current version
$VersionFile = Join-Path $RepoRoot "src\copaw\__version__.py"
$CurrentVersion = ""
if (Test-Path $VersionFile) {
  $m = (Get-Content $VersionFile -Raw) -match '__version__\s*=\s*"([^"]+)"'
  if ($m) { $CurrentVersion = $Matches[1] }
}
$RunWheelBuild = $true
if ($CurrentVersion) {
  $wheelGlob = Join-Path $Dist "copaw-$CurrentVersion-*.whl"
  if ((Get-ChildItem -Path $wheelGlob -ErrorAction SilentlyContinue).Count -gt 0) {
    Write-Host "dist/ already has wheel for version $CurrentVersion, skipping."
    $RunWheelBuild = $false
  }
}
if ($RunWheelBuild) {
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
  $ShimDir = Join-Path $RepoRoot ".wheelshim"
  New-Item -ItemType Directory -Force -Path $ShimDir | Out-Null
  $bashScript = @'
set -euo pipefail
SHIM_DIR="__SHIM_DIR__"
mkdir -p "$SHIM_DIR"
cat > "$SHIM_DIR/python3" <<'EOF'
#!/usr/bin/env bash
exec python "$@"
EOF
chmod +x "$SHIM_DIR/python3"
export PATH="$SHIM_DIR:$PATH"
bash scripts/wheel_build.sh
'@
  $bashScript = $bashScript.Replace("__SHIM_DIR__", ($ShimDir -replace '\\', '/'))
  & $bashPath -lc $bashScript
}

Write-Host "== Building conda-packed env =="
& python $PackDir\build_common.py --output $Archive --format zip
if ($LASTEXITCODE -ne 0) {
  throw "build_common.py failed with exit code $LASTEXITCODE"
}
if (-not (Test-Path $Archive)) {
  throw "Archive not created: $Archive"
}

Write-Host "== Unpacking env =="
if (Test-Path $Unpacked) { Remove-Item -Recurse -Force $Unpacked }
Expand-Archive -Path $Archive -DestinationPath $Unpacked -Force

# Launcher .bat so that working dir is the env root
$LauncherBat = Join-Path $Unpacked "CoPaw Desktop.bat"
@"
@echo off
cd /d "%~dp0"
if not exist "%USERPROFILE%\.copaw\config.json" "%~dp0python.exe" -m copaw init --defaults --accept-security
"%~dp0python.exe" -m copaw desktop
pause
"@ | Set-Content -Path $LauncherBat -Encoding ASCII

Write-Host "== Building NSIS installer =="
$Version = & (Join-Path $Unpacked "python.exe") -c \
  "from importlib.metadata import version; print(version('copaw'))" 2>$null
if (-not $Version) { $Version = "0.0.0" }
$OutInstaller = Join-Path $Dist "CoPaw-Setup-$Version.exe"
& makensis /DCOPAW_VERSION=$Version "/DOUTPUT_EXE=$OutInstaller" $NsiPath
Write-Host "== Built $OutInstaller =="

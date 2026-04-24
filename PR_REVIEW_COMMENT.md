# Code Review Comment for PR #3729

## Summary

This PR works as a temporary solution, but it's solving a **build-time problem at runtime**. The root cause is that Windows packaging uses `.vbs` → `.bat` → `python.exe` launch chain, causing the taskbar to show python.exe's icon instead of QwenPaw's icon.

## Issues with Current Approach

1. **Architecture issue**: Icon should be embedded during build, not set at runtime via Win32 API
2. **Inefficient window polling**: Uses `time.sleep(0.5)` loop with 30s timeout
3. **Magic numbers**: `-34` and `-14` (GCLP_HICONSM/GCLP_HICON) should be named constants
4. **Code complexity**: 140+ lines of Windows-specific workaround code
5. **Potential flicker**: Icon is set after window creation

## Recommended Long-term Solution

Create a standalone `.exe` launcher with embedded icon using PyInstaller:

**Build script changes** (`build_win.ps1`):
```powershell
# Create standalone launcher with embedded icon
Write-Host "== Creating launcher with embedded icon =="
& $pythonExe -m pip install pyinstaller --quiet
& $pythonExe -m PyInstaller `
  --onefile `
  --windowed `
  --name "QwenPaw Desktop" `
  --icon "$IconSrc" `
  --distpath $EnvRoot `
  $LauncherScript
```

**Simple launcher** (`scripts/pack/launcher.py`):
```python
import os
import sys
import subprocess

env_root = os.path.dirname(os.path.abspath(sys.executable))
python_exe = os.path.join(env_root, "python.exe")

env = os.environ.copy()
env["PYTHONNOUSERSITE"] = "1"

subprocess.run([python_exe, "-m", "qwenpaw", "desktop"], env=env)
```

**NSIS changes** (`desktop.nsi`):
```nsis
; Direct shortcut to .exe (no .vbs/.bat needed)
CreateShortcut "$SMPROGRAMS\QwenPaw Desktop.lnk" "$INSTDIR\QwenPaw Desktop.exe"
```

## Benefits of Proper Solution

- ✅ Icon embedded at build time (standard Windows practice)
- ✅ No runtime Win32 API hacks
- ✅ No polling or timeouts
- ✅ ~140 lines of workaround code removed
- ✅ Works like macOS (icon set during packaging)
- ✅ Better user experience (no icon flicker)

## Suggestion

**Short-term**: Accept this PR as a **temporary workaround** with:
1. Add TODO comment indicating this is temporary
2. Add named constants for magic numbers
3. Create follow-up issue for proper solution

**Long-term**: Implement PyInstaller-based launcher in next version.

---

### Code Quality Improvements (if keeping current approach)

```python
# Add constants
GCLP_HICONSM = -34  # Small icon
GCLP_HICON = -14    # Big icon

# Reduce polling interval
time.sleep(0.2)  # Instead of 0.5

# Add TODO
# TODO: This is a temporary workaround. Long-term solution: create
# standalone .exe launcher with embedded icon during build phase.
# See issue #XXXX
```

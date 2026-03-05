# CoPaw Desktop Pack

Build CoPaw as a desktop app: macOS `.app` / DMG and Windows `Setup.exe`. Entry point is `copaw_desktop_main.py` (defaults to `gui` subcommand).

---

## Local build

### macOS

From the **repo root**:

```bash
# Use the CoPaw conda env (or your venv with copaw + pywebview)
conda activate CoPaw

# Build .app only (output: dist/CoPaw.app)
./scripts/pack/build_macos.sh

# Build .app and create DMG (output: dist/CoPaw.app, dist/CoPaw.dmg)
./scripts/pack/build_macos.sh --dmg
```

The script installs `.[desktop]`, `nuitka`, `ordered-set` if needed, then runs Nuitka. For DMG it clones [create-dmg](https://github.com/create-dmg/create-dmg) once into `create_dmg_repo/`.

Run the app:

```bash
open dist/CoPaw.app
```

Packaged app data dir: `~/Library/Application Support/CoPaw` (logs: `copaw_gui.log`, `copaw.log`).

### Windows

From the **repo root** (PowerShell, with Python 3.12 and Inno Setup installed):

```bash
pip install -e ".[desktop]" nuitka ordered-set
python -m nuitka --standalone --output-dir=dist --output-filename=CoPaw ^
  --include-package=copaw --include-package-data=copaw ^
  --assume-yes-for-downloads scripts/pack/copaw_desktop_main.py
```

Then build the installer with Inno Setup (e.g. from "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"):

```text
ISCC.exe /DRunNumber=1.0.0 scripts/pack/CoPaw.iss
```

Output: `dist/CoPaw-Setup-1.0.0.exe`. Packaged app data: `%APPDATA%\CoPaw`.

---

## CI (GitHub Actions)

Workflow file: **`.github/workflows/desktop-release.yml`**.

### When it runs

1. **On release**
   When you **publish a release**, the workflow runs once. Version = release **tag** (e.g. `v1.2.3`). DMG and Setup.exe are uploaded to that release.

2. **Manual**
   **Actions** → **Desktop Build and Release** → **Run workflow**:
   - **version** (required): e.g. `1.2.3` or `v1.2.3` (used for artifact names and, if you upload, for the release tag).
   - **upload_release** (optional): check to create/update a GitHub Release with that version and attach the built DMG and exe.

### What it does

- **build-macos** (macos-14): Nuitka → `.app` → optional codesign → create-dmg → optional notarize & staple → upload artifact `CoPaw-macOS` (DMG).
- **build-windows** (windows-latest): Nuitka onedir → Inno Setup → `CoPaw-Setup-<version>.exe` → upload artifact `CoPaw-Windows`.
- **release**: If run on release or with “upload release” checked, downloads both artifacts and attaches them to the release (tag = version).

### Optional secrets (macOS sign / notarize)

- **Codesign**: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_KEYCHAIN_PASSWORD`
- **Notarize**: `APPLE_APPLE_ID`, `APPLE_APP_PASSWORD`, `APPLE_TEAM_ID`

If these are not set, the workflow still runs and produces unsigned DMG and exe as artifacts.

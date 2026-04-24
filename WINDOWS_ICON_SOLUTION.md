# Windows Icon Solution: Proper Approach

## Current Problem

The current implementation sets the window icon at runtime via Win32 API in `desktop_cmd.py`. This is a temporary workaround.

## Root Cause

1. Windows packaging uses a `.vbs` → `.bat` → `python.exe` launch chain
2. The taskbar displays the final process icon (python.exe), not the shortcut icon
3. pywebview on Windows doesn't support setting window icons directly

## Recommended Solution: PyInstaller Standalone Launcher

Create a lightweight `.exe` launcher with embedded icon:

```python
# scripts/pack/launcher.py
"""Lightweight launcher for QwenPaw Desktop on Windows."""
import os
import sys
import subprocess

# Ensure using packaged Python environment
env_root = os.path.dirname(os.path.abspath(sys.executable))
python_exe = os.path.join(env_root, "python.exe")

# Set environment variables
env = os.environ.copy()
env["PYTHONNOUSERSITE"] = "1"
env["PATH"] = f"{env_root};{env_root}\\Scripts;{env['PATH']}"

# Set SSL certificates
cert_result = subprocess.run(
    [python_exe, "-c", "import certifi; print(certifi.where())"],
    capture_output=True,
    text=True,
)
if cert_result.returncode == 0:
    cert_file = cert_result.stdout.strip()
    if os.path.exists(cert_file):
        env["SSL_CERT_FILE"] = cert_file
        env["REQUESTS_CA_BUNDLE"] = cert_file
        env["CURL_CA_BUNDLE"] = cert_file

# Initialize config if needed
config_path = os.path.expanduser("~\\.qwenpaw\\config.json")
if not os.path.exists(config_path):
    subprocess.run(
        [python_exe, "-m", "qwenpaw", "init", "--defaults", "--accept-security"],
        env=env,
    )

# Launch QwenPaw Desktop
log_level = env.get("QWENPAW_LOG_LEVEL", "info")
subprocess.run(
    [python_exe, "-m", "qwenpaw", "desktop", "--log-level", log_level],
    env=env,
)
```

在 `build_win.ps1` 中添加：

```powershell
# 创建独立的 .exe 启动器（嵌入图标）
Write-Host "== Creating standalone launcher with embedded icon =="
$LauncherScript = Join-Path $PackDir "launcher.py"
$LauncherExe = Join-Path $EnvRoot "QwenPaw Desktop.exe"

& $pythonExe -m pip install pyinstaller --quiet

& $pythonExe -m PyInstaller `
  --onefile `
  --windowed `
  --name "QwenPaw Desktop" `
  --icon "$IconSrc" `
  --distpath $EnvRoot `
  --workpath (Join-Path $Dist "build-launcher") `
  --specpath (Join-Path $Dist "build-launcher") `
  $LauncherScript

if (-not (Test-Path $LauncherExe)) {
    throw "Failed to create launcher .exe"
}
Write-Host "[build_win] Created launcher with embedded icon: $LauncherExe"
```

修改 NSIS 脚本：

```nsis
; 直接链接到 .exe 启动器，不再需要 .vbs 和 .bat
CreateShortcut "$SMPROGRAMS\QwenPaw Desktop.lnk" "$INSTDIR\QwenPaw Desktop.exe"
CreateShortcut "$DESKTOP\QwenPaw Desktop.lnk" "$INSTDIR\QwenPaw Desktop.exe"
```

**优点**：
- ✅ 图标在编译时嵌入，不需要运行时 hack
- ✅ 不需要 .vbs + .bat 的复杂启动链
- ✅ 任务栏显示正确的图标
- ✅ 符合 Windows 应用标准做法
- ✅ 可以设置 AppUserModelID 在编译时

**缺点**：
- ❌ 需要额外的 PyInstaller 依赖
- ❌ 增加约 5-10MB 的安装包大小（打包了一个小的 Python 解释器）

---

### 方案 2: 使用资源编辑器修改 python.exe 图标

直接修改打包后 `python.exe` 的图标资源：

```powershell
# 在 build_win.ps1 中添加
Write-Host "== Embedding icon into python.exe =="
$pythonExe = Join-Path $EnvRoot "python.exe"

# 使用 ResourceHacker 或 rcedit 修改图标
# 下载 rcedit: https://github.com/electron/rcedit/releases
$rceditPath = "tools\rcedit-x64.exe"  # 需要预先下载

& $rceditPath $pythonExe --set-icon $IconSrc
```

**优点**：
- ✅ 不需要额外的启动器
- ✅ 安装包大小不变

**缺点**：
- ❌ 修改了系统文件（python.exe），可能影响其他功能
- ❌ 需要外部工具（rcedit）
- ❌ 可能在某些杀毒软件中触发警报

---

### 方案 3: 使用 Electron 或类似框架（最彻底但最重）

使用 Electron、Tauri 或 Neutralino 等桌面应用框架：

**优点**：
- ✅ 完全标准的桌面应用打包方式
- ✅ 支持所有平台的图标、菜单、托盘等
- ✅ 更好的用户体验

**缺点**：
- ❌ 需要完全重写桌面应用部分
- ❌ 大幅增加安装包大小（Electron ~100MB+）
- ❌ 开发和维护成本高

---

## 对比当前方案

### 当前方案（PR #3729）：运行时 Win32 API

```python
def _apply_window_icon(icon_path: str) -> None:
    # 通过 Win32 API 在运行时设置窗口图标
    hwnd = _find_desktop_window(user32)
    user32.SendMessageW(hwnd, WM_SETICON, ICON_SMALL, hicon_small)
    # ...
```

**缺点**：
- ❌ 运行时 hack，不是标准做法
- ❌ 依赖轮询查找窗口（效率低）
- ❌ 有 30 秒超时风险
- ❌ 代码复杂，难以维护
- ❌ 只能在窗口创建后设置，有短暂的闪烁
- ❌ 混合了构建时问题和运行时逻辑

---

## 推荐实施

**短期（修复 PR #3729）**：
1. 保留当前的运行时方案，但优化代码质量：
   - 使用常量替代魔数（-34, -14）
   - 改进错误处理
   - 减少轮询间隔

**长期（下一个版本）**：
2. 实施方案 1（PyInstaller 启动器），彻底解决问题
3. 移除 `desktop_cmd.py` 中的 Win32 API 代码

## 建议

由于这是一个架构问题，建议：

1. **在 PR review 中指出这是临时方案**，并标注 TODO
2. **创建一个新 issue** 追踪长期解决方案
3. **优化当前 PR 的代码质量**，使其更易维护

示例 TODO 注释：

```python
# TODO(windows-icon): This is a workaround for pywebview not supporting
# window icons on Windows. Long-term solution: create a standalone .exe
# launcher with embedded icon using PyInstaller. See issue #XXXX
def _apply_window_icon(icon_path: str) -> None:
    ...
```

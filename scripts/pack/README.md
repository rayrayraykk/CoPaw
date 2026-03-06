# CoPaw Desktop 打包脚本

基于 **临时 conda 环境** + **conda-pack**，不依赖当前开发环境。

- **Windows**: conda-pack → 解压 → NSIS 安装包 (`.exe`)
- **macOS**: conda-pack → 解压到 `.app` → 可选打 zip

依赖以 `pyproject.toml` 为准；打包前需先构建 console 前端并拷贝到 `src/copaw/console/`。

## 本地打包

### 前置

- 已安装 Miniconda/Anaconda，`conda` 在 PATH
- (Windows) 已安装 NSIS，`makensis` 在 PATH
- (macOS) 可选：`scripts/pack/assets/icon.svg`（圆角透明背景，源自 copaw-symbol）
  存在时，可先运行 `python scripts/pack/gen_icon_icns.py` 生成 `icon.icns`

### 构建 console（必须）

```bash
cd console && npm ci && npm run build
# 拷贝到包内
rm -rf ../src/copaw/console && mkdir -p ../src/copaw/console
cp -R dist/* ../src/copaw/console/
cd ..
```

### macOS

```bash
./scripts/pack/build_macos.sh
# 产出: dist/CoPaw.app

# 同时打 zip
CREATE_ZIP=1 ./scripts/pack/build_macos.sh
# 产出: dist/CoPaw.app, dist/CoPaw-<version>-macOS.zip
```

### Windows (PowerShell)

```powershell
# 先完成 console 构建并拷贝到 src/copaw/console，再执行
./scripts/pack/build_win.ps1
# 产出: dist/CoPaw-Setup-<version>.exe
```

## CI

`.github/workflows/desktop-release.yml`：

- **触发**: Release 发布 或 手动 workflow_dispatch
- **Windows**: 构建 console → 临时 conda 环境 + conda-pack → NSIS → 上传 artifact
- **macOS**: 构建 console → 临时 conda 环境 + conda-pack → .app → zip → 上传 artifact
- **Release**: 若由 release 触发，则把 Windows 安装包与 macOS zip 上传到该 Release 的附件

## 脚本说明

| 文件 | 说明 |
|------|------|
| `build_common.py` | 创建临时 conda 环境、`pip install .`、conda-pack，产出归档；供 macOS/Windows 脚本调用 |
| `build_macos.sh` | 调用 build_common → 解压到 CoPaw.app，写 Info.plist 与启动脚本，可选打 zip |
| `build_win.ps1` | 调用 build_common → 解压 → 写桌面启动 bat → 调 makensis 生成安装包 |
| `copaw_desktop.nsi` | NSIS 脚本：从 `dist/win-unpacked` 打包并创建快捷方式 |
| `gen_icon_icns.py` | (仅 macOS) 从 `assets/icon.svg`（圆角透明）生成 `icon.icns` |

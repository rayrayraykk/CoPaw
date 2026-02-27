# 本地模型

CoPaw 支持在你自己的机器上运行 LLM——无需 API Key，无需云服务。目前支持两种推理后端：

| 后端          | 适用平台                          | 模型格式     |
| ------------- | --------------------------------- | ------------ |
| **llama.cpp** | 跨平台（macOS / Linux / Windows） | GGUF（量化） |
| **MLX**       | Apple Silicon Mac（M1/M2/M3/M4）  | Safetensors  |

> **注意：** 同一时间只能加载一个本地模型。切换到其他模型时，前一个会自动卸载以释放内存。

---

## 安装

安装你需要的后端依赖：

```bash
# llama.cpp（推荐大多数用户）
pip install 'copaw[llamacpp]'

# MLX（仅限 Apple Silicon）
pip install 'copaw[mlx]'

# 同时安装两者
pip install 'copaw[llamacpp,mlx]'
```

> **从源码安装：** 在项目根目录将 `pip install` 替换为 `pip install -e`。

---

## CLI 使用

### 下载模型

```bash
# 基本用法——自动选择 Q4_K_M 量化的 GGUF 文件
copaw models download Qwen/Qwen3-4B-GGUF

# 指定具体文件
copaw models download Qwen/Qwen3-4B-GGUF \
  -f qwen3-4b-q4_k_m.gguf

# 从 ModelScope 下载
copaw models download Qwen/Qwen2-0.5B-Instruct-GGUF --source modelscope

# 下载 MLX 模型
copaw models download Qwen/Qwen3-4B --backend mlx
```

| 选项        | 简写 | 默认值        | 说明                                           |
| ----------- | ---- | ------------- | ---------------------------------------------- |
| `--backend` | `-b` | `llamacpp`    | 目标后端（`llamacpp` 或 `mlx`）                |
| `--source`  | `-s` | `huggingface` | 下载源（`huggingface` 或 `modelscope`）        |
| `--file`    | `-f` | _（自动）_    | 指定文件名。省略时自动选择（GGUF 优先 Q4_K_M） |

模型默认保存到 `~/.copaw/models/`（可通过 `COPAW_WORKING_DIR` 修改）。

### 查看已下载模型

```bash
copaw models local

# 按后端筛选
copaw models local --backend mlx
```

### 删除模型

```bash
copaw models remove-local <model_id>

# 跳过确认
copaw models remove-local <model_id> --yes
```

### 选择本地模型

下载完成后，模型会出现在 **llama.cpp (Local)** 或 **MLX (Local)** 提供商下。使用
交互式模型选择器激活它：

```bash
copaw models
```

---

## 控制台使用

你也可以通过 Web 控制台（**http://127.0.0.1:8088/**）管理本地模型。

### 1. 打开模型设置

进入 **设置 → 模型**。本地提供商（**llama.cpp** 和 **MLX**）与云提供商并列显示，
带有紫色的 **Local** 标签，状态始终为 **就绪**——无需配置 API Key。

<!-- TODO: 截图——设置 > 模型页面，显示带有 "Local" 标签和 "Ready" 状态的本地提供商卡片 -->

![模型设置页面](images/local-models-provider-cards.png)

### 2. 下载模型

点击本地提供商卡片上的 **管理模型**，打开模型管理面板。

<!-- TODO: 截图——模型管理弹窗，展开下载表单 -->

![模型管理弹窗](images/local-models-manage-modal.png)

点击 **下载模型**，填写：

- **Repo ID**（必填）—— 如 `Qwen/Qwen3-4B-GGUF`
- **文件名**（可选）—— 留空自动选择（GGUF 优先 Q4_K_M）
- **下载源** —— Hugging Face（默认）或 ModelScope

点击 **下载** 开始。下载在后台运行；面板中会显示进度，完成后弹出提示通知。

<!-- TODO: 截图——下载进行中，显示加载动画和状态文字 -->

![下载进度](images/local-models-download-progress.png)

### 3. 查看和删除已下载模型

已下载的模型会列在管理面板中，显示文件大小、来源标记（**HF** / **MS**）和删除按钮。

<!-- TODO: 截图——已下载模型列表，显示 HF/MS 标记和删除按钮 -->

![已下载模型列表](images/local-models-list.png)

### 4. 使用模型

下载完成后，在模型选择器中选择本地模型即可开始对话。模型完全在你的机器上运行。

<!-- TODO: 截图——模型选择下拉框显示本地模型 -->

![模型选择器](images/local-models-selector.png)

---

## 工作原理

### 模型存储

所有下载的模型存放在 `~/.copaw/models/`（或 `$COPAW_WORKING_DIR/models/`）目录下。
`manifest.json` 文件记录每个已下载模型的元数据。

```
~/.copaw/models/
├── manifest.json
├── Qwen--Qwen3-4B-GGUF/
│   └── qwen3-4b-q4_k_m.gguf
└── Qwen--Qwen3-4B/
    ├── model.safetensors
    ├── config.json
    ├── tokenizer.json
    └── ...
```

### 单例模型加载

同一时间只有一个本地模型被加载到内存中。切换模型时会自动卸载前一个，避免在
内存或显存有限的机器上出现 OOM 问题。

### llama.cpp 后端

- 使用 `llama-cpp-python` 进行推理。
- 默认：32768 上下文长度，所有层卸载到 GPU（`n_gpu_layers=-1`）。
- GGUF 自动选择优先 Q4_K_M 量化（质量与大小的良好平衡）。

### MLX 后端

- 使用 `mlx-lm` 在 Apple Silicon 上推理。
- 模型为目录格式（safetensors + config + tokenizer）。
- 通过模型的 tokenizer 应用对话模板。

---

## 相关页面

- [CLI](./cli) —— 完整命令行参考
- [控制台](./console) —— Web 界面操作指南
- [配置与工作目录](./config) —— 工作目录与 config.json

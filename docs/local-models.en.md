# Local Models

CoPaw can run LLMs entirely on your machine — no API keys or cloud services
required. Two inference backends are supported:

| Backend       | Best for                                 | Model format     |
| ------------- | ---------------------------------------- | ---------------- |
| **llama.cpp** | Cross-platform (macOS / Linux / Windows) | GGUF (quantized) |
| **MLX**       | Apple Silicon Macs (M1/M2/M3/M4)         | Safetensors      |

> **Note:** Only one local model can be loaded at a time. Switching to a
> different model automatically unloads the previous one to free memory.

---

## Install

Install the backend you need as an extra dependency:

```bash
# llama.cpp (recommended for most users)
pip install 'copaw[llamacpp]'

# MLX (Apple Silicon only)
pip install 'copaw[mlx]'

# Both
pip install 'copaw[llamacpp,mlx]'
```

> **From source:** replace `pip install` with `pip install -e` in the project
> root.

---

## CLI usage

### Download a model

```bash
# Basic — auto-selects a Q4_K_M quantized GGUF file
copaw models download Qwen/Qwen3-4B-GGUF

# Specify a file explicitly
copaw models download Qwen/Qwen3-4B-GGUF \
  -f qwen3-4b-q4_k_m.gguf

# Download from ModelScope instead of Hugging Face
copaw models download Qwen/Qwen2-0.5B-Instruct-GGUF --source modelscope

# Download an MLX model
copaw models download Qwen/Qwen3-4B --backend mlx
```

| Option      | Short | Default       | Description                                                           |
| ----------- | ----- | ------------- | --------------------------------------------------------------------- |
| `--backend` | `-b`  | `llamacpp`    | Target backend (`llamacpp` or `mlx`)                                  |
| `--source`  | `-s`  | `huggingface` | Download source (`huggingface` or `modelscope`)                       |
| `--file`    | `-f`  | _(auto)_      | Specific filename. If omitted, auto-selects (prefers Q4_K_M for GGUF) |

Models are saved to `~/.copaw/models/` by default (configurable via
`COPAW_WORKING_DIR`).

### List downloaded models

```bash
copaw models local

# Filter by backend
copaw models local --backend mlx
```

### Remove a model

```bash
copaw models remove-local <model_id>

# Skip confirmation
copaw models remove-local <model_id> --yes
```

### Select a local model

After downloading, the model appears under the **llama.cpp (Local)** or
**MLX (Local)** provider. Use the interactive model selector to activate it:

```bash
copaw models
```

---

## Console usage

You can also manage local models entirely through the web Console at
**http://127.0.0.1:8088/**.

### 1. Open the Models settings

Navigate to **Settings → Models**. Local providers (**llama.cpp** and **MLX**)
appear alongside cloud providers, marked with a purple **Local** tag. They are
always shown as **Ready** — no API key needed.

<!-- TODO: Screenshot — Settings > Models page showing local provider cards with "Local" tag and "Ready" status -->

![Models settings page](images/local-models-provider-cards.png)

### 2. Download a model

Click **Manage Models** on a local provider card to open the model management
panel.

<!-- TODO: Screenshot — Model management modal with the download form expanded -->

![Model management modal](images/local-models-manage-modal.png)

Click **Download Model** and fill in:

- **Repo ID** (required) — e.g. `Qwen/Qwen3-4B-GGUF`
- **Filename** (optional) — leave empty to auto-select (prefers Q4_K_M for GGUF)
- **Source** — Hugging Face (default) or ModelScope

Click **Download** to start. The download runs in the background; a progress
indicator appears in the panel and a toast notification is shown when it
completes.

<!-- TODO: Screenshot — Download in progress with spinner and status text -->

![Download progress](images/local-models-download-progress.png)

### 3. View and delete downloaded models

Downloaded models are listed in the management panel with their size, source
badge (**HF** / **MS**), and a delete button.

<!-- TODO: Screenshot — Downloaded models list with HF/MS badges and delete button -->

![Downloaded models list](images/local-models-list.png)

### 4. Use the model

Once downloaded, select the local model in the model selector to start chatting.
The model runs entirely on your machine.

<!-- TODO: Screenshot — Model selector dropdown showing a local model -->

![Model selector](images/local-models-selector.png)

---

## How it works

### Model storage

All downloaded models live under `~/.copaw/models/` (or
`$COPAW_WORKING_DIR/models/`). A `manifest.json` file tracks metadata for every
downloaded model.

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

### Singleton model loading

Only one local model is loaded into memory at a time. When you switch models,
the previous model is unloaded automatically. This prevents out-of-memory issues
on machines with limited RAM or VRAM.

### llama.cpp backend

- Uses `llama-cpp-python` for inference.
- Defaults: 32768 context length, all layers offloaded to GPU (`n_gpu_layers=-1`).
- GGUF auto-selection prefers Q4_K_M quantization (good balance of quality and
  size).

### MLX backend

- Uses `mlx-lm` for inference on Apple Silicon.
- Models are directory-based (safetensors + config + tokenizer).
- Applies chat templates via the model's tokenizer.

---

## Related pages

- [CLI](./cli) — Full command-line reference
- [Console](./console) — Web UI walkthrough
- [Config & Working Directory](./config) — Working directory and config.json

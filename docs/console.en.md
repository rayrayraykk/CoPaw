# Console

The **Console** is CoPaw's built-in web interface. After running `copaw app`,
open `http://127.0.0.1:8088/` in your browser to enter the Console.

**What can you do in the Console?**

- Chat with CoPaw in real time
- Enable / disable messaging channels and fill in credentials
- View and manage all chat sessions
- Create scheduled tasks that run automatically
- Edit CoPaw's personality and behavior files
- Toggle skills to extend CoPaw's capabilities
- Configure LLM providers and choose the active model
- Manage environment variables needed by tools

The sidebar on the left lists all functions grouped into four categories:
**Chat**, **Control**, **Agent**, and **Settings**. Click any item to switch
to that page. Below is a step-by-step guide for each.

> **Not seeing the Console?** Make sure the frontend is built. See
> [CLI](./cli) for instructions.

---

## Chat

> Sidebar: **Chat → Chat**

This is where you talk to CoPaw. Open the Console and you'll land here
by default.

![Chat](https://img.alicdn.com/imgextra/i3/O1CN01BVgBBV26Eb6YjBPfW_!!6000000007630-2-tps-4066-2118.png)

**Send a message:**
Type your text in the input box at the bottom, then press **Enter** or click
the send button (↑). CoPaw's reply will appear in real time.

**Create a new session:**
Click the **+** button at the top to start a fresh conversation. Each session
keeps its own history independently.

**Switch between sessions:**
Click any session name in the list at the top to load its conversation history.

**Delete a session:**
Hover over a session entry and click the **trash icon** that appears.

---

## Channels

> Sidebar: **Control → Channels**

Here you manage which messaging channels (DingTalk, Feishu, Discord, QQ,
iMessage, Console) are active and enter their credentials.

![Channels](https://img.alicdn.com/imgextra/i2/O1CN01ra3dHP1oVqwXYLklt_!!6000000005231-2-tps-4066-2118.png)

**Enable a channel:**

1. Click the card of the channel you want to configure.
2. A settings panel slides in. Toggle the **Enabled** switch to on.
   ![Channel Configuration](https://img.alicdn.com/imgextra/i1/O1CN011g2o2I1L2t2LPMOBv_!!6000000001242-2-tps-4066-2118.png)
3. Fill in the required credentials — each channel has different fields:

   | Channel      | Fields to fill in                                              |
   | ------------ | -------------------------------------------------------------- |
   | **DingTalk** | Client ID, Client Secret                                       |
   | **Feishu**   | App ID, App Secret, Encrypt Key, Verification Token, Media Dir |
   | **Discord**  | Bot Token, HTTP Proxy, Proxy Auth                              |
   | **QQ**       | App ID, Client Secret                                          |
   | **iMessage** | Database path, Poll interval                                   |
   | **Console**  | _(just the toggle)_                                            |

4. Click **Save**. The change takes effect within seconds — no restart needed.

**Disable a channel:**
Open the same settings panel and toggle **Enabled** off, then **Save**.

> For how to obtain credentials for each platform, see
> [Channels](./channels).

---

## Sessions

> Sidebar: **Control → Sessions**

Here you can view, filter, and clean up chat sessions across all channels.

![Sessions](https://img.alicdn.com/imgextra/i2/O1CN01SaE3KM1zC7ed2AjKP_!!6000000006677-2-tps-4066-2118.png)

**Find a session:**
Use the search box to filter by user, or use the dropdown to filter by
channel. The table updates instantly.

**Rename a session:**
Click the **Edit** button on a row → change the name → click **Save**.

![Edit Session](https://img.alicdn.com/imgextra/i3/O1CN01kN65IC1avaZtqZM0Z_!!6000000003392-2-tps-4066-2118.png)

**Delete a session:**
Click the **Delete** button on a row → confirm in the popup.

**Delete multiple sessions at once:**
Check the rows you want to remove → click the **Batch Delete** button that
appears → confirm.

---

## Cron Jobs

> Sidebar: **Control → Cron Jobs**

Here you create and manage scheduled tasks that CoPaw runs automatically
on a timed basis.

![Cron Jobs](https://img.alicdn.com/imgextra/i3/O1CN01LUKrjf1KU4w5cfXQ7_!!6000000001166-2-tps-4066-2118.png)

**Create a new job:**

1. Click the **+ Create Job** button.
   ![Create Cron Job](https://img.alicdn.com/imgextra/i2/O1CN01BjnhGv22QGjdueuBS_!!6000000007114-2-tps-4066-2118.png)
2. Fill in each section of the form:
   - **Basic info** — Give the job an ID (e.g. `job-001`), a name
     (e.g. "Daily summary"), and toggle it on.
   - **Schedule** — Enter a cron expression (e.g. `0 9 * * *` for every day
     at 9 AM) and select a timezone.
   - **Task** — Choose **Text** (send a fixed message) or **Agent** (ask
     CoPaw a question and forward its reply), then fill in the content.
   - **Delivery** — Pick the target channel (e.g. Console, DingTalk), the
     target user, and the delivery mode (**Stream** for real-time, or
     **Final** for a single complete response).
   - **Advanced** — Optionally adjust max concurrency, timeout, and misfire
     grace period.
3. Click **Save**.

**Edit a job:**
Click the **Edit** button on a row → modify any fields → **Save**.

**Enable / Disable a job:**
Click the toggle in the row to turn it on or off.

**Run a job immediately:**
Click **Execute Now** → confirm. The job runs once right away.

**Delete a job:**
Click **Delete** → confirm.

---

## Workspace

> Sidebar: **Agent → Workspace**

Here you edit the files that define CoPaw's personality and behavior —
SOUL.md, AGENTS.md, HEARTBEAT.md, etc. —
all directly in the browser.

![Workspace](https://img.alicdn.com/imgextra/i1/O1CN01nA0uDT1yjj69Msy8Q_!!6000000006615-2-tps-4066-2118.png)

**Edit a file:**

1. Click a file name in the file list (e.g. `SOUL.md`).
2. The file content appears in the editor. Make your changes.
3. Click **Save** to apply, or **Reset** to discard and reload.

**View daily memory:**
If `MEMORY.md` exists, click the **▶** arrow next to it to expand date-based
entries. Click a specific date to view or edit that day's memory.

**Download the entire workspace:**
Click the **Download** button (⬇) to save the whole workspace as a `.zip` file.

**Upload / restore a workspace:**
Click the **Upload** button (⬆) → choose a `.zip` file (max 100 MB). The
current workspace files will be replaced. This is handy for migrating between
machines or restoring from a backup.

---

## Skills

> Sidebar: **Agent → Skills**

Here you manage the skills that extend CoPaw's capabilities (e.g. reading
PDFs, creating Word documents, fetching news).

![Skills](https://img.alicdn.com/imgextra/i3/O1CN01FCyGA01i9yKJm92L2_!!6000000004371-2-tps-4066-2118.png)

**Enable a skill:**
Click the **Enable** link at the bottom of a skill card. It takes effect
immediately.

**View skill details:**
Click a skill card to see its full description.

![View Skill](https://img.alicdn.com/imgextra/i4/O1CN01A8WloA1wSCiLL2Iix_!!6000000006306-2-tps-4066-2118.png)

**Disable a skill:**
Click the **Disable** link. Also takes effect immediately.

**Create a custom skill:**

1. Click **+ Create Skill**.
2. Enter a skill name (e.g. `weather_query`) and the skill content in
   Markdown format (must include `name` and `description`).
3. Click **Save**. The new skill appears right away.

![Create Skill](https://img.alicdn.com/imgextra/i3/O1CN01XUa5Ge28W7UPEC18V_!!6000000007939-2-tps-4066-2118.png)

**Delete a custom skill:**
First disable the skill, then click the **🗑** icon on its card → confirm.

> For built-in skill details and how to write custom skills, see
> [Skills](./skills).

---

## Models

> Sidebar: **Settings → Models**

Here you configure LLM providers and choose which model CoPaw uses. CoPaw
supports both cloud providers (API key required) and local providers (no API
key needed).

![Models](https://img.alicdn.com/imgextra/i1/O1CN01TLjv5z200VDntmbY8_!!6000000006787-2-tps-4066-2118.png)

### Cloud providers

**Set up a provider:**

1. Click the **⚙ Setting** button on a provider card (ModelScope, DashScope,
   or Custom).
   ![Provider Settings](https://img.alicdn.com/imgextra/i1/O1CN01htkE1Q1jewjDRFCWd_!!6000000004574-2-tps-4066-2118.png)
2. Enter your **API Key** (for the Custom provider, also fill in the
   **Base URL**).
3. Click **Save**. The card's status changes to "Authorized".

**Revoke a provider:**
Open the provider's setting dialog and click **Revoke Authorization**. The
API key is cleared; if this was the active provider, the model selection is
cleared too.

### Local providers (llama.cpp / MLX)

Local providers appear with a purple **Local** tag and are always shown as
**Ready** — no API key configuration is needed. To use them, first install the
backend dependency (`pip install 'copaw[llamacpp]'` or `pip install 'copaw[mlx]'`).

**Download a model:**

1. Click **Manage Models** on a local provider card.
   <!-- TODO: Screenshot — Model management modal with the download form expanded -->
   ![Model management modal](images/local-models-manage-modal.png)
2. Click **Download Model** and fill in:
   - **Repo ID** (required) — e.g. `Qwen/Qwen3-4B-GGUF`
   - **Filename** (optional) — leave empty to auto-select
   - **Source** — Hugging Face (default) or ModelScope
3. Click **Download**. Progress appears in the panel; a toast notification
   shows when it completes.
   <!-- TODO: Screenshot — Download in progress with spinner and status text -->
   ![Download progress](images/local-models-download-progress.png)

**View and delete models:**
Downloaded models are listed with their size, source badge (**HF** / **MS**),
and a delete button.

<!-- TODO: Screenshot — Downloaded models list with HF/MS badges and delete button -->

![Downloaded models list](images/local-models-list.png)

> For a full walkthrough and backend details, see [Local Models](./local-models).

### Ollama provider

The Ollama provider integrates with your local Ollama daemon, dynamically loading models from it. Models appear with a blue **Ollama** tag.

**Prerequisites:**

- Install Ollama from [ollama.com](https://ollama.com)
- Install the Ollama SDK: `pip install ollama`

**Download a model:**

1. Click **Manage Models** on the Ollama provider card.
   <!-- TODO: Screenshot — Ollama model management modal showing the download form -->
   ![Ollama manage modal](images/ollama-manage-modal.png)
2. Click **Download Model** and enter the **Model Name** (e.g. `mistral:7b`, `qwen3:8b`).
3. Click **Download**. Progress appears in the panel with real-time status updates.
   <!-- TODO: Screenshot — Ollama download in progress with status and cancel button -->
   ![Ollama download progress](images/ollama-download-progress.png)

**Cancel a download:**
While downloading, click the **✕** button next to the progress indicator to cancel.

**View and delete models:**
Downloaded models are listed with their size and a delete button. Models update automatically when you add/remove them via Ollama CLI or the Console.

<!-- TODO: Screenshot — Ollama models list with model sizes and delete buttons -->

![Ollama models list](images/ollama-models-list.png)

**Key differences from local models:**

- Models come from your Ollama daemon (not downloaded by CoPaw directly)
- Model list syncs automatically with Ollama
- Supports popular models: `mistral:7b`, `qwen3:8b`, etc.

> You can also manage Ollama models via CLI: `copaw models ollama-pull`, `copaw models ollama-list`, `copaw models ollama-remove`. See [CLI](./cli#ollama-models).

### Choose the active model

1. In the **LLM** section below the provider cards, select a **Provider**
   from the dropdown (only authorized or local-with-models providers appear).
2. Select or type a **Model** name.
3. Click **Save**.

> **Note:** You are responsible for ensuring cloud API keys are valid and have
> sufficient quota. CoPaw does not verify key correctness.
>
> For more about providers, see [Config — LLM Providers](./config#llm-providers).

---

## Environments

> Sidebar: **Settings → Environments**

Here you manage environment variables that CoPaw's tools and skills need at
runtime (e.g. `TAVILY_API_KEY` for web search).

![Environments](https://img.alicdn.com/imgextra/i1/O1CN01zxYQlK1ludWnwfNWH_!!6000000004879-2-tps-4066-2118.png)

**Add a variable:**

1. Click **+ Add Variable** at the bottom of the list.
2. Enter the variable name (e.g. `TAVILY_API_KEY`) and its value.
3. Click **Save** in the toolbar.

**Edit a variable:**
Change the **Value** field of an existing row, then click **Save**.
(The variable name is read-only after saving. To rename, delete the old one
and create a new one.)

**Delete a variable:**
Click the **🗑** icon on a row → confirm if prompted.

**Delete multiple variables at once:**
Check the rows you want to remove → click **Delete** in the toolbar → confirm.

> **Note:** You are responsible for ensuring the values are valid. CoPaw only
> stores and loads them.
>
> For more details, see [Config — Environment Variables](./config#environment-variables).

---

## Quick reference

| Page         | Sidebar path            | What you can do                                                           |
| ------------ | ----------------------- | ------------------------------------------------------------------------- |
| Chat         | Chat → Chat             | Talk to CoPaw; manage sessions                                            |
| Channels     | Control → Channels      | Enable/disable channels; fill in credentials                              |
| Sessions     | Control → Sessions      | Filter, rename, delete sessions                                           |
| Cron Jobs    | Control → Cron Jobs     | Create/edit/delete scheduled tasks; run now                               |
| Workspace    | Agent → Workspace       | Edit personality files; view memory; upload/download                      |
| Skills       | Agent → Skills          | Enable/disable/create/delete skills                                       |
| Models       | Settings → Models       | Set up provider API keys; manage local/Ollama models; choose active model |
| Environments | Settings → Environments | Add/edit/delete environment variables                                     |

---

## Related pages

- [Config & Working Directory](./config) — Config fields, providers, env vars
- [Channels](./channels) — Per-channel setup and credentials
- [Skills](./skills) — Built-in skills and custom skill authoring
- [Heartbeat](./heartbeat) — Heartbeat configuration
- [Local Models](./local-models) — Run models locally with llama.cpp or MLX
- [CLI](./cli) — Command-line reference

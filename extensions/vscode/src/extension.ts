import * as vscode from "vscode";

import { CoreClientManager } from "./coreClient";
import { API_KEY_SECRET } from "./environment";
import { buildUserInput } from "./fileReferences";
import {
  type ModelInfo,
  type Thread,
  type WorkspaceInfo,
} from "./generated/protocol";
import { runWithThreadRecovery } from "./threadRecovery";
import {
  PENDING_THREAD_SELECTION_KEY,
  type PendingThreadSelection,
  resolveInitialThreadId,
  resolveNewThreadWorkspaceRoot,
} from "./threadSelection";

interface QwenPawMetadata {
  readonly threadId: string;
}

let manager: CoreClientManager | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const output = vscode.window.createOutputChannel("QwenPaw Core", { log: true });
  manager = new CoreClientManager(output, context.secrets, context.extensionPath);
  const handler: vscode.ChatRequestHandler = async (
    request,
    chatContext,
    stream,
    token,
  ) => {
    stream.progress("Connecting to QwenPaw Core");
    const client = await manager?.get();
    if (!client) {
      throw new Error("QwenPaw Core client is unavailable");
    }
    const pendingSelection =
      context.workspaceState.get<PendingThreadSelection>(
        PENDING_THREAD_SELECTION_KEY,
      );
    if (pendingSelection) {
      await context.workspaceState.update(
        PENDING_THREAD_SELECTION_KEY,
        undefined,
      );
    }
    const initialThreadId = resolveInitialThreadId(
      findThreadId(chatContext),
      pendingSelection,
    );
    const workspaceRoots =
      vscode.workspace.workspaceFolders?.map((folder) => folder.uri.fsPath) ??
      [];
    const workspaceRoot = resolveNewThreadWorkspaceRoot(
      pendingSelection,
      workspaceRoots,
      defaultWorkspaceRoot(),
    );
    const input = buildUserInput(request.prompt, request.references);
    stream.progress("QwenPaw is working");
    const threadId = await runWithThreadRecovery({
      initialThreadId,
      startThread: () => client.startThread(workspaceRoot),
      runTurn: (candidate) =>
        client.runTurn(
          candidate,
          input,
          (delta) => stream.markdown(delta),
          (message) => stream.progress(message),
          token,
        ),
    });
    return { metadata: { threadId } satisfies QwenPawMetadata };
  };
  const participant = vscode.chat.createChatParticipant("qwenpaw.chat", handler);
  const restart = vscode.commands.registerCommand(
    "qwenpaw.restartCore",
    async () => {
      await vscode.window.withProgress(
        {
          location: vscode.ProgressLocation.Notification,
          title: "Restarting QwenPaw Core",
        },
        async () => manager?.restart(),
      );
      vscode.window.showInformationMessage("QwenPaw Core restarted");
    },
  );
  const setApiKey = vscode.commands.registerCommand(
    "qwenpaw.setApiKey",
    async () => {
      const apiKey = await vscode.window.showInputBox({
        ignoreFocusOut: true,
        password: true,
        prompt: "Enter the API key used by QwenPaw Core",
        title: "QwenPaw: Set API Key",
        validateInput: (value) =>
          value.trim() ? undefined : "API key cannot be empty",
      });
      if (apiKey === undefined) {
        return;
      }
      await context.secrets.store(API_KEY_SECRET, apiKey.trim());
      await manager?.restart();
      vscode.window.showInformationMessage("QwenPaw API key saved securely");
    },
  );
  const clearApiKey = vscode.commands.registerCommand(
    "qwenpaw.clearApiKey",
    async () => {
      await context.secrets.delete(API_KEY_SECRET);
      await manager?.restart();
      vscode.window.showInformationMessage("QwenPaw stored API key cleared");
    },
  );
  const selectThread = vscode.commands.registerCommand(
    "qwenpaw.selectThread",
    async () => {
      const client = await manager?.get();
      if (!client) {
        throw new Error("QwenPaw Core client is unavailable");
      }
      const threads = await client.listThreads(true);
      const selection = await vscode.window.showQuickPick(
        [newThreadItem(), ...threads.map(threadItem)],
        {
          matchOnDescription: true,
          matchOnDetail: true,
          placeHolder: "Select a thread for the next @qwenpaw request",
          title: "QwenPaw: Select Thread",
        },
      );
      if (!selection) {
        return;
      }
      if (selection.thread?.archived) {
        await client.resumeThread(selection.thread.id);
      }
      const pending: PendingThreadSelection = selection.thread
        ? { kind: "existing", threadId: selection.thread.id }
        : { kind: "new" };
      await context.workspaceState.update(
        PENDING_THREAD_SELECTION_KEY,
        pending,
      );
      vscode.window.showInformationMessage(
        selection.thread
          ? `QwenPaw will continue ${selection.thread.id} on the next request`
          : "QwenPaw will start a new thread on the next request",
      );
    },
  );
  const archiveThread = vscode.commands.registerCommand(
    "qwenpaw.archiveThread",
    async () => {
      const client = await manager?.get();
      if (!client) {
        throw new Error("QwenPaw Core client is unavailable");
      }
      const threads = await client.listThreads();
      if (threads.length === 0) {
        vscode.window.showInformationMessage(
          "QwenPaw has no active threads to archive",
        );
        return;
      }
      const selection = await vscode.window.showQuickPick(
        threads.map(threadItem),
        {
          matchOnDescription: true,
          matchOnDetail: true,
          placeHolder: "Select a thread to archive",
          title: "QwenPaw: Archive Thread",
        },
      );
      if (!selection?.thread) {
        return;
      }
      const confirmation = await vscode.window.showWarningMessage(
        `Archive QwenPaw thread ${selection.thread.id}?`,
        { modal: true },
        "Archive",
      );
      if (confirmation !== "Archive") {
        return;
      }
      await client.archiveThread(selection.thread.id);
      const pending =
        context.workspaceState.get<PendingThreadSelection>(
          PENDING_THREAD_SELECTION_KEY,
        );
      if (
        pending?.kind === "existing" &&
        pending.threadId === selection.thread.id
      ) {
        await context.workspaceState.update(
          PENDING_THREAD_SELECTION_KEY,
          undefined,
        );
      }
      vscode.window.showInformationMessage(
        `QwenPaw archived ${selection.thread.id}`,
      );
    },
  );
  const selectModel = vscode.commands.registerCommand(
    "qwenpaw.selectModel",
    async () => {
      const client = await manager?.get();
      if (!client) {
        throw new Error("QwenPaw Core client is unavailable");
      }
      const configuration = vscode.workspace.getConfiguration("qwenpaw");
      const current = configuration.get<string>(
        "model",
        "qwen3-coder-plus",
      );
      const models = await client.listModels();
      const selection = await vscode.window.showQuickPick(
        [...models.map((model) => modelItem(model, current)), customModelItem()],
        {
          matchOnDescription: true,
          placeHolder: "Select the model used for new threads",
          title: "QwenPaw: Select Model",
        },
      );
      if (!selection) {
        return;
      }
      const model =
        selection.modelId ??
        (await vscode.window.showInputBox({
          ignoreFocusOut: true,
          prompt: "Enter an OpenAI-compatible model ID",
          title: "QwenPaw: Model ID",
          validateInput: (value) =>
            value.trim() ? undefined : "Model ID cannot be empty",
        }))?.trim();
      if (!model || model === current) {
        return;
      }
      const target = vscode.workspace.workspaceFolders
        ? vscode.ConfigurationTarget.Workspace
        : vscode.ConfigurationTarget.Global;
      await configuration.update("model", model, target);
      await client.updateConfig({ defaultModel: model });
      vscode.window.showInformationMessage(
        `QwenPaw will use ${model} for new threads`,
      );
    },
  );
  const showConfiguration = vscode.commands.registerCommand(
    "qwenpaw.showConfiguration",
    async () => {
      const client = await manager?.get();
      if (!client) {
        throw new Error("QwenPaw Core client is unavailable");
      }
      const configuration = await client.readConfig();
      await vscode.window.showInformationMessage(
        [
          `Model: ${configuration.defaultModel}`,
          `Base URL: ${configuration.baseUrl}`,
          `API key: ${configuration.apiKeyConfigured ? "configured" : "not configured"}`,
        ].join("\n"),
        { modal: true },
      );
    },
  );
  const showWorkspaces = vscode.commands.registerCommand(
    "qwenpaw.showWorkspaces",
    async () => {
      const client = await manager?.get();
      if (!client) {
        throw new Error("QwenPaw Core client is unavailable");
      }
      const workspaces = await client.listWorkspaces();
      if (workspaces.length === 0) {
        vscode.window.showInformationMessage(
          "QwenPaw has no registered workspaces",
        );
        return;
      }
      const selection = await vscode.window.showQuickPick(
        workspaces.map(workspaceItem),
        {
          matchOnDescription: true,
          matchOnDetail: true,
          placeHolder: "Select a registered QwenPaw workspace",
          title: "QwenPaw: Workspaces",
        },
      );
      if (!selection) {
        return;
      }
      const workspace = await client.readWorkspace(selection.workspace.root);
      await vscode.window.showInformationMessage(
        [
          workspace.root,
          `Threads: ${workspace.threadCount}`,
          `Archived: ${workspace.archivedThreadCount}`,
        ].join("\n"),
        { modal: true },
      );
    },
  );
  const selectWorkspace = vscode.commands.registerCommand(
    "qwenpaw.selectWorkspace",
    async () => {
      const folders = vscode.workspace.workspaceFolders ?? [];
      if (folders.length === 0) {
        vscode.window.showInformationMessage(
          "QwenPaw has no open VS Code workspace folders",
        );
        return;
      }
      const selection = await vscode.window.showQuickPick(
        folders.map(workspaceFolderItem),
        {
          matchOnDescription: true,
          placeHolder: "Select the workspace for the next new thread",
          title: "QwenPaw: Select Workspace",
        },
      );
      if (!selection) {
        return;
      }
      await context.workspaceState.update(PENDING_THREAD_SELECTION_KEY, {
        kind: "new",
        workspaceRoot: selection.folder.uri.fsPath,
      } satisfies PendingThreadSelection);
      vscode.window.showInformationMessage(
        `QwenPaw will start a new thread in ${selection.folder.name}`,
      );
    },
  );
  const configurationChanged = vscode.workspace.onDidChangeConfiguration(
    (event) => {
      void handleConfigurationChange(event, output).catch((error: unknown) => {
        output.error(`Failed to apply configuration change: ${String(error)}`);
      });
    },
  );
  context.subscriptions.push(
    output,
    participant,
    restart,
    setApiKey,
    clearApiKey,
    selectThread,
    archiveThread,
    selectModel,
    showConfiguration,
    showWorkspaces,
    selectWorkspace,
    configurationChanged,
    manager,
  );
}

export function deactivate(): void {
  manager?.dispose();
  manager = undefined;
}

function findThreadId(context: vscode.ChatContext): string | undefined {
  const history = context.history as readonly unknown[];
  for (let index = history.length - 1; index >= 0; index -= 1) {
    const entry = history[index];
    if (!isRecord(entry) || !isRecord(entry.result)) {
      continue;
    }
    const metadata = entry.result.metadata;
    if (isRecord(metadata) && typeof metadata.threadId === "string") {
      return metadata.threadId;
    }
  }
  return undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

interface ThreadQuickPickItem extends vscode.QuickPickItem {
  readonly thread?: Thread;
}

interface ModelQuickPickItem extends vscode.QuickPickItem {
  readonly modelId?: string;
}

interface WorkspaceQuickPickItem extends vscode.QuickPickItem {
  readonly workspace: WorkspaceInfo;
}

interface WorkspaceFolderQuickPickItem extends vscode.QuickPickItem {
  readonly folder: vscode.WorkspaceFolder;
}

function newThreadItem(): ThreadQuickPickItem {
  return {
    label: "Start a new thread",
    description: "Do not continue an existing conversation",
  };
}

function threadItem(thread: Thread): ThreadQuickPickItem {
  const updated = new Date(thread.updatedAt * 1_000).toLocaleString();
  const lifecycle = thread.archived ? "archived" : thread.status;
  return {
    label: thread.id,
    description: `${thread.model} · ${lifecycle}`,
    detail: `${thread.workspaceRoot ?? "No workspace"} · Updated ${updated}`,
    thread,
  };
}

function modelItem(
  model: ModelInfo,
  current: string,
): ModelQuickPickItem {
  const state = model.id === current ? "current" : model.isDefault ? "default" : "";
  return {
    label: model.displayName,
    description: [model.id, state].filter(Boolean).join(" · "),
    modelId: model.id,
  };
}

function customModelItem(): ModelQuickPickItem {
  return {
    label: "Enter another model ID",
    description: "Use a model exposed by the configured compatible endpoint",
  };
}

function workspaceItem(workspace: WorkspaceInfo): WorkspaceQuickPickItem {
  const updated = new Date(workspace.updatedAt * 1_000).toLocaleString();
  return {
    label: workspace.root,
    description: `${workspace.threadCount} threads · ${workspace.archivedThreadCount} archived`,
    detail: `Updated ${updated}`,
    workspace,
  };
}

function workspaceFolderItem(
  folder: vscode.WorkspaceFolder,
): WorkspaceFolderQuickPickItem {
  return {
    label: folder.name,
    description: folder.uri.fsPath,
    folder,
  };
}

function defaultWorkspaceRoot(): string | undefined {
  const activeDocument = vscode.window.activeTextEditor?.document.uri;
  const activeFolder = activeDocument
    ? vscode.workspace.getWorkspaceFolder(activeDocument)
    : undefined;
  return (
    activeFolder?.uri.fsPath ??
    vscode.workspace.workspaceFolders?.[0]?.uri.fsPath
  );
}

async function handleConfigurationChange(
  event: vscode.ConfigurationChangeEvent,
  output: vscode.LogOutputChannel,
): Promise<void> {
  if (
    event.affectsConfiguration("qwenpaw.core.path") ||
    event.affectsConfiguration("qwenpaw.core.arguments") ||
    event.affectsConfiguration("qwenpaw.mcp.configPath")
  ) {
    await manager?.restart();
    return;
  }
  if (
    !event.affectsConfiguration("qwenpaw.model") &&
    !event.affectsConfiguration("qwenpaw.baseUrl")
  ) {
    return;
  }
  const client = await manager?.get();
  if (!client) {
    return;
  }
  const configuration = vscode.workspace.getConfiguration("qwenpaw");
  const applied = await client.updateConfig({
    baseUrl: configuration.get<string>(
      "baseUrl",
      "https://dashscope.aliyuncs.com/compatible-mode/v1",
    ),
    defaultModel: configuration.get<string>("model", "qwen3-coder-plus"),
  });
  output.info(
    `Applied Core configuration (model=${applied.defaultModel}, baseUrl=${applied.baseUrl})`,
  );
}

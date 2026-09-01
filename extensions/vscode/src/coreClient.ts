import { ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import * as vscode from "vscode";

import { AsyncResourceManager } from "./asyncResourceManager";
import { resolveCoreExecutable } from "./coreExecutable";
import { API_KEY_SECRET, createCoreEnvironment } from "./environment";
import {
  type AgentMessageDeltaNotification,
  type ConfigReadResponse,
  type ConfigWriteParams,
  type ConfigWriteResponse,
  type InitializeResponse,
  type ItemCompletedNotification,
  type ItemStartedNotification,
  type McpClientInfo,
  type McpListResponse,
  type McpOAuthRevokeResponse,
  type McpOAuthStartResponse,
  type McpOAuthStatus,
  type McpOAuthStatusResponse,
  type ModelInfo,
  type ModelListResponse,
  PROTOCOL_VERSION,
  type Thread,
  type ThreadArchiveResponse,
  type ThreadListResponse,
  type ThreadResumeResponse,
  type ThreadStartResponse,
  type ToolApprovalRequestedNotification,
  type ToolApprovalResolvedNotification,
  type ToolApprovalRespondResponse,
  type TurnCompletedNotification,
  type TurnStartResponse,
  type UserInput,
  type WorkspaceInfo,
  type WorkspaceListResponse,
  type WorkspaceReadResponse,
} from "./generated/protocol";
import { RpcClient } from "./rpcClient";
import { collectCursorPages } from "./pagination";
import { TurnProgressTracker, turnOutcome } from "./turnProgress";

export class CoreClient implements vscode.Disposable {
  private constructor(
    private readonly process: ChildProcessWithoutNullStreams,
    private readonly rpc: RpcClient,
    private readonly output: vscode.OutputChannel,
  ) {}

  public static async start(
    output: vscode.OutputChannel,
    secrets: vscode.SecretStorage,
    extensionPath: string,
  ): Promise<CoreClient> {
    const configuration = vscode.workspace.getConfiguration("qwenpaw");
    const coreExecutable = await resolveCoreExecutable({
      configuredPath: configuration.get<string>("core.path", ""),
      extensionPath,
    });
    const args = configuration.get<string[]>("core.arguments", [
      "app-server",
      "--stdio",
    ]);
    const model = configuration.get<string>("model", "qwen3-coder-plus");
    const baseUrl = configuration.get<string>(
      "baseUrl",
      "https://dashscope.aliyuncs.com/compatible-mode/v1",
    );
    const mcpConfigPath = configuration.get<string>("mcp.configPath", "");
    const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    const storedApiKey = await secrets.get(API_KEY_SECRET);
    output.appendLine(
      `Starting QwenPaw Core from ${coreExecutable.source}: ${coreExecutable.path}`,
    );
    const child = spawn(coreExecutable.path, args, {
      cwd,
      env: createCoreEnvironment(process.env, {
        baseUrl,
        mcpConfigPath,
        model,
        storedApiKey,
      }),
      stdio: "pipe",
    });
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk: string) => output.append(chunk));
    const rpc = new RpcClient(child.stdout, child.stdin);
    child.on("error", (error) => rpc.dispose());
    child.on("exit", (code, signal) => {
      output.appendLine(
        `QwenPaw Core exited (code=${String(code)}, signal=${String(signal)})`,
      );
      rpc.dispose();
    });
    const client = new CoreClient(child, rpc, output);
    try {
      const initialized = await rpc.request<InitializeResponse>("initialize", {
        clientInfo: {
          name: "qwenpaw_vscode",
          title: "QwenPaw VS Code Extension",
          version: "0.2.0",
        },
      });
      if (initialized.protocolVersion !== PROTOCOL_VERSION) {
        throw new Error(
          `Unsupported QwenPaw protocol version: ${initialized.protocolVersion}`,
        );
      }
      rpc.notify("initialized", {});
      await client.updateConfig({
        baseUrl,
        defaultModel: model,
      });
      output.appendLine(
        `Connected to ${initialized.serverInfo.name} ${initialized.serverInfo.version}`,
      );
      return client;
    } catch (error) {
      client.dispose();
      throw error;
    }
  }

  public async startThread(workspaceRoot?: string): Promise<string> {
    const model = vscode.workspace
      .getConfiguration("qwenpaw")
      .get<string>("model", "qwen3-coder-plus");
    const response = await this.rpc.request<ThreadStartResponse>(
      "thread/start",
      {
        model,
        workspaceRoot,
      },
    );
    return response.thread.id;
  }

  public async listModels(): Promise<readonly ModelInfo[]> {
    const response = await this.rpc.request<ModelListResponse>(
      "model/list",
      {},
    );
    return response.data;
  }

  public async readConfig(): Promise<ConfigReadResponse["config"]> {
    const response = await this.rpc.request<ConfigReadResponse>(
      "config/read",
      {},
    );
    return response.config;
  }

  public async updateConfig(values: {
    readonly baseUrl?: string;
    readonly defaultModel?: string;
  }): Promise<ConfigWriteResponse["config"]> {
    const params: ConfigWriteParams = {
      baseUrl: values.baseUrl ?? null,
      defaultModel: values.defaultModel ?? null,
    };
    const response = await this.rpc.request<ConfigWriteResponse>(
      "config/write",
      params,
    );
    return response.config;
  }

  public async listThreads(
    includeArchived = false,
  ): Promise<readonly Thread[]> {
    return collectCursorPages(async (cursor) =>
      this.rpc.request<ThreadListResponse>("thread/list", {
        cursor,
        limit: 200,
        includeArchived,
      }),
    );
  }

  public async resumeThread(threadId: string): Promise<Thread> {
    const response = await this.rpc.request<ThreadResumeResponse>(
      "thread/resume",
      { threadId },
    );
    return response.thread;
  }

  public async archiveThread(threadId: string): Promise<Thread> {
    const response = await this.rpc.request<ThreadArchiveResponse>(
      "thread/archive",
      { threadId },
    );
    return response.thread;
  }

  public async listWorkspaces(): Promise<readonly WorkspaceInfo[]> {
    const response = await this.rpc.request<WorkspaceListResponse>(
      "workspace/list",
      {},
    );
    return response.data;
  }

  public async readWorkspace(root: string): Promise<WorkspaceInfo> {
    const response = await this.rpc.request<WorkspaceReadResponse>(
      "workspace/read",
      { root },
    );
    return response.workspace;
  }

  public async listMcpClients(): Promise<readonly McpClientInfo[]> {
    const response = await this.rpc.request<McpListResponse>("mcp/list", {});
    return response.data;
  }

  public async startMcpOAuth(serverId: string): Promise<McpOAuthStartResponse> {
    return this.rpc.request<McpOAuthStartResponse>("mcp/oauth/start", {
      serverId,
    });
  }

  public async readMcpOAuthStatus(serverId: string): Promise<McpOAuthStatus> {
    const response = await this.rpc.request<McpOAuthStatusResponse>(
      "mcp/oauth/status",
      { serverId },
    );
    return response.status;
  }

  public async revokeMcpOAuth(serverId: string): Promise<boolean> {
    const response = await this.rpc.request<McpOAuthRevokeResponse>(
      "mcp/oauth/revoke",
      { serverId },
    );
    return response.revoked;
  }

  public async runTurn(
    threadId: string,
    input: readonly UserInput[],
    onDelta: (delta: string) => void,
    onProgress: (message: string) => void,
    cancellation: vscode.CancellationToken,
  ): Promise<void> {
    let turnId: string | undefined;
    let resolveCompletion: (() => void) | undefined;
    let rejectCompletion: ((error: Error) => void) | undefined;
    const progress = new TurnProgressTracker();
    const completion = new Promise<void>((resolve, reject) => {
      resolveCompletion = resolve;
      rejectCompletion = reject;
    });
    const notifications = this.rpc.onNotification((method, params) => {
      if (method === "item/agentMessage/delta") {
        const delta = params as AgentMessageDeltaNotification;
        if (delta.threadId === threadId && delta.turnId === turnId) {
          onDelta(delta.delta);
        }
        return;
      }
      if (method === "turn/completed") {
        const completed = params as TurnCompletedNotification;
        if (completed.turn.id !== turnId) {
          return;
        }
        const outcome = turnOutcome(completed.turn);
        switch (outcome.kind) {
          case "completed":
            resolveCompletion?.();
            break;
          case "failed":
            rejectCompletion?.(new Error(outcome.message));
            break;
          case "interrupted":
            rejectCompletion?.(new vscode.CancellationError());
            break;
          case "invalid":
            rejectCompletion?.(new Error(outcome.message));
            break;
        }
        return;
      }
      if (method === "item/started") {
        const item = params as ItemStartedNotification;
        if (
          item.threadId === threadId &&
          item.turnId === turnId &&
          item.item.type === "toolCall"
        ) {
          const message = progress.itemStarted(item.item);
          if (message) {
            onProgress(message);
          }
        }
        return;
      }
      if (method === "item/completed") {
        const item = params as ItemCompletedNotification;
        if (item.threadId === threadId && item.turnId === turnId) {
          const message = progress.itemCompleted(item.item);
          if (message) {
            onProgress(message);
          }
        }
        return;
      }
      if (method === "tool/approval/requested") {
        const approval = params as ToolApprovalRequestedNotification;
        if (approval.threadId === threadId && approval.turnId === turnId) {
          onProgress(progress.approvalRequested(approval));
          void this.handleToolApproval(approval, cancellation).catch(
            (error: unknown) => {
              this.output.appendLine(
                `Failed to resolve tool approval: ${String(error)}`,
              );
            },
          );
        }
        return;
      }
      if (method === "tool/approval/resolved") {
        const approval = params as ToolApprovalResolvedNotification;
        if (approval.threadId === threadId && approval.turnId === turnId) {
          onProgress(progress.approvalResolved(approval));
        }
      }
    });
    let connection: vscode.Disposable | undefined;

    try {
      const response = await this.rpc.request<TurnStartResponse>("turn/start", {
        threadId,
        input: [...input],
      });
      turnId = response.turn.id;
      connection = this.rpc.onClose((error) => rejectCompletion?.(error));
      const cancellationSubscription = cancellation.onCancellationRequested(
        () => {
          if (turnId) {
            void this.rpc
              .request("turn/interrupt", { threadId, turnId })
              .catch((error: unknown) => {
                this.output.appendLine(
                  `Failed to interrupt QwenPaw turn: ${String(error)}`,
                );
              });
          }
        },
      );
      try {
        await completion;
      } finally {
        cancellationSubscription.dispose();
      }
    } finally {
      connection?.dispose();
      notifications.dispose();
    }
  }

  public dispose(): void {
    this.rpc.dispose();
    if (!this.process.killed) {
      this.process.kill();
    }
  }

  public onClose(handler: (error: Error) => void): vscode.Disposable {
    return this.rpc.onClose(handler);
  }

  private async handleToolApproval(
    approval: ToolApprovalRequestedNotification,
    cancellation: vscode.CancellationToken,
  ): Promise<void> {
    const detail = [
      `Tool: ${approval.toolName}`,
      `Workspace: ${approval.workspaceRoot}`,
      `Arguments: ${formatArguments(approval.arguments)}`,
    ].join("\n");
    const selection = await vscode.window.showWarningMessage(
      `QwenPaw requests permission to run ${approval.toolName}`,
      { modal: true, detail },
      "Allow once",
    );
    const decision =
      selection === "Allow once" && !cancellation.isCancellationRequested
        ? "approved"
        : "denied";
    const response = await this.rpc.request<ToolApprovalRespondResponse>(
      "tool/approval/respond",
      { approvalId: approval.approvalId, decision },
    );
    if (!response.accepted) {
      this.output.appendLine(
        `Tool approval was no longer pending: ${approval.approvalId}`,
      );
    }
  }
}

export class CoreClientManager implements vscode.Disposable {
  private readonly clients: AsyncResourceManager<CoreClient>;

  public constructor(
    private readonly output: vscode.OutputChannel,
    private readonly secrets: vscode.SecretStorage,
    private readonly extensionPath: string,
  ) {
    this.clients = new AsyncResourceManager(() =>
      CoreClient.start(this.output, this.secrets, this.extensionPath),
    );
  }

  public get(): Promise<CoreClient> {
    return this.clients.get();
  }

  public async restart(): Promise<void> {
    await this.clients.restart();
  }

  public dispose(): void {
    this.clients.dispose();
  }
}

function formatArguments(argumentsJson: string): string {
  try {
    return JSON.stringify(JSON.parse(argumentsJson), null, 2).slice(0, 4_000);
  } catch {
    return argumentsJson.slice(0, 4_000);
  }
}

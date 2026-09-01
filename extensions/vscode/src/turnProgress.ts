import {
  type Item,
  type ToolApprovalRequestedNotification,
  type ToolApprovalResolvedNotification,
  type Turn,
} from "./generated/protocol";

const MAX_TOOL_NAME_LENGTH = 80;

export type TurnOutcome =
  | { readonly kind: "completed" }
  | { readonly kind: "failed"; readonly message: string }
  | { readonly kind: "interrupted" }
  | { readonly kind: "invalid"; readonly message: string };

export function turnOutcome(
  turn: Pick<Turn, "status" | "error">,
): TurnOutcome {
  switch (turn.status) {
    case "completed":
      return { kind: "completed" };
    case "failed":
      return {
        kind: "failed",
        message: turn.error?.message ?? "QwenPaw turn failed",
      };
    case "interrupted":
      return { kind: "interrupted" };
    case "inProgress":
      return {
        kind: "invalid",
        message: "QwenPaw Core returned a non-terminal completion",
      };
  }
}

export class TurnProgressTracker {
  private readonly toolNames = new Map<string, string>();
  private readonly approvalTools = new Map<string, string>();

  public itemStarted(item: Item): string | undefined {
    if (item.type !== "toolCall") {
      return undefined;
    }
    const name = displayToolName(item.name);
    this.toolNames.set(item.callId, name);
    return `Starting ${name}`;
  }

  public itemCompleted(item: Item): string | undefined {
    if (item.type !== "toolResult") {
      return undefined;
    }
    const name = this.toolNames.get(item.callId) ?? "tool";
    this.toolNames.delete(item.callId);
    return item.isError ? `${name} failed` : `${name} completed`;
  }

  public approvalRequested(
    approval: ToolApprovalRequestedNotification,
  ): string {
    const name = displayToolName(approval.toolName);
    this.toolNames.set(approval.callId, name);
    this.approvalTools.set(approval.approvalId, name);
    return `Waiting for approval: ${name}`;
  }

  public approvalResolved(
    approval: ToolApprovalResolvedNotification,
  ): string {
    const name = this.approvalTools.get(approval.approvalId) ?? "tool";
    this.approvalTools.delete(approval.approvalId);
    return approval.decision === "approved"
      ? `Approved ${name}`
      : `Denied ${name}`;
  }
}

function displayToolName(value: string): string {
  const normalized = value.replaceAll(/\s+/g, " ").trim();
  if (!normalized) {
    return "tool";
  }
  return normalized.slice(0, MAX_TOOL_NAME_LENGTH);
}

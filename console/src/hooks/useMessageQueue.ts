export interface ApprovalMessageData {
  requestId: string;
  sessionId: string;
  agentId: string;
  toolName: string;
  severity: string;
  findingsCount: number;
  findingsSummary: string;
  toolParams: Record<string, unknown>;
  createdAt: number;
}

export interface MessageQueueHandlers {
  onApprovalMessage?: (data: ApprovalMessageData) => void;
}

/**
 * Process SSE message payload and extract approval data if present.
 * Returns:
 * - null if message should be filtered (heartbeat)
 * - ApprovalMessageData if it's an approval message
 * - original payload otherwise
 */
export function processMessagePayload(
  payload: unknown,
  handlers?: MessageQueueHandlers,
): unknown | null {
  if (!payload || typeof payload !== "object") return payload;

  const record = payload as Record<string, unknown>;

  // Handle response with output messages
  if (record.object === "response" && Array.isArray(record.output)) {
    const filteredOutput: unknown[] = [];

    for (const msg of record.output) {
      if (!msg || typeof msg !== "object") {
        filteredOutput.push(msg);
        continue;
      }

      const message = msg as Record<string, unknown>;
      const metadata = message.metadata as Record<string, unknown> | undefined;

      // Filter out heartbeat messages
      if (metadata?.message_type === "approval_heartbeat") {
        continue; // Skip this message
      }

      // Extract approval messages
      if (metadata?.message_type === "tool_guard_approval") {
        const data: ApprovalMessageData = {
          requestId: (metadata.approval_request_id as string) || "",
          sessionId: (metadata.session_id as string) || "",
          agentId: (metadata.agent_id as string) || "",
          toolName: (metadata.tool_name as string) || "",
          severity: (metadata.severity as string) || "UNKNOWN",
          findingsCount: (metadata.findings_count as number) || 0,
          findingsSummary: (metadata.findings_summary as string) || "",
          toolParams: (metadata.tool_params as Record<string, unknown>) || {},
          createdAt: (metadata.created_at as number) || Date.now() / 1000,
        };

        handlers?.onApprovalMessage?.(data);
      }

      filteredOutput.push(message);
    }

    return {
      ...record,
      output: filteredOutput,
    };
  }

  // Handle single message
  if (record.object === "message") {
    const metadata = record.metadata as Record<string, unknown> | undefined;

    // Filter out heartbeat
    if (metadata?.message_type === "approval_heartbeat") {
      return null;
    }

    // Extract approval
    if (metadata?.message_type === "tool_guard_approval") {
      const data: ApprovalMessageData = {
        requestId: (metadata.approval_request_id as string) || "",
        sessionId: (metadata.session_id as string) || "",
        agentId: (metadata.agent_id as string) || "",
        toolName: (metadata.tool_name as string) || "",
        severity: (metadata.severity as string) || "UNKNOWN",
        findingsCount: (metadata.findings_count as number) || 0,
        findingsSummary: (metadata.findings_summary as string) || "",
        toolParams: (metadata.tool_params as Record<string, unknown>) || {},
        createdAt: (metadata.created_at as number) || Date.now() / 1000,
      };

      handlers?.onApprovalMessage?.(data);
    }
  }

  return payload;
}

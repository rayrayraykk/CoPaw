export const PENDING_THREAD_SELECTION_KEY =
  "qwenpaw.pendingThreadSelection.v1";

export type PendingThreadSelection =
  | { readonly kind: "existing"; readonly threadId: string }
  | { readonly kind: "new"; readonly workspaceRoot?: string };

export function resolveInitialThreadId(
  historyThreadId: string | undefined,
  pendingSelection: PendingThreadSelection | undefined,
): string | undefined {
  if (pendingSelection?.kind === "new") {
    return undefined;
  }
  if (pendingSelection?.kind === "existing") {
    return pendingSelection.threadId;
  }
  return historyThreadId;
}

export function resolveNewThreadWorkspaceRoot(
  pendingSelection: PendingThreadSelection | undefined,
  availableRoots: readonly string[],
  defaultRoot: string | undefined,
): string | undefined {
  const selectedRoot =
    pendingSelection?.kind === "new"
      ? pendingSelection.workspaceRoot
      : undefined;
  if (selectedRoot && availableRoots.includes(selectedRoot)) {
    return selectedRoot;
  }
  return defaultRoot;
}

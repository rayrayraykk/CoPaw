# Driver Permission Architecture

This note explains how Driver permission checks relate to the existing
QwenPaw and AgentScope permission systems.

## Three Layers

Driver permissions are intentionally split across three layers:

1. AgentScope permission engine is bypassed for the QwenPaw agent runtime.
   QwenPaw switches the AgentScope permission context to BYPASS in
   [react_agent.py](../../src/qwenpaw/agents/react_agent.py#L190). This
   prevents AgentScope's default permission behavior from blocking tools that
   QwenPaw already controls through its own guard layers.
2. Built-in Python tools still use QwenPaw tool guard. They are wrapped as
   `GuardedFunctionTool` when the agent toolkit is assembled, as shown in
   [react_agent.py](../../src/qwenpaw/agents/react_agent.py#L204). That layer
   is for native QwenPaw tools such as shell, file, browser, LSP, and similar
   local operations.
3. Driver tools use Driver Policy. Driver-exposed tools are registered through
   `DriverCapabilityTool` in
   [react_agent.py](../../src/qwenpaw/agents/react_agent.py#L399). The adapter
   returns AgentScope `ALLOW` in
   [driver_capability_tool.py](../../src/qwenpaw/agents/tools/driver_capability_tool.py#L153)
   because the real decision is made inside Driver execution.

## Driver Decision Point

The Driver permission decision happens at invocation time, after the tool call
has been mapped back to a Driver capability. `DriverCapabilityTool.__call__`
passes `capability_id`, payload, and request context into the Driver invoker in
[driver_capability_tool.py](../../src/qwenpaw/agents/tools/driver_capability_tool.py#L168).

Each protocol handler then validates that the capability belongs to itself. MCP
checks protocol, driver name, capability kind, and operation in
[mcp.py](../../src/qwenpaw/drivers/handlers/mcp.py#L161). Only after that does
the call enter the guarded Driver path.

The shared Driver authorization helper is
`DriverHandler._authorize_invocation`, defined in
[handler.py](../../src/qwenpaw/drivers/handler.py#L101). It builds a
`DriverInvocationContext`, evaluates `DriverPolicy`, and maps the result to
runtime behavior:

- `deny` raises `DriverPermissionDeniedError`.
- `ask` delegates to the configured `ApprovalGate`.
- `allow` continues into protocol-specific execution.

## Why DriverCapabilityTool Allows

`DriverCapabilityTool.check_permissions()` must return AgentScope `ALLOW`
because it is only an adapter from AgentScope's tool surface to DriverManager.
If it ran a separate approval system, a Driver call could be approved by one
layer and denied by another, or require duplicate approvals.

The invariant is:

- AgentScope permission mode prevents the framework default from intervening.
- `GuardedFunctionTool` protects QwenPaw-native tools.
- Driver Policy protects Driver capabilities, including MCP tools.

This keeps the subject of each policy clear: QwenPaw-native tools are guarded
by QwenPaw tool guard, while external Driver capabilities are guarded by the
DriverCard policy attached to that Driver.

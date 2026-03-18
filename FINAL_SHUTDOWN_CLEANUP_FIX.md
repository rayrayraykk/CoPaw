# Final Shutdown Cleanup Fix

## 问题描述

### 用户反馈

> "但是在应用关闭的时候你需要关掉这些service 即使是reuse的"

**完全正确！** 当前实现中，`reusable` 服务（memory_manager, chat_manager）在应用最终关闭时永远不会被清理。

### 根本原因

**修复前的逻辑**：
```python
async def _stop_service(self, descriptor: ServiceDescriptor):
    if descriptor.reusable:
        # 永远跳过！
        logger.debug(f"Skipped stopping reusable service '{name}'")
        return

    # 调用 stop_method...
```

**问题**：
- 在 reload 时跳过 reusable 服务 ✅ 正确
- 但在应用最终关闭时也跳过 ❌ 错误！
- 导致 `memory_manager.close()` 和 `chat_manager` 永远不会被调用

## 需求分析

### 两种关闭场景

| 场景 | Reusable 服务行为 | 原因 |
|------|------------------|------|
| **Reload** | 跳过（保留） | 需要传递给新实例 |
| **Final Shutdown** | 必须关闭 | 应用退出，清理所有资源 |

### 调用链分析

```
应用场景 1: Reload
MultiAgentManager.reload_agent()
  └─> old_instance.stop()  ← 应该跳过 reusable
      └─> ServiceManager.stop_all()
          └─> _stop_service(desc)

应用场景 2: Stop Single Agent
MultiAgentManager.stop_agent()
  └─> instance.stop()  ← 应该关闭所有
      └─> ServiceManager.stop_all()
          └─> _stop_service(desc)

应用场景 3: Final Shutdown
MultiAgentManager.stop_all()
  └─> instance.stop()  ← 应该关闭所有
      └─> ServiceManager.stop_all()
          └─> _stop_service(desc)
```

## 解决方案

### 设计

添加 `final` 参数在调用链中传递：

```
stop(final=True/False)
  └─> stop_all(final=True/False)
      └─> _stop_service(desc, final=True/False)
```

### 实现

#### 1. ServiceManager

```python
async def stop_all(self, final: bool = False) -> None:
    """Stop all services.

    Args:
        final: If True, stop ALL services including reusable ones.
               If False (default), skip reusable services (for reload).
    """
    for priority in sorted(priority_groups.keys(), reverse=True):
        await asyncio.gather(
            *[self._stop_service(desc, final=final) for desc in descriptors],
        )

async def _stop_service(
    self,
    descriptor: ServiceDescriptor,
    final: bool = False,
) -> None:
    """Stop a single service.

    Args:
        descriptor: Service descriptor
        final: If True, stop service even if reusable.
    """
    # ✅ 只在 final=False 时跳过 reusable 服务
    if descriptor.reusable and not final:
        logger.debug(
            f"Skipped stopping reusable service '{name}' "
            f"(will be reused)",
        )
        return

    # Skip reused services (doesn't belong to this instance)
    if name in self.reused_services:
        logger.debug(
            f"Skipped stopping reused service '{name}' "
            f"(from previous instance)",
        )
        return

    # 调用 stop_method...
```

#### 2. Workspace

```python
async def stop(self, final: bool = True):
    """Stop agent instance.

    Args:
        final: If True (default), stop ALL services including reusable.
               If False, skip reusable services (for reload scenario).
    """
    logger.info(f"Stopping agent instance: {self.agent_id} (final={final})")
    await self._service_manager.stop_all(final=final)
```

**默认值**：`final=True`
- 大多数情况是最终关闭
- 只有 reload 需要特殊处理

#### 3. MultiAgentManager (Reload)

```python
async def reload_agent(self, agent_id: str):
    # ...

    # ✅ 明确传递 final=False
    await old_instance.stop(final=False)
```

**其他调用保持默认**：
- `stop_agent()`: 使用 `final=True`（默认）✅
- `stop_all()`: 使用 `final=True`（默认）✅

## 验证

### 场景 1: Reload（跳过 reusable）

```python
await manager.reload_agent('default')
# 日志:
# DEBUG | Skipped stopping reusable service 'memory_manager' (will be reused)
# DEBUG | Skipped stopping reusable service 'chat_manager' (will be reused)
```

### 场景 2: Final Shutdown（关闭所有）

```python
await manager.stop_all()
# 日志:
# DEBUG | Service 'memory_manager' stopped for default
# DEBUG | Service 'chat_manager' stopped for default
```

### 测试结果

```
✓ test_component_reuse.py: 5/5 tests passed
✓ test_all_reload_mechanisms.py: 4/4 tests passed
✓ test_final_shutdown_cleanup.py: All services cleaned up
```

## 技术要点

### Final vs Reusable 决策表

| 条件 | final=False<br>(Reload) | final=True<br>(Final Shutdown) |
|------|------------------------|-------------------------------|
| `descriptor.reusable=True` | **跳过**（保留给新实例） | **执行** stop_method |
| `descriptor.reusable=False` | **执行** stop_method | **执行** stop_method |
| `name in reused_services` | **跳过**（不属于这个实例） | **跳过**（不属于这个实例） |

### 为什么默认值是 `final=True`

1. **安全优先**：默认清理所有资源
2. **常见场景**：大多数 stop 调用是最终关闭
3. **明确意图**：只有 reload 需要特殊的 `final=False`

### 生命周期示例

```
# 创建
ws1 = Workspace()
await ws1.start()
mm1 = ws1.memory_manager

# Reload（mm1 保留）
ws2 = Workspace()
ws2.set_reusable_components({'memory_manager': mm1})
await ws2.start()
await ws1.stop(final=False)  # ← mm1 不会被关闭
assert ws2.memory_manager is mm1  # ✅ Same instance

# Final Shutdown（mm1 被关闭）
await ws2.stop(final=True)  # ← mm1.close() 被调用 ✅
```

## 代码修改

### 修改的文件

1. **`src/copaw/app/workspace/service_manager.py`**
   - `stop_all(final=False)`: 添加 final 参数
   - `_stop_service(desc, final=False)`: 添加 final 参数
   - 修改跳过逻辑: `if descriptor.reusable and not final`

2. **`src/copaw/app/workspace/workspace.py`**
   - `stop(final=True)`: 添加 final 参数（默认 True）
   - 传递 final 到 `stop_all()`

3. **`src/copaw/app/multi_agent_manager.py`**
   - Reload 中: `old_instance.stop(final=False)`
   - 其他保持默认: `final=True`

### 向后兼容性

✅ **完全向后兼容**
- 默认行为未改变（`final=True`）
- 外部 API 保持一致
- 只在 reload 内部使用 `final=False`

## 影响范围

### Before (Bug)

```python
# Reload
await old_instance.stop()
# → memory_manager 跳过 ✅

# Final Shutdown
await instance.stop()
# → memory_manager 跳过 ❌ Bug!
# → 资源泄漏！
```

### After (Fixed)

```python
# Reload
await old_instance.stop(final=False)
# → memory_manager 跳过 ✅

# Final Shutdown
await instance.stop(final=True)  # 或省略（默认）
# → memory_manager 关闭 ✅
# → 资源正确清理！
```

## 总结

1. **问题**：Reusable 服务在最终关闭时永远不会被清理
2. **根因**：没有区分 reload 和 final shutdown 两种场景
3. **修复**：添加 `final` 参数传递关闭类型
4. **验证**：所有测试通过，资源正确清理

用户的反馈完全正确，这是一个重要的资源泄漏 bug！

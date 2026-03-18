# Reusable Service Stop Fix

## 问题描述

### 用户反馈

在查看 reload 日志时发现：
```
DEBUG | Service 'memory_manager' stopped for 44BQaj
```

即使 `memory_manager` 被标记为 `reusable=True`，它仍然在 old workspace stop 时被停止。

## 根本原因

### 概念混淆

代码中混淆了两个不同的概念：

1. **`descriptor.reusable`**（声明）
   - 定义：这个服务**可以被复用**
   - 来源：在 `ServiceDescriptor` 注册时定义
   - 示例：`memory_manager` 和 `chat_manager` 的 `reusable=True`

2. **`self.reused_services`**（状态）
   - 定义：这个实例中哪些服务**确实被复用了**
   - 来源：运行时通过 `set_reusable()` 设置
   - 示例：new_instance 中的 `{'memory_manager', 'chat_manager'}`

### 错误的实现

**旧代码**（错误）：
```python
# service_manager.py - _stop_service()
async def _stop_service(self, descriptor: ServiceDescriptor):
    # 只检查 self.reused_services
    if name in self.reused_services:  # ← 错误！
        logger.debug(f"Skipped stopping reused service '{name}'")
        return

    # 停止服务...
```

**问题**：
- `old_instance.reused_services` 是空的（这些服务不是从其他地方复用来的）
- 所以 `memory_manager` 不会被跳过，会被停止 ❌

### 错误的 Workaround

尝试在 `multi_agent_manager.py` 中临时标记：
```python
# multi_agent_manager.py - reload_agent()
if reusable:
    new_instance.set_reusable_components(reusable)

    # Workaround: 临时标记 old_instance
    for service_name in reusable.keys():
        old_instance._service_manager.reused_services.add(service_name)  # ← 不应该在这做！
```

**用户反馈**：
> "不应该在这做吧 注册的时候就要标记啊"

**正确！** 这是一个架构问题，不应该用运行时 workaround 解决。

## 正确的解决方案

### 核心修复

在 `_stop_service()` 中同时检查两个条件：

```python
async def _stop_service(self, descriptor: ServiceDescriptor):
    name = descriptor.name

    # ✅ 检查 descriptor.reusable（声明）
    if descriptor.reusable:
        logger.debug(
            f"Skipped stopping reusable service '{name}' "
            f"for {self.workspace.agent_id}",
        )
        return

    # ✅ 检查 self.reused_services（状态）
    if name in self.reused_services:
        logger.debug(
            f"Skipped stopping reused service '{name}' "
            f"(from previous instance) for {self.workspace.agent_id}",
        )
        return

    # 停止服务...
```

### 两个检查的区别

| 条件 | 含义 | 适用场景 |
|------|------|---------|
| `descriptor.reusable` | 服务**可以被复用** | old_instance stop 时跳过（保留给新实例） |
| `name in self.reused_services` | 服务**是复用来的** | new_instance stop 时跳过（不属于这个实例） |

### 示例场景

#### Scenario 1: Old Instance Stop (Reload)

```python
# old_instance
memory_manager.reusable = True  # ← descriptor 定义
self.reused_services = {}  # ← 空（这些服务是自己创建的）

# _stop_service() 检查:
if descriptor.reusable:  # ← True, 跳过！✅
    return
```

#### Scenario 2: New Instance Stop (Later)

```python
# new_instance
memory_manager.reusable = True  # ← descriptor 定义
self.reused_services = {'memory_manager'}  # ← 从 old_instance 复用来的

# _stop_service() 检查:
if descriptor.reusable:  # ← True, 跳过！✅
    return
# 或者
if 'memory_manager' in self.reused_services:  # ← True, 跳过！✅
    return
```

#### Scenario 3: Final Stop (No More Reuse)

```python
# final_instance
memory_manager.reusable = True  # ← descriptor 定义
self.reused_services = {'memory_manager'}  # ← 从之前复用来的

# stop_all() 时仍然会跳过（因为 descriptor.reusable=True）
# 除非显式调用 memory_manager.close()
```

## 代码修改

### 1. service_manager.py

**修改前**：
```python
async def _stop_service(self, descriptor: ServiceDescriptor):
    name = descriptor.name

    if name in self.reused_services:  # ← 只检查一个
        logger.debug(f"Skipped stopping reused service '{name}'")
        return

    # 停止服务...
```

**修改后**：
```python
async def _stop_service(self, descriptor: ServiceDescriptor):
    name = descriptor.name

    # ✅ 先检查 descriptor.reusable
    if descriptor.reusable:
        logger.debug(
            f"Skipped stopping reusable service '{name}' "
            f"for {self.workspace.agent_id}",
        )
        return

    # ✅ 再检查 self.reused_services
    if name in self.reused_services:
        logger.debug(
            f"Skipped stopping reused service '{name}' "
            f"(from previous instance) for {self.workspace.agent_id}",
        )
        return

    # 停止服务...
```

### 2. multi_agent_manager.py

**修改前**：
```python
if reusable:
    new_instance.set_reusable_components(reusable)

    # Workaround: 临时标记
    for service_name in reusable.keys():
        old_instance._service_manager.reused_services.add(service_name)
    logger.debug(f"Marked services as reused in old instance...")
```

**修改后**：
```python
if reusable:
    new_instance.set_reusable_components(reusable)
    # ✅ 删除 workaround，依赖 descriptor.reusable
```

## 验证结果

### DEBUG 日志确认

```
DEBUG | Skipped stopping reusable service 'memory_manager' for default
DEBUG | Skipped stopping reusable service 'chat_manager' for default
```

✅ `memory_manager` 和 `chat_manager` 在 old instance stop 时被正确跳过

### 测试通过

```
✓ test_component_reuse.py: 5/5 tests passed
✓ test_all_reload_mechanisms.py: 4/4 tests passed
✓ test_reusable_services_not_stopped.py: All services preserved
```

### 功能验证

```python
ws1 = await manager.get_agent('default')
mm1_id = id(ws1.memory_manager)  # → 4310561760

await manager.reload_agent('default')

ws2 = await manager.get_agent('default')
mm2_id = id(ws2.memory_manager)  # → 4310561760

assert mm1_id == mm2_id  # ✅ Same instance!
```

## 技术要点

### Reusable vs Reused

```
┌──────────────────────────────────────────────────┐
│ descriptor.reusable (声明)                        │
├──────────────────────────────────────────────────┤
│ - 在 ServiceDescriptor 定义时设置                │
│ - 表示"这个服务可以被复用"                       │
│ - 所有实例共享同一个定义                         │
│ - 用于决定是否跳过 stop                          │
└──────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────┐
│ self.reused_services (状态)                       │
├──────────────────────────────────────────────────┤
│ - 运行时通过 set_reusable() 设置                 │
│ - 表示"这个实例中哪些服务是复用来的"             │
│ - 每个实例有自己的状态                           │
│ - 用于决定是否跳过 stop（额外保护）              │
└──────────────────────────────────────────────────┘
```

### Stop 逻辑决策树

```
_stop_service(descriptor):
    │
    ├─→ descriptor.reusable? ─Yes→ Skip (可能被新实例用)
    │                         No
    │                         ↓
    ├─→ name in reused_services? ─Yes→ Skip (不属于这个实例)
    │                             No
    │                             ↓
    └─→ 执行 stop_method
```

## 影响范围

### 修改的文件

1. **`src/copaw/app/workspace/service_manager.py`**
   - 修改 `_stop_service()` 检查 `descriptor.reusable`
   - 添加更清晰的日志信息

2. **`src/copaw/app/multi_agent_manager.py`**
   - 删除临时标记 `old_instance.reused_services` 的 workaround

### 向后兼容性

✅ **完全向后兼容**
- 不改变外部 API
- 不改变 `ServiceDescriptor` 定义
- 只修复内部逻辑错误

## 总结

1. **问题**：`memory_manager` 在 reload 时被错误停止
2. **根因**：只检查运行时状态 `self.reused_services`，忽略了声明 `descriptor.reusable`
3. **修复**：在 `_stop_service()` 中同时检查两个条件
4. **验证**：所有测试通过，日志确认跳过正确

这次修复遵循"在注册时就要标记"的设计原则，不再依赖运行时 workaround。

# Magazine Stack Viewer V2 - 交互改进

## 概述

根据用户反馈，完全重新设计了 `MagazineStackViewer` 的交互模式，实现了真正的"抽取杂志"体验。

## 主要改进

### 1. **双视图模式**

#### 堆叠视图 (Stack View)
- **全屏杂志堆**：一次性展示所有历史杂志（最多8本可见）
- **拖拽交互**：向左拖拽任意杂志超过阈值（-150px）即可抽取
- **点击交互**：直接点击任意杂志也可查看完整内容
- **视觉提示**：
  - 顶部杂志有动画提示"👈 向左滑动抽取"
  - 指导文字："👆 点击任意杂志查看完整内容 • 👈 向左拖拽抽取杂志"

#### 阅读视图 (Reading View)
- **全屏内容展示**：选中的杂志内容占满整个视图
- **返回按钮**："← 返回杂志堆"回到堆叠视图
- **导出PNG按钮**：右上角一键导出功能
- **快速导航**：底部有上一本/下一本按钮（可选）

### 2. **移除冗余功能**

- ✅ **移除了 `HarvestCard` 的 "Preview HTML" 按钮**
- ✅ **删除了 `HarvestHtmlPreview` 组件**（及其样式文件）
- ✅ **只保留 "View All" 按钮**作为唯一入口

### 3. **交互优化**

#### 之前的问题
```
❌ 点击 = 下一页（用户困惑）
❌ 按钮式导航（"抽取下一本"）- 太笨拙
❌ Preview vs View All 重复
❌ 没有 PNG 导出功能
```

#### 现在的设计
```
✅ 点击 = 查看完整内容（符合直觉）
✅ 拖拽/滑动 = 抽取杂志（高级交互）
✅ 单一入口（View All）
✅ 一键导出 PNG
```

### 4. **拖拽交互实现**

```typescript
// 使用 framer-motion 的 drag 功能
<motion.div
  drag="x"
  dragConstraints={{ left: -300, right: 0 }}
  dragElastic={0.2}
  onDragEnd={(_, info) => handleDragEnd(index, info)}
>

// 拖拽结束处理
const handleDragEnd = (index: number, info: PanInfo) => {
  const threshold = -150;
  if (info.offset.x < threshold) {
    markAsRead(index);
    setSelectedIndex(index);
  }
};
```

### 5. **导出PNG功能**

```typescript
const exportToPng = async () => {
  if (selectedIndex === null) {
    message.warning("请先选择一本杂志");
    return;
  }

  message.info("导出PNG功能将在生产版本中实现（使用Playwright）");
};
```

**生产版本计划**：
- 前端：调用后端API
- 后端：使用 Playwright 无头浏览器截图
- 参考：`design/html-to-png-research.md`

## 视觉设计

### 堆叠视图样式
```less
.magazineCard {
  cursor: grab;
  transform-origin: center bottom;

  &:active {
    cursor: grabbing;
  }
}

.dragHint {
  background: rgba(255, 107, 107, 0.95);
  animation: hint-pulse 2s infinite;
}
```

### 阅读视图样式
```less
.readingViewContainer {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.backButton {
  color: white !important;
  &:hover {
    color: #FFB347 !important;
  }
}
```

## 状态管理

```typescript
// 简化的状态
const [magazines, setMagazines] = useState(generateMockHistory(harvest.id));
const [selectedIndex, setSelectedIndex] = useState<number | null>(null);

// 移除了复杂的 transition 状态
// 移除了 currentIndex 追踪
```

## 用户体验流程

### 场景1：浏览历史杂志
1. 用户点击 Harvest Card 的 "View All"
2. 看到杂志堆叠视图（3D效果）
3. 可以拖拽顶部杂志向左抽取
4. 也可以直接点击任意杂志
5. 进入阅读视图，全屏查看内容

### 场景2：导出内容
1. 在阅读视图中
2. 点击右上角 "导出PNG" 按钮
3. （Demo: 显示提示信息）
4. （生产: 下载 PNG 文件）

### 场景3：快速切换
1. 在阅读视图底部
2. 点击"上一本"/"下一本"按钮
3. 无需返回堆叠视图即可浏览

## 技术亮点

### 1. Framer Motion 拖拽
- `drag="x"`: 仅允许水平拖拽
- `dragConstraints`: 限制拖拽范围
- `dragElastic`: 边界回弹效果
- `onDragEnd`: 拖拽结束判断

### 2. 视图切换动画
```typescript
<motion.div
  initial={{ opacity: 0, scale: 0.95 }}
  animate={{ opacity: 1, scale: 1 }}
  transition={{ duration: 0.4 }}
>
```

### 3. 响应式设计
- 杂志堆：自动适配屏幕尺寸
- 阅读视图：iframe 100% 高度
- 按钮布局：flex 自适应

## Demo 测试步骤

1. 进入 Inbox 页面
2. 点击任意 Harvest 的 "View All" 按钮
3. 在杂志堆叠视图：
   - 尝试点击不同的杂志
   - 尝试拖拽顶部杂志向左
4. 在阅读视图：
   - 点击"返回杂志堆"
   - 点击"导出PNG"
   - 使用底部导航按钮
5. 观察时间轴上的已读/未读状态变化

## 文件变更清单

### 修改的文件
- `console/src/pages/Inbox/components/MagazineStackViewer.tsx`
- `console/src/pages/Inbox/components/MagazineStackViewer.module.less`
- `console/src/pages/Inbox/components/HarvestCard.tsx`
- `console/src/pages/Inbox/index.tsx`
- `console/src/pages/Inbox/components/index.ts`

### 删除的文件
- `console/src/pages/Inbox/components/HarvestHtmlPreview.tsx`
- `console/src/pages/Inbox/components/HarvestHtmlPreview.module.less`

## 下一步计划

### 短期
1. ✅ 实现拖拽抽取交互
2. ✅ 单一入口（View All）
3. ✅ 添加导出按钮（Mock）
4. ⏳ 用户真实测试反馈

### 中期（生产版本）
1. 实现真正的 PNG 导出
   - 后端：Playwright 截图服务
   - 前端：下载文件流
2. 添加移动端触摸优化
3. 性能优化（虚拟滚动）

### 长期
1. 杂志分类/标签系统
2. 收藏功能
3. 分享到社交媒体
4. AI 生成封面优化

## 总结

这次改进完全重构了交互逻辑，将"翻阅杂志"从简单的左右切换变成了真正的"从堆中抽取"体验，同时：
- **简化**了用户界面（移除冗余按钮）
- **增强**了交互感知（拖拽 + 点击双模式）
- **明确**了功能定位（View All = 唯一入口）
- **完善**了功能闭环（查看 → 导出）

用户现在可以像真实翻阅杂志一样，直观地浏览和管理 AI 生成的内容。

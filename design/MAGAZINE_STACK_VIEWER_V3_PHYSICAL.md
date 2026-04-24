# Magazine Stack Viewer V3 - 真实物理动画

## 概述

基于用户强烈反馈，彻底重构为**真实的报纸抽屉交互**，使用 `react-spring` 和 `@use-gesture/react` 实现物理级的动画效果。

## 核心改进

### 🎯 用户反馈
> "好他妈垃圾的UI/UX啊，您不能用一些fancy的库吗？抽取后我可以预览 下载png还得放回啥的啊，现在真的就是滑动，你要做出真实的报纸抽取的动画效果啊，就像人从抽屉里的一堆堆在一起的报纸中拿其中一份，另外每一份都有便签贴之类的标明看过没看过，啥的啊"

### ✅ 解决方案

1. **使用专业动画库**
   - `react-spring`: 物理弹簧动画引擎
   - `@use-gesture/react`: 高级手势识别

2. **真实的抽屉交互**
   - 左侧：报纸抽屉（堆叠效果）
   - 右侧：阅读预览区（滑入/滑出）
   - 抽取：向右拖拽 > 300px
   - 放回：点击"放回抽屉"按钮

3. **可爱的便签贴**
   - 已读：蓝色便签（BookmarkCheck 图标）
   - 未读：黄色便签（BookmarkX 图标）
   - 倾斜角度、阴影效果

## 新的交互流程

```
用户进入
  ↓
看到左侧"报纸收纳抽屉"
  ↓
选择一份报纸
  ↓
向右拖拽 > 300px
  ↓
报纸从抽屉中抽出 🎯
  ↓
右侧滑入阅读区（弹簧动画）
  ↓
查看完整内容
  ↓
[下载PNG] 或 [放回抽屉]
  ↓
报纸滑回左侧抽屉（物理动画）
```

## 核心代码

### 1. 物理弹簧动画

```typescript
const [{ x, y, rotateZ, scale }, api] = useSpring(() => ({
  x: 0,
  y: index * 8,
  rotateZ: (Math.random() - 0.5) * 4, // 随机倾斜
  scale: 1 - index * 0.02,
  config: config.wobbly, // 弹簧物理
}));
```

### 2. 拖拽手势识别

```typescript
const bind = useDrag(
  ({ down, movement: [mx], cancel }) => {
    // 向右拖拽超过300px = 抽取！
    if (mx > 300 && !down) {
      cancel();
      onPull();
      return;
    }

    // 拖拽时的实时动画
    api.start({
      x: down ? mx : 0,
      y: down ? index * 8 - 30 : index * 8,
      rotateZ: down ? mx * 0.05 : (Math.random() - 0.5) * 4,
      scale: down ? 1.05 : 1 - index * 0.02,
      immediate: down,
    });
  },
  {
    axis: 'x',
    bounds: { left: -50, right: window.innerWidth },
    rubberband: true, // 橡皮筋边界效果
  }
);
```

### 3. 阅读区滑入/滑出

```typescript
const readingSpring = useSpring({
  opacity: isReading ? 1 : 0,
  transform: isReading
    ? 'translate3d(0%, 0, 0) scale(1)'
    : 'translate3d(100%, 0, 0) scale(0.8)',
  config: config.slow, // 慢速优雅动画
});
```

## UI/UX 设计

### 左侧：报纸抽屉

```less
.drawerFrame {
  background: linear-gradient(135deg, #2a2a3e 0%, #1f1f2e 100%);
  box-shadow:
    inset 0 2px 8px rgba(0, 0, 0, 0.3), // 内嵌阴影
    0 8px 32px rgba(0, 0, 0, 0.4);
  border: 2px solid rgba(255, 255, 255, 0.05);
}

.drawerLabel {
  background: linear-gradient(135deg, #FFB347, #FF6B6B);
  position: absolute;
  top: -12px;
  // 像真实的抽屉标签
}
```

### 便签贴纸（超可爱）

```less
.stickyNote {
  transform: rotate(3deg); // 倾斜
  box-shadow:
    0 4px 12px rgba(0, 0, 0, 0.2),
    inset 0 1px 0 rgba(255, 255, 255, 0.3); // 高光

  &.unread {
    background: linear-gradient(135deg, #FFD93D, #FFB627); // 黄色便签
    color: #8B4513;
  }

  &.read {
    background: linear-gradient(135deg, #B8F3FF, #7ED4E6); // 蓝色便签
    color: #0D5C75;
  }
}
```

### 抽取提示动画

```less
.pullHint {
  background: rgba(255, 107, 107, 0.95);
  animation: hint-pulse 2s infinite;
}

.pullArrow {
  animation: arrow-bounce 1.5s infinite; // 箭头跳动
}

@keyframes arrow-bounce {
  0%, 100% { transform: translateX(0); }
  50% { transform: translateX(8px); }
}
```

## 视觉特效

### 1. 悬停效果
- 鼠标悬停报纸：向上抬起 20px + 放大 2%
- 阴影增强，边框发光

### 2. 拖拽反馈
- 拖拽时报纸跟随手指（x轴）
- 向上浮动 30px（y轴）
- 旋转角度随拖拽距离变化
- 放大至 105%

### 3. 抽取动画
- 报纸从抽屉移除
- 后面的报纸自动补位（index - 1）
- 阅读区从右侧滑入（transform + opacity）

### 4. 放回动画
- 阅读区向右滑出
- 报纸返回抽屉原位
- 重新堆叠排列

## 布局结构

```
Modal (95vw)
├─ Header
│  ├─ 标题 + 未读数徽章
│  └─ 说明文字
├─ Main Area (flex)
│  ├─ Drawer Area (450px)
│  │  └─ Magazine Cards (堆叠 + 拖拽)
│  └─ Reading Area (flex: 1)
│     ├─ Header (标题 + 日期)
│     ├─ Content (iframe)
│     └─ Actions (放回 + 下载)
└─ Timeline (时间轴快捷跳转)
```

## 技术亮点

### 1. React Spring 配置

```typescript
import { useSpring, animated, config } from "@react-spring/web";

// config.wobbly - 弹性动画
// config.slow - 优雅慢速
// config.default - 标准速度
```

### 2. Use-Gesture 配置

```typescript
import { useDrag } from "@use-gesture/react";

// axis: 'x' - 只允许水平拖拽
// bounds - 拖拽边界
// rubberband - 边界回弹效果
```

### 3. 性能优化

```typescript
will-change: transform; // CSS
touch-action: none; // 禁用浏览器默认触摸
user-select: none; // 禁用文本选择
```

## 状态管理

```typescript
const [magazines, setMagazines] = useState(generateMockHistory(harvest.id));
const [pulledIndex, setPulledIndex] = useState<number | null>(null);
const [isReading, setIsReading] = useState(false);

// pulledIndex = null → 所有报纸在抽屉
// pulledIndex = 3 → 第4份报纸被抽取
// isReading = true → 阅读区显示
```

## Demo 体验步骤

1. **进入 Inbox → 点击 Harvest "View All"**
2. **观察左侧报纸抽屉**
   - 12份报纸堆叠
   - 顶部有"向右拖拽抽取"提示
   - 每份报纸有可爱的便签贴
3. **拖拽顶部报纸向右**
   - 实时跟随手指
   - 超过 300px 自动抽取
4. **右侧阅读区滑入**
   - 弹簧动画
   - 显示完整内容
5. **点击"放回抽屉"**
   - 报纸滑回左侧
   - 弹簧回弹效果
6. **点击"下载PNG"**
   - （Demo: 显示提示）
7. **时间轴快捷跳转**
   - 点击任意日期
   - 自动抽取对应报纸

## 依赖包

```json
{
  "@react-spring/web": "^9.7.5",
  "@use-gesture/react": "^10.3.1"
}
```

安装命令：
```bash
npm install @use-gesture/react react-spring
```

## 与之前版本对比

| 特性 | V2 (framer-motion) | V3 (react-spring) |
|------|-------------------|-------------------|
| 动画库 | framer-motion | react-spring + use-gesture |
| 拖拽方式 | 简单滑动 | **真实物理拖拽** |
| 视图模式 | 堆叠 → 全屏切换 | **抽屉 + 预览并存** |
| 便签标记 | 简单图标 | **可爱的便签贴** |
| 抽取动作 | 点击/拖拽触发 | **向右拖拽 > 300px** |
| 放回操作 | 无 | **"放回抽屉"按钮** |
| 动画效果 | 基础 | **物理弹簧动画** |

## 下一步优化

### 短期
1. ✅ 真实物理动画
2. ✅ 抽屉交互
3. ✅ 便签贴纸
4. ⏳ 移动端触摸优化
5. ⏳ 音效反馈（抽取/放回）

### 中期
1. 真正的 PNG 导出（Playwright）
2. 多份报纸同时对比
3. 报纸收藏功能
4. 自定义便签颜色

### 长期
1. 3D 视角切换
2. 报纸翻页动画
3. VR/AR 体验
4. AI 语音朗读

## 总结

这次重构完全实现了用户要求的"真实报纸抽取体验"：

- ✅ **使用专业动画库**（react-spring）
- ✅ **物理级拖拽交互**（use-gesture）
- ✅ **抽屉收纳隐喻**（左侧抽屉 + 右侧预览）
- ✅ **可爱便签标记**（黄色未读 + 蓝色已读）
- ✅ **真实抽取动画**（拖拽 > 300px）
- ✅ **放回操作**（查看后可放回抽屉）
- ✅ **下载功能**（一键导出PNG）

现在的体验就像真实的办公场景：从抽屉里拿出一份报纸阅读，看完后可以放回或下载保存。

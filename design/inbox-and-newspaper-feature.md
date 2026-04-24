# Inbox与报社（Newspaper）功能技术方案

## 1. 功能概述

### 1.1 总体描述
构建一个agent/workspace级别的统一收件箱（Inbox）系统，集成以下功能：
- **审批通知**：展示待处理的审批请求
- **Push消息通知**：展示Agent通过各个渠道（微信、Slack、Telegram等）调用后端`push-message`能力推送给用户的通知消息
- **智能收获（AI Harvest）**：基于CronJob的智能资讯推送系统，通过Web搜索+HTML生成+截图的方式，定时为用户生成个性化资讯报告

### 1.2 核心价值
- **统一入口**：一站式查看所有需要关注的信息
- **自动化资讯**：像挂机游戏一样，让Agent自动为你收集和整理资讯
- **个性化定制**：提供报纸模板，用户可以创建自定义的资讯订阅

## 2. 实现分阶段策略

### 2.1 第一阶段：Demo版本（用于演示）
**目标**：快速构建可演示的前端界面，使用Mock数据展示完整交互流程

**分支名称**：`feature/inbox-demo`

**实现范围**：
- ✅ 完整的前端UI/UX
- ✅ 所有交互逻辑（使用Mock数据）
- ✅ 报社卡片的倒计时动画
- ✅ 报纸预览界面
- ❌ 不包含后端API
- ❌ 不包含真实数据持久化

### 2.2 第二阶段：生产版本（完整功能）
**目标**：完整实现前后端功能，包括数据持久化、CronJob集成、Web搜索、截图生成等

**分支名称**：`feature/inbox-production`

**实现范围**：
- ✅ 完整的前后端实现
- ✅ 数据库Schema设计与实现
- ✅ API接口实现
- ✅ CronJob系统集成
- ✅ Web搜索功能
- ✅ HTML生成与截图功能
- ✅ 通知系统集成

## 3. 第一阶段详细设计（Demo版本）

### 3.1 前端架构

#### 3.1.1 路由结构
```
/inbox                    # 收件箱主页面
  /inbox/approvals        # 审批列表（可选子页面）
  /inbox/notifications    # 通知列表（可选子页面）
  /inbox/newspapers       # 报社管理页面
  /inbox/newspapers/:id   # 报纸详情预览页面
```

#### 3.1.2 页面组件结构

```
console/src/pages/Inbox/
├── index.tsx                      # 收件箱主页面
├── index.module.less              # 样式文件
├── components/
│   ├── InboxTabs.tsx              # 顶部Tab切换组件
│   ├── ApprovalCard.tsx           # 审批卡片组件
│   ├── PushMessageCard.tsx        # Push消息卡片组件
│   ├── HarvestCard.tsx            # 收获卡片组件（核心）
│   ├── HarvestCountdown.tsx       # 倒计时圆圈组件
│   ├── HarvestPreview.tsx         # 收获预览Modal（翻页阅读器）
│   ├── CreateHarvestModal.tsx     # 创建收获Modal
│   ├── HarvestTemplates.tsx       # 收获模板选择
│   └── index.ts
├── hooks/
│   ├── useInboxData.ts            # Mock数据管理Hook
│   └── useHarvestCountdown.ts     # 倒计时逻辑Hook
└── types.ts                       # TypeScript类型定义
```

### 3.2 UI/UX设计规范

#### 3.2.1 设计理念
- **高端简约**：遵循"瑞士豪华水疗中心"风格
- **图标统一**：仅使用Lucide-React图标
- **配色一致**：使用项目统一的主色调（#FF7F16）
- **间距精准**：所有元素布局合理，无拥挤感
- **响应式设计**：适配所有屏幕尺寸

#### 3.2.2 收获卡片设计（核心，挂机游戏风格）

**布局结构**：
```
┌─────────────────────────────────────────────┐
│  🌾 科技前沿收获                             │
│  ┌───────────────┐                          │
│  │   ⭕ 65%      │  ✨ 已收获 23 次          │
│  │   05:23:15    │  📊 成功率 95.6%         │
│  │   距离收获     │                          │
│  └───────────────┘                          │
│  💎 上次收获：2小时前                        │
│  📦 最新产出：《AI编程助手趋势报告》          │
│                                             │
│  [⚡ 立即收获]  [📖 查看全部]  [⚙️ 设置]     │
└─────────────────────────────────────────────┘
```

**挂机游戏式视觉元素**：
1. **倒计时圆圈**：
   - 使用 `react-circular-progressbar`
   - 渐变色：从金色（#FFD700）到橙色（#FF7F16）
   - 中央显示剩余时间+进度百分比
   - 完成时有"✨收获完成"的动画效果
   - 类似游戏里的资源生产倒计时

2. **收获统计**：
   - 已收获次数（像游戏里的产出计数）
   - 成功率（质量保障）
   - 连续收获天数（增加粘性）

3. **产出预览**：
   - 显示最新一次的产出标题
   - 悬停显示缩略图预览
   - 点击直接进入阅读模式

4. **状态指示**：
   - 🌱 准备中（灰色）
   - 🌾 生长中（黄色，倒计时进行中）
   - ✨ 已收获（金色，闪烁动画）
   - ⚠️ 收获失败（红色）

**卡片交互**：
- **点击卡片主体** → 进入"收获仓库"（历史产出列表，翻页展示）
- **点击"立即收获"** → 立即触发生成，显示进度动画
- **点击"查看全部"** → 查看所有历史收获
- **点击"设置"** → 编辑收获规则
- **悬停卡片** → 显示3D轻微上浮效果

**收获完成动画**：
- 卡片周围出现金色光环
- 顶部飘出"✨ 新收获"浮动通知
- 产出物以3D翻转效果展示封面
- 播放轻微的"叮"提示音（可选）

#### 3.2.3 收获模板设计

**内置模板（每个都是一个"收获机器"）**：

1. **🚀 科技前沿收获**
   - 定位：每天收获最新的AI/技术动态
   - 关键词：AI, GPT, LLM, 开源项目, 技术趋势
   - 频率建议：每日 09:00
   - 产出物：《今日科技简报》（5-8个要闻）
   - 阅读时间：约8分钟

2. **📊 行业情报收获**
   - 定位：深度行业分析和趋势洞察
   - 关键词：用户自定义行业（如：SaaS, Fintech, AI Agent）
   - 频率建议：每周一 10:00
   - 产出物：《周度行业洞察报告》（含图表分析）
   - 阅读时间：约15分钟

3. **🏢 竞品动态收获**
   - 定位：自动追踪竞品的一举一动
   - 关键词：竞品公司名称、产品名称
   - 频率建议：每日 18:00
   - 产出物：《竞品日报》（融资、发布、用户反馈）
   - 阅读时间：约5分钟

4. **🎓 学术前沿收获**
   - 定位：顶会论文、ArXiv最新研究
   - 关键词：领域关键词（如：Computer Vision, NLP）
   - 频率建议：每周三 11:00
   - 产出物：《本周论文精选》（3-5篇论文摘要）
   - 阅读时间：约20分钟

5. **💼 投资机会收获**
   - 定位：创业公司、融资动态、市场机会
   - 关键词：Venture Capital, Startup, Funding
   - 频率建议：每周五 14:00
   - 产出物：《本周投资观察》
   - 阅读时间：约12分钟

6. **🎯 自定义收获**
   - 用户完全自定义收获规则

**模板配置项**：
```typescript
interface HarvestTemplate {
  id: string;
  name: string;
  emoji: string;  // 挂机游戏风格的图标
  description: string;
  estimatedReadTime: number;  // 预计阅读时间（分钟）

  defaultSchedule: {
    cron: string;
    timezone: string;
  };

  searchConfig: {
    keywords: string[];
    sources: string[];
    language: string;
    maxResults: number;
  };

  layoutConfig: {
    style: 'magazine' | 'newspaper' | 'brief';  // 展示风格
    sections: HarvestSection[];
  };

  // 挂机游戏式的奖励系统
  rewards: {
    xpPerHarvest: number;  // 每次收获获得的"经验值"
    achievements: string[];  // 成就系统（如：连续收获7天）
  };
}
```

### 3.3 Mock数据结构

#### 3.3.1 收件箱总览数据
```typescript
interface InboxSummary {
  approvals: {
    total: number;
    urgent: number;
  };
  pushMessages: {
    total: number;
    unread: number;
  };
  harvests: {
    total: number;
    active: number;
  };
}
```

#### 3.3.2 Push消息数据
```typescript
interface PushMessage {
  id: string;
  channelType: 'wechat' | 'slack' | 'telegram' | 'discord' | 'email';
  channelName: string;
  title: string;
  content: string;
  sender: {
    userId: string;
    username: string;
  };
  createdAt: Date;
  read: boolean;
  // 消息元数据
  metadata: {
    messageId?: string;
    conversationId?: string;
    priority?: 'low' | 'normal' | 'high' | 'urgent';
  };
}
```

#### 3.3.3 收获实例数据
```typescript
interface NewspaperInstance {
  id: string;
  name: string;
  templateId: string;
  icon: LucideIcon;
  schedule: {
    cron: string;
    timezone: string;
    nextRun: Date;
  };
  status: 'active' | 'paused' | 'error';
  lastGenerated?: {
    timestamp: Date;
    success: boolean;
    previewUrl?: string;
  };
  stats: {
    totalGenerated: number;
    successRate: number;
  };
}
```

#### 3.3.4 收获内容数据（生成后）
```typescript
interface NewspaperContent {
  id: string;
  newspaperId: string;
  generatedAt: Date;
  sections: {
    title: string;
    content: string;
    sources: Array<{
      title: string;
      url: string;
      publishedAt?: Date;
    }>;
  }[];
  screenshot: {
    url: string;
    width: number;
    height: number;
  };
  format: 'html' | 'markdown';
  rawHtml?: string;
}
```

### 3.4 产出物阅读体验设计（核心卖点）

#### 3.4.1 翻页式阅读器（Flipbook Reader）

**设计理念**：像在咖啡厅翻阅《纽约客》杂志一样优雅

**推荐库**：
- **react-pageflip** 或 **turn.js**：3D翻页效果
- **react-reader**：电子书阅读体验
- 或者自行实现基于 **framer-motion** 的翻页动画

**阅读器布局**：
```
┌─────────────────────────────────────────────────┐
│  [← 返回]    《今日科技简报》    [分享] [收藏]   │
├─────────────────────────────────────────────────┤
│                                                 │
│  ┌──────────────┐     ┌──────────────┐         │
│  │              │     │              │         │
│  │  左页内容     │ ▸▸  │  右页内容     │         │
│  │              │     │              │         │
│  │              │     │              │         │
│  │              │     │              │         │
│  └──────────────┘     └──────────────┘         │
│                                                 │
│              [◀ 上一页]  3/12  [下一页 ▶]       │
└─────────────────────────────────────────────────┘
```

**翻页交互**：
- **点击页面右侧** → 下一页（带3D翻页动画）
- **点击页面左侧** → 上一页
- **滑动手势** → 翻页（移动端）
- **键盘方向键** → 翻页（桌面端）
- **双击放大** → 查看细节
- **滚轮** → 平滑滚动（可选模式）

**翻页动画效果**：
- 3D翻转效果（像真实的书页）
- 页面阴影效果
- 翻页时的纸张质感
- 翻页声音效果（可选）

#### 3.4.2 页面排版风格

**风格1：杂志风格（Magazine Layout）**
- 大图片 + 标题 + 正文
- 双栏排版
- 引用框、高亮标注
- 适合：科技前沿、行业情报

**风格2：报纸风格（Newspaper Layout）**
- 紧凑排版
- 多栏布局
- 醒目标题
- 适合：竞品动态、投资机会

**风格3：简报风格（Brief Layout）**
- 卡片式布局
- 要点罗列
- 图标+文字
- 适合：快速浏览

**排版示例（杂志风格第一页）**：
```
╔═══════════════════════════════════════╗
║                                       ║
║      📱 今日科技简报                   ║
║      2026年4月24日 · 星期五            ║
║                                       ║
║  ┌─────────────────────────────────┐  ║
║  │                                 │  ║
║  │    [精美的封面配图]              │  ║
║  │                                 │  ║
║  └─────────────────────────────────┘  ║
║                                       ║
║    今日要闻：                          ║
║    • OpenAI发布GPT-5预览版            ║
║    • Meta开源最新AI Agent框架          ║
║    • 谷歌量子计算重大突破              ║
║                                       ║
║           [开始阅读 →]                ║
║                                       ║
╚═══════════════════════════════════════╝
```

**内容页示例（左右对照）**：
```
┌──────────────────────┬──────────────────────┐
│ 🚀 OpenAI发布GPT-5   │  [配图]              │
│                      │                      │
│ 4月23日，OpenAI在... │  主要特性：          │
│                      │  • 推理能力提升3x     │
│ 据知情人士透露...    │  • 支持多模态输入     │
│                      │  • 成本降低50%        │
│ 这次发布标志着...    │                      │
│                      │  分析师观点：         │
│ 💬 行业反应          │  "这将改变..."       │
│ - 微软：积极评价     │                      │
│ - 谷歌：加速竞争     │  📊 [市场影响图表]   │
└──────────────────────┴──────────────────────┘
```

#### 3.4.3 交互增强功能

**1. 高亮与笔记**
- 选中文字 → 高亮颜色选择
- 添加批注
- 导出到笔记系统

**2. 智能摘要**
- 每篇文章顶部显示"TL;DR"
- AI生成的3句话摘要
- 关键词标签

**3. 相关链接**
- 原始来源链接
- 相关历史收获
- 深度阅读推荐

**4. 阅读进度**
- 显示阅读进度条
- 记住阅读位置
- 标记为"已读"

**5. 分享与导出**
- 分享到社交平台
- 导出为PDF
- 生成精美海报

#### 3.4.4 动画效果（使用现成库）

**推荐库**：
- **react-pageflip**：3D翻页效果（核心）
- **framer-motion**：页面过渡、卡片动画
- **react-circular-progressbar**：倒计时圆圈
- **react-spring**：复杂的物理动画效果
- **aos (Animate On Scroll)**：滚动触发动画

**关键动画**：
1. **卡片进入动画**
   - 淡入 + 轻微上浮
   - 错峰动画（stagger）

2. **倒计时圆圈动画**
   - 平滑的进度条动画
   - 剩余时间数字翻转动画（react-flip-numbers）

3. **收获完成动画**
   - 卡片金色光环脉冲
   - 飘出"✨新收获"浮动通知
   - 产出物3D翻转展示

4. **翻页动画**（最重要）
   - 3D翻书效果
   - 页面卷曲阴影
   - 翻页速度曲线（ease-out）

5. **阅读反馈**
   - 高亮动画
   - 书签添加动画
   - 阅读进度平滑更新

#### 3.4.5 响应式适配

**桌面端（>1024px）**：
- 双页展示（像打开的书）
- 3D翻页效果
- 鼠标悬停预览

**平板端（768px-1024px）**：
- 单页展示
- 保留翻页效果
- 手势支持

**移动端（<768px）**：
- 垂直滚动模式（可选）
- 或单页翻页模式
- 针对触摸优化

### 3.5 产出物视觉设计规范

#### 3.5.1 封面设计

**封面元素**：
1. **标题区**
   - 收获名称：大号字体（32px）
   - 日期：副标题（14px）
   - 期数标记：如"第23期"

2. **视觉主体**
   - AI生成的主题配图（使用DALL-E或Stable Diffusion）
   - 或精选的网络图片
   - 渐变背景色（根据收获类型）

3. **元信息**
   - 预计阅读时间
   - 文章数量
   - 关键词标签

**封面模板示例**：
```
┌───────────────────────────────────┐
│                                   │
│    🚀                             │
│                                   │
│    今日科技简报                    │
│    Daily Tech Harvest             │
│                                   │
│    2026年4月24日 · 第23期          │
│                                   │
│    ┌─────────────────────┐        │
│    │                     │        │
│    │  [主题配图]          │        │
│    │                     │        │
│    └─────────────────────┘        │
│                                   │
│    📊 8篇文章  ⏱️ 12分钟阅读      │
│    #AI #OpenSource #Funding       │
│                                   │
└───────────────────────────────────┘
```

#### 3.5.2 内容页设计规范

**排版参数**：
- 字体：系统无衬线字体（Inter / SF Pro）
- 标题：24px / font-weight: 700
- 正文：16px / font-weight: 400 / line-height: 1.8
- 引用：14px / italic / 左侧色条
- 代码：Monaco / 背景色区分

**配色方案**：
```css
/* 亮色主题 */
--harvest-bg: #FAFAFA;
--harvest-text: #1A1A1A;
--harvest-accent: #FF7F16;
--harvest-secondary: #666666;
--harvest-border: #E0E0E0;

/* 暗色主题 */
--harvest-bg-dark: #1A1A1A;
--harvest-text-dark: #E0E0E0;
--harvest-accent-dark: #FF9A3C;
--harvest-secondary-dark: #999999;
--harvest-border-dark: #333333;
```

**内容区块类型**：
1. **标题块**（Hero Section）
   - 大标题 + 副标题
   - 来源标签
   - 发布时间

2. **正文块**（Article Section）
   - 段落文字
   - 行间距1.8
   - 首行缩进或段间距

3. **引用块**（Quote Section）
   - 左侧橙色色条
   - 斜体文字
   - 来源署名

4. **列表块**（List Section）
   - 要点罗列
   - 图标bullet points
   - 层级缩进

5. **图表块**（Chart Section）
   - 数据可视化
   - 图表说明
   - 数据来源

6. **分割块**（Divider Section）
   - 优雅的分割线
   - 或装饰性图案

#### 3.5.3 特殊元素设计

**"TL;DR" 摘要框**：
```
┌─────────────────────────────────────┐
│ 💡 核心要点（TL;DR）                 │
├─────────────────────────────────────┤
│ • OpenAI发布GPT-5，推理能力大幅提升 │
│ • 预计将在Q3正式发布               │
│ • 市场预期将影响整个AI行业格局      │
└─────────────────────────────────────┘
```

**"深度分析" 展开块**：
```
┌─────────────────────────────────────┐
│ 📊 深度分析（点击展开）              │
│                                     │
│ [展开后显示详细的图表、数据、分析]   │
└─────────────────────────────────────┘
```

**"相关链接" 卡片**：
```
┌──────────────────────┐
│ 🔗 相关阅读          │
│                      │
│ • 上周的相关报道     │
│ • 行业专家评论       │
│ • 官方公告链接       │
└──────────────────────┘
```

#### 3.5.4 页脚设计

**每页页脚包含**：
- 页码（如：第3页 / 共12页）
- 收获名称（小号文字）
- 生成时间
- 品牌标识（QwenPaw logo）

```
┌─────────────────────────────────────┐
│                                     │
│  [内容区域]                         │
│                                     │
├─────────────────────────────────────┤
│  今日科技简报     3/12    2026-04-24│
│            QwenPaw AI Harvest       │
└─────────────────────────────────────┘
```

### 3.6 图标选择（Lucide-React）

**功能对应图标**：
- 收获/收件箱：`PackageOpen` / `Gift`
- 倒计时：`Timer` / `Hourglass`
- 立即收获：`Zap` / `Sparkles`
- 查看全部：`BookOpen` / `Library`
- 设置：`Settings` / `Sliders`
- 添加收获：`PlusCircle`
- 模板：`LayoutTemplate`
- 翻页：`ChevronLeft` / `ChevronRight`
- 书签：`Bookmark`
- 分享：`Share2`
- 下载：`Download`
- 高亮：`Highlighter`
- 笔记：`FileEdit`
- 成功：`CheckCircle2` / `Sparkles`
- 失败：`AlertCircle`
- 暂停：`PauseCircle`
- 统计：`BarChart3`
- 奖励：`Trophy` / `Award`

## 4. 第二阶段详细设计（生产版本）

### 4.1 后端架构

#### 4.1.1 数据库Schema

**表1：newspapers（报纸实例）**
```python
class Newspaper(Base):
    __tablename__ = "newspapers"

    id: str = Column(String, primary_key=True)
    agent_id: Optional[str] = Column(String, ForeignKey("agents.id"))
    workspace_id: Optional[str] = Column(String)
    name: str = Column(String, nullable=False)
    template_id: str = Column(String, nullable=False)
    icon: str = Column(String)

    # Schedule (复用CronJob的schedule结构)
    schedule_cron: str = Column(String, nullable=False)
    schedule_timezone: str = Column(String, default="UTC")

    # Search Configuration
    search_keywords: JSON = Column(JSON)  # List[str]
    search_sources: JSON = Column(JSON)   # List[str]
    search_language: str = Column(String, default="en")

    # Layout Configuration
    layout_config: JSON = Column(JSON)

    # Status
    status: str = Column(String, default="active")  # active, paused, error
    enabled: bool = Column(Boolean, default=True)

    # Metadata
    created_at: datetime = Column(DateTime, default=datetime.utcnow)
    updated_at: datetime = Column(DateTime, onupdate=datetime.utcnow)

    # CronJob Integration
    cronjob_id: Optional[str] = Column(String, ForeignKey("cronjobs.id"))
```

**表2：newspaper_contents（报纸内容）**
```python
class NewspaperContent(Base):
    __tablename__ = "newspaper_contents"

    id: str = Column(String, primary_key=True)
    newspaper_id: str = Column(String, ForeignKey("newspapers.id"))

    # Generation Info
    generated_at: datetime = Column(DateTime, default=datetime.utcnow)
    generation_duration: float = Column(Float)  # seconds
    success: bool = Column(Boolean, default=True)
    error_message: Optional[str] = Column(Text)

    # Content
    sections: JSON = Column(JSON)  # List[Section]
    raw_html: Text = Column(Text)

    # Screenshot
    screenshot_path: str = Column(String)
    screenshot_width: int = Column(Integer)
    screenshot_height: int = Column(Integer)

    # Metadata
    sources_count: int = Column(Integer)
    word_count: int = Column(Integer)
```

**表3：inbox_items（收件箱项）**
```python
class InboxItem(Base):
    __tablename__ = "inbox_items"

    id: str = Column(String, primary_key=True)
    agent_id: Optional[str] = Column(String, ForeignKey("agents.id"))
    workspace_id: Optional[str] = Column(String)

    # Item Type
    item_type: str = Column(String, nullable=False)
    # Possible values: approval, push_message, harvest

    # Reference
    reference_id: str = Column(String)  # ID of the related item
    reference_type: str = Column(String)  # Type of the related item

    # Content
    title: str = Column(String, nullable=False)
    description: Optional[str] = Column(Text)
    metadata: JSON = Column(JSON)

    # Status
    read: bool = Column(Boolean, default=False)
    resolved: bool = Column(Boolean, default=False)
    priority: int = Column(Integer, default=0)  # 0=normal, 1=high, 2=urgent

    # Timestamps
    created_at: datetime = Column(DateTime, default=datetime.utcnow)
    read_at: Optional[datetime] = Column(DateTime)
    resolved_at: Optional[datetime] = Column(DateTime)
```

**表4：push_messages（Push消息记录）**
```python
class PushMessage(Base):
    __tablename__ = "push_messages"

    id: str = Column(String, primary_key=True)
    agent_id: Optional[str] = Column(String, ForeignKey("agents.id"))
    workspace_id: Optional[str] = Column(String)

    # Channel Info
    channel_type: str = Column(String, nullable=False)
    # Possible values: wechat, slack, telegram, discord, email, etc.
    channel_id: str = Column(String)
    channel_name: str = Column(String)

    # Sender Info (who triggered the push)
    sender_user_id: str = Column(String)
    sender_username: Optional[str] = Column(String)

    # Message Content
    title: str = Column(String)
    content: Text = Column(Text, nullable=False)
    content_type: str = Column(String, default="text")
    # Possible values: text, markdown, html

    # Status
    status: str = Column(String, default="sent")
    # Possible values: pending, sent, delivered, failed
    read: bool = Column(Boolean, default=False)

    # Metadata
    metadata: JSON = Column(JSON)
    # Contains: messageId, conversationId, priority, etc.

    # Timestamps
    created_at: datetime = Column(DateTime, default=datetime.utcnow)
    sent_at: Optional[datetime] = Column(DateTime)
    delivered_at: Optional[datetime] = Column(DateTime)
    read_at: Optional[datetime] = Column(DateTime)
```

#### 4.1.2 API接口设计

**路由前缀**：`/api/v1/inbox`

**端点列表**：

```python
# ===== Inbox总览 =====
GET    /api/v1/inbox/summary
# 返回：InboxSummary（各类型项目统计）

GET    /api/v1/inbox/items
# 查询参数：item_type, read, resolved, limit, offset
# 返回：分页的InboxItem列表

PATCH  /api/v1/inbox/items/{item_id}
# Body: { read?: bool, resolved?: bool }
# 功能：标记为已读/已处理

# ===== Push消息管理 =====
GET    /api/v1/inbox/push-messages
# 查询参数：channel_type, read, limit, offset
# 返回：PushMessage列表

GET    /api/v1/inbox/push-messages/{message_id}
# 返回：PushMessage详情

PATCH  /api/v1/inbox/push-messages/{message_id}
# Body: { read?: bool }
# 功能：标记为已读

DELETE /api/v1/inbox/push-messages/{message_id}
# 功能：删除Push消息

# ===== Newspaper管理 =====
GET    /api/v1/newspapers
# 查询参数：agent_id, workspace_id, status
# 返回：Newspaper列表

POST   /api/v1/newspapers
# Body: NewspaperCreateInput
# 功能：创建新报纸实例（自动创建关联CronJob）

GET    /api/v1/newspapers/{newspaper_id}
# 返回：Newspaper详情

PATCH  /api/v1/newspapers/{newspaper_id}
# Body: NewspaperUpdateInput
# 功能：更新报纸配置（同步更新CronJob）

DELETE /api/v1/newspapers/{newspaper_id}
# 功能：删除报纸实例（同时删除CronJob）

POST   /api/v1/newspapers/{newspaper_id}/trigger
# 功能：立即触发生成报纸

# ===== Newspaper内容 =====
GET    /api/v1/newspapers/{newspaper_id}/contents
# 查询参数：limit, offset, success_only
# 返回：NewspaperContent列表（历史记录）

GET    /api/v1/newspaper-contents/{content_id}
# 返回：NewspaperContent详情

GET    /api/v1/newspaper-contents/{content_id}/screenshot
# 返回：截图文件（Binary response）

# ===== Newspaper模板 =====
GET    /api/v1/newspaper-templates
# 返回：内置模板列表
```

#### 4.1.3 核心业务逻辑

**NewspaperGenerator类**：
```python
class NewspaperGenerator:
    """报纸生成器"""

    async def generate(
        self,
        newspaper: Newspaper,
        context: Optional[Dict] = None
    ) -> NewspaperContent:
        """
        生成报纸的主流程：
        1. Web搜索（使用WebSearch工具）
        2. 内容整理（使用LLM）
        3. HTML生成（使用模板引擎）
        4. 截图生成（使用Playwright/Puppeteer）
        5. 保存到数据库
        6. 创建InboxItem通知用户
        """
        pass

    async def _search_content(
        self,
        keywords: List[str],
        sources: List[str],
        language: str
    ) -> List[SearchResult]:
        """执行Web搜索"""
        pass

    async def _organize_content(
        self,
        search_results: List[SearchResult],
        layout_config: Dict
    ) -> List[Section]:
        """使用LLM整理内容"""
        pass

    async def _generate_html(
        self,
        sections: List[Section],
        template: str
    ) -> str:
        """生成HTML"""
        pass

    async def _capture_screenshot(
        self,
        html: str,
        output_path: str
    ) -> Tuple[int, int]:
        """生成截图"""
        pass

    async def _create_inbox_notification(
        self,
        newspaper: Newspaper,
        content: NewspaperContent
    ) -> InboxItem:
        """创建收件箱通知"""
        pass
```

**CronJob集成**：
```python
# 创建Newspaper时，自动创建对应的CronJob
async def create_newspaper_with_cronjob(
    newspaper_input: NewspaperCreateInput
) -> Newspaper:
    # 1. 创建Newspaper记录
    newspaper = Newspaper(**newspaper_input.dict())
    db.add(newspaper)

    # 2. 创建CronJob
    cronjob = CronJobSpec(
        id=f"newspaper_{newspaper.id}",
        name=f"Newspaper: {newspaper.name}",
        enabled=True,
        schedule={
            "type": "cron",
            "cron": newspaper.schedule_cron,
            "timezone": newspaper.schedule_timezone
        },
        task_type="agent",  # 或者新增"newspaper"类型
        request={
            "input": {
                "action": "generate_newspaper",
                "newspaper_id": newspaper.id
            }
        },
        dispatch={
            "type": "channel",
            "target": {
                "user_id": current_user.id,
                "session_id": newspaper.workspace_id
            }
        }
    )

    # 3. 注册CronJob到调度器
    scheduler.add_job(cronjob)

    newspaper.cronjob_id = cronjob.id
    db.commit()

    return newspaper
```

### 4.2 技术选型

#### 4.2.1 Web搜索
- **方案1**：使用现有的WebSearch工具（推荐）
- **方案2**：集成SerpAPI / Google Custom Search API
- **方案3**：集成Tavily API（专门为AI设计）

#### 4.2.2 HTML生成
- **Jinja2**：Python模板引擎（推荐）
- **Tailwind CSS**：用于报纸样式

#### 4.2.3 截图生成
- **Playwright**：推荐，跨平台，性能好
- **Puppeteer**：备选方案
- 需要考虑：
  - Docker环境支持
  - 无头浏览器依赖
  - 截图质量与大小

#### 4.2.4 文件存储
- **本地文件系统**：`workspace/.newspapers/screenshots/`
- **对象存储**：可选，S3/MinIO（如果需要）

### 4.3 前端集成

#### 4.3.1 API调用
- 使用现有的`api/modules`结构
- 创建`console/src/api/modules/newspaper.ts`
- 创建`console/src/api/modules/inbox.ts`

#### 4.3.2 状态管理
- 使用Zustand创建`inboxStore`
- 处理实时更新（可选：WebSocket）

#### 4.3.3 轮询机制
- 倒计时状态更新：每秒
- 收件箱项更新：每30秒
- 报纸生成状态检查：每5秒（生成中）

### 4.4 错误处理

**常见错误场景**：
1. Web搜索失败：重试3次，然后标记为失败
2. LLM生成超时：设置30秒超时
3. 截图生成失败：降级为仅HTML
4. CronJob调度失败：记录日志，发送告警通知

**错误通知**：
- 在Inbox中创建错误通知
- 标记Newspaper为`error`状态
- 在下次成功生成后自动恢复

## 5. Demo演示脚本（向老板展示）

### 5.1 开场白（30秒）

**场景设定**：
> "Boss，我想给你展示一个新功能，我们内部叫它'**智能收获站**'（AI Daily Harvest）。
>
> 它解决的核心问题是：**信息过载时代，如何让AI成为你的私人分析师，24小时不间断地为你工作？**
>
> 简单来说，就像玩挂机游戏一样——你设置好规则，AI每天自动为你'收获'价值信息。"

### 5.2 演示流程（3-5分钟）

#### 第一幕：收获站概览（1分钟）
**展示内容**：收获站主页面，3-4个收获卡片

**讲解词**：
> "你看这个界面，每个卡片就像一个自动赚钱的机器。
>
> 比如这个'科技前沿收获'（指向第一个卡片），我设置成每天早上9点自动运行。你看这个倒计时圆圈（指向倒计时），现在显示还有5小时23分钟，到时候AI就会自动开始工作。
>
> 这里显示我已经收获了23次（指向统计），成功率95.6%。上次收获是2小时前，产出了一份《AI编程助手趋势报告》。"

**操作演示**：
- 悬停卡片，展示3D效果
- 指向倒计时圆圈，强调"像游戏一样的体验"

#### 第二幕：立即收获（1分钟）
**展示内容**：点击"立即收获"按钮，触发生成流程

**讲解词**：
> "当然，我也可以随时'立即收获'（点击按钮）。
>
> 你看，AI现在开始工作了（展示进度动画）：
> 1. 正在搜索最新的AI资讯...（进度20%）
> 2. 分析和整理内容...（进度60%）
> 3. 生成精美排版...（进度90%）
> 4. 收获完成！（显示完成动画）"

**操作演示**：
- 点击"立即收获"
- 展示进度条动画
- 完成后显示金色光环效果
- 弹出"新收获"通知

#### 第三幕：翻阅产出物（2分钟，核心卖点）
**展示内容**：打开刚生成的报告，展示翻页阅读体验

**讲解词**：
> "这是刚才生成的报告（点击打开）。注意看这个阅读体验——
>
> （展示封面）这是封面，有标题、日期、预计阅读时间。
>
> （翻页）我们做了3D翻页效果，就像在咖啡厅翻阅《纽约客》杂志一样（演示翻页）。
>
> （指向内容）排版非常考究：标题、正文、引用、图表，都经过精心设计。
>
> （展示TL;DR框）每篇文章都有AI生成的核心要点，如果赶时间，看这个就够了。
>
> （展示交互功能）还可以高亮、做笔记、分享给同事。"

**操作演示**：
- 翻3-4页，展示不同内容排版
- 高亮一段文字
- 展示"分享"功能
- 返回首页

#### 第四幕：创建新收获（1分钟）
**展示内容**：创建收获流程，展示模板选择

**讲解词**：
> "创建新收获也很简单（点击'添加'按钮）。
>
> 我们提供了几个模板（展示模板列表）：
> - 科技前沿收获：追踪AI、技术趋势
> - 行业情报收获：深度行业分析
> - 竞品动态收获：自动追踪竞品
> - 投资机会收获：创业公司、融资动态
>
> 选一个模板，设置频率和关键词，就完成了。之后AI就会自动工作。"

**操作演示**：
- 展示模板列表
- 快速浏览配置选项
- 不保存，返回

### 5.3 收尾总结（30秒）

**核心卖点总结**：
> "总结一下，这个功能的核心价值是：
>
> 1. **被动收入式体验**：设置一次，持续产出价值
> 2. **高质量产出**：不是原始信息堆砌，而是AI整理的成品
> 3. **视觉享受**：优美的翻页式阅读体验
> 4. **差异化竞争**：市场上没有类似的AI挂机式产品
>
> 而且，基于现有的CronJob和WebSearch基础设施，开发成本很低，但用户价值很高。"

### 5.4 预期反馈与应对

**如果老板问：和RSS订阅有什么区别？**
> "核心区别是：RSS给你原始信息，我们给你成品。
>
> RSS订阅可能有100篇文章，但我们的AI会帮你筛选、整理、分析，最后生成一份8分钟就能读完的精美报告。
>
> 而且阅读体验完全不一样——我们是像翻杂志一样的优雅体验，而不是无止境的信息流。"

**如果老板问：这个功能的商业价值是什么？**
> "三个方向：
>
> 1. **用户留存**：挂机游戏式的持续产出，会让用户每天都想回来看看'收获'了什么
> 2. **差异化竞争**：Notion、飞书、Slack都没有这种AI挂机式的产品
> 3. **企业市场**：可以做成团队版，每天自动生成行业简报，CEO很需要这个"

**如果老板问：开发成本多大？**
> "Demo版本3-5天就能完成，主要是前端UI。
>
> 生产版本需要2-3周，但我们已经有CronJob、WebSearch、LLM的基础设施，主要是整合和优化。
>
> ROI很高。"

## 6. 实现时间线

### 6.1 第一阶段（Demo版本）

**预计时间**：3-5天

**Demo演示重点**：
- ✅ 收获卡片（带倒计时动画）—— **视觉冲击力**
- ✅ 翻页阅读体验 —— **核心卖点**
- ✅ 模板选择界面 —— **展示可扩展性**
- ✅ 完整交互流程 —— **证明可行性**

**任务清单**：
- [ ] Day 1: 基础架构搭建
  - [ ] 创建路由和页面结构
  - [ ] 设计TypeScript类型
  - [ ] 创建Mock数据Hook
  - [ ] 集成react-pageflip或turn.js

- [ ] Day 2: 收件箱主页面
  - [ ] InboxTabs组件（三个Tab：审批、Push消息、智能收获）
  - [ ] ApprovalCard组件（Mock审批数据）
  - [ ] PushMessageCard组件（Mock Push消息数据）
  - [ ] 基础样式和布局

- [ ] Day 3: 收获卡片（重点）
  - [ ] HarvestCard组件
  - [ ] 倒计时圆圈动画（react-circular-progressbar）
  - [ ] 收获完成动画效果
  - [ ] 卡片悬停3D效果

- [ ] Day 4: 翻页阅读器（核心卖点）
  - [ ] FlipbookReader组件
  - [ ] 封面页设计
  - [ ] 内容页排版
  - [ ] 3D翻页动画
  - [ ] 页面导航控制

- [ ] Day 5: 创建与模板
  - [ ] CreateHarvestModal
  - [ ] HarvestTemplates展示
  - [ ] 模板配置表单
  - [ ] 模板预览

- [ ] Day 6: 整体优化与演示准备
  - [ ] 响应式适配
  - [ ] 动画流畅度优化
  - [ ] Mock数据完善（准备5-6个不同类型的产出物）
  - [ ] 准备Push消息的Mock数据（模拟不同渠道）
  - [ ] Demo演示脚本排练

### 5.2 第二阶段（生产版本）

**预计时间**：10-15天

**任务清单**：
- [ ] Week 1: 后端基础
  - [ ] 数据库Schema设计与迁移
  - [ ] API路由实现
  - [ ] CronJob集成

- [ ] Week 2: 核心功能
  - [ ] NewspaperGenerator实现
  - [ ] Web搜索集成
  - [ ] HTML模板设计
  - [ ] 截图生成功能

- [ ] Week 3: 前后端联调
  - [ ] 前端API集成
  - [ ] 实时数据更新
  - [ ] 错误处理完善
  - [ ] 性能优化

- [ ] Week 4: 测试与部署
  - [ ] 单元测试
  - [ ] 集成测试
  - [ ] 文档编写
  - [ ] 部署与监控

## 6. 风险与挑战

### 6.1 技术风险
1. **截图生成性能**：无头浏览器可能占用较多资源
   - 缓解：使用队列异步处理，限制并发数

2. **Web搜索API配额**：可能遇到API限制
   - 缓解：实现缓存机制，合理设置频率

3. **LLM Token消耗**：内容整理可能消耗大量Token
   - 缓解：优化Prompt，控制输入长度

### 6.2 产品风险
1. **用户实际需求**：报社功能可能不符合用户期望
   - 缓解：Demo阶段快速验证，收集反馈

2. **内容质量**：自动生成的报纸质量可能不稳定
   - 缓解：提供人工编辑选项，持续优化Prompt

## 7. 后续优化方向

1. **AI编辑功能**：允许用户对生成的报纸进行编辑
2. **分享功能**：将报纸分享给团队成员
3. **订阅管理**：邮件/消息推送
4. **报纸归档**：长期保存和检索
5. **统计分析**：阅读时长、关注话题分析
6. **多语言支持**：支持不同语言的报纸生成
7. **自定义布局**：可视化报纸布局编辑器

## 8. 产品定位与竞品分析

### 8.1 市场定位

**目标用户**：
1. **知识工作者**：需要快速掌握行业动态的专业人士
2. **创业者/投资人**：需要追踪市场趋势和竞品动态
3. **研究人员**：需要跟踪学术前沿
4. **高管/决策者**：需要每日简报但没时间看

**价值主张**：
> "让AI成为你的私人分析师，24小时不间断地为你收集、整理、分析信息。
> 你只需要每天花10分钟，优雅地翻阅AI为你准备的精美报告。"

**定价策略（建议）**：
- **免费版**：1个收获，每周更新
- **专业版**：5个收获，每日更新，$9.9/月
- **团队版**：无限收获+团队协作，$29.9/月/人
- **企业版**：定制收获+白标部署，定制报价

### 8.2 竞品对比

| 产品 | 核心功能 | 优势 | 劣势 | 我们的差异化 |
|------|----------|------|------|--------------|
| **Feedly** | RSS聚合 | 内容源丰富 | 原始信息堆砌，无AI整理 | ✅ AI深度加工<br>✅ 翻页阅读体验 |
| **Notion AI** | 文档+AI | 功能全面 | 需要手动操作，无自动化 | ✅ 全自动挂机式产出 |
| **Perplexity** | AI搜索 | 实时搜索 | 单次查询，无定时产出 | ✅ 定时自动生成<br>✅ 持续积累 |
| **Morning Brew** | 人工编辑简报 | 内容质量高 | 人工编辑成本高，无个性化 | ✅ AI自动化<br>✅ 完全个性化 |
| **Flipboard** | 个性化新闻 | 阅读体验好 | 推荐算法，非定制 | ✅ 用户主动定制<br>✅ AI深度分析 |
| **Artifact** | AI新闻推荐 | 智能推荐 | 已停运 | ✅ 主动收获模式<br>✅ 可持续运营 |

**我们的核心差异化**：
1. **挂机游戏式体验**：设置一次，持续产出（其他产品都需要主动操作）
2. **AI深度加工**：不是简单聚合，而是整理+分析的成品（RSS只是聚合）
3. **精美阅读体验**：翻页式阅读，媲美实体杂志（Flipboard最接近，但内容不够深度）
4. **完全个性化**：用户定制关键词和频率（Morning Brew是通用内容）
5. **持续积累**：收获历史形成知识库（Perplexity是单次查询）

### 8.3 技术护城河

**短期护城河（6-12个月）**：
- 翻页阅读体验的UX打磨
- AI内容整理的Prompt优化
- CronJob + WebSearch的整合经验

**长期护城河（1-2年）**：
- 用户收获历史的数据积累
- 基于用户行为的个性化推荐
- 知识图谱构建（收获之间的关联）
- 团队协作功能（企业市场）

## 9. 附录

### 9.1 参考资料
- CronJob实现：`console/src/pages/Control/CronJobs/`
- Channel通知：`console/src/pages/Control/Channels/`
- Lucide-React图标：https://lucide.dev/icons/
- Framer Motion文档：https://www.framer.com/motion/
- react-pageflip: https://github.com/Nodlik/react-pageflip
- turn.js替代方案: https://github.com/blasten/turn.js

### 8.2 功能命名与Pitch策略

#### 8.2.1 核心Term（向老板Sell）

**推荐名称**：**"AI Daily Harvest"** / **"智能收获站"**

**Pitch话术**：
> "这是一个让AI为你工作、自动收获价值信息的系统。就像挂机游戏一样，你设置好规则后，每天醒来就能'收获'今天的资讯、见解和分析。不是简单的RSS订阅，而是经过AI深度加工、精美排版的知识产品。"

**核心卖点**（向老板展示）：
1. **被动收入式体验**：设置一次，持续产出价值
2. **高质量产出**：不是原始信息堆砌，而是AI整理+分析的成品
3. **视觉享受**：优美的翻页式阅读体验，像在读高端杂志
4. **时间复利**：每天10分钟，累积专业洞察
5. **差异化竞争**：市场上没有类似的AI挂机式资讯产品

#### 8.2.2 候选命名对比

| 名称 | 英文 | 定位 | 适合场景 | 评分 |
|------|------|------|----------|------|
| **智能收获站** | **AI Daily Harvest** | 挂机游戏式产出 | ✅ Demo演示 | ⭐⭐⭐⭐⭐ |
| 知识酿造坊 | Knowledge Brewery | 时间沉淀价值 | 高端定位 | ⭐⭐⭐⭐ |
| 信息熔炉 | Info Forge | 原材料→成品 | 工程师群体 | ⭐⭐⭐ |
| 每日简报 | Daily Brief | 传统简报 | 企业场景 | ⭐⭐⭐ |
| 报刊亭 | Newsstand | 报纸集合 | 偏传统 | ⭐⭐ |

**最终建议**：
- **对外/Demo**: **"AI Daily Harvest"** / **"智能收获站"**
- **技术实现**: `Harvest` / `HarvestItem`
- **UI展示**: 使用"收获"相关的视觉隐喻（如：麦穗、果实、宝箱）

#### 8.2.3 Pitch场景模拟

**场景1：向CEO展示Demo**
```
"我们做了一个叫'智能收获站'的功能。您看这个界面（展示倒计时卡片），
每个卡片就像一个自动赚钱的机器。比如这个'科技前沿收获'，设置成每天
早上9点，AI会自动去搜索最新的AI论文、开源项目、技术趋势，然后整理
成这样一份精美的报告（展示翻页效果）。

您每天只需要花10分钟翻一翻，就能掌握行业动态。这个产品的核心价值是：
**让AI成为你的私人分析师，24小时为你工作**。"
```

**场景2：突出商业价值**
```
"这个功能有三个商业机会：
1. **订阅增长点**：用户为了持续'收获'会保持订阅
2. **差异化竞争**：Notion、飞书都没有这种AI挂机式的产品
3. **企业市场**：可以做成团队版，每天自动生成行业简报"
```

**场景3：技术优势**
```
"我们的优势在于：
1. 已有CronJob基础设施，开发成本低
2. 有WebSearch和LLM能力，内容质量有保障
3. 前端用React做翻页效果，体验可以做到媲美Flipboard"
```

### 8.3 UI样式参考

**色彩方案**：
- 主色调：`#FF7F16`（项目主色）
- 背景色：根据主题自适应（亮色/暗色）
- 卡片阴影：`0 2px 8px rgba(0,0,0,0.08)`
- 边框圆角：`8px`
- 间距单位：`8px` 的倍数（8, 16, 24, 32...）

**字体大小**：
- 标题：`18px` / `font-weight: 600`
- 副标题：`14px` / `font-weight: 500`
- 正文：`14px` / `font-weight: 400`
- 辅助文字：`12px` / `color: rgba(0,0,0,0.45)`

---

**文档版本**：v1.0
**创建日期**：2026-04-24
**作者**：QwenPaw Team
**状态**：待评审

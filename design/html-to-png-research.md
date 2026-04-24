# 📸 HTML转PNG精美图片生成方案调研

## 🎯 需求背景

用户反馈："**日报好丑啊**" - 当前的日报产出需要更精美的视觉呈现。

### 目标
- 生成**真正精美**的日报图片（而不是简单的网页截图）
- 支持多种风格主题（手绘、极简、国风、科技感、Apple风等）
- 1080x1920分辨率（适合手机长图）
- 高清PNG格式，适合分享和推送

---

## ✅ 已完成工作

### 1. 创建了 6 个精美HTML模板

所有模板位于 `/console/public/harvest-templates/`:

| 模板 | 风格 | 适用场景 | 特点 |
|------|------|----------|------|
| `style-1-handdrawn.html` | 🎨 手绘旅行手账 | 轻松科普、趣味资讯 | 温馨童趣、蜡笔质感、路线式布局 |
| `style-2-minimal-magazine.html` | 📰 现代极简杂志 | 深度分析、专业报告 | 大留白、高端质感、Playfair Display |
| `style-3-infographic-card.html` | 🔥 信息图卡片 | 观点输出、励志内容 | 红黑对比、毛笔草书、有力量感 |
| `style-4-tech-dark.html` | 🌌 科技感深色 | 技术报告、AI资讯 | 深色调、未来感、代码块元素 |
| `style-5-chinese-style.html` | 🏮 国风海报 | 中文内容、文化类 | 印章、竖排、诗句引用 |
| `style-6-apple-keynote.html` | 🍎 Apple Keynote | 产品发布、重大宣布 | 超大字体、极简、Apple美学 |

### 2. 创建了预览页面

访问 `http://localhost:5175/harvest-templates/preview.html` 可查看所有模板的画廊式预览。

### 3. 集成到Inbox Demo

在Inbox页面的每个Harvest卡片上添加了 **"Preview"** 按钮，点击可：
- 在Modal中预览完整的HTML模板
- 支持Mobile/Desktop视图切换
- 模拟真实设备边框（Mobile模式）

---

## 🔧 技术实现方案

### 方案对比

| 方案 | 优点 | 缺点 | 推荐度 |
|------|------|------|--------|
| **Playwright** | 功能强大、异步、支持多浏览器、活跃维护 | 需要安装浏览器 | ⭐⭐⭐⭐⭐ |
| Puppeteer | 成熟稳定、文档丰富 | 只支持Chrome | ⭐⭐⭐⭐ |
| wkhtmltoimage | 轻量、CLI工具 | 功能有限、不支持现代CSS | ⭐⭐ |
| html2canvas | 纯前端、无需后端 | 渲染质量差、不支持复杂样式 | ⭐ |

### 推荐方案：Playwright

#### 为什么选择Playwright？
1. ✅ **异步支持** - 适合Python后端
2. ✅ **多浏览器** - Chromium/Firefox/WebKit
3. ✅ **高分辨率** - 支持deviceScaleFactor (2x/3x)
4. ✅ **活跃维护** - Microsoft官方支持
5. ✅ **性能优秀** - 浏览器池复用
6. ✅ **等待机制** - 可等待字体、图片加载完成

---

## 💻 实现代码

### Python后端实现 (推荐)

```python
from playwright.async_api import async_playwright
from pathlib import Path
from typing import Optional
import asyncio

class HtmlToImageService:
    """将HTML渲染为高清PNG图片的服务"""

    def __init__(self):
        self.browser = None
        self.playwright = None

    async def __aenter__(self):
        """异步上下文管理器入口"""
        self.playwright = await async_playwright().start()
        self.browser = await self.playwright.chromium.launch(
            headless=True
        )
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """异步上下文管理器出口"""
        if self.browser:
            await self.browser.close()
        if self.playwright:
            await self.playwright.stop()

    async def render_html_to_png(
        self,
        html_content: str,
        output_path: str,
        width: int = 1080,
        height: int = 1920,
        device_scale_factor: float = 2.0,
        wait_for_fonts: bool = True,
    ) -> str:
        """
        将HTML内容渲染为PNG图片

        Args:
            html_content: HTML字符串内容
            output_path: 输出PNG文件路径
            width: 视口宽度（像素）
            height: 视口高度（像素）
            device_scale_factor: 设备缩放比例（2.0 = 2x Retina）
            wait_for_fonts: 是否等待字体加载

        Returns:
            生成的PNG文件路径
        """
        if not self.browser:
            raise RuntimeError("Browser not initialized. Use context manager.")

        # 创建新页面
        page = await self.browser.new_page(
            viewport={
                "width": width,
                "height": height,
            },
            device_scale_factor=device_scale_factor,
        )

        try:
            # 设置内容
            await page.set_content(html_content, wait_until="networkidle")

            # 等待字体加载
            if wait_for_fonts:
                await page.wait_for_timeout(2000)  # 2秒等待字体

            # 截图
            await page.screenshot(
                path=output_path,
                type="png",
                full_page=False,
            )

            return output_path

        finally:
            await page.close()

    async def render_template_to_png(
        self,
        template_path: str,
        output_path: str,
        context: dict,
        **kwargs
    ) -> str:
        """
        使用Jinja2模板渲染HTML后转PNG

        Args:
            template_path: HTML模板文件路径
            output_path: 输出PNG文件路径
            context: 模板变量字典
            **kwargs: 传递给render_html_to_png的其他参数

        Returns:
            生成的PNG文件路径
        """
        from jinja2 import Template

        # 读取模板
        template_content = Path(template_path).read_text()
        template = Template(template_content)

        # 渲染HTML
        html_content = template.render(**context)

        # 转换为PNG
        return await self.render_html_to_png(
            html_content=html_content,
            output_path=output_path,
            **kwargs
        )


# 使用示例1: 直接渲染HTML
async def example_1():
    async with HtmlToImageService() as service:
        html = Path("console/public/harvest-templates/style-1-handdrawn.html").read_text()
        await service.render_html_to_png(
            html_content=html,
            output_path="output/harvest-001.png",
        )


# 使用示例2: 使用模板渲染
async def example_2():
    async with HtmlToImageService() as service:
        await service.render_template_to_png(
            template_path="templates/tech-report.html",
            output_path="output/tech-report-20260424.png",
            context={
                "title": "今日科技收获",
                "date": "2026年4月24日",
                "articles": [
                    {
                        "emoji": "🚀",
                        "title": "OpenAI发布GPT-5",
                        "content": "推理能力提升3倍...",
                    },
                ],
            }
        )


# 使用示例3: 批量生成（使用浏览器池）
async def example_3():
    templates = [
        ("style-1-handdrawn.html", "output/report-1.png"),
        ("style-2-minimal-magazine.html", "output/report-2.png"),
        ("style-4-tech-dark.html", "output/report-3.png"),
    ]

    async with HtmlToImageService() as service:
        tasks = [
            service.render_html_to_png(
                html_content=Path(f"templates/{tpl}").read_text(),
                output_path=out,
            )
            for tpl, out in templates
        ]
        results = await asyncio.gather(*tasks)
        print(f"Generated {len(results)} images")
```

### 集成到NewspaperGenerator

```python
# src/qwenpaw/services/newspaper_generator.py

from typing import List, Dict
from jinja2 import Environment, FileSystemLoader
from .html_to_image import HtmlToImageService

class NewspaperGenerator:
    """报纸生成器 - 第二阶段Production版本"""

    def __init__(self):
        self.template_env = Environment(
            loader=FileSystemLoader("templates/harvest")
        )
        self.image_service = HtmlToImageService()

    async def generate_harvest(
        self,
        harvest_config: Dict,
        search_results: List[Dict],
    ) -> str:
        """
        生成一份Harvest报纸

        Args:
            harvest_config: Harvest配置（包含theme、template等）
            search_results: Web搜索结果

        Returns:
            生成的PNG图片路径
        """
        # 1. 选择模板
        template_name = harvest_config.get("template", "style-1-handdrawn.html")
        template = self.template_env.get_template(template_name)

        # 2. LLM处理搜索结果
        processed_content = await self._process_with_llm(search_results)

        # 3. 准备模板上下文
        context = {
            "title": harvest_config["name"],
            "date": datetime.now().strftime("%Y年%m月%d日"),
            "articles": processed_content["articles"],
            "theme": harvest_config.get("theme", "apple"),
        }

        # 4. 渲染HTML
        html_content = template.render(**context)

        # 5. 转换为PNG
        output_path = f"output/harvest-{harvest_config['id']}-{int(time.time())}.png"

        async with self.image_service as service:
            await service.render_html_to_png(
                html_content=html_content,
                output_path=output_path,
            )

        return output_path

    async def _process_with_llm(self, search_results: List[Dict]) -> Dict:
        """使用LLM处理和整理搜索结果"""
        # TODO: 调用LLM API
        # 分析、总结、提取关键信息
        pass
```

---

## 📦 依赖安装

### Python环境

```bash
# 安装Playwright
pip install playwright jinja2

# 安装浏览器（首次需要）
playwright install chromium

# 可选：安装其他浏览器
# playwright install firefox webkit
```

### 验证安装

```bash
python -c "from playwright.sync_api import sync_playwright; print('✅ Playwright安装成功')"
```

---

## 🎨 模板系统设计

### 模板结构

```
templates/harvest/
├── style-1-handdrawn.html         # 手绘风格
├── style-2-minimal-magazine.html  # 极简杂志
├── style-3-infographic-card.html  # 信息图卡片
├── style-4-tech-dark.html         # 科技深色
├── style-5-chinese-style.html     # 国风
└── style-6-apple-keynote.html     # Apple风
```

### Jinja2模板变量

所有模板都支持以下变量：

```python
{
    "title": str,          # 标题
    "subtitle": str,       # 副标题
    "date": str,           # 日期
    "articles": [          # 文章列表
        {
            "emoji": str,
            "title": str,
            "content": str,
            "tags": List[str],
            "tldr": str,   # 可选：一句话总结
        }
    ],
    "theme": str,          # 主题ID (apple/deepblue/...)
}
```

### 示例：动态渲染

```python
# 原始HTML中的占位符
"""
<h1>{{ title }}</h1>
<p>{{ date }}</p>

{% for article in articles %}
<div class="article">
  <span>{{ article.emoji }}</span>
  <h2>{{ article.title }}</h2>
  <p>{{ article.content }}</p>
</div>
{% endfor %}
"""

# Python代码渲染
context = {
    "title": "今日科技收获",
    "date": "2026-04-24",
    "articles": [
        {"emoji": "🚀", "title": "GPT-5发布", "content": "..."},
    ]
}
rendered = template.render(**context)
```

---

## ⚡ 性能优化

### 1. 浏览器池复用

```python
# ❌ 不推荐：每次都启动新浏览器
for harvest in harvests:
    async with HtmlToImageService() as service:
        await service.render_html_to_png(...)

# ✅ 推荐：复用同一个浏览器实例
async with HtmlToImageService() as service:
    for harvest in harvests:
        await service.render_html_to_png(...)
```

### 2. 异步并发处理

```python
# 并发生成多个图片
async with HtmlToImageService() as service:
    tasks = [
        service.render_html_to_png(html, f"output/{i}.png")
        for i, html in enumerate(html_contents)
    ]
    results = await asyncio.gather(*tasks, return_exceptions=True)
```

### 3. 缓存策略

```python
# 缓存24小时避免重复生成
CACHE_DIR = "cache/harvest-images"
CACHE_TTL = 86400  # 24小时

def get_cached_image(harvest_id: str) -> Optional[str]:
    cache_file = Path(CACHE_DIR) / f"{harvest_id}.png"
    if cache_file.exists():
        age = time.time() - cache_file.stat().st_mtime
        if age < CACHE_TTL:
            return str(cache_file)
    return None
```

---

## 📊 质量对比

### 分辨率设置

| deviceScaleFactor | 实际分辨率 | 文件大小 | 适用场景 |
|-------------------|-----------|----------|----------|
| 1.0 | 1080x1920 | ~200KB | 快速预览 |
| 2.0 (推荐) | 2160x3840 | ~800KB | 高清分享 |
| 3.0 | 3240x5760 | ~1.8MB | 打印质量 |

### 字体加载等待时间

```python
# 根据字体复杂度调整等待时间
wait_times = {
    "simple": 1000,    # 系统字体
    "normal": 2000,    # Google Fonts
    "complex": 3000,   # 自定义中文字体
}
```

---

## 🚀 第二阶段集成计划

### 数据流

```
1. CronJob触发 Harvest
   ↓
2. WebSearch获取原始数据
   ↓
3. LLM分析和整理
   ↓
4. 选择模板 (根据theme/template配置)
   ↓
5. Jinja2渲染HTML
   ↓
6. Playwright转PNG
   ↓
7. 保存到文件系统
   ↓
8. 创建InboxItem (包含图片URL)
   ↓
9. 可选：推送到Channel
```

### 数据库Schema更新

```python
# harvest_contents表
class HarvestContent(Base):
    __tablename__ = "harvest_contents"

    id = Column(String, primary_key=True)
    harvest_id = Column(String, ForeignKey("harvests.id"))

    # 内容
    title = Column(String)
    subtitle = Column(String)
    sections = Column(JSON)  # 文章列表

    # 生成结果
    html_path = Column(String)      # 原始HTML路径
    image_path = Column(String)     # 生成的PNG路径
    image_url = Column(String)      # 可访问的URL

    # 元数据
    theme = Column(String)          # 应用的主题
    template = Column(String)       # 使用的模板
    generated_at = Column(DateTime)
    file_size = Column(Integer)     # PNG文件大小（字节）
```

---

## ✅ Demo效果展示

### 访问方式

1. **预览画廊页面**
   ```
   http://localhost:5175/harvest-templates/preview.html
   ```
   - 查看所有6个模板的缩略图
   - 点击卡片查看完整效果

2. **在Inbox中预览**
   ```
   http://localhost:5175/inbox
   ```
   - 切换到"AI Harvest"标签
   - 点击任意Harvest卡片的"Preview"按钮
   - 在Modal中查看完整HTML模板
   - 支持Mobile/Desktop视图切换

3. **单独查看某个模板**
   ```
   http://localhost:5175/harvest-templates/style-1-handdrawn.html
   ```

---

## 📝 后续工作

### Demo阶段 (已完成 ✅)
- [x] 创建6个精美HTML模板
- [x] 创建预览画廊页面
- [x] 集成到Inbox Demo
- [x] 添加Preview按钮和Modal

### Production阶段 (待实现)
- [ ] 安装和配置Playwright
- [ ] 实现HtmlToImageService
- [ ] 集成Jinja2模板引擎
- [ ] 实现NewspaperGenerator
- [ ] 添加缓存机制
- [ ] 集成到CronJob
- [ ] 更新数据库Schema
- [ ] 实现文件存储和URL访问
- [ ] 添加分享功能

---

## 🎯 总结

### 核心价值
1. ✅ **真正精美** - 6种专业设计的视觉风格
2. ✅ **高度可定制** - 支持Jinja2动态渲染
3. ✅ **生产就绪** - Playwright提供可靠的渲染能力
4. ✅ **性能优秀** - 浏览器池复用 + 异步并发
5. ✅ **易于扩展** - 添加新模板只需复制和修改CSS

### 技术栈
- **前端**: HTML + CSS + Google Fonts
- **模板引擎**: Jinja2
- **渲染引擎**: Playwright (Chromium)
- **后端**: Python + AsyncIO
- **输出格式**: PNG (1080x1920, 2x Retina)

### 实现难度
- **Demo阶段**: ⭐⭐ (已完成)
- **Production阶段**: ⭐⭐⭐ (中等，主要是Playwright集成)

---

**创建日期**: 2026-04-24
**作者**: QwenPaw Team
**状态**: ✅ Demo完成，Production方案明确

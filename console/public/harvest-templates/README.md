# 🎨 Harvest Templates - 精美日报模板

## 📋 模板列表

### 1. 手绘旅行手账风格 (`style-1-handdrawn.html`)
- **适合内容**: 轻松科普、趣味资讯、生活类
- **视觉特点**: 温馨童趣、蜡笔质感、路线式布局
- **色彩**: 暖色系（米黄、橙色、棕色）
- **字体**: 龙藏体、思源黑体
- **灵感来源**: 旅行手账、儿童插画

### 2. 现代极简杂志风格 (`style-2-minimal-magazine.html`)
- **适合内容**: 深度分析、专业报告、商业内容
- **视觉特点**: 大留白、极简配色、高端质感
- **色彩**: 黑白灰为主
- **字体**: Playfair Display (衬线) + Inter
- **灵感来源**: 高端室内设计、现代杂志

### 3. 信息图卡片风格 (`style-3-infographic-card.html`)
- **适合内容**: 观点输出、励志内容、核心要点
- **视觉特点**: 红黑对比强烈、毛笔草书、有力量感
- **色彩**: 红色（#D32F2F）+ 黑色
- **字体**: 马善政、龙藏体（草书风格）
- **灵感来源**: 你提供的"做IP长期复利"图片

### 4. 科技感深色风格 (`style-4-tech-dark.html`)
- **适合内容**: 技术报告、AI资讯、数据分析
- **视觉特点**: 深色调、未来感、克制配色
- **色彩**: 深蓝（#0A0E27）+ 青色（#00D9FF）
- **字体**: Space Grotesk + JetBrains Mono
- **灵感来源**: 科技公司产品页、开发者工具

### 5. 国风海报风格 (`style-5-chinese-style.html`)
- **适合内容**: 中文内容、文化类、深度思考
- **视觉特点**: 印章、竖排文字、诗句引用
- **色彩**: 米黄底（#F5E6D3）+ 中国红（#B71C1C）
- **字体**: 思源宋体、站酷小薇
- **灵感来源**: 中国传统书籍、文化海报

### 6. Apple Keynote风格 (`style-6-apple-keynote.html`)
- **适合内容**: 产品发布、重大宣布、品牌内容
- **视觉特点**: 超大字体、极简配色、苹果美学
- **色彩**: 白底 + Apple蓝（#007AFF）
- **字体**: SF Pro Display
- **灵感来源**: Apple发布会、产品页面

---

## 🖼️ 预览方式

### 方法1: 浏览器直接预览
```bash
# 启动开发服务器后访问
http://localhost:5173/harvest-templates/preview.html
```

### 方法2: 单独预览某个模板
```bash
http://localhost:5173/harvest-templates/style-1-handdrawn.html
http://localhost:5173/harvest-templates/style-2-minimal-magazine.html
http://localhost:5173/harvest-templates/style-3-infographic-card.html
http://localhost:5173/harvest-templates/style-4-tech-dark.html
http://localhost:5173/harvest-templates/style-5-chinese-style.html
http://localhost:5173/harvest-templates/style-6-apple-keynote.html
```

---

## 🔧 HTML转PNG实现方案

### 方案1: Playwright (推荐)

```python
from playwright.async_api import async_playwright
from pathlib import Path

async def html_to_png(html_file: str, output_png: str):
    async with async_playwright() as p:
        browser = await p.chromium.launch()
        page = await browser.new_page(
            viewport={"width": 1080, "height": 1920},
            device_scale_factor=2  # 2x分辨率
        )

        # 加载HTML
        html_content = Path(html_file).read_text()
        await page.set_content(html_content)

        # 等待字体加载
        await page.wait_for_timeout(2000)

        # 截图
        await page.screenshot(
            path=output_png,
            full_page=False,
            type="png"
        )

        await browser.close()

# 使用示例
await html_to_png(
    "console/public/harvest-templates/style-1-handdrawn.html",
    "output/harvest-20260424-001.png"
)
```

### 方案2: Puppeteer (Node.js)

```javascript
const puppeteer = require('puppeteer');

async function htmlToPng(htmlFile, outputPng) {
  const browser = await puppeteer.launch();
  const page = await browser.newPage();

  await page.setViewport({
    width: 1080,
    height: 1920,
    deviceScaleFactor: 2
  });

  await page.goto(`file://${htmlFile}`, {
    waitUntil: 'networkidle0'
  });

  await page.screenshot({
    path: outputPng,
    type: 'png',
    fullPage: false
  });

  await browser.close();
}
```

### 方案3: wkhtmltoimage (更轻量)

```bash
wkhtmltoimage \
  --width 1080 \
  --height 1920 \
  --quality 100 \
  style-1-handdrawn.html \
  output.png
```

---

## 🎯 推荐配置

### 图片质量
- **分辨率**: 1080x1920 (竖版长图)
- **DPI**: 2x (设置 deviceScaleFactor: 2)
- **格式**: PNG (保留透明度和清晰度)
- **压缩**: 可选使用 tinypng 或 imagemin

### 字体加载
所有模板都使用Google Fonts CDN，确保：
1. 网络连接正常
2. 截图前等待2-3秒让字体加载完成
3. 或者下载字体文件到本地

### 性能优化
- 使用浏览器pool避免重复启动
- 缓存已生成的PNG（24小时）
- 异步队列处理（避免并发过高）

---

## 🎨 样式扩展建议

### 如何添加新主题
1. 复制一个现有模板
2. 修改CSS变量（颜色、字体）
3. 调整布局和装饰元素
4. 在 `preview.html` 中添加预览卡片

### 动态内容替换
模板中的占位符可以用模板引擎替换：

```python
from jinja2 import Template

template_html = Path("style-1-handdrawn.html").read_text()
template = Template(template_html)

rendered = template.render(
    title="今日科技收获",
    date="2026年4月24日",
    articles=[
        {"emoji": "🚀", "title": "...", "content": "..."},
        {"emoji": "🔓", "title": "...", "content": "..."},
    ]
)

# 然后将 rendered HTML 转为 PNG
```

---

## 🚀 集成到第二阶段

在生产版本中的实现流程：

1. **内容生成** (NewspaperGenerator)
   - Web搜索获取原始数据
   - LLM整理和分析
   - 选择合适的模板风格

2. **HTML渲染** (TemplateRenderer)
   - 使用Jinja2填充模板
   - 根据用户选择的主题加载对应HTML
   - 动态替换内容

3. **PNG生成** (ScreenshotService)
   - Playwright渲染HTML
   - 生成高清PNG
   - 保存到文件系统

4. **分发** (NotificationService)
   - 创建InboxItem
   - 可选：推送到Channel
   - 用户在Inbox中查看

---

## 📦 依赖项

### Python后端
```bash
pip install playwright jinja2
playwright install chromium
```

### Node.js (可选)
```bash
npm install puppeteer
```

---

## ✨ 效果预览

所有模板都已经可以直接预览！启动开发服务器后访问：

```
http://localhost:5173/harvest-templates/preview.html
```

点击任意卡片可以查看完整效果。

---

**创建日期**: 2026-04-24
**作者**: QwenPaw Team
**状态**: ✅ 完成 - 可直接用于Demo演示

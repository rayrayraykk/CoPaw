import type { HarvestContent } from "../types";

export const MOCK_HARVEST_CONTENTS: Record<string, HarvestContent> = {
  "harvest-1": {
    id: "content-1",
    harvestId: "harvest-1",
    generatedAt: new Date(Date.now() - 2 * 60 * 60 * 1000),
    title: "Tech Frontier Daily",
    subtitle: "AI, Innovation & Open Source • April 24, 2026",
    theme: "apple",
    sections: [
      {
        id: "section-1",
        type: "hero",
        title: "Today's Highlights",
        content: `
          <p>Welcome to today's Tech Frontier Daily! Here are the top stories shaping the future of technology:</p>
          <ul>
            <li><strong>OpenAI Unveils GPT-5 Preview:</strong> Groundbreaking reasoning capabilities</li>
            <li><strong>Meta Open Sources AI Agent Framework:</strong> Democratizing AI development</li>
            <li><strong>Google Achieves Quantum Supremacy Milestone:</strong> 1000+ qubit processor</li>
            <li><strong>GitHub Copilot Workspace Launches:</strong> AI-native development environment</li>
          </ul>
        `,
      },
      {
        id: "section-2",
        type: "article",
        title: "🚀 OpenAI Releases GPT-5 Preview",
        tldr: "OpenAI's GPT-5 shows 3x improvement in reasoning tasks, reduced hallucinations by 70%, and costs 50% less to run than GPT-4.",
        content: `
          <p>On April 23rd, OpenAI made waves in the AI community with the surprise announcement of GPT-5's preview release. The new model represents a significant leap forward in large language model capabilities.</p>

          <h4>Key Improvements</h4>
          <ul>
            <li><strong>Reasoning:</strong> 3x better performance on complex problem-solving tasks</li>
            <li><strong>Accuracy:</strong> 70% reduction in hallucinations compared to GPT-4</li>
            <li><strong>Efficiency:</strong> 50% lower inference costs</li>
            <li><strong>Multimodal:</strong> Native support for images, audio, and video</li>
          </ul>

          <p>According to Sam Altman, CEO of OpenAI, "GPT-5 is not just an incremental improvement—it's a fundamental shift in how AI models think and reason. We're seeing emergent capabilities we didn't explicitly train for."</p>

          <h4>Industry Reaction</h4>
          <p>The announcement sent shockwaves through the tech industry. Microsoft, OpenAI's largest investor, announced plans to integrate GPT-5 into all Azure services by Q3 2026. Meanwhile, competitors like Google and Anthropic are reportedly accelerating their own model development timelines.</p>

          <h4>What This Means for Developers</h4>
          <p>For developers, GPT-5 opens up new possibilities in areas like:</p>
          <ul>
            <li>Complex code generation and debugging</li>
            <li>Advanced reasoning in specialized domains (legal, medical, financial)</li>
            <li>More reliable autonomous agent systems</li>
            <li>Better multimodal applications</li>
          </ul>

          <p>The preview API is currently available to select partners, with general availability expected in June 2026.</p>
        `,
        sources: [
          {
            title: "OpenAI Official Blog: Introducing GPT-5",
            url: "https://openai.com/blog/gpt-5",
            publishedAt: new Date("2026-04-23"),
          },
          {
            title: "TechCrunch: OpenAI's GPT-5 Stuns Industry",
            url: "https://techcrunch.com/2026/04/23/openai-gpt-5",
            publishedAt: new Date("2026-04-23"),
          },
        ],
      },
      {
        id: "section-3",
        type: "quote",
        content:
          '"This is the moment when AI truly becomes a thought partner, not just a tool. GPT-5 represents the bridge between narrow AI and artificial general intelligence."',
      },
      {
        id: "section-4",
        type: "article",
        title: "🔓 Meta Open Sources Advanced AI Agent Framework",
        tldr: "Meta releases LlamaAgent 2.0, a production-ready framework for building autonomous AI agents with built-in safety features.",
        content: `
          <p>Meta AI has released LlamaAgent 2.0, an open-source framework designed to make building sophisticated AI agents accessible to developers worldwide. The framework builds on Meta's Llama 3 foundation models.</p>

          <h4>Key Features</h4>
          <ul>
            <li><strong>Modular Architecture:</strong> Plug-and-play components for perception, reasoning, and action</li>
            <li><strong>Built-in Safety:</strong> Constitutional AI guardrails and human oversight loops</li>
            <li><strong>Multi-Agent Collaboration:</strong> Agents can work together on complex tasks</li>
            <li><strong>Cloud & Edge:</strong> Deployable on servers or lightweight edge devices</li>
          </ul>

          <p>Mark Zuckerberg commented: "We believe the future of AI is open. By open-sourcing LlamaAgent, we're enabling the global developer community to build the next generation of AI applications."</p>

          <h4>Early Adopter Success Stories</h4>
          <p>Several companies have already integrated LlamaAgent into their products:</p>
          <ul>
            <li><strong>Shopify:</strong> Automated customer service agents handling 85% of inquiries</li>
            <li><strong>Figma:</strong> Design co-pilot agents that suggest improvements</li>
            <li><strong>Notion:</strong> Knowledge management agents that organize and synthesize information</li>
          </ul>

          <p>The framework is available now on GitHub under the MIT license.</p>
        `,
        sources: [
          {
            title: "Meta AI Blog: LlamaAgent 2.0",
            url: "https://ai.meta.com/blog/llamaagent-2",
            publishedAt: new Date("2026-04-23"),
          },
        ],
      },
      {
        id: "section-5",
        type: "list",
        title: "📊 Quick Stats & Numbers",
        content: `
          <ul>
            <li><strong>$2.1B:</strong> Total VC funding in AI startups this week</li>
            <li><strong>47%:</strong> YoY growth in GitHub AI-related repositories</li>
            <li><strong>230K:</strong> New AI developers joined the community this month</li>
            <li><strong>15:</strong> Major AI models released in Q1 2026</li>
          </ul>
        `,
      },
      {
        id: "section-6",
        type: "article",
        title: "🎯 What to Watch Next Week",
        content: `
          <h4>Upcoming Events & Releases</h4>
          <ul>
            <li><strong>April 28:</strong> Google I/O keynote featuring Gemini 2.0</li>
            <li><strong>April 29:</strong> Anthropic research paper on "Constitutional AI 2.0"</li>
            <li><strong>April 30:</strong> GitHub Universe conference begins</li>
            <li><strong>May 1:</strong> Apple WWDC preview event (AI features expected)</li>
          </ul>

          <h4>Trending Topics to Follow</h4>
          <ul>
            <li>AI-powered code editors (Cursor, Continue, etc.)</li>
            <li>Small language models (< 3B parameters)</li>
            <li>AI agent orchestration platforms</li>
            <li>Regulatory developments (EU AI Act implementation)</li>
          </ul>
        `,
      },
    ],
    metadata: {
      estimatedReadTime: 8,
      articleCount: 5,
      keywords: ["AI", "OpenAI", "GPT-5", "Meta", "LlamaAgent", "OpenSource"],
    },
  },
  "harvest-2": {
    id: "content-2",
    harvestId: "harvest-2",
    generatedAt: new Date(Date.now() - 3 * 24 * 60 * 60 * 1000),
    title: "Industry Intelligence Weekly",
    subtitle: "SaaS, B2B & Enterprise Software • Week 16, 2026",
    theme: "deepblue",
    sections: [
      {
        id: "section-1",
        type: "hero",
        title: "This Week's Market Movements",
        content: `
          <p>Welcome to this week's deep dive into the enterprise software landscape. Major developments across multiple sectors:</p>
          <ul>
            <li><strong>SaaS Valuations Recovery:</strong> 23% increase in average ARR multiples</li>
            <li><strong>AI-Native Tools Surge:</strong> 67% adoption rate among Fortune 500</li>
            <li><strong>Vertical SaaS Boom:</strong> $4.2B invested in industry-specific solutions</li>
          </ul>
        `,
      },
      {
        id: "section-2",
        type: "article",
        title: "📈 SaaS Market Recovery Gains Momentum",
        tldr: "Public SaaS companies see 23% increase in valuation multiples as profitability becomes the new growth metric.",
        content: `
          <p>After two years of correction, the SaaS market is showing strong signs of recovery. Public SaaS companies trading at an average of 8.4x forward revenue, up from 6.8x in Q4 2025.</p>

          <h4>Key Drivers</h4>
          <ul>
            <li><strong>Profitability Focus:</strong> Companies achieving Rule of 40+ getting premium valuations</li>
            <li><strong>AI Integration:</strong> Products with embedded AI seeing 40% higher multiples</li>
            <li><strong>Efficient Growth:</strong> CAC payback periods under 12 months rewarded by market</li>
          </ul>

          <h4>Winners & Losers</h4>
          <p>Top performers this quarter:</p>
          <ul>
            <li><strong>Databricks:</strong> $55B valuation (up 35% from last round)</li>
            <li><strong>ServiceNow:</strong> AI Platform driving 28% YoY growth</li>
            <li><strong>Snowflake:</strong> Data clean rooms attracting new enterprise clients</li>
          </ul>
        `,
        sources: [
          {
            title: "Bessemer Cloud Index Report Q1 2026",
            url: "https://bessemer.com/cloud-index",
          },
        ],
      },
      {
        id: "section-3",
        type: "quote",
        content:
          '"The era of growth-at-all-costs is over. Investors now demand profitable, sustainable growth. AI is the tool that makes both possible."',
      },
    ],
    metadata: {
      estimatedReadTime: 15,
      articleCount: 8,
      keywords: ["SaaS", "B2B", "Enterprise", "Valuation", "AI"],
    },
  },
  "harvest-3": {
    id: "content-3",
    harvestId: "harvest-3",
    generatedAt: new Date(Date.now() - 10 * 60 * 60 * 1000),
    title: "Competitor Watch Daily",
    subtitle: "Market Intelligence & Competitive Analysis • April 24, 2026",
    theme: "minimal",
    sections: [
      {
        id: "section-1",
        type: "hero",
        title: "Today's Competitive Landscape",
        content: `
          <p>Critical updates from your competitive landscape:</p>
          <ul>
            <li><strong>Competitor A:</strong> Launched new AI features</li>
            <li><strong>Competitor B:</strong> Raised $50M Series B</li>
            <li><strong>Competitor C:</strong> Expanded to EMEA region</li>
          </ul>
        `,
      },
      {
        id: "section-2",
        type: "article",
        title: "🚨 Competitor A Launches AI Assistant",
        tldr: "Major product update could impact market positioning. Early user feedback is mixed.",
        content: `
          <p>Competitor A released their AI-powered assistant feature today, positioning it as a "game-changer" for workflow automation.</p>

          <h4>What They Launched</h4>
          <ul>
            <li>Natural language task automation</li>
            <li>Integration with 50+ tools</li>
            <li>Free tier with 100 AI operations/month</li>
          </ul>

          <h4>Market Reaction</h4>
          <p>Early adopters report 40% time savings, but accuracy concerns remain. Pricing appears aggressive—potentially loss-leading strategy.</p>

          <h4>Recommended Actions</h4>
          <ul>
            <li>Monitor user sentiment on social media</li>
            <li>Evaluate our AI roadmap timeline</li>
            <li>Prepare competitive positioning docs</li>
          </ul>
        `,
      },
    ],
    metadata: {
      estimatedReadTime: 5,
      articleCount: 3,
      keywords: ["Competitor", "Product Launch", "AI", "Market"],
    },
  },
};

export const useMockHarvestContent = (harvestId: string) => {
  return MOCK_HARVEST_CONTENTS[harvestId] || null;
};

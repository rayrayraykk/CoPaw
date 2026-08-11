import ForceGraph3D from "3d-force-graph";
import {
  ArrowLeft,
  CircleHelp,
  Move3d,
  Orbit,
  Scan,
  Search,
  createIcons,
} from "lucide";

const concept = document.documentElement.dataset.concept || "nebula";

const concepts = {
  nebula: {
    background: "#070806",
    palette: ["#d7b675", "#8ea58d", "#b47d62", "#8a819d", "#d9d5c5", "#718d9b"],
    categories: ["Concept", "Document", "Entity", "Code", "Decision", "Citation"],
    camera: { x: 0, y: 0, z: 760 },
  },
  galaxy: {
    background: "#05090b",
    palette: ["#7fc5bd", "#6c9eaa", "#b8c9c5", "#61737f", "#c5a97d", "#668d78"],
    categories: ["Topic", "Page", "Person", "API", "Example", "Reference"],
    camera: { x: 0, y: 90, z: 720 },
  },
  inference: {
    background: "#0b0807",
    palette: ["#e39462", "#b77d66", "#e0c4a9", "#8e7370", "#c19a6b", "#aeb1a3"],
    categories: ["Source", "Evidence", "Claim", "Reasoning", "Answer", "Reference"],
    camera: { x: 150, y: 80, z: 780 },
  },
  temporal: {
    background: "#090a08",
    palette: ["#b8bd83", "#879b80", "#c1b891", "#778b8d", "#9f7f68", "#d0cec0"],
    categories: ["2021", "2022", "2023", "2024", "2025", "2026"],
    camera: { x: 80, y: 80, z: 790 },
  },
};

const config = concepts[concept];
let seed = 90421;
const random = () => {
  seed = (seed * 48271) % 2147483647;
  return seed / 2147483647;
};

const knowledgeNames = [
  "Agent Runtime", "Context Window", "Graph Retrieval", "Tool Protocol", "Memory Layer",
  "Prompt Compiler", "Semantic Index", "Model Router", "Safety Boundary", "Evaluation Loop",
  "Knowledge Provenance", "Vector Search", "Graph RAG", "Plugin Contract", "Execution Trace",
  "Workspace Files", "Human Feedback", "Long-term Memory", "Code Intelligence", "Agent Identity",
  "Streaming Output", "Tool Discovery", "Query Planning", "Document Parser", "Evidence Ranking",
  "Wiki Revision", "Source Citation", "Multi-agent Control", "Model Context", "Answer Synthesis",
];

function titleFor(index, category) {
  const base = knowledgeNames[index % knowledgeNames.length];
  return index < knowledgeNames.length ? base : `${base} · ${category} ${Math.floor(index / knowledgeNames.length) + 1}`;
}

function nebulaPosition(index, group, count) {
  const centers = [
    [-220, 80, -10], [170, -120, 70], [80, 180, -80], [-120, -170, 100], [245, 120, -20], [-20, 10, 40],
  ];
  const center = centers[group];
  const angle = index * 2.399 + group;
  const radius = 28 + Math.sqrt(index % count) * 19 + random() * 55;
  return {
    x: center[0] + Math.cos(angle) * radius,
    y: center[1] + Math.sin(angle) * radius,
    z: center[2] + (random() - 0.5) * 170,
  };
}

function galaxyPosition(index, total, group) {
  if (index < 6) {
    const angle = (Math.PI * 2 * index) / 6;
    return { x: Math.cos(angle) * 68, y: Math.sin(angle) * 68, z: (index - 3) * 7 };
  }
  const arm = group % 3;
  const progress = (index - 6) / (total - 6);
  const angle = progress * Math.PI * 7 + arm * ((Math.PI * 2) / 3);
  const radius = 75 + progress * 330;
  return {
    x: Math.cos(angle) * radius + (random() - 0.5) * 42,
    y: (random() - 0.5) * 58,
    z: Math.sin(angle) * radius + (random() - 0.5) * 42,
  };
}

function inferencePosition(index, group) {
  const lane = group;
  const row = Math.floor(index / 6);
  return {
    x: (lane - 2.5) * 145,
    y: ((row % 9) - 4) * 62 + (random() - 0.5) * 22,
    z: (Math.floor(row / 9) - 2) * 82 + (random() - 0.5) * 30,
  };
}

function temporalPosition(index, total, group) {
  const progress = index / total;
  const angle = progress * Math.PI * 13;
  const radius = 145 + group * 18 + Math.sin(index * 0.7) * 32;
  return {
    x: (progress - 0.5) * 720,
    y: Math.cos(angle) * radius,
    z: Math.sin(angle) * radius,
  };
}

function makeData() {
  const count = concept === "inference" ? 150 : 210;
  const nodes = Array.from({ length: count }, (_, index) => {
    const group = index % 6;
    let position;
    if (concept === "galaxy") position = galaxyPosition(index, count, group);
    else if (concept === "inference") position = inferencePosition(index, group);
    else if (concept === "temporal") position = temporalPosition(index, count, group);
    else position = nebulaPosition(index, group, count / 6);

    return {
      id: `K-${String(index + 1).padStart(3, "0")}`,
      name: titleFor(index, config.categories[group]),
      group,
      category: config.categories[group],
      value: index < 6 ? 9 : 1.5 + random() * 4.5,
      references: 2 + Math.floor(random() * 26),
      confidence: 74 + Math.floor(random() * 25),
      summary: "由 Wiki 文档、代码上下文与引用证据共同形成的知识节点，可继续展开其关联来源和推理路径。",
      fx: position.x,
      fy: position.y,
      fz: position.z,
    };
  });

  const links = [];
  nodes.forEach((node, index) => {
    if (index < 6) return;
    const localHub = node.group;
    links.push({ source: node.id, target: nodes[localHub].id, strength: 2 });

    if (index > 12 && random() > 0.2) {
      const offset = 1 + Math.floor(random() * Math.min(index, 20));
      const target = nodes[Math.max(0, index - offset)];
      links.push({ source: node.id, target: target.id, strength: 0.7 });
    }
    if (random() > 0.78) {
      const target = nodes[Math.floor(random() * index)];
      links.push({ source: node.id, target: target.id, strength: 0.35 });
    }
  });

  for (let group = 0; group < 6; group += 1) {
    links.push({ source: nodes[group].id, target: nodes[(group + 1) % 6].id, strength: 3 });
  }
  return { nodes, links };
}

const data = makeData();
const graphElement = document.querySelector("#graph");
const panel = document.querySelector("[data-node-panel]");
const searchInput = document.querySelector("[data-search]");
const filters = document.querySelector("[data-filters]");
let activeGroup = null;
let selectedNode = null;
let rotating = true;

const graph = ForceGraph3D()(graphElement)
  .graphData(data)
  .backgroundColor(config.background)
  .showNavInfo(false)
  .nodeLabel((node) => `<div style="padding:8px 10px;background:rgba(5,6,5,.9);border:1px solid rgba(255,255,255,.16);font:10px Inter;color:#f2f0e8"><b>${node.name}</b><br><span style="opacity:.55">${node.category} · ${node.references} refs</span></div>`)
  .nodeColor((node) => config.palette[node.group])
  .nodeVal((node) => node.value)
  .nodeOpacity(0.92)
  .nodeResolution(10)
  .linkColor((link) => {
    const source = typeof link.source === "object" ? link.source : data.nodes.find((node) => node.id === link.source);
    return source ? `${config.palette[source.group]}55` : "#ffffff24";
  })
  .linkWidth((link) => link.strength * 0.28)
  .linkOpacity(0.35)
  .linkDirectionalParticles(concept === "inference" ? 2 : 0)
  .linkDirectionalParticleWidth(1.2)
  .linkDirectionalParticleSpeed(0.004)
  .linkDirectionalParticleColor(() => config.palette[0])
  .enableNodeDrag(false)
  .onNodeHover((node) => {
    graphElement.style.cursor = node ? "pointer" : "grab";
  })
  .onNodeClick((node) => focusNode(node));

graph.renderer().setPixelRatio(Math.min(window.devicePixelRatio, 2));
graph.controls().autoRotate = true;
graph.controls().autoRotateSpeed = concept === "galaxy" ? 0.38 : 0.24;
graph.controls().enableDamping = true;
graph.controls().dampingFactor = 0.08;
graph.cameraPosition(config.camera, { x: 0, y: 0, z: 0 }, 0);

function nodeCoordinates(node) {
  return {
    x: Number(node.x ?? node.fx ?? 0),
    y: Number(node.y ?? node.fy ?? 0),
    z: Number(node.z ?? node.fz ?? 0),
  };
}

function focusNode(node) {
  selectedNode = node;
  const position = nodeCoordinates(node);
  const distance = 115;
  const length = Math.hypot(position.x, position.y, position.z) || 1;
  graph.cameraPosition(
    {
      x: position.x + (position.x / length) * distance,
      y: position.y + (position.y / length) * distance,
      z: position.z + (position.z / length) * distance,
    },
    position,
    1100,
  );
  showPanel(node);
  refreshStyles();
}

function showPanel(node) {
  panel.innerHTML = `
    <header><span class="node-kind">${node.category}</span><span class="node-id">${node.id}</span></header>
    <h2>${node.name}</h2>
    <p>${node.summary}</p>
    <div class="node-meta">
      <div><small>References</small><strong>${node.references}</strong></div>
      <div><small>Confidence</small><strong>${node.confidence}%</strong></div>
      <div><small>Relations</small><strong>${connectedCount(node.id)}</strong></div>
      <div><small>Updated</small><strong>2h ago</strong></div>
    </div>`;
  panel.classList.add("is-visible");
}

function connectedCount(id) {
  return data.links.filter((link) => {
    const source = typeof link.source === "object" ? link.source.id : link.source;
    const target = typeof link.target === "object" ? link.target.id : link.target;
    return source === id || target === id;
  }).length;
}

function refreshStyles() {
  const query = searchInput.value.trim().toLowerCase();
  graph
    .nodeVisibility((node) => activeGroup === null || node.group === activeGroup || node.value >= 8)
    .nodeColor((node) => {
      if (selectedNode && node.id === selectedNode.id) return "#ffffff";
      if (query && node.name.toLowerCase().includes(query)) return "#ffffff";
      return config.palette[node.group];
    })
    .linkVisibility((link) => {
      if (activeGroup === null) return true;
      const source = typeof link.source === "object" ? link.source : data.nodes.find((node) => node.id === link.source);
      const target = typeof link.target === "object" ? link.target : data.nodes.find((node) => node.id === link.target);
      return source?.group === activeGroup || target?.group === activeGroup;
    });
}

config.categories.forEach((category, group) => {
  const button = document.createElement("button");
  button.className = "filter-pill";
  button.innerHTML = `<span class="filter-dot" style="--dot:${config.palette[group]}"></span>${category}`;
  button.addEventListener("click", () => {
    activeGroup = activeGroup === group ? null : group;
    filters.querySelectorAll("button").forEach((item, itemIndex) => {
      item.classList.toggle("is-active", itemIndex === activeGroup);
    });
    refreshStyles();
  });
  filters.append(button);
});

searchInput.addEventListener("input", () => {
  const query = searchInput.value.trim().toLowerCase();
  refreshStyles();
  if (!query) return;
  const match = data.nodes.find((node) => node.name.toLowerCase().includes(query));
  if (match) focusNode(match);
});

document.querySelector('[data-action="reset"]').addEventListener("click", () => {
  selectedNode = null;
  activeGroup = null;
  searchInput.value = "";
  panel.classList.remove("is-visible");
  filters.querySelectorAll("button").forEach((item) => item.classList.remove("is-active"));
  graph.cameraPosition(config.camera, { x: 0, y: 0, z: 0 }, 900);
  refreshStyles();
});

document.querySelector('[data-action="rotate"]').addEventListener("click", (event) => {
  rotating = !rotating;
  graph.controls().autoRotate = rotating;
  event.currentTarget.classList.toggle("is-active", rotating);
  event.currentTarget.setAttribute("aria-pressed", String(rotating));
});

document.querySelector('[data-action="help"]').addEventListener("click", () => {
  document.querySelector(".help-toast")?.remove();
  const toast = document.createElement("div");
  toast.className = "help-toast";
  toast.textContent = "拖拽画布旋转视角，滚轮或双指缩放，点击节点飞行聚焦；搜索与类型筛选可以快速收敛到目标知识。";
  document.body.append(toast);
  window.setTimeout(() => toast.remove(), 5200);
});

window.addEventListener("resize", () => {
  graph.width(window.innerWidth).height(window.innerHeight);
});

document.addEventListener("keydown", (event) => {
  if (event.key === "/" && document.activeElement !== searchInput) {
    event.preventDefault();
    searchInput.focus();
  }
  if (event.key === "Escape") {
    panel.classList.remove("is-visible");
    selectedNode = null;
    refreshStyles();
  }
});

createIcons({ icons: { ArrowLeft, CircleHelp, Move3d, Orbit, Scan, Search } });

import ForceGraph3D from "3d-force-graph";
import {
  ArrowLeft,
  CircleHelp,
  Move3d,
  Orbit,
  Scan,
  Search,
  SlidersHorizontal,
  createIcons,
} from "lucide";

const domains = [
  { id: "domain-cognition", name: "Cognition Systems", color: "#9fd4cc" },
  { id: "domain-knowledge", name: "Knowledge Fabric", color: "#80a9c2" },
  { id: "domain-interaction", name: "Interaction Layer", color: "#d6b77d" },
  { id: "domain-trust", name: "Trust & Operations", color: "#c9948d" },
];

const galaxies = [
  { name: "Foundation Models", short: "Models", domain: 0, color: "#9fd4cc", topics: ["Context Window", "Tokenizer", "Attention", "Inference", "Fine Tuning", "Embedding"] },
  { name: "Agent Systems", short: "Agents", domain: 0, color: "#d6b77d", topics: ["Agent Runtime", "Planning", "Reflection", "Delegation", "Identity", "Orchestration"] },
  { name: "Model Routing", short: "Routing", domain: 0, color: "#8e9fc9", topics: ["Model Router", "Fallback", "Cost Policy", "Capability Map", "Load Balance", "Provider"] },
  { name: "Knowledge Retrieval", short: "Retrieval", domain: 1, color: "#80a9c2", topics: ["Graph RAG", "Vector Search", "Hybrid Search", "Reranking", "Query Rewrite", "Citation"] },
  { name: "Memory Architecture", short: "Memory", domain: 1, color: "#bd9fc8", topics: ["Working Memory", "Long-term Memory", "Episodic Store", "Recall", "Compression", "Context Cache"] },
  { name: "Knowledge Graph", short: "Graph", domain: 1, color: "#d2c89c", topics: ["Entity", "Ontology", "Relation", "Provenance", "Community", "Graph Query"] },
  { name: "Tool Ecosystem", short: "Tools", domain: 2, color: "#d28f72", topics: ["Tool Protocol", "Function Call", "Plugin Contract", "Browser Tool", "Code Runner", "Tool Discovery"] },
  { name: "Multimodal Space", short: "Multimodal", domain: 2, color: "#88b7a1", topics: ["Vision Encoder", "Audio Stream", "Document Vision", "Video Context", "OCR", "Spatial Input"] },
  { name: "Wiki Intelligence", short: "Wiki", domain: 2, color: "#cf9e7e", topics: ["Wiki Revision", "Document Parser", "Source Sync", "Topic Map", "Answer Synthesis", "Change Signal"] },
  { name: "Evaluation Lab", short: "Evaluation", domain: 3, color: "#b5c57c", topics: ["Benchmark", "Judge Model", "Regression", "Groundedness", "Latency", "Human Feedback"] },
  { name: "Safety Boundary", short: "Safety", domain: 3, color: "#ca868c", topics: ["Policy Engine", "Guardrail", "Risk Signal", "Audit Trail", "Permission", "Red Team"] },
  { name: "Runtime Infrastructure", short: "Runtime", domain: 3, color: "#a5aaa2", topics: ["Execution Trace", "Workspace", "Streaming", "Observability", "Queue", "Sandbox"] },
];

const suffixes = [
  "Core", "Pipeline", "Index", "Resolver", "Gateway", "Schema",
  "Registry", "Monitor", "Adapter", "Planner", "Worker", "Archive",
];

const topicPalettes = [
  ["#a9ddd3", "#76bdb5", "#aebf82", "#80a9c5", "#d0ae78", "#a793c0"],
  ["#8ab9d0", "#70c1bc", "#a696c7", "#d0b584", "#7f9eae", "#91b397"],
  ["#d5ae73", "#cf8f77", "#91b39b", "#75aaa8", "#aa94bf", "#d3c7a5"],
  ["#ce9298", "#d0a570", "#9eaf82", "#829eae", "#a997bc", "#b8bbb4"],
];

let seed = 73421;
const random = () => {
  seed = (seed * 48271) % 2147483647;
  return seed / 2147483647;
};

function normalize(vector) {
  const length = Math.hypot(vector.x, vector.y, vector.z) || 1;
  return { x: vector.x / length, y: vector.y / length, z: vector.z / length };
}

function cross(a, b) {
  return {
    x: a.y * b.z - a.z * b.y,
    y: a.z * b.x - a.x * b.z,
    z: a.x * b.y - a.y * b.x,
  };
}

function add(a, b) {
  return { x: a.x + b.x, y: a.y + b.y, z: a.z + b.z };
}

function scale(vector, amount) {
  return { x: vector.x * amount, y: vector.y * amount, z: vector.z * amount };
}

function shadeColor(hex, factor) {
  const value = Number.parseInt(hex.slice(1), 16);
  const red = Math.round(((value >> 16) & 255) * factor);
  const green = Math.round(((value >> 8) & 255) * factor);
  const blue = Math.round((value & 255) * factor);
  return `#${[red, green, blue].map((channel) => channel.toString(16).padStart(2, "0")).join("")}`;
}

function basisFor(direction) {
  const normal = normalize(direction);
  const reference = Math.abs(normal.y) > 0.82
    ? { x: 1, y: 0, z: 0 }
    : { x: 0, y: 1, z: 0 };
  const axisA = normalize(cross(normal, reference));
  const axisB = normalize(cross(normal, axisA));
  return { normal, axisA, axisB };
}

const domainDirections = [
  normalize({ x: 1, y: 1, z: 1 }),
  normalize({ x: -1, y: -1, z: 1 }),
  normalize({ x: -1, y: 1, z: -1 }),
  normalize({ x: 1, y: -1, z: -1 }),
];

function domainCenter(index) {
  return scale(domainDirections[index], 390);
}

function galaxyCenter(index) {
  const galaxy = galaxies[index];
  const center = domainCenter(galaxy.domain);
  const { normal, axisA, axisB } = basisFor(center);
  const localIndex = index % 3;
  const angle = localIndex * Math.PI * 2 / 3;
  return add(
    add(center, scale(axisA, Math.cos(angle) * 155)),
    add(scale(axisB, Math.sin(angle) * 155), scale(normal, 18)),
  );
}

function starPosition(center, index) {
  const { normal, axisA, axisB } = basisFor(center);
  const angle = index * Math.PI * (3 - Math.sqrt(5));
  const radius = 12 + Math.sqrt(index / 72) * 108;
  const depth = (random() - 0.5) * 44;
  return add(
    add(center, scale(axisA, Math.cos(angle) * radius)),
    add(scale(axisB, Math.sin(angle) * radius), scale(normal, depth)),
  );
}

function buildGraphData() {
  const nodes = [];
  const links = [];

  domains.forEach((domain, index) => {
    const center = domainCenter(index);
    nodes.push({
      id: domain.id,
      name: domain.name,
      parentId: "universe",
      kind: "domain",
      color: domain.color,
      domain: index,
      importance: 1,
      fx: center.x,
      fy: center.y,
      fz: center.z,
    });
  });

  galaxies.forEach((galaxy, group) => {
    const center = galaxyCenter(group);
    const galaxyId = `galaxy-${group}`;
    nodes.push({
      id: galaxyId,
      name: galaxy.name,
      parentId: domains[galaxy.domain].id,
      kind: "galaxy",
      color: galaxy.color,
      domain: galaxy.domain,
      group,
      importance: 1,
      fx: center.x,
      fy: center.y,
      fz: center.z,
    });
    links.push({
      source: galaxyId,
      target: domains[galaxy.domain].id,
      type: "hierarchy",
      score: 1,
      color: galaxy.color,
    });

    for (let index = 0; index < 72; index += 1) {
      const position = starPosition(center, index);
      const topicIndex = index % galaxy.topics.length;
      const topic = galaxy.topics[topicIndex];
      const starColor = topicPalettes[galaxy.domain][topicIndex];
      const suffix = suffixes[Math.floor(index / galaxy.topics.length) % suffixes.length];
      const node = {
        id: `star-${group}-${index}`,
        name: `${topic} ${suffix}`,
        parentId: galaxyId,
        kind: "star",
        color: starColor,
        domain: galaxy.domain,
        group,
        topic,
        topicIndex,
        importance: 0.2 + random() * 0.8,
        references: 3 + Math.floor(random() * 48),
        confidence: 78 + Math.floor(random() * 21),
        fx: position.x,
        fy: position.y,
        fz: position.z,
      };
      nodes.push(node);
      links.push({ source: node.id, target: galaxyId, type: "hierarchy", score: 0.58 + random() * 0.35, color: starColor, group });

      if (index > 1) {
        const target = Math.max(0, index - 1 - Math.floor(random() * Math.min(index, 9)));
        links.push({ source: node.id, target: `star-${group}-${target}`, type: "semantic", score: random(), color: starColor, group });
      }
      if (index > 8 && random() > 0.54) {
        const target = Math.floor(random() * index);
        links.push({ source: node.id, target: `star-${group}-${target}`, type: "semantic", score: random(), color: starColor, group });
      }
    }
  });

  domains.forEach((domain, index) => {
    links.push({
      source: domain.id,
      target: domains[(index + 1) % domains.length].id,
      type: "backbone",
      score: 1,
      color: domain.color,
    });
  });

  return { nodes, links };
}

const universe = { id: "universe", name: "LLM Wiki Universe", parentId: null, kind: "universe" };
const data = buildGraphData();
const nodesById = new Map([[universe.id, universe], ...data.nodes.map((node) => [node.id, node])]);
const childrenByParent = new Map();
data.nodes.forEach((node) => {
  if (!childrenByParent.has(node.parentId)) childrenByParent.set(node.parentId, []);
  childrenByParent.get(node.parentId).push(node);
});

const semanticAdjacency = new Map(data.nodes.map((node) => [node.id, new Set()]));
data.links.filter((link) => link.type === "semantic").forEach((link) => {
  semanticAdjacency.get(link.source).add(link.target);
  semanticAdjacency.get(link.target).add(link.source);
});

const graphElement = document.querySelector("#graph");
const labelLayer = document.querySelector("[data-label-layer]");
const panel = document.querySelector("[data-node-panel]");
const searchInput = document.querySelector("[data-search]");
const densityInput = document.querySelector("[data-density]");
const visibleOutput = document.querySelector("[data-visible-count]");
const clarityStatus = document.querySelector("[data-clarity-status]");
const scopePath = document.querySelector("[data-scope-path]");

let scopeId = universe.id;
let selectedNode = null;
let hoveredNode = null;
let selectedDepth = 1;
let rotating = true;
let densityThreshold = 0.76;
let activeNeighborhood = null;
let labelFrame = 0;

function currentScope() {
  return nodesById.get(scopeId);
}

function idOf(endpoint) {
  return typeof endpoint === "object" ? endpoint.id : endpoint;
}

function isDescendant(node, ancestorId) {
  if (ancestorId === universe.id) return true;
  let cursor = node;
  while (cursor?.parentId) {
    if (cursor.parentId === ancestorId) return true;
    cursor = nodesById.get(cursor.parentId);
  }
  return false;
}

function pathToScope(id) {
  const path = [];
  let cursor = nodesById.get(id);
  while (cursor) {
    path.unshift(cursor);
    cursor = cursor.parentId ? nodesById.get(cursor.parentId) : null;
  }
  return path;
}

function neighborhood(nodeId, depth) {
  if (depth === Infinity) {
    return new Set(childrenByParent.get(nodesById.get(nodeId).parentId).map((node) => node.id));
  }
  const visited = new Set([nodeId]);
  let frontier = new Set([nodeId]);
  for (let level = 0; level < depth; level += 1) {
    const next = new Set();
    frontier.forEach((id) => {
      semanticAdjacency.get(id)?.forEach((neighbor) => {
        if (!visited.has(neighbor)) {
          visited.add(neighbor);
          next.add(neighbor);
        }
      });
    });
    frontier = next;
  }
  return visited;
}

const graph = ForceGraph3D()(graphElement)
  .graphData(data)
  .backgroundColor("#05090a")
  .showNavInfo(false)
  .nodeVal((node) => node.kind === "domain" ? 11 : node.kind === "galaxy" ? 7 : 0.55 + node.importance * 1.65)
  .nodeResolution(8)
  .nodeOpacity(0.88)
  .nodeLabel((node) => node.kind === "star" ? `${node.name}<br>${node.topic} · ${galaxies[node.group].name}` : node.name)
  .linkOpacity(0.66)
  .linkWidth((link) => link.type === "backbone" ? 0.48 : 0.18 + link.score * 0.34)
  .linkDirectionalParticles((link) => currentScope().kind === "galaxy" && link.type === "semantic" && link.score > 0.9 ? 1 : 0)
  .linkDirectionalParticleWidth(0.7)
  .linkDirectionalParticleSpeed(0.0024)
  .enableNodeDrag(false)
  .onNodeHover((node) => {
    hoveredNode = node;
    graphElement.style.cursor = node ? "pointer" : "grab";
    refreshGraph();
  })
  .onNodeClick((node) => {
    if (node.kind === "star" && currentScope().id === node.parentId) focusStar(node);
    else if (node.kind === "star") enterScope(node.parentId);
    else enterScope(node.id);
  })
  .onBackgroundClick(() => {
    if (!selectedNode) return;
    selectedNode = null;
    activeNeighborhood = null;
    panel.classList.remove("is-visible");
    refreshGraph();
  });

graph.renderer().setPixelRatio(Math.min(window.devicePixelRatio, 2));
graph.controls().autoRotate = true;
graph.controls().autoRotateSpeed = 0.18;
graph.controls().enableDamping = true;
graph.controls().dampingFactor = 0.08;
graph.cameraPosition({ x: 0, y: 0, z: 1280 }, { x: 0, y: 0, z: 0 }, 0);

function refreshGraph() {
  const scope = currentScope();
  const hoverNeighborhood = hoveredNode?.kind === "star" && scope.id === hoveredNode.parentId
    ? neighborhood(hoveredNode.id, 1)
    : null;

  graph
    .nodeColor((node) => {
      if (scope.kind === "universe") {
        if (node.kind === "domain") return node.color;
        if (node.kind === "galaxy") return shadeColor(node.color, 0.62);
        return shadeColor(node.color, 0.3);
      }
      if (node.id === scope.id) return "#ffffff";
      if (!isDescendant(node, scope.id)) return shadeColor(node.color, 0.16);
      if (node.parentId === scope.id && node.kind !== "star") return node.color;
      if (selectedNode?.id === node.id) return "#ffffff";
      if (activeNeighborhood && !activeNeighborhood.has(node.id)) return shadeColor(node.color, 0.22);
      if (hoverNeighborhood && !hoverNeighborhood.has(node.id)) return shadeColor(node.color, 0.34);
      return node.kind === "star" ? node.color : shadeColor(node.color, 0.68);
    })
    .nodeVal((node) => {
      if (node.id === scope.id) return 12;
      if (node.kind === "domain") return 9;
      if (node.kind === "galaxy") return node.parentId === scope.id ? 9 : 5;
      if (selectedNode?.id === node.id) return 5.2;
      return 0.55 + node.importance * (node.parentId === scope.id ? 2.4 : 1.05);
    })
    .linkVisibility((link) => {
      const source = nodesById.get(idOf(link.source));
      const target = nodesById.get(idOf(link.target));
      if (scope.kind === "universe") return link.type === "backbone";
      if (link.type === "backbone") return false;
      if (link.type === "hierarchy") {
        if (target.id !== scope.id) return false;
        if (scope.kind !== "galaxy") return true;
        return link.score > densityThreshold + 0.06;
      }
      if (scope.kind !== "galaxy" || source.parentId !== scope.id || target.parentId !== scope.id) return false;
      if (activeNeighborhood && (!activeNeighborhood.has(source.id) || !activeNeighborhood.has(target.id))) return false;
      if (hoverNeighborhood && (!hoverNeighborhood.has(source.id) || !hoverNeighborhood.has(target.id))) return false;
      return link.score > densityThreshold;
    })
    .linkColor((link) => link.type === "backbone" ? "#425c59" : link.color)
    .refresh();

  updateInterface();
}

function enterScope(id, animate = true) {
  const node = nodesById.get(id);
  if (!node || node.kind === "star") return;
  scopeId = id;
  selectedNode = null;
  activeNeighborhood = null;
  panel.classList.remove("is-visible");

  if (animate) {
    if (node.kind === "universe") {
      graph.cameraPosition({ x: 0, y: 0, z: 1280 }, { x: 0, y: 0, z: 0 }, 1100);
      rotating = true;
      graph.controls().autoRotate = true;
      syncRotateButton();
    } else {
      const center = { x: node.x ?? node.fx, y: node.y ?? node.fy, z: node.z ?? node.fz };
      const direction = normalize(center);
      const distance = node.kind === "domain" ? 580 : 245;
      graph.cameraPosition(
        { x: center.x + direction.x * distance, y: center.y + direction.y * distance, z: center.z + direction.z * distance },
        center,
        1100,
      );
    }
  }

  if (node.kind !== "universe") {
    graph.controls().autoRotate = false;
    rotating = false;
    syncRotateButton();
  }
  refreshGraph();
}

function focusStar(node) {
  selectedNode = node;
  activeNeighborhood = neighborhood(node.id, selectedDepth);
  const parent = nodesById.get(node.parentId);
  const position = { x: node.x ?? node.fx, y: node.y ?? node.fy, z: node.z ?? node.fz };
  const direction = normalize({ x: position.x - parent.fx, y: position.y - parent.fy, z: position.z - parent.fz });
  graph.cameraPosition(
    { x: position.x + direction.x * 84, y: position.y + direction.y * 84, z: position.z + direction.z * 84 },
    position,
    850,
  );
  showPanel(node);
  refreshGraph();
}

function resetUniverse() {
  searchInput.value = "";
  enterScope(universe.id);
}

function showPanel(node) {
  panel.innerHTML = `
    <header><span class="node-kind">${galaxies[node.group].name} / ${node.topic}</span><span class="node-id">${node.id.toUpperCase()}</span></header>
    <h2>${node.name}</h2>
    <p>该 Star 聚合 Wiki 页面、代码符号与引用证据。当前只呈现它在 ${selectedDepth === Infinity ? "整个星系" : `${selectedDepth} 跳范围`}内的语义关系。</p>
    <div class="node-meta">
      <div><small>References</small><strong>${node.references}</strong></div>
      <div><small>Confidence</small><strong>${node.confidence}%</strong></div>
      <div><small>Relations</small><strong>${semanticAdjacency.get(node.id).size}</strong></div>
      <div><small>Galaxy</small><strong>${galaxies[node.group].short}</strong></div>
    </div>`;
  panel.classList.add("is-visible");
}

function updateInterface() {
  const scope = currentScope();
  const children = childrenByParent.get(scope.id) ?? [];
  clarityStatus.innerHTML = `<span>${scope.kind === "universe" ? "Universe view" : `${scope.kind} focus`}</span><strong>${scope.name}</strong>`;
  visibleOutput.textContent = selectedNode
    ? `${activeNeighborhood.size} STARS`
    : `${children.length} ${children[0]?.kind?.toUpperCase() ?? "ITEMS"}`;

  const path = pathToScope(scope.id);
  scopePath.replaceChildren();
  path.forEach((item, index) => {
    const button = document.createElement("button");
    button.textContent = item.kind === "universe" ? "Universe" : item.name;
    button.disabled = index === path.length - 1;
    button.addEventListener("click", () => enterScope(item.id));
    scopePath.append(button);
    if (index < path.length - 1) {
      const separator = document.createElement("span");
      separator.textContent = "/";
      scopePath.append(separator);
    }
  });
}

function syncRotateButton() {
  const button = document.querySelector('[data-action="rotate"]');
  button.classList.toggle("is-active", rotating);
  button.setAttribute("aria-pressed", String(rotating));
}

function renderLabels() {
  const candidates = childrenByParent.get(scopeId) ?? [];
  const activeIds = new Set(candidates.map((node) => node.id));

  labelLayer.querySelectorAll("button").forEach((button) => {
    if (!activeIds.has(button.dataset.nodeId)) button.remove();
  });

  candidates.forEach((node) => {
    let label = labelLayer.querySelector(`[data-node-id="${node.id}"]`);
    if (!label) {
      label = document.createElement("button");
      label.dataset.nodeId = node.id;
      label.className = `space-label ${node.kind}-name`;
      label.textContent = node.name;
      label.addEventListener("click", () => node.kind === "star" ? focusStar(node) : enterScope(node.id));
      labelLayer.append(label);
    }
    const point = graph.graph2ScreenCoords(node.x ?? node.fx, node.y ?? node.fy, node.z ?? node.fz);
    const inViewport = point.x > -120 && point.x < window.innerWidth + 120 && point.y > 60 && point.y < window.innerHeight - 45;
    label.hidden = !inViewport || Boolean(activeNeighborhood && !activeNeighborhood.has(node.id));
    label.style.transform = `translate3d(${point.x}px, ${point.y}px, 0) translate(-50%, -50%)`;
    label.style.setProperty("--label-color", node.color);
    label.style.setProperty("--label-weight", String(node.importance ?? 1));
    label.style.setProperty("--label-opacity", String(node.kind === "star" ? 0.48 + node.importance * 0.5 : 1));
    label.classList.toggle("is-selected", selectedNode?.id === node.id);
  });

  labelFrame = window.requestAnimationFrame(renderLabels);
}

densityInput.addEventListener("input", () => {
  densityThreshold = 1 - Number(densityInput.value) / 100 * 0.8;
  refreshGraph();
});

document.querySelectorAll("[data-depth]").forEach((button) => {
  button.addEventListener("click", () => {
    document.querySelectorAll("[data-depth]").forEach((item) => item.classList.remove("is-active"));
    button.classList.add("is-active");
    selectedDepth = button.dataset.depth === "all" ? Infinity : Number(button.dataset.depth);
    if (selectedNode) {
      activeNeighborhood = neighborhood(selectedNode.id, selectedDepth);
      showPanel(selectedNode);
      refreshGraph();
    }
  });
});

searchInput.addEventListener("input", () => {
  const query = searchInput.value.trim().toLowerCase();
  if (!query) return;
  const match = data.nodes.find((node) => node.name.toLowerCase().includes(query));
  if (!match) return;
  if (match.kind === "star") {
    enterScope(match.parentId);
    focusStar(match);
  } else {
    enterScope(match.id);
  }
});

document.querySelector('[data-action="reset"]').addEventListener("click", resetUniverse);
document.querySelector('[data-action="rotate"]').addEventListener("click", () => {
  rotating = !rotating;
  graph.controls().autoRotate = rotating;
  syncRotateButton();
});
document.querySelector('[data-action="help"]').addEventListener("click", () => {
  document.querySelector(".help-toast")?.remove();
  const toast = document.createElement("div");
  toast.className = "help-toast";
  toast.textContent = "每层只标注下一层级。依次点击 Domain、Galaxy 和 Star 下钻；面包屑或 Esc 可逐级返回。";
  document.body.append(toast);
  window.setTimeout(() => toast.remove(), 5600);
});

document.addEventListener("keydown", (event) => {
  if (event.key === "/" && document.activeElement !== searchInput) {
    event.preventDefault();
    searchInput.focus();
  }
  if (event.key === "Escape") {
    if (selectedNode) {
      selectedNode = null;
      activeNeighborhood = null;
      panel.classList.remove("is-visible");
      refreshGraph();
    } else if (scopeId !== universe.id) {
      enterScope(currentScope().parentId);
    }
  }
});

window.addEventListener("resize", () => {
  graph.width(window.innerWidth).height(window.innerHeight);
});
window.addEventListener("beforeunload", () => window.cancelAnimationFrame(labelFrame));

createIcons({
  icons: { ArrowLeft, CircleHelp, Move3d, Orbit, Scan, Search, SlidersHorizontal },
});
refreshGraph();
renderLabels();

import * as vscode from "vscode";

// ---------------------------------------------------------------------------
// Call Graph Webview
//
// Opens a webview panel that visualises the call hierarchy of the symbol
// under the cursor as a force-directed graph using inline D3.js (v7 ESM via
// CDN).  Traverses both incoming and outgoing calls up to a configurable
// depth and renders them as an interactive, zoomable graph.
// ---------------------------------------------------------------------------

interface GraphNode {
  id: string;
  label: string;
  kind: "root" | "caller" | "callee";
  uri?: string;
  range?: vscode.Range;
}

interface GraphLink {
  source: string;
  target: string;
}

// ── Public API ─────────────────────────────────────────────────────────────

/** Register the `objc-lsp.showCallGraph` command. Call from `activate()`. */
export function registerCallGraph(context: vscode.ExtensionContext): void {
  context.subscriptions.push(
    vscode.commands.registerCommand("objc-lsp.showCallGraph", () => {
      showCallGraph(context);
    })
  );
}

// ── Core logic ─────────────────────────────────────────────────────────────

const MAX_DEPTH = 2;

async function showCallGraph(
  context: vscode.ExtensionContext
): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    vscode.window.showWarningMessage("No active editor — place cursor on a method or function.");
    return;
  }

  const doc = editor.document;
  const pos = editor.selection.active;

  // Prepare call hierarchy at cursor position.
  const items = await vscode.commands.executeCommand<vscode.CallHierarchyItem[]>(
    "vscode.prepareCallHierarchy",
    doc.uri,
    pos
  );

  if (!items || items.length === 0) {
    vscode.window.showWarningMessage("No call hierarchy data at the cursor position.");
    return;
  }

  const rootItem = items[0];

  // Build graph data.
  const nodes = new Map<string, GraphNode>();
  const links: GraphLink[] = [];

  const rootId = itemId(rootItem);
  nodes.set(rootId, {
    id: rootId,
    label: rootItem.name,
    kind: "root",
    uri: rootItem.uri.toString(),
    range: rootItem.selectionRange,
  });

  await Promise.all([
    collectIncoming(rootItem, rootId, 0, nodes, links),
    collectOutgoing(rootItem, rootId, 0, nodes, links),
  ]);

  // Open webview panel.
  const panel = vscode.window.createWebviewPanel(
    "objcCallGraph",
    `Call Graph: ${rootItem.name}`,
    vscode.ViewColumn.Beside,
    { enableScripts: true, retainContextWhenHidden: true }
  );

  panel.webview.html = buildHtml(
    Array.from(nodes.values()),
    links,
    rootId
  );

  // Handle navigate-to-source messages from webview.
  panel.webview.onDidReceiveMessage(
    async (msg: { command: string; uri: string; line: number; character: number }) => {
      if (msg.command === "navigate") {
        const uri = vscode.Uri.parse(msg.uri);
        const pos = new vscode.Position(msg.line, msg.character);
        const doc = await vscode.workspace.openTextDocument(uri);
        await vscode.window.showTextDocument(doc, {
          selection: new vscode.Range(pos, pos),
          preserveFocus: false,
        });
      }
    },
    undefined,
    context.subscriptions
  );
}

// ── Graph traversal ────────────────────────────────────────────────────────

function itemId(item: vscode.CallHierarchyItem): string {
  return `${item.uri.toString()}:${item.selectionRange.start.line}:${item.selectionRange.start.character}:${item.name}`;
}

async function collectIncoming(
  item: vscode.CallHierarchyItem,
  itemNodeId: string,
  depth: number,
  nodes: Map<string, GraphNode>,
  links: GraphLink[]
): Promise<void> {
  if (depth >= MAX_DEPTH) {
    return;
  }

  const incomingCalls = await vscode.commands.executeCommand<vscode.CallHierarchyIncomingCall[]>(
    "vscode.provideIncomingCalls",
    item
  );

  if (!incomingCalls) {
    return;
  }

  const nextLevel: Promise<void>[] = [];

  for (const call of incomingCalls) {
    const callerId = itemId(call.from);
    if (!nodes.has(callerId)) {
      nodes.set(callerId, {
        id: callerId,
        label: call.from.name,
        kind: "caller",
        uri: call.from.uri.toString(),
        range: call.from.selectionRange,
      });
      nextLevel.push(
        collectIncoming(call.from, callerId, depth + 1, nodes, links)
      );
    }
    links.push({ source: callerId, target: itemNodeId });
  }

  await Promise.all(nextLevel);
}

async function collectOutgoing(
  item: vscode.CallHierarchyItem,
  itemNodeId: string,
  depth: number,
  nodes: Map<string, GraphNode>,
  links: GraphLink[]
): Promise<void> {
  if (depth >= MAX_DEPTH) {
    return;
  }

  const outgoingCalls = await vscode.commands.executeCommand<vscode.CallHierarchyOutgoingCall[]>(
    "vscode.provideOutgoingCalls",
    item
  );

  if (!outgoingCalls) {
    return;
  }

  const nextLevel: Promise<void>[] = [];

  for (const call of outgoingCalls) {
    const calleeId = itemId(call.to);
    if (!nodes.has(calleeId)) {
      nodes.set(calleeId, {
        id: calleeId,
        label: call.to.name,
        kind: "callee",
        uri: call.to.uri.toString(),
        range: call.to.selectionRange,
      });
      nextLevel.push(
        collectOutgoing(call.to, calleeId, depth + 1, nodes, links)
      );
    }
    links.push({ source: itemNodeId, target: calleeId });
  }

  await Promise.all(nextLevel);
}

// ── Webview HTML ───────────────────────────────────────────────────────────

function buildHtml(
  nodes: GraphNode[],
  links: GraphLink[],
  rootId: string
): string {
  // Serialize graph data as JSON for the webview script.
  const graphData = JSON.stringify({ nodes, links, rootId });

  return /* html */ `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Call Graph</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    html, body { width: 100%; height: 100%; overflow: hidden; }
    body {
      background: var(--vscode-editor-background, #1e1e1e);
      color: var(--vscode-editor-foreground, #d4d4d4);
      font-family: var(--vscode-font-family, 'Segoe UI', sans-serif);
    }
    svg { width: 100%; height: 100%; }
    .node-label {
      font-size: 12px;
      fill: var(--vscode-editor-foreground, #d4d4d4);
      pointer-events: none;
      text-anchor: middle;
    }
    .link {
      stroke: var(--vscode-editorWidget-border, #454545);
      stroke-opacity: 0.6;
      fill: none;
      marker-end: url(#arrowhead);
    }
    .legend {
      position: fixed;
      bottom: 12px;
      left: 12px;
      font-size: 11px;
      opacity: 0.8;
    }
    .legend span {
      margin-right: 14px;
    }
    .legend .dot {
      display: inline-block;
      width: 10px;
      height: 10px;
      border-radius: 50%;
      margin-right: 4px;
      vertical-align: middle;
    }
  </style>
</head>
<body>
  <svg id="graph"></svg>
  <div class="legend">
    <span><span class="dot" style="background:#e06c75;"></span>Root</span>
    <span><span class="dot" style="background:#61afef;"></span>Callers (incoming)</span>
    <span><span class="dot" style="background:#98c379;"></span>Callees (outgoing)</span>
  </div>

  <script type="module">
    import * as d3 from "https://cdn.jsdelivr.net/npm/d3@7/+esm";

    const vscode = acquireVsCodeApi();
    const data = ${graphData};

    const width = window.innerWidth;
    const height = window.innerHeight;

    const svg = d3.select("#graph")
      .attr("viewBox", [0, 0, width, height]);

    // Arrow marker.
    svg.append("defs").append("marker")
      .attr("id", "arrowhead")
      .attr("viewBox", "0 -5 10 10")
      .attr("refX", 22)
      .attr("refY", 0)
      .attr("markerWidth", 6)
      .attr("markerHeight", 6)
      .attr("orient", "auto")
      .append("path")
      .attr("d", "M0,-5L10,0L0,5")
      .attr("fill", "#888");

    // Create a map from node id to index for D3 force link references.
    const nodeById = new Map(data.nodes.map((n, i) => [n.id, i]));
    const links = data.links
      .filter(l => nodeById.has(l.source) && nodeById.has(l.target))
      .map(l => ({ source: l.source, target: l.target }));

    const colorMap = { root: "#e06c75", caller: "#61afef", callee: "#98c379" };

    const simulation = d3.forceSimulation(data.nodes)
      .force("link", d3.forceLink(links).id(d => d.id).distance(120))
      .force("charge", d3.forceManyBody().strength(-400))
      .force("center", d3.forceCenter(width / 2, height / 2))
      .force("collision", d3.forceCollide().radius(30));

    // Zoom.
    const g = svg.append("g");
    svg.call(d3.zoom().scaleExtent([0.1, 4]).on("zoom", (e) => {
      g.attr("transform", e.transform);
    }));

    // Links.
    const link = g.append("g")
      .selectAll("line")
      .data(links)
      .join("line")
      .attr("class", "link")
      .attr("stroke-width", 1.5);

    // Nodes.
    const node = g.append("g")
      .selectAll("circle")
      .data(data.nodes)
      .join("circle")
      .attr("r", d => d.kind === "root" ? 14 : 10)
      .attr("fill", d => colorMap[d.kind] || "#888")
      .attr("stroke", "#222")
      .attr("stroke-width", 1.5)
      .style("cursor", "pointer")
      .on("click", (event, d) => {
        if (d.uri && d.range) {
          vscode.postMessage({
            command: "navigate",
            uri: d.uri,
            line: d.range.start.line,
            character: d.range.start.character,
          });
        }
      })
      .call(drag(simulation));

    // Labels.
    const label = g.append("g")
      .selectAll("text")
      .data(data.nodes)
      .join("text")
      .attr("class", "node-label")
      .attr("dy", d => d.kind === "root" ? -20 : -16)
      .text(d => d.label);

    simulation.on("tick", () => {
      link
        .attr("x1", d => d.source.x)
        .attr("y1", d => d.source.y)
        .attr("x2", d => d.target.x)
        .attr("y2", d => d.target.y);

      node
        .attr("cx", d => d.x)
        .attr("cy", d => d.y);

      label
        .attr("x", d => d.x)
        .attr("y", d => d.y);
    });

    function drag(simulation) {
      return d3.drag()
        .on("start", (event, d) => {
          if (!event.active) simulation.alphaTarget(0.3).restart();
          d.fx = d.x;
          d.fy = d.y;
        })
        .on("drag", (event, d) => {
          d.fx = event.x;
          d.fy = event.y;
        })
        .on("end", (event, d) => {
          if (!event.active) simulation.alphaTarget(0);
          d.fx = null;
          d.fy = null;
        });
    }
  </script>
</body>
</html>`;
}

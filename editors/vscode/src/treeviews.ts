import * as vscode from "vscode";

// ---------------------------------------------------------------------------
// ObjC Tree Views
//
// Two sidebar tree views for Objective-C projects:
//   1. Symbols Outline Pro — grouped by #pragma mark sections
//   2. Class Browser — all classes/protocols in the workspace
// ---------------------------------------------------------------------------

// ── Types ─────────────────────────────────────────────────────────────────

interface SymbolNode {
  kind: "mark" | "symbol";
  label: string;
  detail?: string;
  iconId?: string;
  range?: vscode.Range;
  uri?: vscode.Uri;
  children: SymbolNode[];
}

// ── Symbols Outline Pro ───────────────────────────────────────────────────

class SymbolsOutlineProvider implements vscode.TreeDataProvider<SymbolNode> {
  private _onDidChange = new vscode.EventEmitter<SymbolNode | undefined | void>();
  readonly onDidChangeTreeData = this._onDidChange.event;

  private roots: SymbolNode[] = [];

  refresh(): void {
    this.roots = [];
    this._onDidChange.fire();
  }

  getTreeItem(element: SymbolNode): vscode.TreeItem {
    const collapsible =
      element.children.length > 0
        ? vscode.TreeItemCollapsibleState.Expanded
        : vscode.TreeItemCollapsibleState.None;

    const item = new vscode.TreeItem(element.label, collapsible);

    if (element.kind === "mark") {
      item.iconPath = new vscode.ThemeIcon("symbol-namespace");
      item.description = element.detail;
    } else {
      item.iconPath = new vscode.ThemeIcon(
        element.iconId || "symbol-method"
      );
      item.description = element.detail;
    }

    // Click to navigate.
    if (element.uri && element.range) {
      item.command = {
        command: "vscode.open",
        title: "Go to Symbol",
        arguments: [
          element.uri,
          {
            selection: element.range,
          },
        ],
      };
    }

    return item;
  }

  getChildren(element?: SymbolNode): vscode.ProviderResult<SymbolNode[]> {
    if (element) {
      return element.children;
    }

    // Top level: build from active editor.
    return this.buildOutline();
  }

  private async buildOutline(): Promise<SymbolNode[]> {
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
      return [];
    }

    const doc = editor.document;
    if (doc.languageId !== "objective-c" && doc.languageId !== "objective-cpp") {
      return [];
    }

    // Get document symbols from the LSP server.
    const symbols = await vscode.commands.executeCommand<vscode.DocumentSymbol[]>(
      "vscode.executeDocumentSymbolProvider",
      doc.uri
    );

    // Build pragma mark sections from the raw text.
    const marks = parsePragmaMarks(doc);
    const nodes = buildGroupedOutline(doc.uri, symbols || [], marks);
    this.roots = nodes;
    return nodes;
  }
}

// ── Pragma mark parsing ───────────────────────────────────────────────────

interface PragmaMark {
  label: string;
  line: number;
}

function parsePragmaMarks(document: vscode.TextDocument): PragmaMark[] {
  const marks: PragmaMark[] = [];
  for (let i = 0; i < document.lineCount; i++) {
    const text = document.lineAt(i).text;
    const match = text.match(/^#pragma\s+mark\s+[-–—]?\s*(.*)/);
    if (match) {
      const label = match[1].trim();
      if (label) {
        marks.push({ label, line: i });
      }
    }
  }
  return marks;
}

/** Group symbols under their nearest preceding #pragma mark. */
function buildGroupedOutline(
  uri: vscode.Uri,
  symbols: vscode.DocumentSymbol[],
  marks: PragmaMark[]
): SymbolNode[] {
  if (marks.length === 0) {
    // No pragma marks — flat list.
    return symbols.map((s) => docSymbolToNode(uri, s));
  }

  // Create mark group nodes.
  const groups: SymbolNode[] = [];
  const ungroups: SymbolNode[] = []; // Symbols before first mark.

  // Sort marks by line.
  marks.sort((a, b) => a.line - b.line);

  // Create a group for each mark.
  const markNodes = marks.map(
    (m): SymbolNode => ({
      kind: "mark",
      label: `── ${m.label} ──`,
      detail: "",
      range: new vscode.Range(m.line, 0, m.line, 0),
      uri,
      children: [],
    })
  );

  // Assign symbols to groups.
  for (const sym of symbols) {
    const symLine = sym.range.start.line;
    let assigned = false;

    // Find the last mark before this symbol.
    for (let i = marks.length - 1; i >= 0; i--) {
      if (marks[i].line <= symLine) {
        markNodes[i].children.push(docSymbolToNode(uri, sym));
        assigned = true;
        break;
      }
    }

    if (!assigned) {
      ungroups.push(docSymbolToNode(uri, sym));
    }
  }

  groups.push(...ungroups, ...markNodes);
  return groups;
}

function docSymbolToNode(uri: vscode.Uri, sym: vscode.DocumentSymbol): SymbolNode {
  return {
    kind: "symbol",
    label: sym.name,
    detail: sym.detail,
    iconId: symbolKindToIcon(sym.kind),
    range: sym.selectionRange,
    uri,
    children: sym.children.map((c) => docSymbolToNode(uri, c)),
  };
}

function symbolKindToIcon(kind: vscode.SymbolKind): string {
  switch (kind) {
    case vscode.SymbolKind.Class:
      return "symbol-class";
    case vscode.SymbolKind.Method:
      return "symbol-method";
    case vscode.SymbolKind.Property:
      return "symbol-property";
    case vscode.SymbolKind.Function:
      return "symbol-function";
    case vscode.SymbolKind.Interface:
      return "symbol-interface";
    case vscode.SymbolKind.Module:
      return "symbol-module";
    case vscode.SymbolKind.Variable:
      return "symbol-variable";
    case vscode.SymbolKind.Enum:
      return "symbol-enum";
    case vscode.SymbolKind.EnumMember:
      return "symbol-enum-member";
    case vscode.SymbolKind.Constant:
      return "symbol-constant";
    case vscode.SymbolKind.Struct:
      return "symbol-struct";
    default:
      return "symbol-misc";
  }
}

// ── Class Browser ─────────────────────────────────────────────────────────

interface ClassNode {
  name: string;
  kind: string;
  location?: vscode.Location;
  children: ClassNode[];
}

class ClassBrowserProvider implements vscode.TreeDataProvider<ClassNode> {
  private _onDidChange = new vscode.EventEmitter<ClassNode | undefined | void>();
  readonly onDidChangeTreeData = this._onDidChange.event;

  refresh(): void {
    this._onDidChange.fire();
  }

  getTreeItem(element: ClassNode): vscode.TreeItem {
    const collapsible =
      element.children.length > 0
        ? vscode.TreeItemCollapsibleState.Collapsed
        : vscode.TreeItemCollapsibleState.None;

    const item = new vscode.TreeItem(element.name, collapsible);
    item.description = element.kind;

    switch (element.kind) {
      case "class":
        item.iconPath = new vscode.ThemeIcon("symbol-class");
        break;
      case "protocol":
        item.iconPath = new vscode.ThemeIcon("symbol-interface");
        break;
      case "category":
        item.iconPath = new vscode.ThemeIcon("symbol-module");
        break;
      case "method":
        item.iconPath = new vscode.ThemeIcon("symbol-method");
        break;
      case "property":
        item.iconPath = new vscode.ThemeIcon("symbol-property");
        break;
      default:
        item.iconPath = new vscode.ThemeIcon("symbol-misc");
    }

    if (element.location) {
      item.command = {
        command: "vscode.open",
        title: "Go to Symbol",
        arguments: [
          element.location.uri,
          { selection: element.location.range },
        ],
      };
    }

    return item;
  }

  async getChildren(element?: ClassNode): Promise<ClassNode[]> {
    if (element) {
      return element.children;
    }

    // Root: query workspace symbols for classes and protocols.
    return this.buildClassList();
  }

  private async buildClassList(): Promise<ClassNode[]> {
    // Use workspace symbol search with empty query to get all symbols.
    // The LSP server returns classes, protocols, categories, methods, properties.
    const symbols = await vscode.commands.executeCommand<vscode.SymbolInformation[]>(
      "vscode.executeWorkspaceSymbolProvider",
      ""
    );

    if (!symbols || symbols.length === 0) {
      return [];
    }

    // Group: collect top-level classes/protocols, then nest methods under them.
    const classMap = new Map<string, ClassNode>();
    const topLevel: ClassNode[] = [];

    // First pass: collect classes, protocols, categories.
    for (const sym of symbols) {
      if (
        sym.kind === vscode.SymbolKind.Class ||
        sym.kind === vscode.SymbolKind.Interface ||
        sym.kind === vscode.SymbolKind.Module
      ) {
        const kindLabel =
          sym.kind === vscode.SymbolKind.Class
            ? "class"
            : sym.kind === vscode.SymbolKind.Interface
              ? "protocol"
              : "category";

        const node: ClassNode = {
          name: sym.name,
          kind: kindLabel,
          location: sym.location,
          children: [],
        };
        classMap.set(sym.name, node);
        topLevel.push(node);
      }
    }

    // Second pass: try to nest methods/properties under their class.
    for (const sym of symbols) {
      if (
        sym.kind === vscode.SymbolKind.Method ||
        sym.kind === vscode.SymbolKind.Property
      ) {
        const kindLabel =
          sym.kind === vscode.SymbolKind.Method ? "method" : "property";

        // Try to find the parent class by container name.
        const parent = sym.containerName
          ? classMap.get(sym.containerName)
          : undefined;

        const node: ClassNode = {
          name: sym.name,
          kind: kindLabel,
          location: sym.location,
          children: [],
        };

        if (parent) {
          parent.children.push(node);
        }
        // Otherwise skip — orphan methods without a known class
      }
    }

    // Sort: classes first, then protocols, then categories.
    topLevel.sort((a, b) => {
      const order: Record<string, number> = {
        class: 0,
        protocol: 1,
        category: 2,
      };
      return (order[a.kind] ?? 3) - (order[b.kind] ?? 3) || a.name.localeCompare(b.name);
    });

    return topLevel;
  }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/** Register tree views. Call from `activate()`. */
export function registerTreeViews(context: vscode.ExtensionContext): void {
  // ── Symbols Outline Pro ──
  const symbolsProvider = new SymbolsOutlineProvider();
  const symbolsView = vscode.window.createTreeView("objcSymbolsOutline", {
    treeDataProvider: symbolsProvider,
    showCollapseAll: true,
  });
  context.subscriptions.push(symbolsView);

  // Refresh when active editor changes.
  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor(() => {
      symbolsProvider.refresh();
    })
  );
  // Refresh on document save.
  context.subscriptions.push(
    vscode.workspace.onDidSaveTextDocument(() => {
      symbolsProvider.refresh();
    })
  );

  // ── Class Browser ──
  const classProvider = new ClassBrowserProvider();
  const classView = vscode.window.createTreeView("objcClassBrowser", {
    treeDataProvider: classProvider,
    showCollapseAll: true,
  });
  context.subscriptions.push(classView);

  // Refresh command.
  context.subscriptions.push(
    vscode.commands.registerCommand("objc-lsp.refreshClassBrowser", () => {
      classProvider.refresh();
    })
  );
}

import * as vscode from "vscode";

// ---------------------------------------------------------------------------
// ObjC Code Lens Provider
//
// Shows reference counts above methods/functions, protocol conformance on
// @implementation lines, and #pragma mark section labels.
// ---------------------------------------------------------------------------

/** Data attached to unresolved code lenses for deferred reference counting. */
interface RefLensData {
  kind: "references";
  uri: vscode.Uri;
  position: vscode.Position;
}

interface StaticLensData {
  kind: "static";
}

type LensData = RefLensData | StaticLensData;

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

class ObjCCodeLensProvider implements vscode.CodeLensProvider {
  private _onDidChange = new vscode.EventEmitter<void>();
  readonly onDidChangeCodeLenses = this._onDidChange.event;

  /** Force refresh (e.g. after config change). */
  refresh(): void {
    this._onDidChange.fire();
  }

  provideCodeLenses(
    document: vscode.TextDocument,
    _token: vscode.CancellationToken
  ): vscode.CodeLens[] {
    const config = vscode.workspace.getConfiguration("objc-lsp");
    if (!config.get<boolean>("enableCodeLens", true)) {
      return [];
    }

    const lenses: vscode.CodeLens[] = [];

    // Build a map of class → protocols from @interface declarations.
    const protocolMap = buildProtocolMap(document);

    for (let i = 0; i < document.lineCount; i++) {
      const line = document.lineAt(i);
      const text = line.text;

      // ── Method / function declarations ───────────────────────────────
      // ObjC methods: - (type)name... or + (type)name...  (at start of line)
      // C functions: type name(...)  { (simplified)
      const methodMatch = text.match(
        /^[+-]\s*\([^)]+\)\s*(\w+)/
      );
      if (methodMatch) {
        const nameStart = text.indexOf(methodMatch[1]);
        const pos = new vscode.Position(i, nameStart >= 0 ? nameStart : 0);
        const range = new vscode.Range(pos, pos);
        const lens = new vscode.CodeLens(range);
        (lens as vscode.CodeLens & { data: LensData }).data = {
          kind: "references",
          uri: document.uri,
          position: pos,
        };
        lenses.push(lens);
        continue;
      }

      // ── @implementation protocol conformance ─────────────────────────
      const implMatch = text.match(/^@implementation\s+(\w+)/);
      if (implMatch) {
        const className = implMatch[1];
        const protocols = protocolMap.get(className);
        if (protocols && protocols.length > 0) {
          const range = new vscode.Range(
            new vscode.Position(i, 0),
            new vscode.Position(i, 0)
          );
          lenses.push(
            new vscode.CodeLens(range, {
              title: `$(symbol-interface) Conforms to: ${protocols.join(", ")}`,
              command: "",
            })
          );
        }
        continue;
      }

      // ── #pragma mark ────────────────────────────────────────────────
      const pragmaMatch = text.match(/^#pragma\s+mark\s+[-–—]?\s*(.*)/);
      if (pragmaMatch) {
        const label = pragmaMatch[1].trim();
        if (label) {
          const range = new vscode.Range(
            new vscode.Position(i, 0),
            new vscode.Position(i, 0)
          );
          lenses.push(
            new vscode.CodeLens(range, {
              title: `── ${label} ──`,
              command: "",
            })
          );
        }
      }
    }

    return lenses;
  }

  async resolveCodeLens(
    codeLens: vscode.CodeLens,
    token: vscode.CancellationToken
  ): Promise<vscode.CodeLens> {
    const data = (codeLens as vscode.CodeLens & { data?: LensData }).data;
    if (!data || data.kind !== "references") {
      return codeLens;
    }

    if (token.isCancellationRequested) {
      return codeLens;
    }

    try {
      const locations = await vscode.commands.executeCommand<vscode.Location[]>(
        "vscode.executeReferenceProvider",
        data.uri,
        data.position
      );
      const count = locations ? locations.length : 0;
      codeLens.command = {
        title: `$(references) ${count} reference${count !== 1 ? "s" : ""}`,
        command: count > 0 ? "editor.action.referenceSearch.trigger" : "",
      };
    } catch {
      codeLens.command = {
        title: "$(references) ? references",
        command: "",
      };
    }

    return codeLens;
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Scan the document for `@interface ClassName : Super <Proto1, Proto2>`
 * and build a map of class name → protocol list.
 */
function buildProtocolMap(
  document: vscode.TextDocument
): Map<string, string[]> {
  const map = new Map<string, string[]>();
  const regex = /@interface\s+(\w+)\s*(?::\s*\w+)?\s*<([^>]+)>/g;

  const text = document.getText();
  let match: RegExpExecArray | null;
  while ((match = regex.exec(text)) !== null) {
    const className = match[1];
    const protocols = match[2]
      .split(",")
      .map((p) => p.trim())
      .filter((p) => p.length > 0);
    if (protocols.length > 0) {
      // Merge in case multiple @interface declarations (categories, extensions)
      const existing = map.get(className) || [];
      const merged = [...new Set([...existing, ...protocols])];
      map.set(className, merged);
    }
  }

  return map;
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/** Register the Code Lens provider. Call from `activate()`. */
export function registerCodeLens(context: vscode.ExtensionContext): void {
  const selector: vscode.DocumentSelector = [
    { language: "objective-c", scheme: "file" },
    { language: "objective-cpp", scheme: "file" },
  ];

  const provider = new ObjCCodeLensProvider();
  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider(selector, provider)
  );

  // Refresh code lenses when config changes.
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("objc-lsp.enableCodeLens")) {
        provider.refresh();
      }
    })
  );
}

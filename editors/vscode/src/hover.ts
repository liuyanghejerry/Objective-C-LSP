import * as vscode from "vscode";

// ---------------------------------------------------------------------------
// ObjC Hover Extension Provider
//
// Supplements the LSP hover (which provides signatures + doc comments) with
// extension-side information:
//   1. Related methods — other methods in the same @implementation block
//   2. API availability — parsed from API_AVAILABLE / NS_AVAILABLE macros
//   3. Deprecation info — parsed from NS_DEPRECATED / __deprecated_msg
//   4. Quick Fix links — command links for relevant quick-fix actions
// ---------------------------------------------------------------------------

// ── Regex patterns ──────────────────────────────────────────────────────────

/** Match ObjC method declaration/definition lines. */
const METHOD_RE = /^[+-]\s*\([^)]+\)\s*(\w[\w:]*)/;

/** Match @implementation ClassName */
const IMPL_START_RE = /^@implementation\s+(\w+)/;

/** Match @end */
const IMPL_END_RE = /^@end\b/;

/**
 * Match API availability macros.
 *
 * Covers:
 *   API_AVAILABLE(ios(14.0), macos(11.0))
 *   NS_AVAILABLE_IOS(8_0)
 *   NS_AVAILABLE(10_10, 8_0)
 *   __TVOS_AVAILABLE(13_0)
 *   __IOS_AVAILABLE(11_0)
 */
const API_AVAILABLE_RE =
  /(?:API_AVAILABLE|NS_AVAILABLE(?:_IOS)?|__(?:IOS|TVOS|WATCHOS)_AVAILABLE)\s*\(([^)]+)\)/;

/**
 * Match deprecation macros.
 *
 * Covers:
 *   NS_DEPRECATED_IOS(2_0, 9_0, "Use X instead")
 *   NS_DEPRECATED(10_0, 10_6, 2_0, 9_0)
 *   __deprecated_msg("Use something else")
 *   DEPRECATED_ATTRIBUTE
 *   __attribute__((deprecated("msg")))
 */
const DEPRECATED_RE =
  /(?:NS_DEPRECATED(?:_IOS)?\s*\(([^)]+)\)|__deprecated_msg\s*\(\s*"([^"]+)"\s*\)|DEPRECATED_ATTRIBUTE|__attribute__\s*\(\s*\(\s*deprecated\s*(?:\(\s*"([^"]+)"\s*\))?\s*\)\s*\))/;

/** Match @property declarations — used to detect property hover for quick-fix links. */
const PROPERTY_RE = /^@property\s*\([^)]*\)\s+\w+/;

/** Match ivar-like declarations for addProperty quick-fix link. */
const IVAR_RE = /^\s+\w+\s*\*?\s*_\w+\s*;/;

/** Match `self` references inside blocks — hint for retain-cycle fix. */
const SELF_IN_BLOCK_RE = /\^\s*(?:\([^)]*\))?\s*\{[^}]*\bself\b/;

// ── Parsed structures ───────────────────────────────────────────────────────

interface MethodInfo {
  name: string;
  line: number;
  isClassMethod: boolean;
}

interface ImplementationBlock {
  className: string;
  startLine: number;
  endLine: number; // inclusive, line of @end
  methods: MethodInfo[];
}

interface AvailabilityInfo {
  raw: string;
  platforms: { platform: string; version: string }[];
}

interface DeprecationInfo {
  raw: string;
  message?: string;
}

// ── Provider ────────────────────────────────────────────────────────────────

class ObjCHoverProvider implements vscode.HoverProvider {
  provideHover(
    document: vscode.TextDocument,
    position: vscode.Position,
    _token: vscode.CancellationToken
  ): vscode.Hover | null {
    const config = vscode.workspace.getConfiguration("objc-lsp");
    if (!config.get<boolean>("enableHoverExtensions", true)) {
      return null;
    }

    const line = document.lineAt(position.line);
    const lineText = line.text;

    const parts: vscode.MarkdownString[] = [];

    // ── 1. API Availability ───────────────────────────────────────────────
    const availability = this.parseAvailability(document, position.line);
    if (availability) {
      const md = new vscode.MarkdownString();
      md.supportThemeIcons = true;
      md.appendMarkdown(
        `**$(versions) API Availability**\n\n`
      );
      for (const p of availability.platforms) {
        md.appendMarkdown(`- \`${p.platform}\` ${p.version}+\n`);
      }
      parts.push(md);
    }

    // ── 2. Deprecation ────────────────────────────────────────────────────
    const deprecation = this.parseDeprecation(document, position.line);
    if (deprecation) {
      const md = new vscode.MarkdownString();
      md.supportThemeIcons = true;
      md.appendMarkdown(
        `**$(warning) Deprecated**`
      );
      if (deprecation.message) {
        md.appendMarkdown(`: ${deprecation.message}`);
      }
      md.appendMarkdown(`\n\n\`${deprecation.raw}\``);
      parts.push(md);
    }

    // ── 3. Related Methods ────────────────────────────────────────────────
    const methodMatch = lineText.match(METHOD_RE);
    if (methodMatch) {
      const implBlock = this.findEnclosingImplementation(
        document,
        position.line
      );
      if (implBlock && implBlock.methods.length > 1) {
        const currentMethod = methodMatch[1];
        const siblings = implBlock.methods.filter(
          (m) => m.name !== currentMethod
        );
        if (siblings.length > 0) {
          const md = new vscode.MarkdownString();
          md.isTrusted = true;
          md.supportThemeIcons = true;
          md.appendMarkdown(
            `**$(symbol-method) Related Methods** in \`${implBlock.className}\` (${implBlock.methods.length} total)\n\n`
          );
          // Show up to 15 sibling methods with go-to links
          const shown = siblings.slice(0, 15);
          for (const m of shown) {
            const prefix = m.isClassMethod ? "+" : "-";
            const lineUri = document.uri.toString();
            const targetPos = { lineNumber: m.line + 1, column: 1 };
            const targetLoc = {
              uri: lineUri,
              range: {
                startLineNumber: m.line + 1,
                startColumn: 1,
                endLineNumber: m.line + 1,
                endColumn: 1,
              },
            };
            const args = encodeURIComponent(
              JSON.stringify([lineUri, targetPos, [targetLoc]])
            );
            md.appendMarkdown(
              `- [\`${prefix}${m.name}\`](command:editor.action.goToLocations?${args} "Go to method")\n`
            );
          }
          if (siblings.length > 15) {
            md.appendMarkdown(
              `\n_...and ${siblings.length - 15} more_\n`
            );
          }
          parts.push(md);
        }
      }
    }

    // ── 4. Quick Fix Links ────────────────────────────────────────────────
    const quickFixes = this.getQuickFixLinks(document, position);
    if (quickFixes.length > 0) {
      const md = new vscode.MarkdownString();
      md.isTrusted = true;
      md.supportThemeIcons = true;
      md.appendMarkdown(`**$(lightbulb) Quick Fixes**\n\n`);
      for (const qf of quickFixes) {
        md.appendMarkdown(`- [${qf.title}](command:${qf.command}) \n`);
      }
      parts.push(md);
    }

    if (parts.length === 0) {
      return null;
    }

    return new vscode.Hover(parts);
  }

  // ── Availability parsing ──────────────────────────────────────────────

  /**
   * Look for API_AVAILABLE / NS_AVAILABLE macros on the hovered line
   * and the preceding line (macros are sometimes on the line above).
   */
  private parseAvailability(
    document: vscode.TextDocument,
    line: number
  ): AvailabilityInfo | null {
    // Check current line + up to 2 preceding lines (macros can appear above)
    for (let i = line; i >= Math.max(0, line - 2); i--) {
      const text = document.lineAt(i).text;
      const match = text.match(API_AVAILABLE_RE);
      if (match) {
        return this.parseAvailabilityArgs(match[1], match[0]);
      }
    }
    return null;
  }

  /**
   * Parse the arguments of an availability macro.
   *
   * API_AVAILABLE(ios(14.0), macos(11.0))
   *   → [{ platform: "iOS", version: "14.0" }, { platform: "macOS", version: "11.0" }]
   *
   * NS_AVAILABLE_IOS(8_0)
   *   → [{ platform: "iOS", version: "8.0" }]
   */
  private parseAvailabilityArgs(
    args: string,
    raw: string
  ): AvailabilityInfo {
    const platforms: { platform: string; version: string }[] = [];

    // Try platform(version) style: ios(14.0), macos(11.0)
    const platformVersionRe = /(\w+)\s*\(\s*([0-9_.]+)\s*\)/g;
    let m: RegExpExecArray | null;
    while ((m = platformVersionRe.exec(args)) !== null) {
      platforms.push({
        platform: normalizePlatformName(m[1]),
        version: m[2].replace(/_/g, "."),
      });
    }

    // If no platform(version) match, try bare version: NS_AVAILABLE_IOS(8_0)
    if (platforms.length === 0) {
      const bareVersion = args.trim().replace(/_/g, ".");
      if (/^[0-9.]+$/.test(bareVersion)) {
        // Infer platform from macro name
        let platform = "iOS";
        if (raw.includes("TVOS")) {
          platform = "tvOS";
        } else if (raw.includes("WATCHOS")) {
          platform = "watchOS";
        } else if (raw.includes("macos") || raw.includes("MACOS")) {
          platform = "macOS";
        }
        platforms.push({ platform, version: bareVersion });
      }
      // NS_AVAILABLE(macos_ver, ios_ver) — two comma-separated bare versions
      const twoParts = args.split(",").map((s) => s.trim().replace(/_/g, "."));
      if (
        twoParts.length === 2 &&
        /^[0-9.]+$/.test(twoParts[0]) &&
        /^[0-9.]+$/.test(twoParts[1])
      ) {
        // NS_AVAILABLE(macOS_version, iOS_version)
        platforms.length = 0; // clear the single one above
        platforms.push({ platform: "macOS", version: twoParts[0] });
        platforms.push({ platform: "iOS", version: twoParts[1] });
      }
    }

    return { raw, platforms };
  }

  // ── Deprecation parsing ───────────────────────────────────────────────

  /**
   * Check current line and up to 2 preceding lines for deprecation macros.
   */
  private parseDeprecation(
    document: vscode.TextDocument,
    line: number
  ): DeprecationInfo | null {
    for (let i = line; i >= Math.max(0, line - 2); i--) {
      const text = document.lineAt(i).text;
      const match = text.match(DEPRECATED_RE);
      if (match) {
        // Extract message from whichever group matched
        const message = match[2] || match[3] || this.extractDeprecationMessage(match[1]);
        return { raw: match[0], message: message || undefined };
      }
    }
    return null;
  }

  /**
   * Try to extract a deprecation message from NS_DEPRECATED_IOS(from, to, "msg")
   * where the first group captured the full argument list.
   */
  private extractDeprecationMessage(args: string | undefined): string | null {
    if (!args) {
      return null;
    }
    const msgMatch = args.match(/"([^"]+)"/);
    return msgMatch ? msgMatch[1] : null;
  }

  // ── Related methods ───────────────────────────────────────────────────

  /**
   * Find the @implementation ... @end block enclosing the given line,
   * and collect all method definitions within it.
   */
  private findEnclosingImplementation(
    document: vscode.TextDocument,
    line: number
  ): ImplementationBlock | null {
    // Search backward for @implementation
    let implLine = -1;
    let className = "";
    for (let i = line; i >= 0; i--) {
      const text = document.lineAt(i).text;
      // If we hit @end before @implementation, we're not inside a block
      if (i < line && IMPL_END_RE.test(text)) {
        return null;
      }
      const match = text.match(IMPL_START_RE);
      if (match) {
        implLine = i;
        className = match[1];
        break;
      }
    }
    if (implLine < 0) {
      return null;
    }

    // Search forward for @end
    let endLine = document.lineCount - 1;
    for (let i = implLine + 1; i < document.lineCount; i++) {
      if (IMPL_END_RE.test(document.lineAt(i).text)) {
        endLine = i;
        break;
      }
    }

    // Collect all methods in this block
    const methods: MethodInfo[] = [];
    for (let i = implLine + 1; i < endLine; i++) {
      const text = document.lineAt(i).text;
      const methodMatch = text.match(METHOD_RE);
      if (methodMatch) {
        methods.push({
          name: methodMatch[1],
          line: i,
          isClassMethod: text.trimStart().startsWith("+"),
        });
      }
    }

    return { className, startLine: implLine, endLine, methods };
  }

  // ── Quick Fix links ───────────────────────────────────────────────────

  /**
   * Return relevant quick-fix command links for the hovered line.
   */
  private getQuickFixLinks(
    document: vscode.TextDocument,
    position: vscode.Position
  ): { title: string; command: string }[] {
    const lineText = document.lineAt(position.line).text;
    const links: { title: string; command: string }[] = [];

    // Ivar → @property conversion
    if (IVAR_RE.test(lineText)) {
      links.push({
        title: "$(symbol-property) Generate @property",
        command: "objc-lsp.addProperty",
      });
    }

    // @property → @synthesize
    if (PROPERTY_RE.test(lineText)) {
      links.push({
        title: "$(gear) Add @synthesize",
        command: "objc-lsp.addSynthesize",
      });
    }

    // Block with self → retain cycle fix
    // Look at current line + a few surrounding lines for block-with-self pattern
    const searchStart = Math.max(0, position.line - 2);
    const searchEnd = Math.min(document.lineCount - 1, position.line + 5);
    const regionText = document.getText(
      new vscode.Range(searchStart, 0, searchEnd, 999)
    );
    if (SELF_IN_BLOCK_RE.test(regionText)) {
      links.push({
        title: "$(shield) Fix Retain Cycle",
        command: "objc-lsp.fixRetainCycle",
      });
    }

    // Method on hover → extract method (useful when selecting within a method)
    if (METHOD_RE.test(lineText)) {
      links.push({
        title: "$(split-horizontal) Extract Method",
        command: "objc-lsp.extractMethod",
      });
    }

    return links;
  }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/** Normalize lowercase platform identifiers to display names. */
function normalizePlatformName(name: string): string {
  const map: Record<string, string> = {
    ios: "iOS",
    macos: "macOS",
    tvos: "tvOS",
    watchos: "watchOS",
    visionos: "visionOS",
    maccatalyst: "Mac Catalyst",
  };
  return map[name.toLowerCase()] || name;
}

// ── Registration ────────────────────────────────────────────────────────────

/** Register the Hover Extension provider. Call from `activate()`. */
export function registerHoverExtensions(
  context: vscode.ExtensionContext
): void {
  const selector: vscode.DocumentSelector = [
    { language: "objective-c", scheme: "file" },
    { language: "objective-cpp", scheme: "file" },
  ];

  const provider = new ObjCHoverProvider();
  context.subscriptions.push(
    vscode.languages.registerHoverProvider(selector, provider)
  );
}

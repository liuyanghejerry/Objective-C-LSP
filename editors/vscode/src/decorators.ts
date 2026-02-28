import * as vscode from "vscode";

// ---------------------------------------------------------------------------
// ObjC Text Decorators
//
// Inline decorations for common ObjC pitfalls:
//   1. Retain cycle warnings (self in block)
//   2. Strong delegate warnings
//   3. Magic number highlights
// ---------------------------------------------------------------------------

// ── Decoration types ──────────────────────────────────────────────────────

const retainCycleDecoration = vscode.window.createTextEditorDecorationType({
  backgroundColor: "rgba(255, 200, 0, 0.15)",
  border: "1px solid rgba(255, 200, 0, 0.4)",
  borderRadius: "2px",
  overviewRulerColor: "rgba(255, 200, 0, 0.7)",
  overviewRulerLane: vscode.OverviewRulerLane.Right,
  after: {
    contentText: " ⚠️ retain cycle",
    color: "rgba(255, 180, 0, 0.7)",
    fontStyle: "italic",
    margin: "0 0 0 1em",
  },
});

const strongDelegateDecoration = vscode.window.createTextEditorDecorationType({
  textDecoration: "underline wavy rgba(255, 140, 0, 0.6)",
  overviewRulerColor: "rgba(255, 140, 0, 0.7)",
  overviewRulerLane: vscode.OverviewRulerLane.Right,
});

const magicNumberDecoration = vscode.window.createTextEditorDecorationType({
  textDecoration: "underline dotted rgba(80, 160, 255, 0.5)",
  overviewRulerColor: "rgba(80, 160, 255, 0.4)",
  overviewRulerLane: vscode.OverviewRulerLane.Left,
});

// ── Debounce timer ────────────────────────────────────────────────────────

let updateTimer: ReturnType<typeof setTimeout> | undefined;
const DEBOUNCE_MS = 500;

// ── Analysis functions ────────────────────────────────────────────────────

/**
 * Find `self` references inside block literals that are not already
 * weakSelf/strongSelf patterns.
 */
function findRetainCycles(
  document: vscode.TextDocument
): vscode.DecorationOptions[] {
  const decorations: vscode.DecorationOptions[] = [];
  const text = document.getText();

  // Find block starts: ^{ or ^(...) {
  // We use a simple brace-depth tracker after each block start.
  const blockStartRegex = /\^\s*(?:\([^)]*\))?\s*\{/g;
  let blockMatch: RegExpExecArray | null;

  while ((blockMatch = blockStartRegex.exec(text)) !== null) {
    const blockOpenIdx = text.indexOf("{", blockMatch.index);
    if (blockOpenIdx < 0) {
      continue;
    }

    // Find the matching closing brace.
    const blockEnd = findMatchingBrace(text, blockOpenIdx);
    if (blockEnd < 0) {
      continue;
    }

    // Search for bare `self` within the block body.
    const blockBody = text.substring(blockOpenIdx + 1, blockEnd);
    const selfRegex = /\bself\b/g;
    let selfMatch: RegExpExecArray | null;

    while ((selfMatch = selfRegex.exec(blockBody)) !== null) {
      const absOffset = blockOpenIdx + 1 + selfMatch.index;

      // Skip if preceded by "weak" or "strong" (i.e., weakSelf, strongSelf).
      const before = text.substring(Math.max(0, absOffset - 10), absOffset);
      if (/(?:weak|strong)$/i.test(before.trimEnd())) {
        continue;
      }

      const startPos = document.positionAt(absOffset);
      const endPos = document.positionAt(absOffset + 4); // "self".length

      // Check if there's already a weakSelf declaration before this block.
      const linesBefore = text.substring(
        Math.max(0, blockMatch.index - 200),
        blockMatch.index
      );
      if (/weakSelf\s*=\s*self/.test(linesBefore)) {
        continue; // Already handled
      }

      decorations.push({
        range: new vscode.Range(startPos, endPos),
        hoverMessage: new vscode.MarkdownString(
          "⚠️ **Possible retain cycle**: `self` captured strongly in block.\n\n" +
            "Consider using `__weak typeof(self) weakSelf = self;` before the block."
        ),
      });
    }
  }

  return decorations;
}

/**
 * Find @property declarations where delegate/dataSource uses strong.
 */
function findStrongDelegates(
  document: vscode.TextDocument
): vscode.DecorationOptions[] {
  const decorations: vscode.DecorationOptions[] = [];

  for (let i = 0; i < document.lineCount; i++) {
    const line = document.lineAt(i);
    const text = line.text;

    // Match: @property (... strong ...) SomeType *delegate;
    // or @property (... strong ...) id<SomeProtocol> dataSource;
    const match = text.match(
      /@property\s*\(([^)]*)\)\s+\S+.*?\b(delegate|dataSource)\b/i
    );
    if (!match) {
      continue;
    }

    const attributes = match[1].toLowerCase();
    // Only flag if explicitly `strong` or has `retain` (legacy),
    // but NOT `weak` or `assign` or `unsafe_unretained`
    const hasStrong =
      attributes.includes("strong") || attributes.includes("retain");
    const hasWeak =
      attributes.includes("weak") ||
      attributes.includes("assign") ||
      attributes.includes("unsafe_unretained");

    if (!hasStrong || hasWeak) {
      continue;
    }

    const delegateIdx = text.indexOf(match[2]);
    if (delegateIdx < 0) {
      continue;
    }

    decorations.push({
      range: new vscode.Range(
        new vscode.Position(i, delegateIdx),
        new vscode.Position(i, delegateIdx + match[2].length)
      ),
      hoverMessage: new vscode.MarkdownString(
        "⚠️ **Strong delegate**: `" +
          match[2] +
          "` property uses `strong`. " +
          "This typically causes retain cycles.\n\n" +
          "Use `weak` instead: `@property (nonatomic, weak) ...`"
      ),
    });
  }

  return decorations;
}

/**
 * Find hardcoded magic numbers in method bodies.
 *
 * Skips: 0, 1, 2, -1, numbers in #define/enum/const/static const,
 * array subscripts, and common framework constants.
 */
function findMagicNumbers(
  document: vscode.TextDocument
): vscode.DecorationOptions[] {
  const decorations: vscode.DecorationOptions[] = [];
  let inMethodBody = false;

  for (let i = 0; i < document.lineCount; i++) {
    const line = document.lineAt(i);
    const text = line.text;
    const trimmed = text.trimStart();

    // Track method body boundaries.
    if (/^[+-]\s*\([^)]+\)/.test(trimmed)) {
      inMethodBody = true;
      continue;
    }
    if (trimmed === "@end" || trimmed === "@implementation" || trimmed.startsWith("@interface")) {
      inMethodBody = false;
      continue;
    }

    if (!inMethodBody) {
      continue;
    }

    // Skip preprocessor, const, enum, static lines.
    if (
      /^\s*#/.test(text) ||
      /\bconst\b/.test(text) ||
      /\benum\b/.test(text) ||
      /\bstatic\s+/.test(text) ||
      /^\s*\/\//.test(text) ||
      /^\s*\*/.test(text) ||
      /^\s*case\b/.test(text) ||
      /return\s/.test(text)
    ) {
      continue;
    }

    // Find numeric literals (integer and float).
    const numRegex = /\b(\d+(?:\.\d+)?f?)\b/g;
    let numMatch: RegExpExecArray | null;

    while ((numMatch = numRegex.exec(text)) !== null) {
      const numStr = numMatch[1].replace(/f$/, "");
      const num = parseFloat(numStr);

      // Skip small common values.
      if (num >= -1 && num <= 2) {
        continue;
      }

      // Skip if inside array subscript: [123]
      const charBefore = numMatch.index > 0 ? text[numMatch.index - 1] : "";
      const charAfter =
        numMatch.index + numMatch[0].length < text.length
          ? text[numMatch.index + numMatch[0].length]
          : "";
      if (charBefore === "[" || charAfter === "]") {
        continue;
      }

      // Skip if part of a string literal (rough check).
      const beforeNum = text.substring(0, numMatch.index);
      const quoteCount = (beforeNum.match(/@?"/g) || []).length;
      if (quoteCount % 2 !== 0) {
        continue;
      }

      decorations.push({
        range: new vscode.Range(
          new vscode.Position(i, numMatch.index),
          new vscode.Position(i, numMatch.index + numMatch[0].length)
        ),
        hoverMessage: new vscode.MarkdownString(
          "💡 **Magic number**: Consider extracting `" +
            numStr +
            "` to a named constant for readability."
        ),
      });
    }
  }

  return decorations;
}

// ── Brace matching helper ─────────────────────────────────────────────────

/** Find the index of the closing brace matching the opening brace at `pos`. */
function findMatchingBrace(text: string, pos: number): number {
  let depth = 0;
  for (let i = pos; i < text.length; i++) {
    if (text[i] === "{") {
      depth++;
    } else if (text[i] === "}") {
      depth--;
      if (depth === 0) {
        return i;
      }
    }
  }
  return -1;
}

// ── Update trigger ────────────────────────────────────────────────────────

function updateDecorations(editor: vscode.TextEditor): void {
  const config = vscode.workspace.getConfiguration("objc-lsp");
  if (!config.get<boolean>("enableDecorators", true)) {
    editor.setDecorations(retainCycleDecoration, []);
    editor.setDecorations(strongDelegateDecoration, []);
    editor.setDecorations(magicNumberDecoration, []);
    return;
  }

  const lang = editor.document.languageId;
  if (lang !== "objective-c" && lang !== "objective-cpp") {
    return;
  }

  editor.setDecorations(retainCycleDecoration, findRetainCycles(editor.document));
  editor.setDecorations(strongDelegateDecoration, findStrongDelegates(editor.document));
  editor.setDecorations(magicNumberDecoration, findMagicNumbers(editor.document));
}

function scheduleUpdate(editor: vscode.TextEditor | undefined): void {
  if (updateTimer) {
    clearTimeout(updateTimer);
  }
  if (!editor) {
    return;
  }
  updateTimer = setTimeout(() => updateDecorations(editor), DEBOUNCE_MS);
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/** Register text decorators. Call from `activate()`. */
export function registerDecorators(context: vscode.ExtensionContext): void {
  // Apply on active editor change.
  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor((editor) => {
      if (editor) {
        scheduleUpdate(editor);
      }
    })
  );

  // Apply on document edit (debounced).
  context.subscriptions.push(
    vscode.workspace.onDidChangeTextDocument((event) => {
      const editor = vscode.window.activeTextEditor;
      if (editor && event.document === editor.document) {
        scheduleUpdate(editor);
      }
    })
  );

  // Refresh when config changes.
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("objc-lsp.enableDecorators")) {
        const editor = vscode.window.activeTextEditor;
        if (editor) {
          updateDecorations(editor);
        }
      }
    })
  );

  // Dispose decoration types on deactivation.
  context.subscriptions.push(retainCycleDecoration);
  context.subscriptions.push(strongDelegateDecoration);
  context.subscriptions.push(magicNumberDecoration);

  // Initial run for the currently active editor.
  if (vscode.window.activeTextEditor) {
    scheduleUpdate(vscode.window.activeTextEditor);
  }
}

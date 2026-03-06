import * as vscode from "vscode";

// ---------------------------------------------------------------------------
// ObjC Quick Fix Commands
//
// Pure extension-side text transformations registered via
// `commands.registerCommand`. Each operates on the active TextEditor.
// ---------------------------------------------------------------------------

/**
 * addProperty — Convert a selected ivar declaration to a @property.
 *
 * Matches patterns like:
 *   ClassName *_name;
 *   NSString *_title;
 *   NSInteger _count;
 *
 * Generates:
 *   @property (nonatomic, strong) ClassName *name;
 *   @property (nonatomic, copy) NSString *title;
 *   @property (nonatomic, assign) NSInteger count;
 */
async function addProperty(editor: vscode.TextEditor): Promise<void> {
  const selection = editor.selection;
  const line = editor.document.lineAt(selection.active.line);
  const text = line.text.trim();

  // Match: Type *_name;  or  Type _name;  (with optional leading underscore)
  const pointerMatch = text.match(
    /^(\w+)\s*\*\s*_?(\w+)\s*;/
  );
  const valueMatch = text.match(
    /^(\w+)\s+_?(\w+)\s*;/
  );

  const match = pointerMatch || valueMatch;
  if (!match) {
    vscode.window.showWarningMessage(
      "Place cursor on an ivar line like: NSString *_name;"
    );
    return;
  }

  const typeName = match[1];
  const varName = match[2];
  const isPointer = !!pointerMatch;

  // Infer memory attribute from type
  const attribute = inferAttribute(typeName, isPointer);
  const star = isPointer ? " *" : " ";

  const property = `@property (nonatomic, ${attribute}) ${typeName}${star}${varName};`;

  await editor.edit((editBuilder) => {
    editBuilder.replace(line.range, property);
  });
}

/** Infer property attribute based on ObjC type conventions. */
export function inferAttribute(typeName: string, isPointer: boolean): string {
  if (!isPointer) {
    return "assign";
  }
  // Types that should use `copy`
  const copyTypes = [
    "NSString",
    "NSMutableString",
    "NSArray",
    "NSMutableArray",
    "NSDictionary",
    "NSMutableDictionary",
    "NSSet",
    "NSMutableSet",
    "NSNumber",
    "NSData",
    "NSAttributedString",
    "NSMutableAttributedString",
  ];
  if (copyTypes.includes(typeName)) {
    return "copy";
  }
  // Delegate/dataSource patterns → weak
  if (/delegate|datasource/i.test(typeName)) {
    return "weak";
  }
  return "strong";
}

/**
 * wrapAutoreleasepool — Wrap the selected lines in @autoreleasepool { ... }.
 */
async function wrapAutoreleasepool(editor: vscode.TextEditor): Promise<void> {
  const selection = editor.selection;
  if (selection.isEmpty) {
    vscode.window.showWarningMessage(
      "Select the code you want to wrap in @autoreleasepool."
    );
    return;
  }
  await wrapSelection(editor, selection, "@autoreleasepool {", "}");
}

/**
 * wrapDispatchAsync — Wrap the selected lines in
 * dispatch_async(dispatch_get_main_queue(), ^{ ... });
 */
async function wrapDispatchAsync(editor: vscode.TextEditor): Promise<void> {
  const selection = editor.selection;
  if (selection.isEmpty) {
    vscode.window.showWarningMessage(
      "Select the code you want to wrap in dispatch_async."
    );
    return;
  }
  await wrapSelection(
    editor,
    selection,
    "dispatch_async(dispatch_get_main_queue(), ^{",
    "});"
  );
}

/**
 * addSynthesize — Insert @synthesize for the @property on the current line.
 *
 * Reads the property name from the current line's @property declaration
 * and inserts `@synthesize name = _name;` after the @implementation line.
 */
async function addSynthesize(editor: vscode.TextEditor): Promise<void> {
  const line = editor.document.lineAt(editor.selection.active.line);
  const text = line.text.trim();

  // Match @property (...) Type *name; or @property (...) Type name;
  const match = text.match(
    /@property\s*\([^)]*\)\s+\w+\s*\*?\s*(\w+)\s*;/
  );
  if (!match) {
    vscode.window.showWarningMessage(
      "Place cursor on a @property line."
    );
    return;
  }
  const propName = match[1];
  const synthesize = `@synthesize ${propName} = _${propName};`;

  // Find the @implementation line to insert after it.
  const doc = editor.document;
  let insertLine = -1;
  for (let i = 0; i < doc.lineCount; i++) {
    if (doc.lineAt(i).text.trim().startsWith("@implementation")) {
      insertLine = i + 1;
      break;
    }
  }

  if (insertLine < 0) {
    // No @implementation found, insert below current line.
    insertLine = line.lineNumber + 1;
  }

  await editor.edit((editBuilder) => {
    editBuilder.insert(
      new vscode.Position(insertLine, 0),
      synthesize + "\n"
    );
  });
}

/**
 * fixRetainCycle — Insert weakSelf/strongSelf boilerplate.
 *
 * If text is selected, wraps it with the weakSelf/strongSelf pattern.
 * Otherwise, inserts the weakSelf declaration at the cursor.
 */
async function fixRetainCycle(editor: vscode.TextEditor): Promise<void> {
  const selection = editor.selection;

  if (selection.isEmpty) {
    // Just insert weakSelf declaration at cursor.
    await editor.edit((editBuilder) => {
      editBuilder.insert(
        selection.active,
        "__weak typeof(self) weakSelf = self;\n"
      );
    });
    return;
  }

  // Wrap selected code in weakSelf/strongSelf pattern.
  const indent = getIndent(editor, selection.start.line);
  const selectedText = editor.document.getText(selection);

  // Replace `self` with `strongSelf` in the selected code.
  const fixed = selectedText.replace(/\bself\b/g, "strongSelf");

  const wrapped =
    `${indent}__weak typeof(self) weakSelf = self;\n` +
    `${indent}/* ... */ ^{\n` +
    `${indent}    __strong typeof(weakSelf) strongSelf = weakSelf;\n` +
    `${indent}    if (!strongSelf) return;\n` +
    `${indent}    ${fixed.trim()}\n` +
    `${indent}};`;

  await editor.edit((editBuilder) => {
    editBuilder.replace(selection, wrapped);
  });
}

/**
 * extractMethod — Extract the selected code into a new ObjC method.
 *
 * Prompts for the method name, then:
 * 1. Replaces the selection with a call to the new method.
 * 2. Appends the new method definition after the current method.
 */
async function extractMethod(editor: vscode.TextEditor): Promise<void> {
  const selection = editor.selection;
  if (selection.isEmpty) {
    vscode.window.showWarningMessage(
      "Select the code you want to extract into a method."
    );
    return;
  }

  const methodName = await vscode.window.showInputBox({
    prompt: "New method name (without return type or params)",
    placeHolder: "doSomething",
    validateInput: (value) => {
      if (!value || !/^[a-zA-Z_]\w*$/.test(value)) {
        return "Enter a valid Objective-C method name";
      }
      return undefined;
    },
  });

  if (!methodName) {
    return; // User cancelled.
  }

  const selectedText = editor.document.getText(selection);
  const indent = getIndent(editor, selection.start.line);

  // Replace selection with method call.
  const call = `${indent}[self ${methodName}];`;

  // Find end of current method (next line starting with `}` at column 0,
  // or end of file).
  let insertLine = editor.document.lineCount;
  for (
    let i = selection.end.line + 1;
    i < editor.document.lineCount;
    i++
  ) {
    const lineText = editor.document.lineAt(i).text;
    if (lineText.match(/^}/)) {
      insertLine = i + 1;
      break;
    }
  }

  const newMethod =
    `\n- (void)${methodName} {\n` +
    `    ${selectedText.trim()}\n` +
    `}\n`;

  await editor.edit((editBuilder) => {
    editBuilder.replace(selection, call);
    editBuilder.insert(
      new vscode.Position(insertLine, 0),
      newMethod
    );
  });
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Wrap a selection with open/close text, preserving indentation. */
async function wrapSelection(
  editor: vscode.TextEditor,
  selection: vscode.Selection,
  open: string,
  close: string
): Promise<void> {
  const indent = getIndent(editor, selection.start.line);
  const selectedText = editor.document.getText(selection);

  // Indent the selected text by one extra level.
  const indented = selectedText
    .split("\n")
    .map((line) => (line.trim() ? `    ${line}` : line))
    .join("\n");

  const wrapped = `${indent}${open}\n${indented}\n${indent}${close}`;

  await editor.edit((editBuilder) => {
    editBuilder.replace(selection, wrapped);
  });
}

/** Get the leading whitespace of a given line. */
function getIndent(editor: vscode.TextEditor, lineNumber: number): string {
  const line = editor.document.lineAt(lineNumber);
  const match = line.text.match(/^(\s*)/);
  return match ? match[1] : "";
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/** Register all Quick Fix commands. Call from `activate()`. */
export function registerCommands(
  context: vscode.ExtensionContext
): void {
  const commands: [string, (editor: vscode.TextEditor) => Promise<void>][] = [
    ["objc-lsp.addProperty", addProperty],
    ["objc-lsp.wrapAutoreleasepool", wrapAutoreleasepool],
    ["objc-lsp.wrapDispatchAsync", wrapDispatchAsync],
    ["objc-lsp.addSynthesize", addSynthesize],
    ["objc-lsp.fixRetainCycle", fixRetainCycle],
    ["objc-lsp.extractMethod", extractMethod],
  ];

  for (const [id, handler] of commands) {
    context.subscriptions.push(
      vscode.commands.registerTextEditorCommand(id, handler)
    );
  }
}

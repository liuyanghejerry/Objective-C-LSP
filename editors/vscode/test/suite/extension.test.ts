import * as assert from "assert";
import * as vscode from "vscode";

// ---------------------------------------------------------------------------
// Extension activation integration test
//
// These tests run inside the Extension Development Host, which actually loads
// and activates the extension.  They verify that the expected commands are
// registered and that the basic VS Code surface area is available — without
// requiring a live objc-lsp binary.
// ---------------------------------------------------------------------------

suite("Extension activation", () => {
  // Give the extension time to activate before the suite runs.
  suiteSetup(async () => {
    // Open a tiny .m document so the extension's activationEvents fire.
    const doc = await vscode.workspace.openTextDocument({
      content: "// test\n",
      language: "objective-c",
    });
    await vscode.window.showTextDocument(doc);
    // Allow async activation to settle.
    await new Promise((resolve) => setTimeout(resolve, 500));
  });

  test("Quick Fix commands are registered", async () => {
    const allCommands = await vscode.commands.getCommands(true);

    const expected = [
      "objc-lsp.addProperty",
      "objc-lsp.wrapAutoreleasepool",
      "objc-lsp.wrapDispatchAsync",
      "objc-lsp.addSynthesize",
      "objc-lsp.fixRetainCycle",
      "objc-lsp.extractMethod",
    ];

    for (const cmd of expected) {
      assert.ok(
        allCommands.includes(cmd),
        `Expected command "${cmd}" to be registered`
      );
    }
  });

  test("Utility commands are registered", async () => {
    const allCommands = await vscode.commands.getCommands(true);

    const expected = [
      "objc-lsp.restart",
      "objc-lsp.showOutput",
      "objc-lsp.reportIssue",
    ];

    for (const cmd of expected) {
      assert.ok(
        allCommands.includes(cmd),
        `Expected command "${cmd}" to be registered`
      );
    }
  });
});

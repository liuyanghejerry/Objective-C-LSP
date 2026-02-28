import * as vscode from "vscode";
import { startClient, stopClient, getClient, createStatusBar } from "./server";

export async function activate(
  context: vscode.ExtensionContext
): Promise<void> {
  // Status bar — visible for all ObjC files
  createStatusBar(context);

  // Start the language server
  await startClient(context);

  // ── Commands ──────────────────────────────────────────────────────────────

  context.subscriptions.push(
    vscode.commands.registerCommand("objc-lsp.restart", async () => {
      await stopClient();
      await startClient(context);
      vscode.window.showInformationMessage(
        "Objective-C Language Server restarted."
      );
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("objc-lsp.showOutput", () => {
      getClient()?.outputChannel.show();
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("objc-lsp.reportIssue", () => {
      vscode.env.openExternal(
        vscode.Uri.parse(
          "https://github.com/objc-lsp/objc-lsp/issues/new?template=bug_report.md"
        )
      );
    })
  );

  // ── Restart on server-path change ─────────────────────────────────────────

  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async (e) => {
      if (e.affectsConfiguration("objc-lsp")) {
        const choice = await vscode.window.showInformationMessage(
          "Objective-C LSP settings changed. Restart the language server to apply?",
          "Restart"
        );
        if (choice === "Restart") {
          await stopClient();
          await startClient(context);
        }
      }
    })
  );
}

export async function deactivate(): Promise<void> {
  await stopClient();
}

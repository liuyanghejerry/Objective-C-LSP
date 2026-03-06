import * as vscode from "vscode";
import {
  DocumentFormattingRequest,
} from "vscode-languageserver-protocol";
import { startClient, stopClient, getClient, createStatusBar } from "./server";
import { registerCommands } from "./commands";
import { registerCodeLens } from "./codelens";
import { registerDecorators } from "./decorators";
import { registerTreeViews } from "./treeviews";
import { registerCallGraph } from "./callgraph";
import { registerHoverExtensions } from "./hover";

export async function activate(
  context: vscode.ExtensionContext
): Promise<void> {
  // Status bar — visible for all ObjC files
  createStatusBar(context);

  // ── Commands ──────────────────────────────────────────────────────────────
  // Register server management commands first so they are always available.

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

  // ── Quick Fix commands ───────────────────────────────────────────────────
  // Pure text-transformation commands — no language server required.
  registerCommands(context);

  // ── Code Lens ──────────────────────────────────────────────────────────────
  // Regex-based provider — no language server required.
  registerCodeLens(context);

  // ── Decorators ─────────────────────────────────────────────────────────────
  // Regex-based decorations — no language server required.
  registerDecorators(context);

  // ── Tree Views ──────────────────────────────────────────────────────
  registerTreeViews(context);

  // ── Call Graph ─────────────────────────────────────────────────────────────
  registerCallGraph(context);

  // ── Hover Extensions ────────────────────────────────────────────────────
  registerHoverExtensions(context);

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

  // ── Language server ────────────────────────────────────────────────────────
  // Started after all UI features are registered so that a missing or
  // slow-to-start server binary never blocks command/lens/decorator setup.
  const client = await startClient(context);

  // ── Formatting provider ─────────────────────────────────────────────────
  // Register a DocumentFormattingEditProvider manually so VS Code
  // always sees us as a formatter, regardless of vscode-languageclient's
  // automatic capability registration.
  if (client) {
    const selector: vscode.DocumentSelector = [
      { language: "objective-c", scheme: "file" },
      { language: "objective-cpp", scheme: "file" },
    ];
    context.subscriptions.push(
      vscode.languages.registerDocumentFormattingEditProvider(selector, {
        async provideDocumentFormattingEdits(
          document: vscode.TextDocument,
          options: vscode.FormattingOptions,
          token: vscode.CancellationToken
        ): Promise<vscode.TextEdit[]> {
          const filesConfig = vscode.workspace.getConfiguration("files", document);
          const params = {
            textDocument: client.code2ProtocolConverter.asTextDocumentIdentifier(document),
            options: client.code2ProtocolConverter.asFormattingOptions(options, {
              trimTrailingWhitespace: filesConfig.get<boolean>("trimTrailingWhitespace"),
              trimFinalNewlines: filesConfig.get<boolean>("trimFinalNewlines"),
              insertFinalNewline: filesConfig.get<boolean>("insertFinalNewline"),
            }),
          };
          const result = await client.sendRequest(
            DocumentFormattingRequest.type,
            params,
            token
          );
          if (!result) {
            return [];
          }
          return await client.protocol2CodeConverter.asTextEdits(result, token) ?? [];
        },
      })
    );
  }
}

export async function deactivate(): Promise<void> {
  await stopClient();
}

import * as path from "path";
import * as fs from "fs";
import * as vscode from "vscode";

/**
 * Priority order for locating the objc-lsp binary:
 * 1. User's explicit setting: objc-lsp.serverPath
 * 2. Binary bundled alongside the extension (editors/vscode/bin/objc-lsp)
 * 3. PATH lookup
 */
export function findServerBinary(
  context: vscode.ExtensionContext
): string | undefined {
  const cfg = vscode.workspace.getConfiguration("objc-lsp");
  const explicit = cfg.get<string>("serverPath", "").trim();
  if (explicit && fs.existsSync(explicit)) {
    return explicit;
  }

  // Bundled binary (shipped inside the .vsix)
  const bundled = path.join(context.extensionPath, "bin", "objc-lsp");
  if (fs.existsSync(bundled)) {
    return bundled;
  }

  // PATH lookup
  const pathDirs = (process.env.PATH ?? "").split(path.delimiter);
  for (const dir of pathDirs) {
    const candidate = path.join(dir, "objc-lsp");
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  return undefined;
}

/**
 * Show a notification that objc-lsp was not found and offer remediation
 * actions.
 */
export async function promptInstall(): Promise<void> {
  const HOMEBREW = "Install via Homebrew";
  const SET_PATH = "Set Path Manually";

  const choice = await vscode.window.showErrorMessage(
    "objc-lsp binary not found. The Objective-C Language Server requires the `objc-lsp` binary to be installed.",
    HOMEBREW,
    SET_PATH
  );

  if (choice === HOMEBREW) {
    // Open terminal with install command
    const terminal = vscode.window.createTerminal("objc-lsp install");
    terminal.show();
    terminal.sendText("brew install objc-lsp", false);
    vscode.window.showInformationMessage(
      "Run the command in the terminal, then use 'ObjC: Restart Language Server' to activate."
    );
  } else if (choice === SET_PATH) {
    await vscode.commands.executeCommand(
      "workbench.action.openSettings",
      "objc-lsp.serverPath"
    );
  }
}

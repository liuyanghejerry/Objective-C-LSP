import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  State,
} from "vscode-languageclient/node";
import { findServerBinary, promptInstall } from "./install";
import { readConfig, buildInitializationOptions } from "./config";

/** Singleton client, recreated on restart. */
let client: LanguageClient | undefined;

/** Status bar item shared across the extension lifetime. */
let statusBar: vscode.StatusBarItem | undefined;

export async function startClient(
  context: vscode.ExtensionContext
): Promise<LanguageClient | undefined> {
  const config = readConfig();

  const serverBin = findServerBinary(context);
  if (!serverBin) {
    // Show the install prompt without blocking activation — the user can
    // interact with the notification asynchronously while extension features
    // (commands, code lens, decorators) that don't need the server are already
    // registered and functional.
    void promptInstall();
    return undefined;
  }

  const serverOptions: ServerOptions = {
    command: serverBin,
    args: ["--log-level", config.logLevel],
    options: {
      env: {
        ...process.env,
        // Ensure dyld can resolve libclang.dylib at runtime.
        // Using Xcode's bundled libclang (Apple clang 17) which correctly
        // handles iOS 26.2 / macOS 26 SDKs. Homebrew LLVM 22 conflicts.
        DYLD_LIBRARY_PATH: `/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib${process.env.DYLD_LIBRARY_PATH ? ':' + process.env.DYLD_LIBRARY_PATH : ''}`,
      },
    },
  };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { language: "objective-c", scheme: "file" },
      { language: "objective-cpp", scheme: "file" },
      // .h files: VS Code may assign them "c" language id by default,
      // so we also register by language "c" with .h pattern to ensure
      // header files get LSP features including formatting.
      { language: "c", scheme: "file", pattern: "**/*.h" },
    ],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher(
        "**/*.{m,mm,h,xcodeproj,compile_commands.json}"
      ),
    },
    initializationOptions: buildInitializationOptions(config),
    outputChannelName: "Objective-C LSP",
  };

  client = new LanguageClient(
    "objc-lsp",
    "Objective-C Language Server",
    serverOptions,
    clientOptions
  );

  // Wire up status bar to client state changes
  client.onDidChangeState((event) => {
    updateStatusBar(event.newState);
  });

  context.subscriptions.push(client);
  await client.start();
  return client;
}

export async function stopClient(): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
}

export function getClient(): LanguageClient | undefined {
  return client;
}

// ── Status bar ──────────────────────────────────────────────────────────────

export function createStatusBar(context: vscode.ExtensionContext): void {
  statusBar = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    10
  );
  statusBar.command = "objc-lsp.showOutput";
  statusBar.name = "Objective-C LSP";
  context.subscriptions.push(statusBar);
  setStatusBarIdle();
  statusBar.show();
}

function updateStatusBar(state: State): void {
  if (!statusBar) {
    return;
  }
  switch (state) {
    case State.Starting:
      statusBar.text = "$(sync~spin) ObjC LSP";
      statusBar.tooltip = "Objective-C LSP: starting…";
      statusBar.backgroundColor = undefined;
      break;
    case State.Running:
      statusBar.text = "$(check) ObjC LSP";
      statusBar.tooltip = "Objective-C LSP: ready";
      statusBar.backgroundColor = undefined;
      break;
    case State.Stopped:
      statusBar.text = "$(error) ObjC LSP";
      statusBar.tooltip = "Objective-C LSP: stopped";
      statusBar.backgroundColor = new vscode.ThemeColor(
        "statusBarItem.errorBackground"
      );
      break;
  }
}

function setStatusBarIdle(): void {
  if (!statusBar) {
    return;
  }
  statusBar.text = "$(circle-outline) ObjC LSP";
  statusBar.tooltip = "Objective-C LSP: not started";
  statusBar.backgroundColor = undefined;
}

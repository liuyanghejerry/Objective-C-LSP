import * as vscode from "vscode";

export interface ObjcLspConfig {
  serverPath: string;
  logLevel: "error" | "warn" | "info" | "debug";
  extraCompilerFlags: string[];
  enableNullabilityChecks: boolean;
  enableStaticAnalyzer: boolean;
}

export function readConfig(): ObjcLspConfig {
  const cfg = vscode.workspace.getConfiguration("objc-lsp");
  return {
    serverPath: cfg.get<string>("serverPath", ""),
    logLevel: cfg.get<"error" | "warn" | "info" | "debug">("logLevel", "info"),
    extraCompilerFlags: cfg.get<string[]>("extraCompilerFlags", []),
    enableNullabilityChecks: cfg.get<boolean>("enableNullabilityChecks", true),
    enableStaticAnalyzer: cfg.get<boolean>("enableStaticAnalyzer", false),
  };
}

/**
 * Build the initializationOptions object that is sent to objc-lsp during
 * the LSP initialize handshake.
 */
export function buildInitializationOptions(
  config: ObjcLspConfig
): Record<string, unknown> {
  return {
    logLevel: config.logLevel,
    extraCompilerFlags: config.extraCompilerFlags,
    enableNullabilityChecks: config.enableNullabilityChecks,
    enableStaticAnalyzer: config.enableStaticAnalyzer,
  };
}

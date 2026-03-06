import * as assert from "assert";
import * as path from "path";
import * as vscode from "vscode";
import { findServerBinary } from "../../src/install";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// Use require() to get the raw (writable) fs module — TypeScript's
// __importStar wraps every property as a getter, making direct assignment fail.
// eslint-disable-next-line @typescript-eslint/no-var-requires
const fsRaw: typeof import("fs") = require("fs");

/** Stub fs.existsSync for the duration of a callback. */
function withExistsStub(
  stub: (p: string) => boolean,
  fn: () => void
): void {
  const original = fsRaw.existsSync;
  fsRaw.existsSync = stub as typeof fsRaw.existsSync;
  try {
    fn();
  } finally {
    fsRaw.existsSync = original;
  }
}

/** Stub vscode.workspace.getConfiguration for the duration of a callback. */
function withConfigStub(
  values: Record<string, unknown>,
  fn: () => void
): void {
  const ws = vscode.workspace as unknown as Record<string, unknown>;
  const original = ws["getConfiguration"];
  ws["getConfiguration"] = (_section?: string) => ({
    get: <T>(key: string, defaultValue?: T): T => {
      return (key in values ? values[key] : defaultValue) as T;
    },
    has: (key: string) => key in values,
    inspect: () => undefined,
    update: async () => undefined,
  });
  try {
    fn();
  } finally {
    ws["getConfiguration"] = original;
  }
}

/** Minimal fake ExtensionContext — only extensionPath is used by findServerBinary. */
function fakeContext(extensionPath: string): vscode.ExtensionContext {
  return { extensionPath } as unknown as vscode.ExtensionContext;
}

// ---------------------------------------------------------------------------
// findServerBinary
// ---------------------------------------------------------------------------
suite("findServerBinary", () => {
  test("returns explicit serverPath when it exists on disk", () => {
    const explicitPath = "/custom/path/to/objc-lsp";
    withConfigStub({ serverPath: explicitPath }, () => {
      withExistsStub((p) => p === explicitPath, () => {
        const result = findServerBinary(fakeContext("/ext"));
        assert.strictEqual(result, explicitPath);
      });
    });
  });

  test("ignores explicit serverPath when it does NOT exist on disk", () => {
    const explicitPath = "/custom/path/to/objc-lsp";
    const bundledPath = path.join("/ext", "bin", "objc-lsp");
    withConfigStub({ serverPath: explicitPath }, () => {
      withExistsStub((p) => p === bundledPath, () => {
        const result = findServerBinary(fakeContext("/ext"));
        assert.strictEqual(result, bundledPath);
      });
    });
  });

  test("returns bundled binary when explicit path is empty and bundled exists", () => {
    const bundledPath = path.join("/ext", "bin", "objc-lsp");
    withConfigStub({ serverPath: "" }, () => {
      withExistsStub((p) => p === bundledPath, () => {
        const result = findServerBinary(fakeContext("/ext"));
        assert.strictEqual(result, bundledPath);
      });
    });
  });

  test("returns PATH binary when no explicit or bundled binary exists", () => {
    const pathDir = "/usr/local/bin";
    const pathBinary = path.join(pathDir, "objc-lsp");
    const originalPath = process.env.PATH;
    process.env.PATH = pathDir;
    withConfigStub({ serverPath: "" }, () => {
      withExistsStub((p) => p === pathBinary, () => {
        const result = findServerBinary(fakeContext("/ext"));
        assert.strictEqual(result, pathBinary);
      });
    });
    process.env.PATH = originalPath;
  });

  test("returns undefined when nothing is found", () => {
    const originalPath = process.env.PATH;
    process.env.PATH = "";
    withConfigStub({ serverPath: "" }, () => {
      withExistsStub(() => false, () => {
        const result = findServerBinary(fakeContext("/ext"));
        assert.strictEqual(result, undefined);
      });
    });
    process.env.PATH = originalPath;
  });
});

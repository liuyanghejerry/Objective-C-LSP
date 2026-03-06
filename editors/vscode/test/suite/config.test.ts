import * as assert from "assert";
import { buildInitializationOptions, ObjcLspConfig } from "../../src/config";

// ---------------------------------------------------------------------------
// buildInitializationOptions
// ---------------------------------------------------------------------------
suite("buildInitializationOptions", () => {
  function makeConfig(overrides: Partial<ObjcLspConfig> = {}): ObjcLspConfig {
    return {
      serverPath: "",
      logLevel: "info",
      extraCompilerFlags: [],
      enableNullabilityChecks: true,
      enableStaticAnalyzer: false,
      ...overrides,
    };
  }

  test("maps logLevel correctly", () => {
    const result = buildInitializationOptions(makeConfig({ logLevel: "debug" }));
    assert.strictEqual(result["logLevel"], "debug");
  });

  test("maps extraCompilerFlags correctly", () => {
    const flags = ["-DDEBUG", "-I/usr/local/include"];
    const result = buildInitializationOptions(makeConfig({ extraCompilerFlags: flags }));
    assert.deepStrictEqual(result["extraCompilerFlags"], flags);
  });

  test("maps enableNullabilityChecks: true", () => {
    const result = buildInitializationOptions(makeConfig({ enableNullabilityChecks: true }));
    assert.strictEqual(result["enableNullabilityChecks"], true);
  });

  test("maps enableNullabilityChecks: false", () => {
    const result = buildInitializationOptions(makeConfig({ enableNullabilityChecks: false }));
    assert.strictEqual(result["enableNullabilityChecks"], false);
  });

  test("maps enableStaticAnalyzer: false by default", () => {
    const result = buildInitializationOptions(makeConfig());
    assert.strictEqual(result["enableStaticAnalyzer"], false);
  });

  test("maps enableStaticAnalyzer: true", () => {
    const result = buildInitializationOptions(makeConfig({ enableStaticAnalyzer: true }));
    assert.strictEqual(result["enableStaticAnalyzer"], true);
  });

  test("does NOT include serverPath in output", () => {
    const result = buildInitializationOptions(makeConfig({ serverPath: "/usr/bin/objc-lsp" }));
    assert.ok(!("serverPath" in result), "serverPath should not be forwarded to initializationOptions");
  });

  test("empty flags produces empty array", () => {
    const result = buildInitializationOptions(makeConfig({ extraCompilerFlags: [] }));
    assert.deepStrictEqual(result["extraCompilerFlags"], []);
  });

  test("all four expected keys are present", () => {
    const result = buildInitializationOptions(makeConfig());
    const keys = Object.keys(result).sort();
    assert.deepStrictEqual(keys, [
      "enableNullabilityChecks",
      "enableStaticAnalyzer",
      "extraCompilerFlags",
      "logLevel",
    ]);
  });
});

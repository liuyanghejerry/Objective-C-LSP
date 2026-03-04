import * as assert from "assert";
import * as vscode from "vscode";
import {
  findRetainCycles,
  findStrongDelegates,
  findMagicNumbers,
  findMatchingBrace,
} from "../../src/decorators";

// ---------------------------------------------------------------------------
// Helper: open a vscode.TextDocument from a string in memory
// ---------------------------------------------------------------------------
async function doc(content: string, lang = "objective-c"): Promise<vscode.TextDocument> {
  return vscode.workspace.openTextDocument({ content, language: lang });
}

// ---------------------------------------------------------------------------
// findMatchingBrace
// ---------------------------------------------------------------------------
suite("findMatchingBrace", () => {
  test("finds matching brace at depth 1", () => {
    const text = "^{ foo }";
    assert.strictEqual(findMatchingBrace(text, 1), 7);
  });

  test("handles nested braces", () => {
    const text = "^{ if (x) { bar } }";
    assert.strictEqual(findMatchingBrace(text, 1), 19);
  });

  test("returns -1 when unmatched", () => {
    const text = "^{ foo";
    assert.strictEqual(findMatchingBrace(text, 1), -1);
  });

  test("returns pos itself when single brace pair", () => {
    assert.strictEqual(findMatchingBrace("{}", 0), 1);
  });
});

// ---------------------------------------------------------------------------
// findRetainCycles
// ---------------------------------------------------------------------------
suite("findRetainCycles", () => {
  test("flags bare self inside a block", async () => {
    const d = await doc(
      "- (void)foo {\n    dispatch_async(q, ^{\n        [self bar];\n    });\n}\n"
    );
    const result = findRetainCycles(d);
    assert.ok(result.length > 0, "expected at least one retain cycle decoration");
  });

  test("ignores self when weakSelf is declared before the block", async () => {
    const d = await doc(
      "- (void)foo {\n" +
      "    __weak typeof(self) weakSelf = self;\n" +
      "    dispatch_async(q, ^{\n" +
      "        [weakSelf bar];\n" +
      "    });\n" +
      "}\n"
    );
    const result = findRetainCycles(d);
    assert.strictEqual(result.length, 0);
  });

  test("ignores weakSelf and strongSelf references", async () => {
    const d = await doc(
      "- (void)foo {\n" +
      "    __weak typeof(self) weakSelf = self;\n" +
      "    void (^blk)(void) = ^{\n" +
      "        __strong typeof(weakSelf) strongSelf = weakSelf;\n" +
      "        [strongSelf bar];\n" +
      "    };\n" +
      "}\n"
    );
    const result = findRetainCycles(d);
    assert.strictEqual(result.length, 0);
  });

  test("returns empty array for file with no blocks", async () => {
    const d = await doc("- (void)foo {\n    NSLog(@\"hello\");\n}\n");
    const result = findRetainCycles(d);
    assert.strictEqual(result.length, 0);
  });
});

// ---------------------------------------------------------------------------
// findStrongDelegates
// ---------------------------------------------------------------------------
suite("findStrongDelegates", () => {
  test("flags @property (nonatomic, strong) delegate", async () => {
    const d = await doc(
      "@interface Foo : NSObject\n" +
      "@property (nonatomic, strong) id<FooDelegate> delegate;\n" +
      "@end\n"
    );
    const result = findStrongDelegates(d);
    assert.strictEqual(result.length, 1);
  });

  test("flags @property (retain) dataSource", async () => {
    const d = await doc(
      "@interface Foo : NSObject\n" +
      "@property (retain) id<FooDataSource> dataSource;\n" +
      "@end\n"
    );
    const result = findStrongDelegates(d);
    assert.strictEqual(result.length, 1);
  });

  test("does not flag @property (nonatomic, weak) delegate", async () => {
    const d = await doc(
      "@interface Foo : NSObject\n" +
      "@property (nonatomic, weak) id<FooDelegate> delegate;\n" +
      "@end\n"
    );
    const result = findStrongDelegates(d);
    assert.strictEqual(result.length, 0);
  });

  test("does not flag @property (assign) delegate", async () => {
    const d = await doc(
      "@property (assign) id<FooDelegate> delegate;\n"
    );
    const result = findStrongDelegates(d);
    assert.strictEqual(result.length, 0);
  });

  test("returns empty for file with no properties", async () => {
    const d = await doc("- (void)foo { }\n");
    const result = findStrongDelegates(d);
    assert.strictEqual(result.length, 0);
  });
});

// ---------------------------------------------------------------------------
// findMagicNumbers
// ---------------------------------------------------------------------------
suite("findMagicNumbers", () => {
  test("flags a large integer literal inside a method body", async () => {
    const d = await doc(
      "@implementation Foo\n" +
      "- (void)bar {\n" +
      "    CGFloat width = 320;\n" +
      "}\n" +
      "@end\n"
    );
    const result = findMagicNumbers(d);
    assert.ok(result.length > 0, "expected magic number decoration for 320");
  });

  test("does not flag 0, 1, 2, -1", async () => {
    const d = await doc(
      "@implementation Foo\n" +
      "- (void)bar {\n" +
      "    NSInteger a = 0;\n" +
      "    NSInteger b = 1;\n" +
      "    NSInteger c = 2;\n" +
      "}\n" +
      "@end\n"
    );
    const result = findMagicNumbers(d);
    assert.strictEqual(result.length, 0);
  });

  test("does not flag numbers in #define lines", async () => {
    const d = await doc(
      "@implementation Foo\n" +
      "- (void)bar {\n" +
      "#define MAX_COUNT 100\n" +
      "}\n" +
      "@end\n"
    );
    const result = findMagicNumbers(d);
    assert.strictEqual(result.length, 0);
  });

  test("does not flag numbers in const declarations", async () => {
    const d = await doc(
      "@implementation Foo\n" +
      "- (void)bar {\n" +
      "    const CGFloat kPadding = 16;\n" +
      "}\n" +
      "@end\n"
    );
    const result = findMagicNumbers(d);
    assert.strictEqual(result.length, 0);
  });

  test("returns empty outside of method body", async () => {
    const d = await doc(
      "@interface Foo : NSObject\n" +
      "static const int kSize = 500;\n" +
      "@end\n"
    );
    const result = findMagicNumbers(d);
    assert.strictEqual(result.length, 0);
  });
});

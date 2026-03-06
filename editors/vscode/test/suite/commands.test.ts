import * as assert from "assert";
import { inferAttribute } from "../../src/commands";

// ---------------------------------------------------------------------------
// inferAttribute
// ---------------------------------------------------------------------------
suite("inferAttribute", () => {
  // Non-pointer types always get `assign`
  suite("assign for non-pointer types", () => {
    test("NSInteger → assign", () => {
      assert.strictEqual(inferAttribute("NSInteger", false), "assign");
    });

    test("BOOL → assign", () => {
      assert.strictEqual(inferAttribute("BOOL", false), "assign");
    });

    test("CGFloat → assign", () => {
      assert.strictEqual(inferAttribute("CGFloat", false), "assign");
    });

    test("int → assign", () => {
      assert.strictEqual(inferAttribute("int", false), "assign");
    });
  });

  // Known Foundation value types get `copy`
  suite("copy for Foundation value types", () => {
    test("NSString → copy", () => {
      assert.strictEqual(inferAttribute("NSString", true), "copy");
    });

    test("NSMutableString → copy", () => {
      assert.strictEqual(inferAttribute("NSMutableString", true), "copy");
    });

    test("NSArray → copy", () => {
      assert.strictEqual(inferAttribute("NSArray", true), "copy");
    });

    test("NSMutableArray → copy", () => {
      assert.strictEqual(inferAttribute("NSMutableArray", true), "copy");
    });

    test("NSDictionary → copy", () => {
      assert.strictEqual(inferAttribute("NSDictionary", true), "copy");
    });

    test("NSMutableDictionary → copy", () => {
      assert.strictEqual(inferAttribute("NSMutableDictionary", true), "copy");
    });

    test("NSSet → copy", () => {
      assert.strictEqual(inferAttribute("NSSet", true), "copy");
    });

    test("NSMutableSet → copy", () => {
      assert.strictEqual(inferAttribute("NSMutableSet", true), "copy");
    });

    test("NSNumber → copy", () => {
      assert.strictEqual(inferAttribute("NSNumber", true), "copy");
    });

    test("NSData → copy", () => {
      assert.strictEqual(inferAttribute("NSData", true), "copy");
    });

    test("NSAttributedString → copy", () => {
      assert.strictEqual(inferAttribute("NSAttributedString", true), "copy");
    });

    test("NSMutableAttributedString → copy", () => {
      assert.strictEqual(inferAttribute("NSMutableAttributedString", true), "copy");
    });
  });

  // Delegate / dataSource patterns get `weak`
  suite("weak for delegate/dataSource patterns", () => {
    test("FooDelegate → weak", () => {
      assert.strictEqual(inferAttribute("FooDelegate", true), "weak");
    });

    test("BarDataSource → weak (case-insensitive datasource match)", () => {
      assert.strictEqual(inferAttribute("BarDataSource", true), "weak");
    });

    test("delegate (lowercase) → weak", () => {
      assert.strictEqual(inferAttribute("delegate", true), "weak");
    });

    test("MyTableViewDelegate → weak", () => {
      assert.strictEqual(inferAttribute("MyTableViewDelegate", true), "weak");
    });
  });

  // Arbitrary pointer types get `strong`
  suite("strong for other pointer types", () => {
    test("UIView → strong", () => {
      assert.strictEqual(inferAttribute("UIView", true), "strong");
    });

    test("NSObject → strong", () => {
      assert.strictEqual(inferAttribute("NSObject", true), "strong");
    });

    test("MyCustomClass → strong", () => {
      assert.strictEqual(inferAttribute("MyCustomClass", true), "strong");
    });

    test("id → strong", () => {
      assert.strictEqual(inferAttribute("id", true), "strong");
    });
  });
});

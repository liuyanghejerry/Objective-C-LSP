import * as assert from "assert";
import * as vscode from "vscode";
import { buildProtocolMap } from "../../src/codelens";

async function doc(content: string): Promise<vscode.TextDocument> {
  return vscode.workspace.openTextDocument({ content, language: "objective-c" });
}

suite("buildProtocolMap", () => {
  test("extracts single protocol from @interface", async () => {
    const d = await doc(
      "@interface Foo : NSObject <NSCopying>\n@end\n"
    );
    const map = buildProtocolMap(d);
    assert.deepStrictEqual(map.get("Foo"), ["NSCopying"]);
  });

  test("extracts multiple protocols", async () => {
    const d = await doc(
      "@interface MyVC : UIViewController <UITableViewDelegate, UITableViewDataSource>\n@end\n"
    );
    const map = buildProtocolMap(d);
    assert.deepStrictEqual(map.get("MyVC"), ["UITableViewDelegate", "UITableViewDataSource"]);
  });

  test("merges protocols from multiple @interface declarations (category)", async () => {
    const d = await doc(
      "@interface Foo : NSObject <NSCopying>\n@end\n" +
      "@interface Foo (Printing) <NSCoding>\n@end\n"
    );
    const map = buildProtocolMap(d);
    const protos = map.get("Foo") ?? [];
    assert.ok(protos.includes("NSCopying"));
    assert.ok(protos.includes("NSCoding"));
  });

  test("returns empty map when no @interface with protocols", async () => {
    const d = await doc("@interface Foo : NSObject\n@end\n");
    const map = buildProtocolMap(d);
    assert.strictEqual(map.size, 0);
  });

  test("handles whitespace around protocol list", async () => {
    const d = await doc(
      "@interface Bar : NSObject < NSObject , NSCopying >\n@end\n"
    );
    const map = buildProtocolMap(d);
    assert.deepStrictEqual(map.get("Bar"), ["NSObject", "NSCopying"]);
  });

  test("ignores duplicate protocols when same @interface appears twice", async () => {
    const d = await doc(
      "@interface Foo : NSObject <NSCopying>\n@end\n" +
      "@interface Foo (Ext) <NSCopying>\n@end\n"
    );
    const map = buildProtocolMap(d);
    const protos = map.get("Foo") ?? [];
    assert.strictEqual(protos.filter((p) => p === "NSCopying").length, 1);
  });
});

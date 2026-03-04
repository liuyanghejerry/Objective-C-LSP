/**
 * GUI Test: Code Lens display
 *
 * Scenario:
 *   1. Open UITestFixture.m (contains @implementation + methods)
 *   2. Wait for Code Lens provider to resolve
 *   3. Assert that Code Lens items appear for method declarations and @implementation
 *
 * API:
 *   TextEditor.getCodeLenses() → CodeLens[]
 *   TextEditor.getCodeLens(title: string) → CodeLens | undefined
 *   CodeLens.getText() → string  (inherited from AbstractElement/WebElement)
 */

import * as path from "path";
import {
  VSBrowser,
  TextEditor,
  By,
} from "vscode-extension-tester";
import { openAndWaitForContent, safeCloseAllEditors, waitForCodeLenses } from "./helpers";

const FIXTURE_PATH = path.resolve(
  __dirname,
  "../../../test/fixtures/workspace/UITestFixture.m"
);

describe("Code Lens display", function () {
  this.timeout(90_000);

  let editor: TextEditor;

  before(async function () {
    editor = await openAndWaitForContent(FIXTURE_PATH, "UITestFixture.m");
    // Give the extension activation + Code Lens resolution time, then poll
    await VSBrowser.instance.driver.sleep(2000);
    await waitForCodeLenses(editor, 15_000);
  });

  after(async function () {
    await safeCloseAllEditors();
  });

  it("shows at least one Code Lens in UITestFixture.m", async function () {
    // The file has methods (-loadData, -updateUI) — the provider should emit
    // reference-count lenses for each.
    const lenses = await editor.getCodeLenses();

    if (lenses.length === 0) {
      throw new Error(
        "Expected at least one Code Lens to appear in UITestFixture.m, but getCodeLenses() returned 0.\n" +
          "Hint: check that objc-lsp.enableCodeLens is true and the extension is activated."
      );
    }
  });

  it("shows a 'references' Code Lens above a method declaration", async function () {
    // Our Code Lens titles look like "$(references) N reference(s)"
    // We search by partial title "reference"
    const referenceLens = await editor.getCodeLens("reference");

    if (!referenceLens) {
      // Provide a diagnostic: list what lenses were actually found
      const lenses = await editor.getCodeLenses();
      const titles: string[] = [];
      for (const lens of lenses) {
        try {
          titles.push(await lens.getText());
        } catch {
          titles.push("(unreadable)");
        }
      }

      throw new Error(
        `Expected a Code Lens with title matching "reference" but none found.\n` +
          `Found ${lenses.length} lens(es): ${titles.join(", ") || "(none)"}`
      );
    }
  });

  it("shows a protocol-conformance Code Lens on @implementation", async function () {
    // UITestFixture.m: MyClass has @property (nonatomic, strong) id<UITableViewDelegate> delegate;
    // but the @interface does NOT declare protocol conformance, so this lens
    // will NOT fire for this fixture. We instead verify via Sample.m which has
    //   @interface MyViewController : UIViewController <UITableViewDelegate, UITableViewDataSource>
    // We open Sample.m instead.
    const driver = VSBrowser.instance.driver;
    const samplePath = path.resolve(
      __dirname,
      "../../../test/fixtures/workspace/Sample.m"
    );

    const sampleEditor = await openAndWaitForContent(samplePath, "Sample.m");
    await driver.sleep(2000);
    await waitForCodeLenses(sampleEditor, 15_000);

    // Strategy 1: use the API (works when command is non-empty)
    const conformsLens = await sampleEditor.getCodeLens("Conforms to");

    // Strategy 2: search the DOM for any element containing "Conforms to"
    // (covers cases where the API misses static lenses with non-empty commands)
    let domFound = false;
    if (!conformsLens) {
      const domElements = await driver.findElements(
        By.xpath("//*[contains(., 'Conforms to')]")
      );
      domFound = domElements.length > 0;
    }

    // Collect diagnostics for error messages — must happen BEFORE closing
    let diagnosticTitles: string[] = [];
    if (!conformsLens && !domFound) {
      try {
        const lenses = await sampleEditor.getCodeLenses();
        for (const lens of lenses) {
          try {
            diagnosticTitles.push(await lens.getText());
          } catch {
            diagnosticTitles.push("(unreadable)");
          }
        }
      } catch {
        diagnosticTitles = ["(could not read lenses)"];
      }
    }

    // Clean up
    await safeCloseAllEditors();

    if (!conformsLens && !domFound) {
      throw new Error(
        `Expected a "Conforms to:" Code Lens on @implementation MyViewController but none found.\n` +
          `Found ${diagnosticTitles.length} lens(es): ${diagnosticTitles.join(", ") || "(none)"}` 
      );
    }
  });
});

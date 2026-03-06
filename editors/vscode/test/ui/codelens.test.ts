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
 *
 * DOM fallback:
 *   VS Code 1.110 changed the code lens DOM structure so the vscode-extension-tester
 *   TextEditor.getCodeLenses() API (XPath: `span[contains(@widgetid, 'codelens.widget')]/a[@id]`)
 *   no longer reliably finds lenses. Each test therefore tries the API first, then falls
 *   back to direct DOM inspection — the same pattern used by the protocol-conformance test.
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
    const driver = VSBrowser.instance.driver;
    const lenses = await editor.getCodeLenses();

    if (lenses.length > 0) {
      return; // API found lenses — pass
    }

    // Strategy 2: search the DOM for code lens elements.
    // VS Code renders code lenses as elements with class "codelens-decoration"
    // or with widgetid/class containing "codelens". The exact structure varies by
    // VS Code version, so we use a broad XPath.
    const domByClass = await driver.findElements(
      By.xpath("//*[contains(@class,'codelens') or contains(@widgetid,'codelens')]")
    );
    if (domByClass.length > 0) {
      return;
    }

    // Strategy 3: look for "reference" text anywhere in the editor (our code lens
    // provider always emits "N reference(s)" or "? references" for method lenses).
    const domByText = await driver.findElements(
      By.xpath("//*[contains(.,'reference') and not(self::script) and not(self::style)]")
    );
    // Filter to elements that are likely code lens text (short, visible, in editor area)
    for (const el of domByText) {
      try {
        const text = await el.getText();
        if (text && /\d+\s+references?|[?]\s+references?/i.test(text)) {
          return; // Found a resolved reference-count code lens
        }
      } catch {
        continue;
      }
    }

    throw new Error(
      "Expected at least one Code Lens to appear in UITestFixture.m, but getCodeLenses() returned 0.\n" +
        "Hint: check that objc-lsp.enableCodeLens is true and the extension is activated."
    );
  });

  it("shows a 'references' Code Lens above a method declaration", async function () {
    // Our Code Lens titles look like "$(references) N reference(s)"
    // We search by partial title "reference"
    const driver = VSBrowser.instance.driver;
    const referenceLens = await editor.getCodeLens("reference");

    if (referenceLens) {
      return; // API found it
    }

    // Strategy 2: look for a DOM element with "codelens" in its class that
    // contains "reference" in its text content.
    const domByClass = await driver.findElements(
      By.xpath("//*[contains(@class,'codelens') and contains(.,'reference')]")
    );
    if (domByClass.length > 0) {
      return;
    }

    // Strategy 3: look for "reference" text in the content-widgets overlay area.
    const domWidgets = await driver.findElements(
      By.xpath(
        "//div[contains(@class,'contentWidgets') or contains(@class,'view-overlays')]" +
        "//*[contains(.,'reference')]"
      )
    );
    if (domWidgets.length > 0) {
      return;
    }

    // Strategy 4: look for any rendered "N references" or "? references" text
    const domByText = await driver.executeScript<boolean>(`
      try {
        const all = document.querySelectorAll('*');
        for (const el of all) {
          if (el.childElementCount === 0 && el.textContent) {
            const t = el.textContent.trim();
            if (/\\d+\\s+references?|[?]\\s+references?/i.test(t)) return true;
          }
        }
        return false;
      } catch(e) { return false; }
    `);
    if (domByText) {
      return;
    }

    // Provide diagnostics for the error message
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

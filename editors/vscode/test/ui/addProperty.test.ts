/**
 * GUI Test: ObjC: Generate @property from ivar (addProperty command)
 *
 * Scenario:
 *   1. Open UITestFixture.m
 *   2. Place cursor on the "NSString *_name;" ivar line via Monaco API
 *   3. Execute "ObjC: Generate @property from ivar" via runObjcCommand
 *   4. Assert the line was transformed to "@property (nonatomic, copy) NSString *name;"
 */

import * as path from "path";
import { VSBrowser, Workbench } from "vscode-extension-tester";
import {
  openAndWaitForContent,
  safeCloseAllEditors,
  getLineText,
  gotoLine,
  runObjcCommand,
} from "./helpers";

const FIXTURE_PATH = path.resolve(
  __dirname,
  "../../../test/fixtures/workspace/UITestFixture.m"
);

describe("addProperty command", function () {
  // GUI tests are inherently slow — allow generous timeouts
  this.timeout(60_000);

  before(async function () {
    await openAndWaitForContent(FIXTURE_PATH, "UITestFixture.m");
  });

  afterEach(async function () {
    // Revert the fixture file after each test so the next test starts clean.
    const workbench = new Workbench();
    const driver = VSBrowser.instance.driver;
    try {
      await workbench.executeCommand("workbench.action.revertFile");
      await driver.sleep(500);
    } catch {
      // Ignore — file may already be clean
    }
  });

  after(async function () {
    await safeCloseAllEditors();
  });

  it("transforms NSString *_name; into a @property declaration", async function () {
    // Line 5 in UITestFixture.m is "    NSString *_name;"
    const driver = VSBrowser.instance.driver;

    // Navigate cursor to line 5 using gotoLine (reliable, avoids Monaco API race conditions)
    await gotoLine(5);
    await VSBrowser.instance.driver.sleep(300);

    // Use runObjcCommand to select the exact command by title (avoids fuzzy-match misfires)
    await runObjcCommand("ObjC: Generate @property from ivar");
    await driver.sleep(2000);

    const transformedLine = await getLineText(5);
    const expected = "@property (nonatomic, copy) NSString *name;";
    // Strip ALL non-printable and invisible Unicode chars Monaco injects between tokens
    const cleanLine = transformedLine.replace(/\u00a0/g, ' ').replace(/[\u0000-\u001f\u007f-\u009f\u200b-\u200f\u2028-\u202f\u205f-\u206f\ufeff]/g, '').trim();

    // Debug: log char codes of both strings to diagnose invisible-char mismatches
    const expectedCodes = Array.from(expected).map(c => c.charCodeAt(0)).join(',');
    const gotCodes = Array.from(cleanLine.slice(0, expected.length + 5)).map(c => c.charCodeAt(0)).join(',');
    console.error('[addProperty test1] expected codes:', expectedCodes);
    console.error('[addProperty test1] got codes:', gotCodes);

    if (!cleanLine.includes(expected)) {
      throw new Error(
        `Expected line 5 to contain:\n  ${expected}\nGot:\n  ${cleanLine}\nGot codes: ${gotCodes}`
      );
    }
  });

  it("transforms NSInteger _count; into an assign @property declaration", async function () {
    // Line 6 is "    NSInteger _count;"
    const driver = VSBrowser.instance.driver;
    // Navigate cursor to line 6
    await gotoLine(6);
    await VSBrowser.instance.driver.sleep(300);

    await runObjcCommand("ObjC: Generate @property from ivar");
    await driver.sleep(2000);

    const transformedLine = await getLineText(6);
    const expected = "@property (nonatomic, assign) NSInteger count;";
    const cleanLine = transformedLine.replace(/\u00a0/g, ' ').replace(/[\u200b\u200c\u200d\ufeff]/g, '').trim();

    if (!cleanLine.includes(expected)) {
      throw new Error(
        `Expected line 6 to contain:\n  ${expected}\nGot:\n  ${cleanLine}`
      );
    }
  });
});

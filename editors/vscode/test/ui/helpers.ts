/**
 * Shared test helpers for vscode-extension-tester GUI tests.
 *
 * Provides reliable file-open, modal-dismiss, editor-close, and
 * Code Lens wait utilities that work around common test-environment issues:
 *   - Modal dialogs from other installed extensions blocking UI
 *   - Editor content rendering delays
 *   - close-button click interception by overlapping tabs
 */

import * as childProcess from "child_process";
import * as path from "path";
import {
  VSBrowser,
  TextEditor,
  EditorView,
  Workbench,
  By,
  Key,
  InputBox,
} from "vscode-extension-tester";

/**
 * Dismiss any visible modal dialogs.
 * - For "Save changes?" dialogs: clicks "Don't Save" to discard and continue.
 * - For all other modals: presses Escape.
 * Safe to call when no dialog is present.
 */
export async function dismissModals(): Promise<void> {
  const driver = VSBrowser.instance.driver;
  try {
    const modals = await driver.findElements(
      By.css(".monaco-dialog-modal-block")
    );
    if (modals.length > 0) {
      // Use executeScript to find and click "Don't Save" (handles both ASCII and Unicode apostrophe).
      try {
        const clicked = await driver.executeScript<boolean>(
          "const buttons = document.querySelectorAll('.monaco-button');" +
          "for (const btn of buttons) {" +
          "  const t = btn.textContent || '';" +
          "  if (t.includes(\"Don't Save\") || t.includes('Don\u2019t Save')) { btn.click(); return true; }" +
          "}" +
          "return false;"
        );
        if (clicked) {
          await driver.sleep(400);
          return;
        }
      } catch {
        // No button found — fall through
      }
      await driver.actions().sendKeys(Key.ESCAPE).perform();
      await driver.sleep(500);
    }
  } catch {
    // No dialog present — safe to continue
  }
}

/**
 * Close all editors using the command palette, handling any "Save changes?"
 * dialog that appears by clicking "Don't Save".
 */
export async function safeCloseAllEditors(): Promise<void> {
  const workbench = new Workbench();
  const driver = VSBrowser.instance.driver;
  try {
    await workbench.executeCommand("workbench.action.closeAllEditors");
    // Wait briefly for any save dialog to appear, then dismiss it
    await driver.sleep(600);
    await dismissModals();
    await driver.sleep(300);
  } catch {
    try {
      await new EditorView().closeAllEditors();
      await driver.sleep(600);
      await dismissModals();
    } catch {
      // Ignore — test environment may already be clean
    }
  }
}

/**
 * Wait until the Monaco editor DOM has rendered at least one view-line
 * with non-empty text content.
 *
 * This polls the DOM directly (not via TextEditor.getText()) to avoid the
 * Ctrl+A / Ctrl+C clipboard interaction that fails when the input area is
 * not yet interactable.
 */
/**
 * Wait until the Monaco editor DOM has rendered view-line elements.
 * Polls the DOM directly (not via TextEditor.getText()) to avoid the
 * Ctrl+A / Ctrl+C clipboard interaction that fails when the input area is
 * not yet interactable.
 *
 * Strategy: look for the editor container (.editor-container) to be present,
 * and for at least one .view-line to exist. Then ensure the input area is
 * interactable by clicking it.
 */
async function waitForEditorDOMContent(
  timeoutMs: number,
  minLines = 5
): Promise<void> {
  const driver = VSBrowser.instance.driver;
  const deadline = Date.now() + timeoutMs;
  let lastLogTime = 0;

  while (Date.now() < deadline) {
    await dismissModals();
    try {
      const viewLines = await driver.findElements(
        By.css(".view-lines .view-line")
      );
      if (viewLines.length >= minLines) {
        await driver.sleep(200);
        return;
      }
      // Log DOM state every ~5s to help diagnose blank-editor issues
      const now = Date.now();
      if (now - lastLogTime >= 5000) {
        lastLogTime = now;
        const debugInfo = await driver.executeScript<string>(`
          try {
            const vl = document.querySelectorAll('.view-lines .view-line');
            const tabs = Array.from(document.querySelectorAll('.tab')).slice(0,3).map(t => t.getAttribute('aria-label'));
            const ec = document.querySelector('.editor-container');
            const editorRect = ec ? { w: ec.getBoundingClientRect().width, h: ec.getBoundingClientRect().height } : null;
            const viewLinesEl = document.querySelector('.view-lines');
            const viewLinesRect = viewLinesEl ? { w: viewLinesEl.getBoundingClientRect().width, h: viewLinesEl.getBoundingClientRect().height } : null;
            const om = Array.from(document.querySelectorAll('.overlay-message-text')).slice(0,2).map(e => e.textContent && e.textContent.slice(0,50));
            return JSON.stringify({ viewLineCount: vl.length, tabs, editorRect, viewLinesRect, om });
          } catch(e) { return String(e); }
        `);
        console.error('[waitForEditorDOMContent] DOM state:', debugInfo);
      }
    } catch {
      // DOM not ready yet
    }
    await driver.sleep(500);
  }

  // Final debug dump before throwing
  let finalDebug = '';
  try {
    finalDebug = await driver.executeScript<string>(`try {
      const vl = document.querySelectorAll('.view-lines .view-line');
      const tabs = Array.from(document.querySelectorAll('.tab')).slice(0,3).map(t => t.getAttribute('aria-label'));
      const ec = document.querySelector('.editor-container');
      const editorRect = ec ? { w: ec.getBoundingClientRect().width, h: ec.getBoundingClientRect().height } : null;
      const viewLinesEl = document.querySelector('.view-lines');
      const viewLinesRect = viewLinesEl ? { w: viewLinesEl.getBoundingClientRect().width, h: viewLinesEl.getBoundingClientRect().height } : null;
      const modelUri = window._lastMonacoModelUri || (window.monaco && window.monaco.editor && window.monaco.editor.getModels().map(m => m.uri.path).join(',')) || null;
      return JSON.stringify({ viewLineCount: vl.length, tabs, editorRect, viewLinesRect, modelUri });
    } catch(e) { return String(e); }`);
  } catch {
    finalDebug = '(executeScript failed)';
  }

  throw new Error(
    `Editor DOM never rendered ${minLines}+ view-line(s) within ${timeoutMs}ms. Debug: ${finalDebug}`
  );
}

/**
 * Click the Monaco editor input area to ensure it has keyboard focus.
 * This is required before any keyboard interactions (getText, moveCursor, etc.).
 */
export async function focusEditorInputArea(): Promise<void> {
  const driver = VSBrowser.instance.driver;
  try {
    // Try clicking the native-edit-context (VS Code >= 1.101)
    const inputAreas = await driver.findElements(
      By.css(".native-edit-context")
    );
    if (inputAreas.length > 0) {
      await driver.actions().click(inputAreas[0]).perform();
      await driver.sleep(200);
      return;
    }
  } catch {
    // Fall through to textarea fallback
  }
  try {
    // Fallback for older VS Code: the .inputarea textarea
    const textareas = await driver.findElements(
      By.css("textarea.inputarea")
    );
    if (textareas.length > 0) {
      await driver.actions().click(textareas[0]).perform();
      await driver.sleep(200);
    }
  } catch {
    // Ignore — focus attempt best-effort
  }
}

/**
 * Open a file and wait until the Monaco editor DOM is showing content.
 * Returns a focused TextEditor ready for interaction.
 *
 * @param filePath  Absolute path to the file to open
 * @param fileName  Tab title (file name) to select in EditorView
 * @param contentWaitMs  Max ms to wait for content to appear (default 20s)
 */
export async function openAndWaitForContent(
  filePath: string,
  fileName: string,
  contentWaitMs = 15_000
): Promise<TextEditor> {
  const driver = VSBrowser.instance.driver;
  await dismissModals();

  // Close all existing editors so no stale blank tab interferes.
  await safeCloseAllEditors();
  await driver.sleep(500);

  // Use the test VS Code's own CLI binary to open the file.
  // The system `code` CLI sends the IPC signal to the wrong (system) VS Code
  // window. The test VS Code binary sends the signal to itself.
  const vscodeCli = path.resolve(
    __dirname,
    "../../../.ui-test",
    "Visual Studio Code.app/Contents/Resources/app/bin/code"
  );
  const userDataDir = path.resolve(
    __dirname,
    "../../../.ui-test/settings"
  );
  try {
    const cliOut = childProcess.execSync(
      `"${vscodeCli}" -r "${filePath}" --user-data-dir="${userDataDir}"`,
      { stdio: ["ignore", "pipe", "pipe"], timeout: 8000 }
    );
    console.error('[openAndWaitForContent] CLI stdout:', cliOut.toString().slice(0, 200));
  } catch (e: unknown) {
    const err = e as { stdout?: Buffer; stderr?: Buffer; message?: string };
    console.error('[openAndWaitForContent] CLI failed:', err.message);
    if (err.stdout) console.error('  stdout:', err.stdout.toString().slice(0, 200));
    if (err.stderr) console.error('  stderr:', err.stderr.toString().slice(0, 200));
  }
  // Wait for the file to be received and opened by the test VS Code window.
  await driver.sleep(3000);
  await dismissModals();

  const editorView = new EditorView();
  const editor = (await editorView.openEditor(fileName)) as TextEditor;
  await driver.sleep(500);
  // Take a diagnostic screenshot to see the visual state
  try {
    const screenshot = await driver.takeScreenshot();
    const fs = require('fs');
    const screenshotDir = require('path').resolve(__dirname, '../../../.ui-test/screenshots/diag');
    fs.mkdirSync(screenshotDir, { recursive: true });
    fs.writeFileSync(require('path').join(screenshotDir, `after-open-${Date.now()}.png`), screenshot, 'base64');
    console.error('[openAndWaitForContent] Screenshot saved to', screenshotDir);
  } catch (e: unknown) {
    console.error('[openAndWaitForContent] Screenshot failed:', (e as Error).message);
  }

  // Wait for DOM to render meaningful content (UITestFixture.m has 23 lines).
  await waitForEditorDOMContent(contentWaitMs, 10);

  // Click the editor to ensure keyboard focus.
  await focusEditorInputArea();
  await driver.sleep(300);

  return editor;
}

/**
 * Wait for Code Lenses to appear in the editor, polling with retries.
 * Also triggers a refresh to help the provider resolve quickly.
 */
export async function waitForCodeLenses(
  editor: TextEditor,
  timeoutMs = 15_000
): Promise<void> {
  const driver = VSBrowser.instance.driver;
  const workbench = new Workbench();

  // Trigger a Code Lens refresh (VS Code built-in command)
  try {
    await workbench.executeCommand("editor.action.refreshCodeLenses");
    await driver.sleep(500);
  } catch {
    // Command may not be available in all VS Code versions — ignore
  }

  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const lenses = await editor.getCodeLenses();
      if (lenses.length > 0) {
        return;
      }
    } catch {
      // Ignore transient errors during polling
    }
    await driver.sleep(800);
  }
  // Don't throw here — let individual tests report the failure with context
}

/**
 * Navigate the active editor to a specific line number using the Go-to-Line
 * command palette (Ctrl+G / workbench.action.gotoLine).
 *
 * This avoids TextEditor.moveCursor() which internally calls getText() via
 * the clipboard (Ctrl+A/C) — a path that fails on VS Code >=1.101 where the
 * input area is a `native-edit-context` div instead of a textarea.
 *
 * @param lineNumber  1-based line number to navigate to
 */
export async function gotoLine(lineNumber: number): Promise<void> {
  const driver = VSBrowser.instance.driver;

  // Use Quick Open (Ctrl+P) with `:lineNumber` syntax.
  // VS Code Quick Open supports `:N` to jump to line N in the active editor.
  // This works reliably on all platforms without requiring editor focus first.
  await driver.actions()
    .keyDown(Key.META)
    .sendKeys('p')
    .keyUp(Key.META)
    .perform();
  await driver.sleep(400);

  const input = await InputBox.create();
  // Clear any existing text (Quick Open may have leftover from previous use)
  await input.setText(`:${lineNumber}`);
  await driver.sleep(300);
  await input.confirm();
  await driver.sleep(300);

  // Ensure the input is fully closed before continuing
  await driver.actions().sendKeys(Key.ESCAPE).perform();
  await driver.sleep(200);
}

/**
 * Get the text content of a specific line in the active Monaco editor using
 * the Monaco JavaScript API via executeScript.
 *
 * This completely bypasses TextEditor.getTextAtLine() / getText() which uses
 * the clipboard (Ctrl+A/C) — a path broken in VS Code >=1.109 where EditContext
 * is the stable default and the legacy textarea input area is gone.
 *
 * @param lineNumber  1-based line number
 * @returns The text content of the line (without trailing newline)
 */
export async function getLineText(lineNumber: number): Promise<string> {
  const driver = VSBrowser.instance.driver;

  // Strategy 1: DOM gutter approach — match gutter line-number to view-line by Y position.
  // Try multiple CSS selector patterns for the line-number gutter.
  const lineText = await driver.executeScript<string | null>(`
    try {
      const lineNum = arguments[0];
      const gutterSelectors = [
        '.margin .line-numbers',
        '.margin-view-overlays .line-numbers',
        '.line-numbers'
      ];
      let gutterEls = [];
      for (const sel of gutterSelectors) {
        gutterEls = Array.from(document.querySelectorAll(sel));
        if (gutterEls.length > 0) break;
      }
      for (const ln of gutterEls) {
        if (ln.textContent && ln.textContent.trim() === String(lineNum)) {
          const top = ln.getBoundingClientRect().top;
          const viewLines = Array.from(document.querySelectorAll('.view-lines .view-line'));
          let closest = null, minDist = Infinity;
          for (const vl of viewLines) {
            const dist = Math.abs(vl.getBoundingClientRect().top - top);
            if (dist < minDist) { minDist = dist; closest = vl; }
          }
          // Use a 40px threshold (generous for any zoom level)
          if (closest && minDist < 40) return closest.textContent || '';
        }
      }
      return null;
    } catch(e) { return null; }`, lineNumber);

  if (lineText !== null && lineText !== undefined) {
    return lineText;
  }

  // Strategy 2: By line index — use the Nth view-line directly (works for small files
  // where Monaco doesn't virtualize, e.g. UITestFixture.m has only 23 lines).
  const byIndex = await driver.executeScript<string | null>(`
    try {
      const lineNum = arguments[0];
      const viewLines = Array.from(document.querySelectorAll('.view-lines .view-line'));
      // Sort by top position to ensure correct order
      viewLines.sort((a, b) => a.getBoundingClientRect().top - b.getBoundingClientRect().top);
      const idx = lineNum - 1;
      if (idx >= 0 && idx < viewLines.length) return viewLines[idx].textContent || '';
      return null;
    } catch(e) { return null; }`, lineNumber);

  if (byIndex !== null && byIndex !== undefined) {
    return byIndex;
  }

  // Debug: log DOM state to help diagnose failures
  const debugInfo = await driver.executeScript<string>(`
    try {
      const vl = document.querySelectorAll('.view-lines .view-line');
      const gutterEls = document.querySelectorAll('.margin .line-numbers, .line-numbers');
      return JSON.stringify({
        viewLineCount: vl.length,
        gutterEls: Array.from(gutterEls).slice(0,5).map(e => e.textContent && e.textContent.trim()),
        firstViewLine: vl[0] && vl[0].textContent && vl[0].textContent.slice(0,80),
      });
    } catch(e) { return String(e); }`);
  console.error('[getLineText] DOM debug for line', lineNumber, ':', debugInfo);
  throw new Error(`Could not read line ${lineNumber} from the Monaco editor DOM. Debug: ${debugInfo}`);
}

/**
 * Execute an ObjC LSP command by exact display title.
 *
 * workbench.executeCommand() fuzzy-matches and often picks the wrong command
 * (e.g. 'ObjC: Generate @property from ivar' matches 'ObjC: Show Language
 * Server Output' because it appears first in the list). This helper opens the
 * command palette, types the full title, waits for the exact pick to appear,
 * and selects it directly.
 *
 * @param title  The exact display title from package.json 'commands[].title'
 */
export async function runObjcCommand(title: string): Promise<void> {
  const driver = VSBrowser.instance.driver;

  // Use the library's openCommandPrompt() which handles Ctrl/Cmd+Shift+P internally,
  // then set text with the > prefix (same as Workbench.executeCommand does).
  // This avoids the double-> bug caused by manually pressing Cmd+Shift+P and then
  // calling setText without the > prefix.
  const workbench = new Workbench();
  const input = await workbench.openCommandPrompt();
  await input.setText(`>${title}`);
  await driver.sleep(800); // Wait for the command palette to filter results

  // Try to select the exact pick by label; fall back to confirm()
  try {
    const picks = await input.getQuickPicks();
    for (const pick of picks) {
      const label = await pick.getLabel();
      if (label === title) {
        await pick.select();
        return;
      }
    }
  } catch {
    // Fall through to confirm
  }
  await input.confirm();
}

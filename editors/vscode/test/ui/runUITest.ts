/**
 * Entry point for vscode-extension-tester GUI tests.
 *
 * Usage:
 *   npx extest setup-and-run "out/test/ui/**\/*.test.js" --vsix <path-to.vsix> --resources test/fixtures/workspace
 *
 * Or via npm script:
 *   npm run test:ui
 */

import * as fs from "fs";
import * as path from "path";
import { ExTester, ReleaseQuality } from "vscode-extension-tester";

const STORAGE_FOLDER = path.resolve(__dirname, "../../../.ui-test");
const PKG_JSON = JSON.parse(
  fs.readFileSync(path.resolve(__dirname, "../../../package.json"), "utf-8")
);
const VSIX_PATH = path.resolve(
  __dirname,
  `../../../objc-lsp-${PKG_JSON.version}.vsix`
);
const WORKSPACE = path.resolve(
  __dirname,
  "../../fixtures/workspace"
);
const TEST_GLOB = path.resolve(__dirname, "*.test.js");

// Use an isolated, empty extensions directory so local extensions
// (GitHub Copilot, Bookmarks, etc.) don't interfere with tests.
const ISOLATED_EXTENSIONS_DIR = path.resolve(
  STORAGE_FOLDER,
  "extensions-isolated"
);

// Custom VS Code settings that disable distracting features during tests:
//   - inline suggestions (Copilot ghost text that prevents editor focus)
//   - native edit context (VS Code >=1.101 uses a div; revert to textarea for getText())
//   - telemetry / updates / notifications
const CUSTOM_SETTINGS_PATH = path.resolve(
  STORAGE_FOLDER,
  "ui-test-settings.json"
);

const CUSTOM_SETTINGS = {
  "editor.inlineSuggest.enabled": false,
  "editor.suggest.preview": false,
  "github.copilot.enable": {
    "*": false,
    "objective-c": false
  },
  "github.copilot.inlineSuggest.enable": false,
  // Use the legacy textarea input area so TextEditor.getText() (clipboard copy)
  // works reliably across all VS Code versions tested.
  "editor.experimentalEditContextEnabled": false,
  "telemetry.telemetryLevel": "off",
  "update.mode": "none",
  "extensions.autoUpdate": false,
  "workbench.welcomePage.walkthroughs.openOnInstall": false,
  // Never ask to save files when closing — prevents save dialogs from blocking tests
  "files.hotExit": "off",
  "window.confirmBeforeClose": "never",
  // Ensure objc-lsp features are enabled during tests
  "objc-lsp.enableCodeLens": true,
  "objc-lsp.enableDecorators": true,
  "objc-lsp.enableHoverExtensions": true,
};

async function main(): Promise<void> {
  // Ensure the isolated extensions directory exists
  if (!fs.existsSync(ISOLATED_EXTENSIONS_DIR)) {
    fs.mkdirSync(ISOLATED_EXTENSIONS_DIR, { recursive: true });
  }

  // Clear stale workspace storage so VS Code reads files from disk instead of
  // restoring a cached blank buffer from a previous test run.
  const wsStoragePath = path.resolve(
    STORAGE_FOLDER,
    "settings/User/workspaceStorage"
  );
  if (fs.existsSync(wsStoragePath)) {
    fs.rmSync(wsStoragePath, { recursive: true, force: true });
    console.log("[runUITest] Cleared stale workspace storage:", wsStoragePath);
  }

  // Write custom settings file (ExTester will merge it into default settings)
  fs.writeFileSync(CUSTOM_SETTINGS_PATH, JSON.stringify(CUSTOM_SETTINGS, null, 2));

  const tester = new ExTester(
    STORAGE_FOLDER,
    ReleaseQuality.Stable,
    ISOLATED_EXTENSIONS_DIR
  );

  // Download VS Code + ChromeDriver once (cached on re-runs)
  await tester.downloadCode();
  await tester.downloadChromeDriver();

  // Install our extension VSIX into the test VS Code
  await tester.installVsix({ vsixFile: VSIX_PATH });

  const exitCode = await tester.runTests(TEST_GLOB, {
    resources: [WORKSPACE],
    settings: CUSTOM_SETTINGS_PATH,
  });

  process.exit(exitCode);
}

main().catch((err) => {
  console.error("UI test runner failed:", err);
  process.exit(1);
});

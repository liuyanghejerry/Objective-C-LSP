import { defineConfig } from "@vscode/test-cli";

export default defineConfig({
  files: "out/test/suite/**/*.test.js",
  version: "stable",
  workspaceFolder: "./test/fixtures/workspace",
  mocha: {
    timeout: 10000,
  },
});

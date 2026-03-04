import * as path from "path";
import { runTests } from "@vscode/test-electron";

async function main(): Promise<void> {
  const extensionDevelopmentPath = path.resolve(__dirname, "../..");
  const extensionTestsPath = path.resolve(__dirname, "./suite/index");

  await runTests({
    extensionDevelopmentPath,
    extensionTestsPath,
    // Use a minimal workspace so tests don't accidentally load a real project.
    launchArgs: [path.resolve(__dirname, "../../test/fixtures/workspace")],
  });
}

main().catch((err) => {
  console.error("Test runner failed:", err);
  process.exit(1);
});

import { test } from "node:test";
import assert from "node:assert";
import fs from "node:fs";

// Test that verifies the README installation path is truthful and not misleading
test("README installation instructions are truthful", () => {
  // Read the README.md file
  const readmeContent = fs.readFileSync("README.md", "utf8");

  // The README should NOT claim that binaries exist at src-tauri/target/release/
  // since no binaries are published and this path doesn't exist in the repo
  const hasProblematicPathClaim = /src-tauri\/target\/release\/lobster-pulse\.exe/;

  assert.strictEqual(
    hasProblematicPathClaim.test(readmeContent),
    false,
    "README contains misleading claims about binaries at src-tauri/target/release/ " +
      "that don't exist in the repository. The installation path needs to be " +
      "made truthful - either by creating actual releases or making it explicitly " +
      "source-only with accurate instructions."
  );
});
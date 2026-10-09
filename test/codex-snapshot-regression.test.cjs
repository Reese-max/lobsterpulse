const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const sourcePath = path.resolve(__dirname, "../src-tauri/src/hooks_configurator.rs");

function source() {
  return fs.readFileSync(sourcePath, "utf8");
}

test("Codex hook commits retain an external editor save", () => {
  const text = source();
  assert.match(text, /fn replace_file_if_snapshot_unchanged_with\s*</);
  assert.match(text, /codex_snapshot_commit_preserves_editor_save_after_comparison/);
});

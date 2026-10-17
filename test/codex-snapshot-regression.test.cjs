const { test } = require("node:test");
const assert = require("node:assert/strict");
const { spawnSync } = require("node:child_process");
const path = require("node:path");

const regression = "hooks_configurator::r37_silent_fail_surfacing_tests::codex_snapshot_commit_preserves_editor_save_after_comparison";

test("Codex hook commits retain an external editor save", () => {
  const result = spawnSync("cargo", [
    "test", "--locked", "--manifest-path", "src-tauri/Cargo.toml",
    "--lib", regression, "--", "--exact",
  ], {
    cwd: path.resolve(__dirname, ".."),
    encoding: "utf8",
    timeout: 600_000,
    windowsHide: true,
  });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  assert.ok(result.stdout.includes(`test ${regression} ... ok`), "the exact Rust regression must run and pass");
});

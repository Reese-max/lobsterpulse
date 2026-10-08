const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const ROOT = path.resolve(__dirname, "..");
const README = fs.readFileSync(path.join(ROOT, "README.md"), "utf8");
const LANDING = fs.readFileSync(path.join(ROOT, "docs", "index.html"), "utf8");
const RELEASE_WORKFLOW = fs.readFileSync(
  path.join(ROOT, ".github", "workflows", "release.yml"),
  "utf8"
);
const TAURI_CONFIG = JSON.parse(
  fs.readFileSync(path.join(ROOT, "src-tauri", "tauri.conf.json"), "utf8")
);

const INSTALL_CLI = "cargo install tauri-cli --version 2.11.4 --locked";
const BUILD_COMMAND = "cargo tauri build --no-bundle";

function markdownCodeBlocks(source) {
  return [...source.matchAll(/\x60\x60\x60[^\r\n]*\r?\n([\s\S]*?)\x60\x60\x60/g)]
    .map((match) => match[1])
    .join("\n");
}

function htmlCodeBlocks(source) {
  return [...source.matchAll(/<pre><code>([\s\S]*?)<\/code><\/pre>/g)]
    .map((match) => match[1])
    .join("\n");
}

test("installation docs pin Tauri CLI and require the Tauri build path", () => {
  for (const [name, source] of [
    ["README.md", README],
    ["docs/index.html", LANDING],
  ]) {
    assert.ok(source.includes(INSTALL_CLI), name + " must pin Tauri CLI 2.11.4");
    assert.ok(source.includes(BUILD_COMMAND), name + " must use the Tauri build");
    assert.ok(
      !source.includes("cargo install tauri-cli --locked"),
      name + " must not imply --locked selects a fixed CLI release"
    );
  }

  const executableExamples = [
    markdownCodeBlocks(README),
    htmlCodeBlocks(LANDING),
  ].join("\n");
  assert.doesNotMatch(
    executableExamples,
    /^\s*cargo build --release\b/m,
    "plain cargo release builds must never be presented as an executable install step"
  );
  assert.doesNotMatch(
    executableExamples,
    /^\s*cd\s+(?:\.?[\\/])?src-tauri\b/im,
    "installation commands must stay at repository root so documented artifact paths remain correct"
  );
});

test("Windows source-build prerequisites and both artifacts are explicit", () => {
  for (const term of [
    "stable-x86_64-pc-windows-msvc",
    "Visual Studio Build Tools 2022",
    "MSVC x64/x86",
    "Windows 10/11 SDK",
    "WebView2 Runtime",
  ]) {
    assert.ok(README.includes(term), "README.md is missing Windows prerequisite: " + term);
  }

  for (const artifact of [
    "src-tauri\\target\\release\\lobster-pulse.exe",
    "src-tauri\\target\\release\\lobster-pulse-hook.exe",
  ]) {
    assert.ok(README.includes(artifact), "README.md is missing Windows artifact: " + artifact);
    assert.ok(LANDING.includes(artifact), "docs/index.html is missing Windows artifact: " + artifact);
  }
  assert.ok(README.includes("Get-FileHash -Algorithm SHA256"));
  assert.ok(LANDING.includes("Get-FileHash -Algorithm SHA256"));
});

test("macOS and Linux examples use Unix artifact paths and native hash tools", () => {
  for (const artifact of [
    "src-tauri/target/release/lobster-pulse",
    "src-tauri/target/release/lobster-pulse-hook",
  ]) {
    assert.ok(README.includes(artifact), "README.md is missing Unix artifact: " + artifact);
    assert.ok(LANDING.includes(artifact), "docs/index.html is missing Unix artifact: " + artifact);
  }
  assert.ok(README.includes("shasum -a 256"));
  assert.ok(README.includes("sha256sum"));
  assert.ok(LANDING.includes("shasum -a 256"));
  assert.ok(LANDING.includes("sha256sum"));
});

test("documentation does not make release or Owner distribution decisions", () => {
  const combined = README + "\n" + LANDING;
  assert.doesNotMatch(combined, /SOURCE_ONLY|OWNER DECISION|Owner 決策/);
  assert.doesNotMatch(combined, /git\s+tag\b|git\s+push\s+origin\b/);

  const workflowHasChecksumStep =
    /\b(?:sha256sum|shasum|Get-FileHash|checksum)\b/i.test(RELEASE_WORKFLOW);
  assert.equal(
    workflowHasChecksumStep,
    false,
    "update this guard and the installation wording together if release.yml gains checksums"
  );
  assert.match(README, /release workflow 沒有產生 checksum manifest/);
  assert.match(LANDING, /release workflow 目前沒有建立 checksum manifest/);
  assert.doesNotMatch(combined, /(?:release|workflow)[^.\n]{0,120}(?:含|建立|產生)[^.\n]{0,40}SHA-256 checksum/i);
});

test("landing-page version follows the Tauri application version", () => {
  assert.ok(
    LANDING.includes("版本：<strong>v" + TAURI_CONFIG.version + "</strong>"),
    "landing-page hero must display the Tauri application version"
  );
  assert.ok(
    LANDING.includes("版本：<code>v" + TAURI_CONFIG.version + "</code>"),
    "installation section must display the Tauri application version"
  );
});

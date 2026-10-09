const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const ROOT = path.resolve(__dirname, "..");
const README = fs.readFileSync(path.join(ROOT, "README.md"), "utf8");
const LANDING = fs.readFileSync(path.join(ROOT, "docs", "index.html"), "utf8");
const CLAUDE = fs.readFileSync(path.join(ROOT, "CLAUDE.md"), "utf8");
const PACKAGE = JSON.parse(fs.readFileSync(path.join(ROOT, "package.json"), "utf8"));
const BUILD_WORKFLOW = fs.readFileSync(
  path.join(ROOT, ".github", "workflows", "build.yml"),
  "utf8"
);
const RELEASE_WORKFLOW = fs.readFileSync(
  path.join(ROOT, ".github", "workflows", "release.yml"),
  "utf8"
);
const TAURI_CONFIG = JSON.parse(
  fs.readFileSync(path.join(ROOT, "src-tauri", "tauri.conf.json"), "utf8")
);

const INSTALL_CLI = "cargo install tauri-cli --version 2.11.4 --locked";
const BUILD_COMMAND = "cargo tauri build --no-bundle";
const CARGO_TEST_COMMAND =
  "cargo test --manifest-path src-tauri/Cargo.toml --locked";
const DOC_GUARD_COMMAND =
  "node --test test/version-consistency.test.js test/installation-docs.test.js";

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

function markdownBlockList(source) {
  return [...source.matchAll(/\x60\x60\x60[^\r\n]*\r?\n([\s\S]*?)\x60\x60\x60/g)]
    .map((match) => match[1]);
}

function htmlBlockList(source) {
  return [...source.matchAll(/<pre><code>([\s\S]*?)<\/code><\/pre>/g)]
    .map((match) => match[1]);
}

function normalizeCommand(line) {
  return line.trim().replace(/\s+/g, " ");
}

function auditWorkingDirectories(name, blocks) {
  let buildCount = 0;
  for (const block of blocks) {
    let cwd = ".";
    for (const rawLine of block.split(/\r?\n/)) {
      const line = normalizeCommand(rawLine);
      if (!line || line.startsWith("#")) continue;

      const cd = /^cd\s+(.+)$/i.exec(line);
      if (cd) {
        cwd = cd[1].replace(/^['"]|['"]$/g, "").replace(/[\\/]+$/, "");
        continue;
      }

      if (line === BUILD_COMMAND) {
        buildCount += 1;
        assert.equal(cwd, ".", name + ": Tauri build must run from repository root");
      }

      if (line.startsWith("cargo test")) {
        assert.equal(cwd, ".", name + ": Cargo test must run from repository root");
        assert.equal(line, CARGO_TEST_COMMAND);
        const manifest = line.split(" ")[3];
        assert.ok(
          fs.existsSync(path.join(ROOT, ...manifest.split("/"))),
          name + ": documented Cargo manifest must exist"
        );
      }

      if (/[\\/]?target[\\/]release[\\/]lobster-pulse/.test(line)) {
        assert.equal(cwd, ".", name + ": artifact checks must run from repository root");
        assert.match(
          line,
          /src-tauri[\\/]target[\\/]release[\\/]lobster-pulse/,
          name + ": release artifacts must retain the src-tauri prefix"
        );
      }
    }
  }
  assert.ok(buildCount > 0, name + ": no executable Tauri build command found");
}

function workflowNamedStep(source, name) {
  const lines = source.split(/\r?\n/);
  const marker = "- name: " + name;
  const start = lines.findIndex((line) => line.trim() === marker);
  assert.notEqual(start, -1, "missing workflow step: " + name);
  const indent = lines[start].search(/\S/);
  const body = [];
  for (let index = start + 1; index < lines.length; index += 1) {
    const line = lines[index];
    const trimmed = line.trim();
    const currentIndent = line.search(/\S/);
    if (
      trimmed &&
      currentIndent === indent &&
      (trimmed.startsWith("- name:") || trimmed.startsWith("- uses:"))
    ) break;
    body.push(line);
  }
  return body.join("\n");
}

test("installation docs pin Tauri CLI and require the Tauri build path", () => {
  for (const [name, source] of [
    ["README.md", README],
    ["docs/index.html", LANDING],
    ["CLAUDE.md", CLAUDE],
  ]) {
    assert.ok(source.includes(INSTALL_CLI), name + " must pin Tauri CLI 2.11.4");
    assert.ok(source.includes(BUILD_COMMAND), name + " must use the Tauri build");
    assert.ok(
      !source.includes("cargo install tauri-cli --locked"),
      name + " must not imply --locked selects a fixed CLI release"
    );
    assert.ok(
      source.includes("tauri-cli@^2.0"),
      name + " must disclose the wider existing CI/release range"
    );
  }

  assert.ok(BUILD_WORKFLOW.includes("tauri-cli@^2.0"));
  assert.ok(RELEASE_WORKFLOW.includes("tauri-cli@^2.0"));

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

test("copy-paste commands remain valid from repository root", () => {
  auditWorkingDirectories("README.md", markdownBlockList(README));
  auditWorkingDirectories("docs/index.html", htmlBlockList(LANDING));

  for (const [name, source] of [
    ["README.md", README],
    ["docs/index.html", LANDING],
  ]) {
    assert.ok(
      source.includes(CARGO_TEST_COMMAND),
      name + " must publish the root-runnable Cargo test command"
    );
    assert.doesNotMatch(
      source,
      /cargo test --locked/,
      name + " must not publish the root-invalid shorthand"
    );
  }
});

test("Build CI runs only the offline documentation and version guards", () => {
  assert.equal(PACKAGE.scripts.test, DOC_GUARD_COMMAND);

  const step = workflowNamedStep(
    BUILD_WORKFLOW,
    "Test installation documentation and version guards"
  );
  assert.match(step, /if:\s*matrix\.platform == 'ubuntu-22\.04'/);
  const run = /^\s*run:\s*(.+)$/m.exec(step);
  assert.ok(run, "documentation guard step must have a run command");
  assert.equal(normalizeCommand(run[1]), DOC_GUARD_COMMAND);
  assert.doesNotMatch(step, /npm install|npm ci|runtime-sync|codex-runtime|profile|hook/i);

  const guardedRuns = [...BUILD_WORKFLOW.matchAll(/^\s*run:\s*(node --test .+)$/gm)]
    .map((match) => normalizeCommand(match[1]))
    .filter((command) => command === DOC_GUARD_COMMAND);
  assert.equal(guardedRuns.length, 1);
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

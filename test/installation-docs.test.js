const { test } = require("node:test");
const assert = require("node:assert/strict");
const { execFileSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
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
    RELEASE_WORKFLOW.includes("python scripts/package_release.py");
  assert.ok(workflowHasChecksumStep, "release workflow must use the tested archive/checksum packager");
  assert.match(README, /zip\.sha256/);
  assert.match(LANDING, /zip\.sha256/);
  assert.match(README, /尚未發佈/);
  assert.match(LANDING, /尚未發佈/);
});

test("release workflow validates tags before building and uploads archive checksums as a draft", () => {
  const validation = workflowNamedStep(RELEASE_WORKFLOW, "Validate release tag and versions");
  assert.match(validation, /GITHUB_REF_TYPE/);
  assert.match(validation, /tag/);
  assert.match(validation, /python scripts\/package_release\.py "\$GITHUB_REF_NAME"/);
  assert.ok(RELEASE_WORKFLOW.indexOf("Validate release tag and versions") <
    RELEASE_WORKFLOW.indexOf("- name: Build (no bundle)"));
  assert.match(RELEASE_WORKFLOW, /python -m unittest scripts\/test_package_release\.py/);
  const upload = workflowNamedStep(RELEASE_WORKFLOW, "Upload to GitHub Release");
  assert.match(upload, /\.zip\s*\n/);
  assert.match(upload, /\.zip\.sha256/);
  assert.match(upload, /draft:\s*true/);
  assert.match(upload, /fail_on_unmatched_files:\s*true/);
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

test(
  "release notes use the CHANGELOG section that matches the tag",
  { skip: process.platform === "win32" && "runs the workflow step with a POSIX bash" },
  () => {
    const lines = RELEASE_WORKFLOW.split(/\r?\n/);
    const step = lines.findIndex((line) => line.includes("name: Extract changelog section"));
    const run = lines.findIndex((line, i) => i > step && /^\s+run: \|$/.test(line));
    assert.ok(step >= 0 && run > step, "release.yml must keep the changelog extraction step");
    const indent = lines[run + 1].match(/^ */)[0].length;
    const body = [];
    for (const line of lines.slice(run + 1)) {
      if (line.trim() && line.match(/^ */)[0].length < indent) break;
      body.push(line.slice(indent));
    }
    const script = body.join("\n").replace("${{ github.ref_name }}", "$GITHUB_REF_NAME");
    const notesFor = (tag, changelog) => {
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), "release-notes-"));
      try {
        fs.writeFileSync(path.join(dir, "CHANGELOG.md"), changelog);
        execFileSync("bash", ["-c", script], {
          cwd: dir,
          env: { ...process.env, GITHUB_REF_NAME: tag },
        });
        return fs.readFileSync(path.join(dir, "release-notes.md"), "utf8");
      } finally {
        fs.rmSync(dir, { recursive: true, force: true });
      }
    };

    const tag = "v" + TAURI_CONFIG.version;
    const notes = notesFor(tag, fs.readFileSync(path.join(ROOT, "CHANGELOG.md"), "utf8"));
    assert.ok(notes.startsWith("## " + tag + " "), notes.slice(0, 200));
    assert.doesNotMatch(notes, /\n## /, "notes must stop at the next CHANGELOG section");

    const fixture = "# Changelog\r\n\r\n## [v1.2.0]\r\n- bracketed\r\n\r\n## v1.2 · older\r\n- prefix only\r\n";
    assert.match(notesFor("v1.2.0", fixture), /^## \[v1\.2\.0\]\n- bracketed\n/);
    assert.match(notesFor("v1.2", fixture), /^## v1\.2 · older\n- prefix only\n/);
    assert.match(notesFor("v9.9.9", fixture), /were not found in CHANGELOG\.md/);
  }
);

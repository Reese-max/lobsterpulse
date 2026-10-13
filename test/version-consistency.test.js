// test/version-consistency.test.js
// 護欄：App 的三處版本來源必須指向同一個正式版本。
// package.json（前端/NPM）、src-tauri/tauri.conf.json（Tauri 套件）與
// src-tauri/Cargo.toml（Rust 套件）必須一致，才能對齊 CHANGELOG.md 的
// v0.5.x 段，避免發行時出現版本落差。
const { test } = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");

const ROOT = path.resolve(__dirname, "..");

function readPackageJsonVersion() {
  const raw = fs.readFileSync(path.join(ROOT, "package.json"), "utf8");
  const pkg = JSON.parse(raw);
  return pkg.version;
}

function readTauriConfigVersion() {
  const raw = fs.readFileSync(path.join(ROOT, "src-tauri", "tauri.conf.json"), "utf8");
  const cfg = JSON.parse(raw);
  return cfg.version;
}

function readCargoVersion() {
  const raw = fs.readFileSync(path.join(ROOT, "src-tauri", "Cargo.toml"), "utf8");
  const m = /^version\s*=\s*"([^"]+)"$/m.exec(raw);
  if (!m) {
    throw new Error("src-tauri/Cargo.toml: version field not found");
  }
  return m[1];
}

test("版本契約：package.json / tauri.conf.json / Cargo.toml 指向同一個 App 版本", () => {
  const pkgVersion = readPackageJsonVersion();
  const tauriVersion = readTauriConfigVersion();
  const cargoVersion = readCargoVersion();

  assert.strictEqual(
    pkgVersion,
    tauriVersion,
    `package.json version (${pkgVersion}) 必須與 Tauri 設定版本 (${tauriVersion}) 一致；請以 src-tauri/ 的正式版本為基準調整 package.json`
  );
  assert.strictEqual(
    cargoVersion,
    tauriVersion,
    `Cargo.toml version (${cargoVersion}) 必須與 Tauri 設定版本 (${tauriVersion}) 一致；桌面版 App 的正式版本由 src-tauri/ 統一`
  );
});

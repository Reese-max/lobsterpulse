#!/usr/bin/env node
// 把本機執行期檔案同步進 repo（單向：本機 → repo）。
//
// 為什麼要有這支：quota 數字全由 ~/.lobsterpulse/scripts/*.js 產生，但那個目錄
// 不在任何 git repo 內，也沒有遠端備份——機器掛了就重寫。config.json 同理，
// 但它含 Discord bot token，只能存去敏感化的範本。
//
//   node runtime/sync-from-local.mjs          # 同步
//   node runtime/sync-from-local.mjs --check  # 只檢查有無漂移（CI/commit 前用）
//
// 還原到新機器：把 runtime/scripts/* 複製到 ~/.lobsterpulse/scripts/，
// 再把 config.template.json 複製成 %APPDATA%\lobsterpulse\config.json 並填回
// 被清空的欄位（見下方 SECRET_KEY_RE）。

import fs from "node:fs";
import path from "node:path";
import os from "node:os";

const HOME = os.homedir();
const SRC_SCRIPTS = path.join(HOME, ".lobsterpulse", "scripts");
const SRC_CONFIG = path.join(process.env.APPDATA || "", "lobsterpulse", "config.json");
const REPO = path.resolve(import.meta.dirname);
const DST_SCRIPTS = path.join(REPO, "scripts");
const DST_CONFIG = path.join(REPO, "config.template.json");

// 值會被清空的鍵。刻意窄：provider_sounds 之類含 "bot" 的鍵不該被清掉。
const SECRET_KEY_RE = /(token|secret|webhook|password|api_?key)/i;

const checkOnly = process.argv.includes("--check");
const drift = [];

// 行尾正規化後再比：repo 檔案經 git checkout 會變成 CRLF，本機檔是 LF，
// 直接比字串會永遠判定漂移（踩雷 §21）。內容一樣就是一樣。
const lf = (s) => s.replace(/\r\n/g, "\n");

function put(dst, content, label) {
  const old = fs.existsSync(dst) ? fs.readFileSync(dst, "utf8") : null;
  if (old !== null && lf(old) === lf(content)) return;
  drift.push(label);
  if (!checkOnly) fs.writeFileSync(dst, content);
}

// 1) runner 腳本
fs.mkdirSync(DST_SCRIPTS, { recursive: true });
const keep = new Set();
for (const f of fs.readdirSync(SRC_SCRIPTS)) {
  // 排除備份檔：命名有 .bak-YYYYMMDD 也有 .bak2-YYYYMMDD，比對 ".bak" 就好
  if (!/\.(js|cmd)$/.test(f) || f.includes(".bak")) continue;
  keep.add(f);
  put(path.join(DST_SCRIPTS, f), fs.readFileSync(path.join(SRC_SCRIPTS, f), "utf8"), `scripts/${f}`);
}
// 本機已刪除的檔案，repo 也要跟著刪，否則會留下誤導人的殭屍腳本
for (const f of fs.readdirSync(DST_SCRIPTS)) {
  if (keep.has(f)) continue;
  drift.push(`scripts/${f} (本機已無)`);
  if (!checkOnly) fs.rmSync(path.join(DST_SCRIPTS, f));
}

// 2) Copilot hook 設定（獨立小檔、全是我們自己的內容，可原樣備份；
//    Claude/Gemini 的 hook 混在各自的大 settings.json 裡且含其他設定，不動）
const COPILOT_HOOKS = path.join(HOME, ".copilot", "hooks", "lobster.json");
if (fs.existsSync(COPILOT_HOOKS)) {
  fs.mkdirSync(path.join(REPO, "hooks"), { recursive: true });
  put(
    path.join(REPO, "hooks", "copilot-lobster.json"),
    fs.readFileSync(COPILOT_HOOKS, "utf8"),
    "hooks/copilot-lobster.json"
  );
}

// 3) config 範本（去敏感化）
// 物件鍵排序輸出：config 由 Rust HashMap 序列化，每次啟動鍵順序都不同，
// 不正規化的話每重啟一次就被判定一次漂移（純噪音，會讓人開始忽略這個守門）。
// 陣列不動——usage_runners 的順序就是卡片顯示順序，有意義。
function sanitize(node, key = "") {
  if (Array.isArray(node)) return node.map((v) => sanitize(v));
  if (node && typeof node === "object") {
    return Object.fromEntries(
      Object.entries(node)
        .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
        .map(([k, v]) => [k, sanitize(v, k)])
    );
  }
  if (typeof node === "string" && SECRET_KEY_RE.test(key)) return "";
  return node;
}
if (fs.existsSync(SRC_CONFIG)) {
  const clean = sanitize(JSON.parse(fs.readFileSync(SRC_CONFIG, "utf8")));
  const text = JSON.stringify(clean, null, 2) + "\n";
  // 保險：去敏感化後仍出現金鑰樣式就中止，寧可不同步也不外洩
  const leak = text.match(/(sk-[A-Za-z0-9]{20,}|gh[pous]_[A-Za-z0-9]{20,}|discord(app)?\.com\/api\/webhooks)/);
  if (leak) {
    console.error(`中止：去敏感化後仍偵測到疑似金鑰（${leak[0].slice(0, 12)}…），請擴充 SECRET_KEY_RE`);
    process.exit(2);
  }
  put(DST_CONFIG, text, "config.template.json");
}

if (drift.length === 0) {
  console.log("同步：無差異");
} else if (checkOnly) {
  console.error("本機與 repo 有差異，請跑 node runtime/sync-from-local.mjs：\n  " + drift.join("\n  "));
  process.exit(1);
} else {
  console.log("已同步：\n  " + drift.join("\n  "));
}

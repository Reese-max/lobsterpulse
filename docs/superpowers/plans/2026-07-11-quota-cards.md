# Quota 卡片面板 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 LobsterPulse 展開面板的 quota runner 區塊（`#quota-bar`）改版為仿 menu-bar 截圖的卡片式面板（Session/Weekly 進度條 + % left + Resets in 倒數 + 7d sparkline，依資料自動分級）。

**Architecture:** 純前端方案 A。純邏輯（欄位 normalize、reset 字串 parse）抽到新檔 `src/quota-cards-lib.js`（UMD 樣式：瀏覽器掛 `window.QuotaCards`，Node 走 `module.exports`）以便用 Node 內建 test runner 做 TDD；渲染與互動加在 `src/main.js`（單檔慣例）；樣式獨立 `src/lp-quota-cards.css`。只替換 `#quota-bar` 的渲染呼叫點，dashboard bot 卡的 mini ring 與 Rust 端完全不動。

**Tech Stack:** Vanilla JS（無框架無 bundler）、Canvas 2D sparkline、CSS custom properties 主題、`node --test`（Node 內建，零依賴）、Tauri 2。

**Spec:** `docs/superpowers/specs/2026-07-11-quota-cards-design.md`

## Global Constraints

- 工作分支：`feature/quota-cards`（自 `auto-dev/burn-20260601` 切出）
- **不動 Rust**：`src-tauri/` 任何檔案零修改；資料來源沿用 `read_usage_snapshots` / `get_live_quota_snapshot` / `get_quota_history` 三個既有 invoke
- **不可破壞面**（Explore 盤點 2026-07-11）：
  - `renderQuotaRunner`（src/main.js:1470-1490）**保留不動**——dashboard bot 卡（:1616 `#bot-card-quota-*`）仍在用；只替換 `#quota-bar` 呼叫點（:1985 一帶）
  - `selectQuotaSnapshot`（:1448）/ `isQuotaSnapshotStale`（:1443）/ `window.__lastQuotaSnapshots` 形狀不動
  - 5 個 raw %-欄位讀取集（`session_5h_remaining, week_7d_remaining, h5_remaining, wk_remaining, remaining_pct`）語意不變
  - `updateCapsuleQuota`（:2458-2497）與 `#capsule-quota` 的 `warn`/`crit`/`hidden` class 語意不動；膠囊點擊跳轉（:2527 的 `.quota-runner[data-provider]` selector）改為優先找 `.quota-card[data-provider]`、fallback 舊 selector
- 卡片配色只用 `src/styles.css:1-45` 既有主題變數（`--accent`、`--text*`、`--border`、`--capsule-bg`、`--hover`、`--radius`），**不 hardcode 截圖配色**；警示紅可用固定 `#e5484d`（深淺主題皆可讀）
- 警示門檻：任一窗剩餘 < 20% → bar 轉紅 + 🔥（僅視覺；推播沿用既有 quota_low rule，不新做）
- 收合狀態存 localStorage，key 前綴 `lp-qc-collapsed:`（codebase 首次用 localStorage，全部 try/catch 包裹）
- Extra Usage 金額列不做（spec §3 明確不做）
- JS 純邏輯測試：`node --test test/quota-cards-lib.test.js`（repo 根執行；package.json 無 `"type":"module"` → CommonJS `require` 可用）
- Build 驗證必用 `cargo tauri build --no-bundle`（README「Build SOP」：純 cargo build 會白屏）
- commit 格式：`type(scope): description`

## File Structure

```
src/
  quota-cards-lib.js     # 新：純邏輯（parseResetDuration / normalizeRunnerCard），無 DOM
  lp-quota-cards.css     # 新：卡片樣式（主題變數驅動）
  index.html             # 改：加 css link + lib script tag
  main.js                # 改：renderQuotaCard/收合/sparkline/呼叫點替換/膠囊 selector
test/
  quota-cards-lib.test.js  # 新：node --test 單元測試
```

---

### Task 0: 建立工作分支

**Files:** 無（git 操作）

- [ ] **Step 1: 確認乾淨並切分支**

```bash
cd "/d/Users/Administrator/Desktop/監控"
git status --short          # 預期空
git checkout -b feature/quota-cards
git branch --show-current   # 預期 feature/quota-cards
```

---

### Task 1: 純邏輯層 quota-cards-lib.js（TDD）

**Files:**
- Create: `src/quota-cards-lib.js`
- Test: `test/quota-cards-lib.test.js`

**Interfaces:**
- Produces（Task 3/4 依賴，全域 `window.QuotaCards.*`）:
  - `parseResetDuration(s: string) -> string | null`——`"3h19m"`→`"3h 19m"`、`"41h29m"`→`"41h 29m"`、≥48h 轉天 `"166h20m"`→`"6d 22h"`、`"45m"`→`"0h 45m"`；parse 失敗/非字串/零時長回 `null`
  - `normalizeRunnerCard(r) -> card`——輸入 runner `{name,label,color,ok,raw}`，輸出：
    - full 卡：`{kind:"full", name, label, color, subtitle, windows:[{key:"session",label:"Session",remainPct,resetText},{key:"weekly",...}], pct}`（pct = 兩窗最小值取整）
    - simple 卡：`{kind:"simple", name, label, color, subtitle:"", windows:[{key:"quota",label:"Quota",remainPct,resetText:null}], pct}`
    - 無資料：`{kind:"none", name}`
    - claude 形狀：`session_5h_* + week_7d_*`，subtitle=`raw.tier`；codex 形狀：`h5_* + wk_*`，subtitle=`raw.plan`；只有零散 % → simple；`ok===false`/無 raw/無合法 % → none

- [ ] **Step 1: 寫失敗測試**

```js
// test/quota-cards-lib.test.js
const { test } = require("node:test");
const assert = require("node:assert");
const { parseResetDuration, normalizeRunnerCard } = require("../src/quota-cards-lib.js");

test("parseResetDuration 基本格式與天數轉換", () => {
  assert.strictEqual(parseResetDuration("3h19m"), "3h 19m");
  assert.strictEqual(parseResetDuration("41h29m"), "41h 29m");
  assert.strictEqual(parseResetDuration("166h20m"), "6d 22h");
  assert.strictEqual(parseResetDuration("45m"), "0h 45m");
  assert.strictEqual(parseResetDuration("garbage"), null);
  assert.strictEqual(parseResetDuration(""), null);
  assert.strictEqual(parseResetDuration(undefined), null);
  assert.strictEqual(parseResetDuration("0h0m"), null);
});

test("claude 形狀 -> full 卡（tier 副標）", () => {
  const c = normalizeRunnerCard({
    name: "claude", label: "🤖 Claude Code (Max)", color: "#d97757", ok: true,
    raw: { session_5h_remaining: 64, session_5h_reset: "3h19m",
           week_7d_remaining: 61, week_7d_reset: "41h29m", tier: "Claude Max" },
  });
  assert.strictEqual(c.kind, "full");
  assert.strictEqual(c.subtitle, "Claude Max");
  assert.strictEqual(c.windows.length, 2);
  assert.strictEqual(c.windows[0].label, "Session");
  assert.strictEqual(c.windows[0].remainPct, 64);
  assert.strictEqual(c.windows[0].resetText, "3h 19m");
  assert.strictEqual(c.windows[1].remainPct, 61);
  assert.strictEqual(c.pct, 61);
});

test("codex 形狀 -> full 卡（plan 副標）", () => {
  const c = normalizeRunnerCard({
    name: "codex", label: "🟢 OpenAI Codex CLI", ok: true,
    raw: { h5_remaining: 36, h5_reset: "3h20m", wk_remaining: 90,
           wk_reset: "166h20m", plan: "prolite" },
  });
  assert.strictEqual(c.kind, "full");
  assert.strictEqual(c.subtitle, "prolite");
  assert.strictEqual(c.windows[1].resetText, "6d 22h");
  assert.strictEqual(c.pct, 36);
});

test("單一 remaining_pct -> simple 卡", () => {
  const c = normalizeRunnerCard({ name: "cicx", label: "🤖 CICX", ok: true,
                                  raw: { remaining_pct: 55 } });
  assert.strictEqual(c.kind, "simple");
  assert.strictEqual(c.windows.length, 1);
  assert.strictEqual(c.windows[0].remainPct, 55);
  assert.strictEqual(c.windows[0].resetText, null);
  assert.strictEqual(c.pct, 55);
});

test("ok:false / 無 raw / 無合法欄位 -> none", () => {
  assert.strictEqual(normalizeRunnerCard({ name: "x", ok: false, raw: { h5_remaining: 5 } }).kind, "none");
  assert.strictEqual(normalizeRunnerCard({ name: "x", ok: true }).kind, "none");
  assert.strictEqual(normalizeRunnerCard({ name: "x", ok: true, raw: { foo: 1, h5_remaining: 999 } }).kind, "none");
  assert.strictEqual(normalizeRunnerCard(null).kind, "none");
});

test("reset 壞字串 -> full 卡照出、resetText null", () => {
  const c = normalizeRunnerCard({
    name: "claude", label: "C", ok: true,
    raw: { session_5h_remaining: 50, session_5h_reset: "???",
           week_7d_remaining: 40, week_7d_reset: null, tier: "Max" },
  });
  assert.strictEqual(c.kind, "full");
  assert.strictEqual(c.windows[0].resetText, null);
  assert.strictEqual(c.windows[1].resetText, null);
});
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `node --test test/quota-cards-lib.test.js`
Expected: FAIL（Cannot find module '../src/quota-cards-lib.js'）

- [ ] **Step 3: 實作 quota-cards-lib.js**

```js
// src/quota-cards-lib.js
// Quota 卡片純邏輯層（無 DOM）：瀏覽器掛 window.QuotaCards；node --test 走 module.exports。
(function (root) {
  "use strict";

  // "3h19m" -> "3h 19m"；>=48h 轉天；零時長/壞格式回 null
  function parseResetDuration(s) {
    if (typeof s !== "string") return null;
    const m = s.trim().match(/^(?:(\d+)h)?(?:(\d+)m)?$/);
    if (!m || (m[1] === undefined && m[2] === undefined)) return null;
    const h = parseInt(m[1] || "0", 10);
    const min = parseInt(m[2] || "0", 10);
    if (h === 0 && min === 0) return null;
    if (h >= 48) return `${Math.floor(h / 24)}d ${h % 24}h`;
    return `${h}h ${min}m`;
  }

  function _pct(v) {
    return typeof v === "number" && v >= 0 && v <= 100 ? v : null;
  }

  function normalizeRunnerCard(r) {
    if (!r || r.ok === false || !r.raw) return { kind: "none", name: (r && r.name) || "" };
    const raw = r.raw;
    const base = { name: r.name, label: r.label || r.name, color: r.color || "" };
    const sessA = _pct(raw.session_5h_remaining);
    const weekA = _pct(raw.week_7d_remaining);
    const sessB = _pct(raw.h5_remaining);
    const weekB = _pct(raw.wk_remaining);

    let windows = null;
    let subtitle = "";
    if (sessA !== null && weekA !== null) {
      windows = [
        { key: "session", label: "Session", remainPct: sessA, resetText: parseResetDuration(raw.session_5h_reset) },
        { key: "weekly", label: "Weekly", remainPct: weekA, resetText: parseResetDuration(raw.week_7d_reset) },
      ];
      subtitle = raw.tier || "";
    } else if (sessB !== null && weekB !== null) {
      windows = [
        { key: "session", label: "Session", remainPct: sessB, resetText: parseResetDuration(raw.h5_reset) },
        { key: "weekly", label: "Weekly", remainPct: weekB, resetText: parseResetDuration(raw.wk_reset) },
      ];
      subtitle = raw.plan || "";
    }
    if (windows) {
      const pct = Math.round(Math.min(windows[0].remainPct, windows[1].remainPct));
      return Object.assign({ kind: "full", subtitle, windows, pct }, base);
    }
    const singles = [sessA, weekA, sessB, weekB, _pct(raw.remaining_pct)].filter(function (v) { return v !== null; });
    if (singles.length === 0) return { kind: "none", name: r.name };
    const pct = Math.round(Math.min.apply(null, singles));
    return Object.assign({
      kind: "simple", subtitle: "",
      windows: [{ key: "quota", label: "Quota", remainPct: pct, resetText: null }],
      pct,
    }, base);
  }

  const api = { parseResetDuration, normalizeRunnerCard };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.QuotaCards = api;
})(typeof window !== "undefined" ? window : globalThis);
```

- [ ] **Step 4: 跑測試確認通過**

Run: `node --test test/quota-cards-lib.test.js`
Expected: 6 tests pass

- [ ] **Step 5: Commit**

```bash
git add src/quota-cards-lib.js test/quota-cards-lib.test.js
git commit -m "feat(ui): quota 卡片純邏輯層（normalize + reset parse，node --test）"
```

---

### Task 2: 卡片樣式 + index.html 接線

**Files:**
- Create: `src/lp-quota-cards.css`
- Modify: `src/index.html:12-14`（link 區）與 main.js 的 script tag 之前

**Interfaces:**
- Produces: class 名供 Task 3 模板使用：`.quota-card`（root，帶 `data-provider`）、`.qc-collapsed`、`.qc-header`、`.qc-title`、`.qc-subtitle`、`.qc-summary`、`.qc-chevron`、`.qc-window`、`.qc-warn`、`.qc-window-label`、`.qc-bar`、`.qc-bar-fill`、`.qc-window-meta`、`.qc-trend`、`.qc-spark`

- [ ] **Step 1: 寫 lp-quota-cards.css**

```css
/* Quota 卡片面板（仿 menu-bar 截圖，配色走主題變數；深淺主題自動適應） */
.quota-card {
  background: var(--capsule-bg);
  border: 1px solid var(--border);
  border-radius: var(--radius, 10px);
  padding: 10px 12px;
  margin: 6px 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.quota-card.stale { opacity: 0.55; }
.qc-header {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  user-select: none;
  -webkit-user-select: none;
}
.qc-title { font-weight: 600; color: var(--text); font-size: 13px; }
.qc-subtitle { color: var(--text-dim); font-size: 11px; }
.qc-chevron { margin-left: auto; color: var(--text-muted); font-size: 11px; }
.qc-summary {
  margin-left: auto;
  color: var(--text-secondary);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}
.qc-collapsed .qc-chevron { margin-left: 8px; }
.qc-window { display: flex; flex-direction: column; gap: 3px; }
.qc-window-label { color: var(--text-secondary); font-size: 12px; }
.qc-bar {
  height: 6px;
  border-radius: 3px;
  background: var(--hover);
  overflow: hidden;
}
.qc-bar-fill {
  height: 100%;
  border-radius: 3px;
  background: var(--accent);
  transition: width 0.3s ease;
}
.qc-warn .qc-bar-fill { background: #e5484d; }
.qc-window-meta {
  display: flex;
  justify-content: space-between;
  color: var(--text-dim);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}
.qc-trend { display: flex; flex-direction: column; gap: 2px; }
.qc-trend.hidden { display: none; }
.qc-spark { width: 100%; height: 28px; display: block; }
```

- [ ] **Step 2: index.html 接線**

在 `src/index.html` 的 `<link rel="stylesheet" href="lp-patch-v3.css">`（約 :14）之後加：

```html
    <link rel="stylesheet" href="lp-quota-cards.css">
```

在載入 `main.js` 的 `<script>` tag **之前**加（lib 必須先於 main.js 取得全域）：

```html
    <script src="quota-cards-lib.js"></script>
```

- [ ] **Step 3: 驗證 HTML 引用無誤**

Run: `rg -n "lp-quota-cards|quota-cards-lib" src/index.html`
Expected: 兩行命中，且 script 行在 main.js script 行之前（用 `rg -n "\.js" src/index.html` 對照順序）

- [ ] **Step 4: Commit**

```bash
git add src/lp-quota-cards.css src/index.html
git commit -m "feat(ui): quota 卡片樣式與 index.html 接線"
```

---

### Task 3: main.js 渲染層——卡片模板、收合、呼叫點替換、膠囊 selector 遷移

**Files:**
- Modify: `src/main.js`（新增函式放在 `renderQuotaRunner`（:1470-1490）之後；替換 :1985 一帶 `#quota-bar` 渲染；改 :2527 膠囊跳轉 selector；init 區掛一次性 click delegation）

**Interfaces:**
- Consumes: `window.QuotaCards.normalizeRunnerCard`（Task 1）、Task 2 的 class 名、既有 `selectQuotaSnapshot`/`isQuotaSnapshotStale`/`refreshQuotas`
- Produces: `renderQuotaCard(card, {stale}) -> string`；`qcCollapsed(name) -> bool`；`qcSetCollapsed(name, v)`；佔位函式 `drawQuotaCardSparks(names)`（本 task 先放 no-op，Task 4 實作——避免前向引用炸掉）。卡片 root 為 `<div class="quota-card" data-provider="${name}">`

- [ ] **Step 1: 新增渲染與收合函式（放 renderQuotaRunner 定義之後）**

```js
// ===== Quota 卡片面板（spec: docs/superpowers/specs/2026-07-11-quota-cards-design.md）=====
const QC_COLLAPSE_PREFIX = "lp-qc-collapsed:";

function qcCollapsed(name) {
  try { return localStorage.getItem(QC_COLLAPSE_PREFIX + name) === "1"; }
  catch (_) { return false; }
}

function qcSetCollapsed(name, v) {
  try {
    if (v) localStorage.setItem(QC_COLLAPSE_PREFIX + name, "1");
    else localStorage.removeItem(QC_COLLAPSE_PREFIX + name);
  } catch (_) { /* localStorage 不可用時收合僅存活於當次 render */ }
}

function renderQcWindow(w) {
  const warn = w.remainPct < 20;
  const reset = w.resetText ? `Resets in ${w.resetText}` : "—";
  return `
    <div class="qc-window${warn ? " qc-warn" : ""}">
      <div class="qc-window-label">${w.label}${warn ? " 🔥" : ""}</div>
      <div class="qc-bar"><div class="qc-bar-fill" style="width:${w.remainPct}%"></div></div>
      <div class="qc-window-meta"><span>${w.remainPct}% left</span><span>${reset}</span></div>
    </div>`;
}

function renderQuotaCard(card, { stale = false } = {}) {
  const collapsed = qcCollapsed(card.name);
  const sub = card.subtitle ? `<span class="qc-subtitle">${card.subtitle}</span>` : "";
  const staleCls = stale ? " stale" : "";
  if (collapsed) {
    return `<div class="quota-card qc-collapsed${staleCls}" data-provider="${card.name}">
      <div class="qc-header" data-qc-toggle="${card.name}">
        <span class="qc-title">${card.label}</span>${sub}
        <span class="qc-summary">${card.pct}%</span>
        <span class="qc-chevron">▸</span>
      </div>
    </div>`;
  }
  const spark = card.kind === "full"
    ? `<div class="qc-trend">
        <span class="qc-window-label">Usage Trend</span>
        <canvas class="qc-spark" data-qc-spark="${card.name}" width="240" height="28"></canvas>
      </div>`
    : "";
  return `<div class="quota-card${staleCls}" data-provider="${card.name}">
    <div class="qc-header" data-qc-toggle="${card.name}">
      <span class="qc-title">${card.label}</span>${sub}
      <span class="qc-chevron">▾</span>
    </div>
    ${card.windows.map(renderQcWindow).join("")}
    ${spark}
  </div>`;
}

// Task 4 才實作 sparkline；先佔位避免呼叫點炸掉
function drawQuotaCardSparks(_names) {}
```

- [ ] **Step 2: 替換 `#quota-bar` 渲染呼叫點（src/main.js:1985 一帶，refreshQuotas 內）**

找到把 `renderQuotaRunner(..., { includeProvider: true })` join 進 `#quota-bar` 的那段（其 stale 判斷變數沿用該段既有的），整段替換為：

```js
      const cards = runners
        .map((r) => window.QuotaCards.normalizeRunnerCard(r))
        .filter((c) => c.kind !== "none");
      quotaBar.innerHTML = cards.map((c) => renderQuotaCard(c, { stale })).join("");
      drawQuotaCardSparks(cards.filter((c) => c.kind === "full").map((c) => c.name));
```

（變數名 `runners`/`quotaBar`/`stale` 以該段實際名稱為準——只換渲染方式，資料來源與 stale 邏輯一行不動。dashboard 的 :1616 呼叫點**不要動**。）

- [ ] **Step 3: 收合 click delegation（掛在 init 流程中既有 DOM ready 區塊，`#quota-bar` 只掛一次）**

```js
  document.getElementById("quota-bar").addEventListener("click", (e) => {
    const t = e.target.closest("[data-qc-toggle]");
    if (!t) return;
    const name = t.getAttribute("data-qc-toggle");
    qcSetCollapsed(name, !qcCollapsed(name));
    refreshQuotas();
  });
```

- [ ] **Step 4: 膠囊跳轉 selector 遷移（src/main.js:2527）**

原：

```js
    const el = document.querySelector(`.quota-runner[data-provider="${prov}"]`);
```

改為（卡片優先、舊 selector fallback）：

```js
    const el = document.querySelector(`.quota-card[data-provider="${prov}"]`)
      || document.querySelector(`.quota-runner[data-provider="${prov}"]`);
```

（若該行實際寫法略異，以「先找 .quota-card 再 fallback .quota-runner」的語意改寫。）

- [ ] **Step 5: 邏輯層測試不迴歸**

Run: `node --test test/quota-cards-lib.test.js`
Expected: 6 pass

- [ ] **Step 6: Commit**

```bash
git add src/main.js
git commit -m "feat(ui): quota 卡片渲染/收合/呼叫點替換，膠囊跳轉遷移至 .quota-card"
```

---

### Task 4: 7d sparkline（Canvas）

**Files:**
- Modify: `src/main.js`（把 Task 3 的 no-op `drawQuotaCardSparks` 換成實作，並加 `drawQcSpark`）

**Interfaces:**
- Consumes: 既有 invoke `get_quota_history`（無參數，回 `{ runnerName: [[unix_ts, pct], ...] }`，見 src-tauri/src/lib.rs:3216）；Task 3 產生的 `canvas[data-qc-spark="${name}"]`
- Produces: `drawQuotaCardSparks(names: string[])`（async，5 分鐘記憶體快取）；`drawQcSpark(canvas, series)`

- [ ] **Step 1: 實作（替換 no-op）**

```js
let __qcHistCache = { ts: 0, data: null };

async function drawQuotaCardSparks(names) {
  if (!names || names.length === 0) return;
  try {
    const now = Date.now();
    if (!__qcHistCache.data || now - __qcHistCache.ts > 300000) {
      __qcHistCache = { ts: now, data: await invoke("get_quota_history") };
    }
    const hist = __qcHistCache.data || {};
    const cutoff = Math.floor(now / 1000) - 7 * 86400;
    for (const name of names) {
      const canvas = document.querySelector(`canvas[data-qc-spark="${name}"]`);
      if (!canvas) continue;
      const series = (hist[name] || []).filter((pt) => pt[0] >= cutoff);
      if (series.length === 0) {
        const wrap = canvas.closest(".qc-trend");
        if (wrap) wrap.classList.add("hidden");   // spec §4：無歷史 -> 該列隱藏
        continue;
      }
      drawQcSpark(canvas, series);
    }
  } catch (e) {
    console.warn("quota card sparkline 失敗", e);
  }
}

function drawQcSpark(canvas, series) {
  const ctx = canvas.getContext("2d");
  const W = canvas.width;
  const H = canvas.height;
  ctx.clearRect(0, 0, W, H);
  const accent = getComputedStyle(document.documentElement)
    .getPropertyValue("--accent").trim() || "#f93";
  const n = Math.min(series.length, 60);
  const pts = series.slice(-n);
  const slot = W / n;
  const bw = Math.max(2, Math.floor(slot) - 1);
  ctx.fillStyle = accent;
  ctx.globalAlpha = 0.85;
  pts.forEach((pt, i) => {
    const used = 100 - pt[1];   // 畫「使用量」高度，對齊截圖語意（用越多柱越高）
    const h = Math.max(1, Math.round((used / 100) * (H - 2)));
    ctx.fillRect(Math.round(i * slot), H - h, bw, h);
  });
  ctx.globalAlpha = 1;
}
```

- [ ] **Step 2: 邏輯層測試不迴歸**

Run: `node --test test/quota-cards-lib.test.js`
Expected: 6 pass

- [ ] **Step 3: Commit**

```bash
git add src/main.js
git commit -m "feat(ui): quota 卡片 7d sparkline（canvas + 5min 快取）"
```

---

### Task 5: Build + 實機視覺驗收（三狀態）

**Files:** 無新檔（驗收 + 可能的微調 fix commit）

**Interfaces:**
- Consumes: 全部前置 task

- [ ] **Step 1: 前端 guard 腳本**

```bash
cd "/d/Users/Administrator/Desktop/監控"
npm run test:provider-inventory
```
Expected: PASS（本改版不動 provider 清單，此 guard 必須仍綠）

- [ ] **Step 2: Release build（README Build SOP）**

```bash
cd src-tauri && cargo tauri build --no-bundle
```
Expected: 成功產出 `src-tauri/target/release/lobster-pulse.exe`（若 build 失敗回 BLOCKED 附完整錯誤）

- [ ] **Step 3: 實機視覺驗收——正常狀態**

注意：機器上可能已有 lobster-pulse 在跑（port 19380）。先確認：若在跑，請使用者手動關閉或跳過重啟（**不可盲殺**，硬規則見 CLAUDE.md 踩雷 §6）；無法重啟時改用 `cargo tauri dev` 起第二實例驗證（dev 用不同 port 檔）。

啟動後展開面板，確認並截圖：
- claude / codex 顯示完整卡（Session/Weekly 雙 bar、% left、Resets in、Usage Trend sparkline、tier/plan 副標）
- 有 usage snapshot 的 OpenAB bot 顯示簡化卡（單 bar）
- grokx/lpbot/mimo/openx 無卡
- 收合 chevron 點擊可收合/展開，重開 app 後收合狀態保留（localStorage）
- 膠囊 quota 摘要點擊可跳轉捲動到對應卡片

- [ ] **Step 4: 警示狀態驗證（假資料注入）**

暫時備份並修改 `~/.lobsterpulse/usage-local.json`：把 claude 的 `session_5h_remaining` 改成 `12`，等 15 秒輪詢刷新（或重開面板），確認該 Session bar 轉紅 + 🔥、% left 顯示 12%。截圖後**還原備份**並確認畫面恢復。

```bash
cp ~/.lobsterpulse/usage-local.json ~/.lobsterpulse/usage-local.json.bak-e2e
python -c "
import json, pathlib
p = pathlib.Path.home() / '.lobsterpulse' / 'usage-local.json'
d = json.loads(p.read_text(encoding='utf-8'))
for r in d['runners']:
    if r['name'] == 'claude':
        r['raw']['session_5h_remaining'] = 12
p.write_text(json.dumps(d, ensure_ascii=False), encoding='utf-8')
print('injected')
"
# ...截圖後：
mv ~/.lobsterpulse/usage-local.json.bak-e2e ~/.lobsterpulse/usage-local.json
```

（注意：usage poller 可能在下一輪覆寫此檔——15 秒內完成觀察即可；被覆寫恰好代表資料流活著，重注入一次再看。）

- [ ] **Step 5: 深淺主題檢查**

Settings 切換 light theme，確認卡片文字/bar/邊框在淺色下可讀（全部主題變數驅動應自動適應），截圖。切回原主題。

- [ ] **Step 6: 不迴歸清單**

- Dashboard bot 卡的 mini quota ring 照常（renderQuotaRunner 未動）
- Trend grid（7/30 天切換 + CSV 匯出）照常
- 膠囊 quota 摘要（`#capsule-quota`）照常更新
- `node --test test/quota-cards-lib.test.js` 全綠

- [ ] **Step 7: 結案 commit（若驗收中有微調）**

```bash
git status --short   # 嚴格核對變更清單
git add <實際微調檔>
git commit -m "fix(ui): quota 卡片視覺驗收微調"
```

（無微調則跳過；驗收證據〔截圖路徑 + guard/build 輸出〕記入報告。）

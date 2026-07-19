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

test("無 raw / 無合法欄位 -> none", () => {
  assert.strictEqual(normalizeRunnerCard({ name: "x", ok: true }).kind, "none");
  assert.strictEqual(normalizeRunnerCard({ name: "x", ok: true, raw: { foo: 1, h5_remaining: 999 } }).kind, "none");
  assert.strictEqual(normalizeRunnerCard(null).kind, "none");
});

test("ok:false 有 raw 且有合法 % -> 降級 simple 卡，failed: true", () => {
  const c = normalizeRunnerCard({ name: "x", ok: false, raw: { h5_remaining: 5 } });
  assert.strictEqual(c.kind, "simple");
  assert.strictEqual(c.failed, true);
  assert.strictEqual(c.pct, 5);
});

test("ok:false 無 raw -> none", () => {
  assert.strictEqual(normalizeRunnerCard({ name: "x", ok: false }).kind, "none");
});

test("ok:false 有 raw 但無合法 % -> none", () => {
  assert.strictEqual(normalizeRunnerCard({ name: "x", ok: false, raw: { foo: 1 } }).kind, "none");
});

test("正常 simple/full 卡 failed 為 falsy", () => {
  const full = normalizeRunnerCard({
    name: "claude", label: "C", ok: true,
    raw: { session_5h_remaining: 64, week_7d_remaining: 61, tier: "Max" },
  });
  const simple = normalizeRunnerCard({ name: "cicx", label: "C", ok: true, raw: { remaining_pct: 55 } });
  assert.ok(!full.failed);
  assert.ok(!simple.failed);
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

test("部分窗（只有 weekly）-> full 卡單窗", () => {
  const c = normalizeRunnerCard({
    name: "codex", label: "Codex CLI", ok: true,
    raw: { week_7d_remaining: 91, week_7d_reset: "164h33m", tier: "Pro" },
  });
  assert.strictEqual(c.kind, "full");
  assert.strictEqual(c.windows.length, 1);
  assert.strictEqual(c.windows[0].label, "Weekly");
  assert.strictEqual(c.windows[0].remainPct, 91);
  assert.strictEqual(c.pct, 91);
  assert.strictEqual(c.subtitle, "Pro");
});

test("部分窗（只有 session alias）-> full 卡單窗", () => {
  const c = normalizeRunnerCard({
    name: "x", label: "X", ok: true,
    raw: { h5_remaining: 40, h5_reset: "1h5m", plan: "Free" },
  });
  assert.strictEqual(c.kind, "full");
  assert.strictEqual(c.windows.length, 1);
  assert.strictEqual(c.windows[0].label, "Session");
  assert.strictEqual(c.pct, 40);
});

test("recentCompletions 過濾/去重/排序/上限", () => {
  const { recentCompletions } = require("../src/quota-cards-lib.js");
  const ev = (name, provider, sid, iso) => ({ event_name: name, provider, session_id: sid, timestamp: iso });
  const events = [
    ev("Stop", "claude", "s1", "2026-07-19T10:00:00Z"),
    ev("Stop", "claude", "s1", "2026-07-19T10:05:00Z"),   // 同 session 較新 → 取這筆
    ev("SessionEnd", "codex", "s2", "2026-07-19T10:03:00Z"),
    ev("UserPromptSubmit", "claude", "s3", "2026-07-19T10:09:00Z"), // 非完成事件 → 濾掉
    ev("Stop", "irisx_bot", "s4", "2026-07-19T10:08:00Z"),          // bot 不在白名單 → 濾掉
    ev("Stop", "copilot", "s5", "bad-timestamp"),                    // 壞時間 → 濾掉
  ];
  const rows = recentCompletions(events, ["claude", "codex", "copilot"]);
  assert.deepStrictEqual(
    rows.map((r) => [r.provider, r.ts]),
    [
      ["claude", Date.parse("2026-07-19T10:05:00Z")],
      ["codex", Date.parse("2026-07-19T10:03:00Z")],
    ]
  );
});

test("recentCompletions 上限 8 筆與空輸入", () => {
  const { recentCompletions } = require("../src/quota-cards-lib.js");
  const many = Array.from({ length: 12 }, (_, i) => ({
    event_name: "Stop", provider: "claude", session_id: "s" + i,
    timestamp: new Date(Date.UTC(2026, 6, 19, 10, i)).toISOString(),
  }));
  assert.strictEqual(recentCompletions(many, ["claude"]).length, 8);
  assert.deepStrictEqual(recentCompletions(null, ["claude"]), []);
  assert.deepStrictEqual(recentCompletions(many, null), []);
});

test("recentCompletions 帶出 cwd（取最新一筆的）", () => {
  const { recentCompletions } = require("../src/quota-cards-lib.js");
  const rows = recentCompletions([
    { event_name: "Stop", provider: "codex", session_id: "s1", timestamp: "2026-07-19T10:00:00Z", cwd: "D:/舊專案" },
    { event_name: "Stop", provider: "codex", session_id: "s1", timestamp: "2026-07-19T10:05:00Z", cwd: "D:/Users/x/監控" },
    { event_name: "Stop", provider: "claude", session_id: "s2", timestamp: "2026-07-19T10:01:00Z" }, // 無 cwd
  ], ["codex", "claude"]);
  assert.strictEqual(rows[0].cwd, "D:/Users/x/監控", "同 session 取最新那筆的 cwd");
  assert.strictEqual(rows[1].cwd, null, "無 cwd 回 null");
});

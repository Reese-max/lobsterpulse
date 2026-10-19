const { test } = require("node:test");
const assert = require("node:assert");
const {
  parseResetDuration,
  normalizeRunnerCard,
  normalizeCodexModelUsage,
} = require("../src/quota-cards-lib.js");

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

test("agy 未開始計時的窗 -> resetNote，不給滾動倒數；模型列也帶得到", () => {
  const c = normalizeRunnerCard({
    name: "agy", label: "Antigravity", ok: true,
    raw: {
      h5_remaining: 100, h5_reset: null, h5_idle: true,
      wk_remaining: 96, wk_reset: "75h12m", wk_idle: false,
      plan: "Google AI Pro",
      models: [
        { name: "Gemini Models", h5_remaining: 100, h5_reset: null, h5_idle: true, wk_remaining: 96, wk_reset: "75h12m", wk_idle: false },
        { name: "Claude and GPT models", h5_remaining: 100, h5_reset: null, h5_idle: true, wk_remaining: 100, wk_reset: null, wk_idle: true },
      ],
    },
  });
  assert.strictEqual(c.subtitle, "Google AI Pro");
  assert.strictEqual(c.windows[0].resetNote, "Not started");
  assert.strictEqual(c.windows[0].resetText, null);
  assert.strictEqual(c.windows[1].resetNote, null, "已開始的窗不加註記");
  assert.strictEqual(c.windows[1].resetText, "3d 3h");
  assert.strictEqual(c.models.length, 2);
  assert.strictEqual(c.models[1].windows[1].resetNote, "Not started");
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

test("basis 透傳到主卡，models 子卡繼承", () => {
  const c = normalizeRunnerCard({
    name: "codex", label: "Codex CLI", ok: true,
    raw: {
      basis: "provider_api",
      h5_remaining: 75,
      models: [{ name: "Spark", h5_remaining: 60 }],
    },
  });
  assert.strictEqual(c.basis, "provider_api");
  assert.strictEqual(c.models.length, 1);
  assert.strictEqual(c.models[0].basis, "provider_api");
});

test("codex rollout 逐模型用量保留無價模型並依 token 排序", () => {
  const rows = normalizeCodexModelUsage([
    { name: " gpt-5.4 ", total_tokens: 10, estimated_cost_usd: 0.01 },
    { name: "gpt-5.6-sol", total_tokens: 1000 },
    { name: "", total_tokens: 9999, estimated_cost_usd: 99 },
    { name: "broken", total_tokens: NaN },
  ]);

  assert.deepStrictEqual(rows, [
    {
      name: "gpt-5.6-sol",
      total_tokens: 1000,
      estimated_cost_usd: null,
    },
    {
      name: "gpt-5.4",
      total_tokens: 10,
      estimated_cost_usd: 0.01,
    },
  ]);
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

test("waitingSessions 只留等待中的本機 CLI、等最久在前", () => {
  const { waitingSessions } = require("../src/quota-cards-lib.js");
  const rows = waitingSessions([
    { id: "a", provider: "claude", state: "waiting_for_user", last_event_secs_ago: 30 },
    { id: "b", provider: "claude", state: "working", last_event_secs_ago: 5 },
    { id: "c", provider: "codex", state: "waiting_for_user", last_event_secs_ago: 300 },
    { id: "d", provider: "openx", state: "waiting_for_user", last_event_secs_ago: 999 }, // bot 不進來
    null,
  ], ["claude", "codex"]);
  assert.deepStrictEqual(rows.map((r) => r.id), ["c", "a"], "等最久在前、非等待與 bot 排除");
});

test("waitingSessions 空輸入安全", () => {
  const { waitingSessions } = require("../src/quota-cards-lib.js");
  assert.deepStrictEqual(waitingSessions(null, ["claude"]), []);
  assert.deepStrictEqual(waitingSessions([{ id: "a", provider: "claude", state: "waiting_for_user" }], null), []);
});

test("statsFreshness 判過期：停更數月/缺日期視為過期，昨天算新鮮", () => {
  const { statsFreshness } = require("../src/quota-cards-lib.js");
  assert.deepStrictEqual(statsFreshness("2026-07-20", "2026-07-20"), { stale: false, days: 0 });
  assert.deepStrictEqual(statsFreshness("2026-07-19", "2026-07-20"), { stale: false, days: 1 });
  assert.deepStrictEqual(statsFreshness("2026-07-18", "2026-07-20"), { stale: true, days: 2 });
  assert.strictEqual(statsFreshness("2026-04-10", "2026-07-20").stale, true);
  assert.strictEqual(statsFreshness("2026-04-10", "2026-07-20").days, 101);
  assert.deepStrictEqual(statsFreshness(null, "2026-07-20"), { stale: true, days: null });
  assert.deepStrictEqual(statsFreshness("壞掉", "2026-07-20"), { stale: true, days: null });
});

test("providerUsageUrl / keyManageUrl 對應正確且未知的回 null", () => {
  const { providerUsageUrl, keyManageUrl } = require("../src/quota-cards-lib.js");
  assert.strictEqual(providerUsageUrl("openrouter"), "https://openrouter.ai/activity");
  assert.strictEqual(providerUsageUrl("claude"), "https://claude.ai/settings/usage");
  assert.strictEqual(providerUsageUrl("不存在的家"), null);
  // 金鑰名的變體都要對到同一家（A~E 後綴、DIRECT 變體）
  assert.strictEqual(keyManageUrl("OPENROUTER_API_KEY_A"), "https://openrouter.ai/settings/keys");
  assert.strictEqual(keyManageUrl("OPENROUTER_API_KEY"), "https://openrouter.ai/settings/keys");
  assert.ok(keyManageUrl("MINIMAX_DIRECT_KEY").startsWith("https://platform.minimax.io/"));
  assert.strictEqual(keyManageUrl("GROQ_API_KEY_3"), "https://console.groq.com/keys");
  assert.strictEqual(keyManageUrl("KAGGLE_API_TOKEN"), null, "沒對照的不要亂給連結");
  assert.strictEqual(keyManageUrl(undefined), null);
});

test("apiKeyStatusTag：有效／無效／被擋各自分色，unchecked 與 expired 不畫", () => {
  const { apiKeyStatusTag } = require("../src/quota-cards-lib.js");
  assert.strictEqual(apiKeyStatusTag({ status: "valid", detail: "" }).cls, "apikey-ok");
  const bad = apiKeyStatusTag({ status: "invalid", detail: "HTTP 401：供應商不認這把金鑰" });
  assert.strictEqual(bad.cls, "apikey-bad");
  assert.strictEqual(bad.title, "HTTP 401：供應商不認這把金鑰", "tooltip 帶後端說明");
  assert.strictEqual(apiKeyStatusTag({ status: "blocked", detail: "x" }).cls, "apikey-warn", "被擋網路不是紅色");
  assert.strictEqual(apiKeyStatusTag({ status: "unchecked", detail: "" }), null);
  assert.strictEqual(apiKeyStatusTag({ status: "expired", detail: "" }), null, "過期由到期日標籤負責，不重複");
  assert.strictEqual(apiKeyStatusTag(undefined), null);
});

test("apiKeyExpiryTag：過期標紅、未過期標日期、非 JWT 不畫", () => {
  const { apiKeyExpiryTag } = require("../src/quota-cards-lib.js");
  const exp = Date.UTC(2026, 9, 26, 4, 0, 0) / 1000; // 10/26 中午（UTC+8）
  const future = apiKeyExpiryTag(exp, exp - 3600);
  assert.strictEqual(future.cls, "apikey-used");
  assert.match(future.text, /^到期 10\/26$/);
  const past = apiKeyExpiryTag(exp, exp + 1);
  assert.strictEqual(past.cls, "apikey-bad");
  assert.match(past.text, /已過期 10\/26/);
  assert.strictEqual(apiKeyExpiryTag(null, 0), null);
  assert.strictEqual(apiKeyExpiryTag(undefined, 0), null);
});

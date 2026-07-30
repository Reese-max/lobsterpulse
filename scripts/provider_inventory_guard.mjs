import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const mainJs = readFileSync(resolve(root, "src", "main.js"), "utf8");

const EXPECTED_OPENAB_BOTS = [
  "cicx",
  "gitx",
  "giminix",
  "codex_bot",
  "openx",
  "irisx_bot",
  "grokx",
  "lpbot",
  "mimo",
];

const EXPECTED_LOCAL_PROVIDERS = ["claude", "codex", "copilot", "gemini"];

function fail(message) {
  console.error(`[provider-inventory-guard] ${message}`);
  process.exitCode = 1;
}

function parseArrayConst(name) {
  const match = mainJs.match(new RegExp(`const\\s+${name}\\s*=\\s*\\[([^\\]]*)\\]`));
  if (!match) return null;
  return [...match[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
}

function assertSameArray(name, actual, expected) {
  if (!actual) {
    fail(`missing const ${name}`);
    return;
  }
  const sameLength = actual.length === expected.length;
  const sameValues = sameLength && actual.every((v, i) => v === expected[i]);
  if (!sameValues) {
    fail(`${name} drift: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

assertSameArray("OPENAB_BOTS", parseArrayConst("OPENAB_BOTS"), EXPECTED_OPENAB_BOTS);
assertSameArray("LOCAL_PROVIDERS", parseArrayConst("LOCAL_PROVIDERS"), EXPECTED_LOCAL_PROVIDERS);

if (!/const\s+PROVIDER_ORDER\s*=\s*\[\s*\.\.\.OPENAB_BOTS\s*,\s*\.\.\.LOCAL_PROVIDERS\s*\]/.test(mainJs)) {
  fail("PROVIDER_ORDER must be derived from OPENAB_BOTS + LOCAL_PROVIDERS");
}

if (!/renderDashboardGrid\("bot-grid",\s*OPENAB_BOTS,\s*sessions\)/.test(mainJs)) {
  fail("dashboard bot-grid must render OPENAB_BOTS, not a stale inline list");
}

if (!/renderDashboardGrid\("local-grid",\s*LOCAL_PROVIDERS,\s*sessions\)/.test(mainJs)) {
  fail("dashboard local-grid must render LOCAL_PROVIDERS, not a stale inline list");
}

if (!/const\s+openabBots\s*=\s*OPENAB_BOTS\s*;/.test(mainJs)) {
  fail("events view must use OPENAB_BOTS for OpenAB tabs");
}

if (!/const\s+TIMELINE_KNOWN_PROVIDERS\s*=\s*PROVIDER_ORDER\s*;/.test(mainJs)) {
  fail("timeline must use PROVIDER_ORDER instead of a second provider list");
}

if (/:\s*\[\s*"cicx"\s*\]/.test(mainJs)) {
  fail("empty capsule state must not synthesize a cicx provider icon");
}

if (!/timeline_recorded_event_count/.test(mainJs)) {
  fail("timeline must check recorded event count before rendering idle-filled snapshots");
}

if (/stateLabel\s*=\s*"離線"/.test(mainJs)) {
  fail("dashboard cards with no sessions must not claim provider is offline");
}

if (/PROVIDER_ICONS\[providerId\]\s*\|\|\s*PROVIDER_ICONS\.claude/.test(mainJs)) {
  fail("unknown provider icons must not fallback to Claude, which mislabels unknown data");
}

if (/\(evt\s*&&\s*evt\.payload\)\s*\|\|\s*"cicx"/.test(mainJs)) {
  fail("missing task event payload must not fallback to CICX fake provider");
}

if (!/const\s+QUOTA_STALE_SECONDS\s*=\s*3600\s*;/.test(mainJs)) {
  fail("quota snapshots need an explicit 1h stale threshold before rendering runners");
}

if (!/function\s+quotaSnapshotAgeSeconds\s*\(/.test(mainJs) || !/function\s+isQuotaSnapshotStale\s*\(/.test(mainJs)) {
  fail("quota rendering must derive stale state from snapshot updated_at");
}

if (!/function\s+renderQuotaRunner\s*\(/.test(mainJs)) {
  fail("quota runner rendering must go through a shared helper so stale handling cannot drift");
}

if (!/function\s+selectQuotaSnapshot\s*\(/.test(mainJs)) {
  fail("quota source selection must be centralized so capsule and expanded quota do not disagree");
}

if (!/stale\s*\?\s*null\s*:\s*runnerPct\(r\)/.test(mainJs)) {
  fail("stale quota snapshots must not render percent rings or warn/crit classes as fresh quota");
}

if (!/cls\s*\+=\s*" stale"/.test(mainJs) || !/quota-runner-stale/.test(mainJs)) {
  fail("stale quota runners must be visibly marked instead of looking like normal runner data");
}

if (!/renderQuotaRunner\(r,\s*\{[^}]*stale:\s*isQuotaSnapshotStale\(snap\)/s.test(mainJs)) {
  fail("bot-card quota runners must pass their snapshot stale state into renderQuotaRunner");
}

if (!/const\s+globalStale\s*=\s*isQuotaSnapshotStale\(representativeSnap\)/.test(mainJs)) {
  fail("global quota row must derive stale state from representative snapshot");
}

if (!/renderQuotaCard\(c,\s*\{\s*stale:\s*globalStale\s*\}\)/.test(mainJs)) {
  fail("global quota runners must pass representative snapshot stale state into renderQuotaCard");
}

if (!/const\s+selectedQuota\s*=\s*selectQuotaSnapshot\(snapshots\)/.test(mainJs)) {
  fail("global quota row must use selectQuotaSnapshot instead of an inline stale provider fallback list");
}

// 2026-07-17 定案：capsule 極簡，不放任何額度資訊；badge 必須無條件隱藏並清空 provider 狀態。
if (!/function\s+updateCapsuleQuota\s*\(\)\s*\{[\s\S]*?badge\.classList\.add\("hidden"\);[\s\S]*?delete\s+badge\.dataset\.provider;/m.test(mainJs)) {
  fail("capsule quota badge must stay unconditionally hidden (2026-07-17 capsule-minimal decision)");
}

if (!/function\s+renderStateUnavailable\s*\(/.test(mainJs)) {
  fail("get_state failures must render an explicit unavailable state instead of preserving stale UI");
}

if (!/lastState\s*=\s*null;[\s\S]*lastStructureJson\s*=\s*"__state_unavailable__"/m.test(mainJs)) {
  fail("state unavailable handling must clear lastState and force the next good state to re-render");
}

if (!/catch\s*\(e\)\s*\{\s*renderStateUnavailable\(e\);/m.test(mainJs)) {
  fail("refreshState must call renderStateUnavailable when get_state fails");
}

if (!/get_recent_events"\);[\s\S]*catch\s*\(e\)\s*\{[\s\S]*capsule-error-dot[\s\S]*dot\.classList\.add\("hidden"\)/m.test(mainJs)) {
  fail("recent failure polling errors must clear the capsule error dot instead of preserving stale alerts");
}

if (!/事件資料來源中斷，暫停顯示舊 event/.test(mainJs)) {
  fail("events view must show datasource interruption instead of treating get_recent_events failure as an empty event list");
}

if (!/catch\s*\(e\)\s*\{[\s\S]*載入失敗:[\s\S]*strip\.innerHTML\s*=\s*"";/m.test(mainJs)) {
  fail("timeline failures must clear the old strip instead of leaving stale timeline rows visible");
}

if (!/const\s+quotaSourcesUnavailable\s*=[\s\S]*snapshotsRes\.status\s*!==\s*"fulfilled"[\s\S]*liveRes\.status\s*!==\s*"fulfilled"/m.test(mainJs)) {
  fail("quota refresh must detect when all quota sources are unavailable");
}

if (!/quota 資料來源中斷：暫停顯示舊額度資料/.test(mainJs)) {
  fail("quota refresh must show datasource interruption instead of reusing stale quota data");
}

if (process.exitCode) process.exit(process.exitCode);
console.log("[provider-inventory-guard] provider inventory is aligned");

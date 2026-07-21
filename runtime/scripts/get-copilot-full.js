#!/usr/bin/env node
// Wrapper: openab/get-copilot-quota.js + 反推月總額度（對齊已知 Copilot 方案）
const { execFileSync } = require('child_process');
const path = require('path');
const home = process.env.USERPROFILE || 'C:/Users/Administrator';

let base = {};
try {
  const out = execFileSync('node',
    [path.join(home, '.lobsterpulse', 'scripts', 'copilot-direct.js')],
    { timeout: 20000, encoding: 'utf8', windowsHide: true });
  base = JSON.parse(out.trim().split('\n').pop());
} catch (e) {
  base = { ok: false, error: 'copilot-script:' + e.message };
}

// 反推 total：used / (1 - remaining_pct/100)，round 到已知方案
function inferTotal(used, remainingPct) {
  if (remainingPct == null || remainingPct >= 100 || used <= 0) return null;
  const raw = used / (1 - remainingPct / 100);
  const KNOWN = [50, 300, 1000, 1500]; // Free, Pro/Biz, Enterprise, Pro+
  let best = KNOWN[0];
  let bestDiff = Math.abs(raw - best);
  for (const v of KNOWN) {
    const d = Math.abs(raw - v);
    if (d < bestDiff) { best = v; bestDiff = d; }
  }
  // 誤差 >15% 則回 raw round（方案可能變動）
  return bestDiff / best > 0.15 ? Math.round(raw) : best;
}

const total = inferTotal(base.used ?? 0, base.remaining_pct);
const planHint = { 50: 'Free', 300: 'Pro/Biz', 1000: 'Enterprise', 1500: 'Pro+' }[total] || '';

const merged = {
  ...base,
  total: total ?? '?',
  plan_hint: planHint,
};
console.log(JSON.stringify(merged));

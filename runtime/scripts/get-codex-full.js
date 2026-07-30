#!/usr/bin/env node
// LP runner：優先讀 codex-cache.json（毫秒級）；cache 缺或超過 6 分鐘舊再直接 call（15s）。
// N/A fallback 優雅化：rate 不可用時顯示 model + effort + threads + reset time
const fs = require('fs');
const { execFileSync } = require('child_process');
const path = require('path');
const home = process.env.USERPROFILE || 'C:/Users/Administrator';
const cache = path.join(home, '.lobsterpulse', 'codex-cache.json');

function readCache() {
  try {
    const st = fs.statSync(cache);
    const age = (Date.now() - st.mtimeMs) / 1000;
    if (age > 360) return null;
    return JSON.parse(fs.readFileSync(cache, 'utf8'));
  } catch { return null; }
}
function directCall() {
  try {
    const out = execFileSync('node',
      [path.join(home, '.lobsterpulse', 'scripts', 'codex-direct.js')],
      { timeout: 25000, encoding: 'utf8', windowsHide: true });
    return JSON.parse(out.trim().split('\n').pop());
  } catch (e) { return { ok: false, error: 'codex-script:' + e.message }; }
}

const base = readCache() || directCall();
const isNA = v => v === 'N/A' || v === undefined || v === null || v === '';
const has5h = !isNA(base.h5_remaining);
const hasWk = !isNA(base.wk_remaining);
const rateAvailable = has5h || hasWk;

let line1, line2;
if (rateAvailable) {
  // rate 正常：主線顯示配額
  // 標籤照 API 實際視窗長度（codex-direct 算好的 session_label/weekly_label），
  // 不存在的視窗整段不畫——舊版寫死「5h —」會讓人以為有個 5h 視窗只是讀不到。
  const segs = [];
  if (has5h) segs.push(`⏱ ${base.session_label || '5h'} **${base.h5_remaining}%**`);
  if (hasWk) segs.push(`📅 ${base.weekly_label || 'Wk'} **${base.wk_remaining}%**`);
  line1 = segs.join(' · ');
  line2 = `🤖 ${base.current_model ?? '-'} · ${base.total_tokens ?? '-'}`;
} else {
  // rate N/A fallback：優雅顯示其他資訊
  line1 = `🤖 ${base.current_model ?? '-'} · ${base.effort ?? '-'} effort`;
  const threadsPart = `🧵 ${base.thread_count ?? 0} threads`;
  const tokensPart = `📊 ${base.total_tokens ?? '-'}`;
  line2 = `${threadsPart} · ${tokensPart}`;
  // 若知道 reset 時間，append 提示
  const resets = [base.h5_reset, base.wk_reset].filter(r => r && r !== 'N/A');
  if (resets.length === 0 && base.note) {
    // API unavailable — 用 note 說明
    line2 += ` · (${base.note})`;
  }
}

const merged = { ...base, basis: 'provider_api', line1, line2 };
console.log(JSON.stringify(merged));

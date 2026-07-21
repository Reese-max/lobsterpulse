#!/usr/bin/env node
// Get Codex CLI usage from ChatGPT backend API + local files.
// Node 18+ native fetch (避免 execSync + cmd.exe ETIMEDOUT 坑)
const fs = require('fs');
const path = require('path');
const CODEX_DIR = path.join(process.env.USERPROFILE || 'C:/Users/Administrator', '.codex');

function fmt(n) { return n >= 1e6 ? (n/1e6).toFixed(1)+'M' : n >= 1e3 ? (n/1e3).toFixed(1)+'K' : String(n); }
function fmtCD(s) { if(s<=0)return'resetting...'; const h=Math.floor(s/3600),m=Math.floor((s%3600)/60); return h>0?`${h}h${m}m`:`${m}m`; }

(async () => {
  try {
    const auth = JSON.parse(fs.readFileSync(path.join(CODEX_DIR, 'auth.json'), 'utf8'));
    const token = auth.tokens?.access_token;
    if (!token) throw new Error('no token');

    let currentModel = 'unknown', effort = 'medium';
    try {
      const cfg = fs.readFileSync(path.join(CODEX_DIR, 'config.toml'), 'utf8');
      const mm = cfg.match(/^model\s*=\s*"([^"]+)"/m);
      const em = cfg.match(/^model_reasoning_effort\s*=\s*"([^"]+)"/m);
      if(mm) currentModel=mm[1];
      if(em) effort=em[1];
    } catch {}

    let totalTokens = 0, threadCount = 0;
    try {
      const { DatabaseSync } = require('node:sqlite');
      const db = new DatabaseSync(path.join(CODEX_DIR, 'state_5.sqlite'), { open: true, readOnly: true });
      const t = db.prepare('SELECT SUM(tokens_used) as total, COUNT(*) as cnt FROM threads WHERE tokens_used > 0').get();
      totalTokens = t.total||0; threadCount = t.cnt||0;
      db.close();
    } catch {}

    // Decode JWT exp 看 token 還有多久過期
    let tokenExpiresIn = null;
    try {
      const payload = JSON.parse(Buffer.from(token.split('.')[1], 'base64').toString('utf8'));
      if (payload.exp) {
        const secLeft = payload.exp - Math.floor(Date.now()/1000);
        tokenExpiresIn = secLeft;
      }
    } catch {}

    let usage, httpCode = 0;
    try {
      const { execFileSync } = require('child_process');
      const out = execFileSync('C:\\Windows\\System32\\curl.exe', [
        '-sS', '-w', '\n__HTTP_CODE__%{http_code}',
        'https://chatgpt.com/backend-api/codex/usage?client_version=0.120.0',
        '-H', `Authorization: Bearer ${token}`,
        '-H', 'User-Agent: codex-cli/0.120.0',
      ], { timeout: 15000, encoding: 'utf8', windowsHide: true });
      const m = out.match(/\n__HTTP_CODE__(\d+)$/);
      httpCode = m ? parseInt(m[1], 10) : 0;
      const body = m ? out.slice(0, m.index) : out;
      if (httpCode >= 400) throw new Error(`HTTP ${httpCode}`);
      usage = JSON.parse(body);
    } catch (e) {
      let note = `API unavailable: ${e.message}`;
      if (httpCode === 401 || /401|Unauthorized/.test(e.message)) {
        note = `⚠ Token 過期 → 請跑 \`codex login\` 重新授權`;
      } else if (httpCode === 403 || /403|Forbidden/.test(e.message)) {
        note = `⚠ 無權限（403）→ 檢查帳號 plan`;
      } else if (tokenExpiresIn !== null && tokenExpiresIn < 0) {
        note = `⚠ Token 已過期 ${Math.abs(tokenExpiresIn/3600).toFixed(1)}h → \`codex login\``;
      } else if (tokenExpiresIn !== null && tokenExpiresIn < 86400) {
        note = `⚠ Token ${(tokenExpiresIn/3600).toFixed(1)}h 後過期 · ${e.message}`;
      }
      console.log(JSON.stringify({
        ok: true, h5_remaining: 'N/A', h5_used: 0, h5_reset: 'N/A',
        wk_remaining: 'N/A', wk_used: 0, wk_reset: 'N/A',
        rate_allowed: false, plan: 'unknown',
        current_model: currentModel, effort,
        total_tokens: fmt(totalTokens), thread_count: threadCount,
        http_code: httpCode,
        token_expires_in_secs: tokenExpiresIn,
        note, ts: new Date().toISOString()
      }));
      process.exit(0);
    }

    // 視窗標籤依 API 實際回傳的 limit_window_seconds 決定，不寫死「5h/Wk」——
    // 2026-07-20 實測 API 只回 primary_window 且 limit_window_seconds=604800（7 天），
    // 舊版硬標成 5h 導致「5h 視窗還有 115 小時才重置」的矛盾顯示；secondary_window
    // 不存在時 `100-(sw.used_percent||0)` 還會憑空生出「Wk 100%」假資料。
    // 現在：有幾個視窗畫幾個（缺的送 null，前端自動不畫），標籤照實際長度算。
    const rl = usage.rate_limit || {};
    const wins = [rl.primary_window, rl.secondary_window]
      .filter(w => w && typeof w.used_percent === 'number')
      .sort((a, b) => (a.limit_window_seconds || 0) - (b.limit_window_seconds || 0));
    const isShort = w => (w.limit_window_seconds || 0) <= 86400; // ≤24h 算短窗
    const shortW = wins.find(isShort) || null;
    const longW = wins.find(w => !isShort(w)) || null;
    // 兩窗同類別時只畫得下一個——留痕跡，別無聲丟資料（無聲造假的另一面）
    const dropped = wins.filter(w => w !== shortW && w !== longW)
      .map(w => `${Math.round((w.limit_window_seconds || 0) / 3600)}h:${w.used_percent}%`);
    const winLabel = (w, fallback) => {
      const s = w && w.limit_window_seconds;
      if (!s) return fallback;
      const h = Math.round(s / 3600);
      return h >= 48 ? `${Math.round(h / 24)}d` : `${h}h`;
    };
    console.log(JSON.stringify({
      ok: true,
      session_label: winLabel(shortW, '5h'),
      weekly_label: winLabel(longW, 'Wk'),
      h5_remaining: shortW ? 100 - shortW.used_percent : null,
      h5_used: shortW ? shortW.used_percent : null,
      h5_reset: shortW ? fmtCD(shortW.reset_after_seconds || 0) : null,
      wk_remaining: longW ? 100 - longW.used_percent : null,
      wk_used: longW ? longW.used_percent : null,
      wk_reset: longW ? fmtCD(longW.reset_after_seconds || 0) : null,
      rate_allowed: rl.allowed,
      plan: ({plus:'Plus',pro:'Pro',prolite:'Pro Lite',business:'Business',team:'Team',enterprise:'Enterprise'})[usage.plan_type]||usage.plan_type||'unknown',
      current_model: currentModel, effort,
      total_tokens: fmt(totalTokens), thread_count: threadCount,
      ...(dropped.length ? { note: `API 回傳同類別視窗多於一個，未顯示：${dropped.join(', ')}` } : {}),
      ts: new Date().toISOString()
    }));
  } catch(e) {
    console.log(JSON.stringify({ ok: false, error: e.message }));
    process.exit(1);
  }
})();

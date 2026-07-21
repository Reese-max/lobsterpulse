#!/usr/bin/env node
// Get Copilot quota via the local copilot-rpc helper.
// Also reads live session cost_totals from disk for per-model breakdown.

const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const TARGET_DIR = path.join(process.env.USERPROFILE || 'C:/Users/Administrator', '.lobsterpulse', 'scripts');
const TIMEOUT_MS = 120000;

function readSessionBreakdown() {
  try {
    const lpDir = path.join(process.env.APPDATA || 'C:/Users/Administrator/AppData/Roaming', 'lobsterpulse');
    const openabDir = path.join(process.env.APPDATA || 'C:/Users/Administrator/AppData/Roaming', 'openab');
    const dir = fs.existsSync(lpDir) && fs.readdirSync(lpDir).some(f=>f.startsWith('cost-totals-')) ? lpDir : openabDir;
    const files = fs.readdirSync(dir).filter((f) => f.startsWith('cost-totals-'));
    let bestPath = null;
    let bestMtime = 0;

    for (const file of files) {
      const fullPath = path.join(dir, file);
      const stat = fs.statSync(fullPath);
      if (stat.mtimeMs > bestMtime) {
        bestMtime = stat.mtimeMs;
        bestPath = fullPath;
      }
    }

    if (!bestPath || Date.now() - bestMtime >= 86400000) {
      return '';
    }

    const totals = JSON.parse(fs.readFileSync(bestPath, 'utf8'));
    const lines = [];
    for (const [model, data] of Object.entries(totals.perModel || {})) {
      const input = data.inputTokens ? `${(data.inputTokens / 1000).toFixed(1)}k in` : '';
      const output = data.outputTokens ? `${(data.outputTokens / 1000).toFixed(1)}k out` : '';
      const cached = data.cacheReadTokens ? `${(data.cacheReadTokens / 1000).toFixed(1)}k cached` : '';
      const tokenSummary = [input, output, cached].filter(Boolean).join(' · ');
      const turnLabel = data.turns === 1 ? 'turn' : 'turns';
      lines.push(`${data.turns} ${turnLabel} **${model}** (${tokenSummary})`);
    }

    return lines.join('\n');
  } catch {
    return '';
  }
}

function main() {
  const child = spawn('node', ['copilot-rpc.js', 'usage'], {
    cwd: TARGET_DIR,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, OPENAB_BOT: '1' },
  });

  let stdout = '';
  let stderr = '';

  child.stdout.on('data', (chunk) => {
    stdout += chunk.toString();
  });
  child.stderr.on('data', (chunk) => {
    stderr += chunk.toString();
  });

  const timer = setTimeout(() => {
    try { child.kill(); } catch {}
    console.log(JSON.stringify({ ok: false, error: 'timeout' }));
    process.exit(1);
  }, TIMEOUT_MS);

  child.on('close', () => {
    clearTimeout(timer);

    const lines = stdout.trim().split(/\r?\n/).filter(Boolean);
    const last = lines.at(-1);
    if (!last) {
      console.log(JSON.stringify({ ok: false, error: stderr.trim() || 'empty output' }));
      process.exit(1);
    }

    let parsed;
    try {
      parsed = JSON.parse(last);
    } catch {
      console.log(JSON.stringify({ ok: false, error: 'non-json output', raw: last }));
      process.exit(1);
    }

    if (!parsed.ok) {
      console.log(JSON.stringify(parsed));
      process.exit(1);
    }

    const quota = parsed.data?.quotaSnapshots?.premium_interactions;
    if (!quota) {
      console.log(JSON.stringify({
        ok: true,
        remaining_pct: 100,
        note: 'no quota data',
        ts: new Date().toISOString(),
      }));
      process.exit(0);
    }

    const resetDate = quota.resetDate ? new Date(quota.resetDate).toLocaleDateString('zh-TW') : '';
    console.log(JSON.stringify({
      ok: true,
      remaining_pct: quota.remainingPercentage,
      used: quota.usedRequests || 0,
      overage: quota.overage || 0,
      reset_date: resetDate,
      session_breakdown: readSessionBreakdown(),
      ts: new Date().toISOString(),
    }));
    process.exit(0);
  });
}

main();

#!/usr/bin/env node
// 每 3 分鐘跑 openab/get-codex-usage.js（15s 調 API）→ 寫 ~/.lobsterpulse/codex-cache.json
const { execFileSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const home = process.env.USERPROFILE;
const cache = path.join(home, '.lobsterpulse', 'codex-cache.json');
const tmp = cache + '.tmp';
try {
  const out = execFileSync('node',
    [path.join(home, '.lobsterpulse', 'scripts', 'codex-direct.js')],
    { timeout: 25000, encoding: 'utf8', windowsHide: true });
  const trimmed = out.trim();
  // 大小閘門 + JSON 合法性
  if (trimmed.length > 50 && trimmed.startsWith('{')) {
    JSON.parse(trimmed); // validate
    fs.writeFileSync(tmp, trimmed);
    fs.renameSync(tmp, cache);
    console.log('ok', trimmed.length);
  } else {
    console.error('invalid codex output');
    process.exit(2);
  }
} catch (e) {
  console.error('err:', e.message);
  process.exit(1);
}

#!/usr/bin/env node
const {execSync} = require('child_process');
const fs = require('fs');
const path = require('path');
const home = process.env.USERPROFILE;
const now = new Date();
const today = `${now.getFullYear()}${String(now.getMonth()+1).padStart(2,'0')}${String(now.getDate()).padStart(2,'0')}`;
const cache = path.join(home, '.lobsterpulse', 'ccusage-today.json');
const tmp = cache + '.tmp';
try {
  const out = execSync(`ccusage daily --json --since ${today}`, {encoding:'utf8', timeout:120000, shell:true, windowsHide:true});
  if (out.length > 100 && out.trim().startsWith('{')) {
    fs.writeFileSync(tmp, out);
    fs.renameSync(tmp, cache);
    console.log('ok', out.length);
  } else {
    console.error('ccusage empty/invalid output');
    process.exit(2);
  }
} catch(e) {
  console.error('err:', e.message);
  process.exit(1);
}

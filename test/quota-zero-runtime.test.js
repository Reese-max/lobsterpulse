const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { normalizeRunnerCard } = require('../src/quota-cards-lib.js');

test('Claude exhaustion keeps both quota windows and the reset time without network access', () => {
  const source = fs.readFileSync(path.join(__dirname, '../runtime/scripts/claude-direct.js'), 'utf8');
  const now = Date.UTC(2026, 9, 24, 13);
  for (const [used, expected, weeklyUsed = used, weeklyExpected = expected] of [['1.03', 0, '0.48', 52], ['1.25', 0], ['1', 0], ['0', 100], ['0.48', 52], ['bad', null], ['-0.2', null], ['Infinity', null], [undefined, null]]) {
    let output, onEnd;
    const req = {
      on() {}, setTimeout() {}, write() {},
      end() {
        respond({ statusCode: 429, headers: {
          'anthropic-ratelimit-unified-5h-utilization': used,
          'anthropic-ratelimit-unified-7d-utilization': weeklyUsed,
          'anthropic-ratelimit-unified-5h-reset': String(now / 1000 + 5400),
          'anthropic-ratelimit-unified-7d-reset': String(now / 1000 + 86400),
        }, on(event, fn) { if (event === 'end') onEnd = fn; } });
        onEnd();
      },
    };
    let respond;
    vm.runInNewContext(source, {
      require(name) {
        if (name === 'https') return { request(options, callback) { respond = callback; return req; } };
        if (name === 'fs') return { readFileSync() { return '{"claudeAiOauth":{"accessToken":"test-only"}}'; } };
        if (name === 'path') return path;
        throw new Error('Unexpected dependency: ' + name);
      },
      process: { env: { USERPROFILE: '/test-home' }, exit(code) { throw new Error('Unexpected exit: ' + code); } },
      Buffer, Date: class extends Date { constructor(...args) { super(...(args.length ? args : [now])); } static now() { return now; } },
      console: { log(value) { output = JSON.parse(value); } },
    });
    if (used === undefined) {
      assert.equal(output.ok, false, '429 without quota headers stays unknown');
      continue;
    }
    assert.equal(output.session_5h_remaining, expected, '5h utilization=' + used);
    assert.equal(output.week_7d_remaining, weeklyExpected, '7d utilization=' + weeklyUsed);
    if (expected !== null) {
      const card = normalizeRunnerCard({ name: 'claude', ok: true, raw: output });
      assert.equal(card.windows.length, 2);
      assert.equal(card.windows[0].remainPct, expected);
      assert.equal(card.windows[0].resetText, '1h 30m');
      assert.equal(card.windows[1].remainPct, weeklyExpected);
      assert.equal(card.windows[1].resetText, '24h 0m');
    }
  }
});

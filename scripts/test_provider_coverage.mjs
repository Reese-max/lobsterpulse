import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { build } = require("../src/provider-coverage.js");
const registry = JSON.parse(readFileSync(new URL("../src/provider-capabilities.json", import.meta.url), "utf8"));
const now = 1_780_000_000;
const enabled = { providers: Object.fromEntries(registry.providers.map(p => [p.id, { enabled: true }])) };
const detected = Object.fromEntries(registry.providers.filter(p => p.scope === "local_cli").map(p => [p.id, true]));

test("registered is not live; disabled, external and unsupported stay visible", () => {
  const config = structuredClone(enabled);
  config.providers.mimo.enabled = false;
  const host = { ...detected, codex: false };
  const coverage = build(registry, config, { provider_totals: {}, sessions: [] }, {}, host, now);
  assert.equal(coverage.dimensions.registered.numerator, 13);
  assert.equal(coverage.dimensions.configured.numerator, 12);
  assert.equal(coverage.dimensions.liveEmitting.numerator, 0);
  assert.equal(coverage.dimensions.nonzeroSessions.numerator, 0);
  assert.equal(coverage.dimensions.quotaObservable.numerator, 0);
  assert.equal(coverage.rows.find(r => r.id === "mimo").healthStatus, "NOT_CONFIGURED");
  assert.equal(coverage.rows.find(r => r.id === "codex").healthStatus, "UNSUPPORTED_ON_THIS_HOST");
  assert.equal(coverage.rows.find(r => r.id === "irisx_bot").quotaStatus, "EXTERNAL_DEPENDENCY");
});

test("recent OpenAB hook is live while stale quota is not observable", () => {
  const state = {
    provider_totals: { irisx_bot: { events_total: 2, last_event_at: new Date((now - 10) * 1000).toISOString() } },
    sessions: [{ provider: "irisx_bot" }],
  };
  const snapshots = { irisx_bot: { updated_at: now - 90000 } };
  const coverage = build(registry, enabled, state, snapshots, detected, now);
  assert.equal(coverage.rows.find(r => r.id === "irisx_bot").healthStatus, "LIVE_EMITTING");
  assert.equal(coverage.rows.find(r => r.id === "irisx_bot").quotaStatus, "STALE");
  assert.equal(coverage.dimensions.liveEmitting.numerator, 1);
  assert.equal(coverage.dimensions.nonzeroSessions.numerator, 1);
  assert.equal(coverage.dimensions.quotaObservable.numerator, 0);
});

test("unavailable state and snapshots remain unknown, not zero coverage", () => {
  const coverage = build(registry, enabled, null, null, detected, now, false);
  assert.equal(coverage.dimensions.liveEmitting.numerator, null);
  assert.equal(coverage.dimensions.nonzeroSessions.numerator, null);
  assert.equal(coverage.dimensions.quotaObservable.numerator, null);
  assert.equal(coverage.rows.find(r => r.id === "cicx").healthStatus, "UNAVAILABLE");
});

test("deprecated provider requires explicit exclusion rather than disappearing", () => {
  const next = structuredClone(registry);
  next.providers.find(p => p.id === "mimo").lifecycle = "deprecated";
  const coverage = build(next, enabled, { provider_totals: {}, sessions: [] }, {}, detected, now);
  assert.equal(coverage.dimensions.registered.denominator, 13);
  assert.equal(coverage.dimensions.liveEmitting.denominator, 12);
  assert.deepEqual(coverage.dimensions.liveEmitting.exclusions,
    [{ id: "mimo", reason: "lifecycle:deprecated" }]);
});

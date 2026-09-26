/* Coverage labels are computed from runtime state. The registry only supplies
 * identities, signal contracts and freshness thresholds. */
(function (root) {
  function ageSeconds(timestamp, nowSeconds) {
    const value = typeof timestamp === "number" ? timestamp : Date.parse(timestamp) / 1000;
    return Number.isFinite(value) && value > 0 ? Math.max(0, nowSeconds - value) : null;
  }

  function build(registry, config, state, snapshots, detected, nowSeconds, snapshotReadAvailable = true) {
    const providers = registry.providers;
    const rows = providers.map((provider) => {
      const id = provider.id;
      const active = provider.lifecycle === "active";
      const configured = config?.providers?.[id]?.enabled === true;
      const hostSupported = provider.scope !== "local_cli" || detected?.[id] === true;
      const total = state?.provider_totals?.[id];
      const eventAge = ageSeconds(total?.last_event_at, nowSeconds);
      const live = active && state != null && (total?.events_total || 0) > 0 &&
        eventAge !== null && eventAge < provider.freshness_seconds;
      const nonzero = active && state != null && (state.sessions || []).some(s => s.provider === id);
      const snap = provider.scope === "local_cli"
        ? ((snapshots?.__local__?.runners || []).some(r => r?.name === id) ? snapshots.__local__ : null)
        : snapshots?.[id];
      const quotaAge = ageSeconds(snap?.updated_at, nowSeconds);
      const quota = active && snapshotReadAvailable && quotaAge !== null &&
        quotaAge < provider.quota_freshness_seconds;
      let healthStatus = "NOT_MONITORED";
      if (!active) healthStatus = "OUT_OF_SCOPE";
      else if (!hostSupported) healthStatus = "UNSUPPORTED_ON_THIS_HOST";
      else if (!configured) healthStatus = "NOT_CONFIGURED";
      else if (state == null) healthStatus = "UNAVAILABLE";
      else if (live) healthStatus = "LIVE_EMITTING";
      else if (eventAge !== null) healthStatus = "STALE";
      else if (provider.scope === "openab_push") healthStatus = "EXTERNAL_DEPENDENCY";
      let quotaStatus = "NOT_MONITORED";
      if (!active) quotaStatus = "OUT_OF_SCOPE";
      else if (!snapshotReadAvailable) quotaStatus = "UNAVAILABLE";
      else if (quota) quotaStatus = "FRESH";
      else if (quotaAge !== null) quotaStatus = "STALE";
      else if (provider.support_level === "hook_intake_quota_external") quotaStatus = "EXTERNAL_DEPENDENCY";
      return { id, configured, live, nonzero, quota, healthStatus, quotaStatus,
        eventAgeSeconds: eventAge, quotaAgeSeconds: quotaAge };
    });
    const active = rows.filter((row, i) => providers[i].lifecycle === "active");
    const excluded = providers.filter(p => p.lifecycle !== "active")
      .map(p => ({ id: p.id, reason: `lifecycle:${p.lifecycle}` }));
    const dimension = (numerator, known = true) => ({
      numerator: known ? numerator : null, denominator: active.length,
      registeredDenominator: rows.length, exclusions: excluded,
    });
    return {
      registryVersion: registry.registry_version, baselineId: registry.baseline_id,
      timestamp: new Date(nowSeconds * 1000).toISOString(), rows,
      dimensions: {
        registered: { numerator: rows.length, denominator: rows.length, registeredDenominator: rows.length, exclusions: [] },
        configured: dimension(active.filter(r => r.configured).length, config != null),
        liveEmitting: dimension(active.filter(r => r.live).length, state != null),
        nonzeroSessions: dimension(active.filter(r => r.nonzero).length, state != null),
        quotaObservable: dimension(active.filter(r => r.quota).length, snapshotReadAvailable),
      },
    };
  }

  const api = { build };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  root.ProviderCoverage = api;
})(typeof window !== "undefined" ? window : globalThis);

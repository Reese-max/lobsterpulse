// Quota 卡片純邏輯層（無 DOM）：瀏覽器掛 window.QuotaCards；node --test 走 module.exports。
(function (root) {
  "use strict";

  // "3h19m" -> "3h 19m"；>=48h 轉天；零時長/壞格式回 null
  function parseResetDuration(s) {
    if (typeof s !== "string") return null;
    const m = s.trim().match(/^(?:(\d+)h)?(?:(\d+)m)?$/);
    if (!m || (m[1] === undefined && m[2] === undefined)) return null;
    const h = parseInt(m[1] || "0", 10);
    const min = parseInt(m[2] || "0", 10);
    if (h === 0 && min === 0) return null;
    if (h >= 48) return `${Math.floor(h / 24)}d ${h % 24}h`;
    return `${h}h ${min}m`;
  }

  function _pct(v) {
    return typeof v === "number" && v >= 0 && v <= 100 ? v : null;
  }

  function normalizeRunnerCard(r) {
    if (!r) return { kind: "none", name: "" };
    if (r.ok === false) {
      // 降級路徑：ok:false 但有 raw 且有合法 % -> 簡化卡（帶 failed: true）；否則 none。
      const raw = r.raw;
      if (!raw) return { kind: "none", name: r.name || "" };
      const base = { name: r.name, label: r.label || r.name, color: r.color || "" };
      const singles = [
        _pct(raw.session_5h_remaining), _pct(raw.week_7d_remaining),
        _pct(raw.h5_remaining), _pct(raw.wk_remaining), _pct(raw.remaining_pct),
      ].filter(function (v) { return v !== null; });
      if (singles.length === 0) return { kind: "none", name: r.name || "" };
      const pct = Math.round(Math.min.apply(null, singles));
      return Object.assign({
        kind: "simple", subtitle: "",
        windows: [{ key: "quota", label: "Quota", remainPct: pct, resetText: null }],
        pct, failed: true,
      }, base);
    }
    if (!r.raw) return { kind: "none", name: r.name || "" };
    const raw = r.raw;
    const base = { name: r.name, label: r.label || r.name, color: r.color || "" };
    const sessA = _pct(raw.session_5h_remaining);
    const weekA = _pct(raw.week_7d_remaining);
    const sessB = _pct(raw.h5_remaining);
    const weekB = _pct(raw.wk_remaining);

    let windows = null;
    let subtitle = "";
    if (sessA !== null && weekA !== null) {
      windows = [
        { key: "session", label: "Session", remainPct: sessA, resetText: parseResetDuration(raw.session_5h_reset) },
        { key: "weekly", label: "Weekly", remainPct: weekA, resetText: parseResetDuration(raw.week_7d_reset) },
      ];
      subtitle = raw.tier || "";
    } else if (sessB !== null && weekB !== null) {
      windows = [
        { key: "session", label: "Session", remainPct: sessB, resetText: parseResetDuration(raw.h5_reset) },
        { key: "weekly", label: "Weekly", remainPct: weekB, resetText: parseResetDuration(raw.wk_reset) },
      ];
      subtitle = raw.plan || "";
    }
    if (windows) {
      const pct = Math.round(Math.min(windows[0].remainPct, windows[1].remainPct));
      return Object.assign({ kind: "full", subtitle, windows, pct, failed: false }, base);
    }
    const singles = [sessA, weekA, sessB, weekB, _pct(raw.remaining_pct)].filter(function (v) { return v !== null; });
    if (singles.length === 0) return { kind: "none", name: r.name };
    const pct = Math.round(Math.min.apply(null, singles));
    return Object.assign({
      kind: "simple", subtitle: "",
      windows: [{ key: "quota", label: "Quota", remainPct: pct, resetText: null }],
      pct, failed: false,
    }, base);
  }

  const api = { parseResetDuration, normalizeRunnerCard };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.QuotaCards = api;
})(typeof window !== "undefined" ? window : globalThis);

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
      const base = { name: r.name, label: r.label || r.name, color: r.color || "", basis: raw.basis };
      const singles = [
        _pct(raw.session_5h_remaining), _pct(raw.week_7d_remaining),
        _pct(raw.h5_remaining), _pct(raw.wk_remaining), _pct(raw.remaining_pct),
      ].filter(function (v) { return v !== null; });
      if (singles.length === 0) return { kind: "none", name: r.name || "", basis: raw.basis };
      const pct = Math.round(Math.min.apply(null, singles));
      return Object.assign({
        kind: "simple", subtitle: "",
        windows: [{ key: "quota", label: "Quota", remainPct: pct, resetText: null }],
        pct, failed: true,
      }, base);
    }
    if (!r.raw) return { kind: "none", name: r.name || "" };
    const raw = r.raw;
    const base = { name: r.name, label: r.label || r.name, color: r.color || "", basis: raw.basis };
    // raw.models[]：逐模型獨立額度（如 Codex Spark、agy Opus/Flash），遞迴正規化成子卡。
    const models = Array.isArray(raw.models)
      ? raw.models.map(function (model) {
          if (!model || typeof model !== "object") return null;
          const card = normalizeRunnerCard(Object.assign({}, r, {
            label: model.name || r.label || r.name,
            raw: Object.assign({ basis: raw.basis }, model),
          }));
          return card.kind === "none" ? null : card;
        }).filter(Boolean)
      : [];
    const withModels = function (card) {
      if (models.length) card.models = models;
      return card;
    };
    const sessA = _pct(raw.session_5h_remaining);
    const weekA = _pct(raw.week_7d_remaining);
    const sessB = _pct(raw.h5_remaining);
    const weekB = _pct(raw.wk_remaining);

    // 部分窗也接受（如 Codex 只回 weekly）：有幾個窗畫幾條，不再要求成對。
    // raw.session_label / raw.weekly_label 可覆寫預設窗名（Copilot=Premium、Devin=Daily）。
    const sessLabel = raw.session_label || "Session";
    const weekLabel = raw.weekly_label || "Weekly";
    let windows = null;
    let subtitle = "";
    if (sessA !== null || weekA !== null) {
      windows = [
        sessA !== null ? { key: "session", label: sessLabel, remainPct: sessA, resetText: parseResetDuration(raw.session_5h_reset) } : null,
        weekA !== null ? { key: "weekly", label: weekLabel, remainPct: weekA, resetText: parseResetDuration(raw.week_7d_reset) } : null,
      ].filter(Boolean);
      subtitle = raw.tier || "";
    } else if (sessB !== null || weekB !== null) {
      windows = [
        sessB !== null ? { key: "session", label: sessLabel, remainPct: sessB, resetText: parseResetDuration(raw.h5_reset) } : null,
        weekB !== null ? { key: "weekly", label: weekLabel, remainPct: weekB, resetText: parseResetDuration(raw.wk_reset) } : null,
      ].filter(Boolean);
      subtitle = raw.plan || "";
    }
    if (windows) {
      const pct = Math.round(Math.min.apply(null, windows.map(function (w) { return w.remainPct; })));
      return withModels(Object.assign({ kind: "full", subtitle, windows, pct, failed: false }, base));
    }
    const singles = [sessA, weekA, sessB, weekB, _pct(raw.remaining_pct)].filter(function (v) { return v !== null; });
    if (singles.length === 0 && models.length === 0) return { kind: "none", name: r.name, basis: raw.basis };
    if (singles.length === 0) {
      // 只有 models[] 有資料（如 agy 頂層無彙總窗）：以全部模型窗當卡片窗。
      const modelWindows = models.flatMap(function (card) { return card.windows; });
      const pct = Math.round(Math.min.apply(null, modelWindows.map(function (w) { return w.remainPct; })));
      return Object.assign({ kind: "full", subtitle: raw.tier || raw.plan || "", windows: modelWindows, models, pct, failed: false }, base);
    }
    const pct = Math.round(Math.min.apply(null, singles));
    return withModels(Object.assign({
      kind: "simple", subtitle: "",
      windows: [{ key: "quota", label: "Quota", remainPct: pct, resetText: null }],
      pct, failed: false,
    }, base));
  }

  // Codex rollout command 的純資料正規化：後端已按模型彙總，前端只做
  // 不可信 IPC shape 防線與穩定排序。查不到價格時保留 token、成本回 null。
  function normalizeCodexModelUsage(rows) {
    return (Array.isArray(rows) ? rows : [])
      .filter(function (row) {
        return row && typeof row.name === "string" && row.name.trim() &&
          typeof row.total_tokens === "number" && Number.isFinite(row.total_tokens) &&
          row.total_tokens >= 0;
      })
      .map(function (row) {
        const cost = typeof row.estimated_cost_usd === "number" &&
          Number.isFinite(row.estimated_cost_usd) && row.estimated_cost_usd >= 0
          ? row.estimated_cost_usd : null;
        return {
          name: row.name.trim(),
          total_tokens: row.total_tokens,
          estimated_cost_usd: cost,
        };
      })
      .sort(function (left, right) {
        return right.total_tokens - left.total_tokens || left.name.localeCompare(right.name);
      });
  }

  // 「最近完成」清單：hook 事件 → 完成紀錄列。
  // 只收 Stop/SessionEnd、provider 限 allowedIds（本機 CLI，OpenAB bot 24/7 loop
  // 會洗版所以不進來）、同 provider+session 去重留最新，時間新→舊取前 8 筆。
  function recentCompletions(events, allowedIds, limit) {
    const cap = typeof limit === "number" && limit > 0 ? limit : 8;
    const allowed = new Set(allowedIds || []);
    const bySession = new Map(); // provider|session_id -> {provider, ts}
    for (const e of events || []) {
      if (!e || (e.event_name !== "Stop" && e.event_name !== "SessionEnd")) continue;
      if (!allowed.has(e.provider)) continue;
      const ts = Date.parse(e.timestamp);
      if (!Number.isFinite(ts)) continue;
      const key = e.provider + "|" + (e.session_id || "");
      const prev = bySession.get(key);
      if (!prev || ts > prev.ts) {
        bySession.set(key, { provider: e.provider, ts, cwd: e.cwd || null, session_id: e.session_id || "" });
      }
    }
    return Array.from(bySession.values())
      .sort(function (a, b) { return b.ts - a.ts; })
      .slice(0, cap);
  }

  // 「等待回應」清單：state=waiting_for_user 的本機 CLI session，等最久的排前面
  // （starving 優先——愈久沒理它的 agent 愈該先處理）。
  function waitingSessions(sessions, allowedIds) {
    const allowed = new Set(allowedIds || []);
    return (sessions || [])
      .filter((s) => s && s.state === "waiting_for_user" && allowed.has(s.provider))
      .sort((a, b) => (b.last_event_secs_ago || 0) - (a.last_event_secs_ago || 0));
  }

  // stats-cache 新鮮度：Claude Code 自己寫的快取會停更（實測停過 3 個月），
  // 停更時舊值仍會被當成「今天」顯示 → 先判斷是否過期，過期就不給數字。
  // 回 { stale, days }；日期字串壞掉/缺失一律視為過期。
  function statsFreshness(computedDate, todayStr) {
    const t = Date.parse(todayStr);
    const c = computedDate ? Date.parse(computedDate) : NaN;
    if (!Number.isFinite(t) || !Number.isFinite(c)) return { stale: true, days: null };
    const days = Math.round((t - c) / 86400000);
    return { stale: days > 1, days }; // 昨天算新鮮（跨日剛好還沒重算）
  }

  const api = {
    parseResetDuration, normalizeRunnerCard, normalizeCodexModelUsage,
    recentCompletions, waitingSessions, statsFreshness,
  };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.QuotaCards = api;
})(typeof window !== "undefined" ? window : globalThis);

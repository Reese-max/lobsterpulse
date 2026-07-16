// OpenUsage 風格 Usage 面板（spec: docs/superpowers/specs/2026-07-16-openusage-panel-design.md）
// 載入順序在 main.js 之後，直接用其全域：invoke / esc / cssEsc / selectQuotaSnapshot /
// providerIconHtml / drawQcSpark / showView / fitWindow / currentView。
(function () {
  "use strict";

  const REFRESH_MS = 60000;
  let refreshTimer = null;
  let countdownTimer = null;
  let nextUpdateAt = 0;
  let renderInFlight = false;
  const detailOpen = new Set(); // 卡片詳情展開狀態（session 內存活即可）

  function fmtTok(n) {
    const v = Number(n);
    if (!Number.isFinite(v) || v < 0) return "—";
    if (v >= 1e9) return (v / 1e9).toFixed(1) + "B";
    if (v >= 1e6) return (v / 1e6).toFixed(1) + "M";
    if (v >= 1e3) return (v / 1e3).toFixed(1) + "K";
    return String(Math.round(v));
  }

  function fmtCost(v) {
    if (typeof v !== "number" || !Number.isFinite(v)) return null;
    if (v >= 1000) return "$" + (v / 1000).toFixed(1) + "K";
    return "$" + v.toFixed(2);
  }

  // 合併 live + 選定 snapshot 的 runner；同名先出現者優先（live 較即時）
  function mergeRunners(snapshots) {
    const out = [];
    const seen = new Set();
    const sources = [];
    if (snapshots.__live__) sources.push(snapshots.__live__);
    const sel = selectQuotaSnapshot(snapshots);
    if (sel && sel.snap && sel.snap !== snapshots.__live__) sources.push(sel.snap);
    for (const src of sources) {
      for (const r of (src && src.runners) || []) {
        if (!r || !r.name || seen.has(r.name)) continue;
        seen.add(r.name);
        out.push(r);
      }
    }
    return out;
  }

  function barRow(label, remainPct, resetText) {
    const warn = remainPct !== null && remainPct < 20;
    if (remainPct === null) {
      return `<div class="uv-sec">${esc(label)}</div>
        <div class="uv-bar uv-bar-empty"></div>
        <div class="uv-meta"><span>—</span><span>No data</span></div>`;
    }
    return `<div class="uv-sec">${esc(label)}</div>
      <div class="uv-bar"><div class="uv-fill${warn ? " uv-warn" : ""}" style="width:${remainPct}%"></div></div>
      <div class="uv-meta"><span>${remainPct}% left${warn ? " 🔥" : ""}</span><span>${resetText ? "Resets in " + esc(resetText) : "No data"}</span></div>`;
  }

  function detailRows(stats) {
    if (!stats) return "";
    const cost30 = fmtCost(stats.total_cost_usd);
    const rows = [
      ["Today", fmtTok(stats.today_tokens) + " tokens"],
      ["Yesterday", fmtTok(stats.yesterday_tokens) + " tokens"],
      ["Last 30 Days", fmtTok(stats.tokens_30d) + " tokens" + (cost30 ? " · " + cost30 + " 累計" : "")],
    ].map(([k, v]) => `<div class="uv-krow"><span>${k}</span><span>${esc(v)}</span></div>`).join("");
    const note = stats.computed_date
      ? `<div class="uv-note">stats-cache 統計日期：${esc(stats.computed_date)}</div>` : "";
    return rows + note;
  }

  function renderCard(runner, dailyStats) {
    const card = window.QuotaCards.normalizeRunnerCard(runner);
    const name = runner.name || "";
    const label = (runner.label || name).replace(/^[^\w]*\s/, ""); // 去掉開頭 emoji
    const plan = (card.kind !== "none" && card.subtitle) ? `<span class="uv-plan">${esc(card.subtitle)}</span>` : "";
    const failed = runner.ok === false ? `<span class="uv-err" title="runner 回報失敗">⚠</span>` : "";

    let body;
    if (card.kind === "full" || card.kind === "simple") {
      body = card.windows.map((w) => barRow(w.label, w.remainPct, w.resetText)).join("");
    } else {
      // 無配額 %（codex/copilot/gemini 現況）：保留骨架列，對齊 OpenUsage 的 No data 樣式
      body = barRow("Session", null, null) + barRow("Weekly", null, null);
    }

    const trend = card.kind === "full"
      ? `<div class="uv-trendrow"><span class="uv-sec">Usage Trend</span>
           <canvas class="uv-spark" data-uv-spark="${esc(name)}" width="150" height="24"></canvas></div>`
      : "";

    const stats = name === "claude" ? dailyStats : null;
    const open = detailOpen.has(name);
    const detail = stats
      ? `<button class="uv-chevron" data-uv-toggle="${esc(name)}" title="展開統計">${open ? "︿" : "﹀"}</button>
         <div class="uv-detail${open ? "" : " hidden"}">${detailRows(stats)}</div>`
      : "";

    return `<div class="uv-card" data-provider="${esc(name)}">
      <div class="uv-head">${providerIconHtml(name, 16)}<span class="uv-name">${esc(label)}</span>${failed}${plan}</div>
      ${body}${trend}${detail}
    </div>`;
  }

  async function render() {
    const root = document.getElementById("usage-cards");
    if (!root) return;
    if (renderInFlight) return;
    renderInFlight = true;
    try {
      const [snapRes, liveRes, statsRes] = await Promise.allSettled([
        invoke("read_usage_snapshots"),
        invoke("get_live_quota_snapshot"),
        invoke("get_claude_daily_stats"),
      ]);
      const snapshots = snapRes.status === "fulfilled" ? (snapRes.value || {}) : {};
      const liveSnap = liveRes.status === "fulfilled" ? liveRes.value : null;
      const dailyStats = statsRes.status === "fulfilled" ? statsRes.value : null;
      snapshots.__live__ = (liveSnap && (liveSnap.runners || []).length)
        ? { runners: liveSnap.runners, source: liveSnap.source || "live_api", updated_at: liveSnap.updated_at || 0 }
        : null;

      const runners = mergeRunners(snapshots);
      if (runners.length === 0) {
        root.innerHTML = `<div class="uv-empty">尚無額度資料來源（live API 與 snapshot 皆空）</div>`;
      } else {
        root.innerHTML = runners.map((r) => renderCard(r, dailyStats)).join("");
        drawSparks(runners.map((r) => r.name));
      }
      nextUpdateAt = Date.now() + REFRESH_MS;
      updateCountdown();
    } catch (e) {
      console.warn("[usage-view] render 失敗", e);
    } finally {
      renderInFlight = false;
      if (typeof currentView !== "undefined" && currentView === "usage") fitWindow();
    }
  }

  async function drawSparks(names) {
    try {
      const hist = await invoke("get_quota_history");
      const cutoff = Math.floor(Date.now() / 1000) - 7 * 86400;
      for (const name of names) {
        const canvas = document.querySelector(`canvas[data-uv-spark="${cssEsc(name)}"]`);
        if (!canvas) continue;
        const series = ((hist && hist[name]) || []).filter((pt) => pt[0] >= cutoff);
        if (series.length === 0) {
          const wrap = canvas.closest(".uv-trendrow");
          if (wrap) wrap.classList.add("hidden");
          continue;
        }
        drawQcSpark(canvas, series);
      }
    } catch (e) {
      console.warn("[usage-view] sparkline 失敗", e);
    }
  }

  function updateCountdown() {
    const el = document.getElementById("uv-next-update");
    if (!el) return;
    const s = Math.max(0, Math.round((nextUpdateAt - Date.now()) / 1000));
    el.textContent = `Next update in ${s}s`;
  }

  function start() {
    render();
    if (!refreshTimer) refreshTimer = setInterval(render, REFRESH_MS);
    if (!countdownTimer) countdownTimer = setInterval(updateCountdown, 1000);
  }

  function stop() {
    clearInterval(refreshTimer);
    clearInterval(countdownTimer);
    refreshTimer = null;
    countdownTimer = null;
  }

  // chevron 展開/收合（事件委派，一次綁定）
  document.addEventListener("click", (e) => {
    const btn = e.target.closest("[data-uv-toggle]");
    if (!btn) return;
    const name = btn.dataset.uvToggle;
    if (detailOpen.has(name)) detailOpen.delete(name); else detailOpen.add(name);
    const card = btn.closest(".uv-card");
    const detail = card && card.querySelector(".uv-detail");
    if (detail) detail.classList.toggle("hidden", !detailOpen.has(name));
    btn.textContent = detailOpen.has(name) ? "︿" : "﹀";
    fitWindow();
  });

  window.UsageView = { start, stop, render };
})();

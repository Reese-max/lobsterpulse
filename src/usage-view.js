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
  let renderQueued = false; // 完成事件撞上 60s refresh in-flight 時補跑一次（不然「即時插入」被吞）
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

  // 卡片清單以本機實際安裝的 CLI 為準（detect_installed_clis），
  // quota 資料（live fetch）按 id 併入；沒安裝的 CLI 不顯示、OpenAB bot 不混入。
  // last-known-good：單輪 API 瞬失不把已顯示的好資料洗成「—」，退回上次成功值並標 stale。
  const lastGood = new Map(); // name -> 最近一次帶 raw 且 ok 的 runner
  function cliRunners(clis, liveSnap) {
    const byName = new Map();
    for (const r of (liveSnap && liveSnap.runners) || []) {
      if (r && r.name) byName.set(r.name, r);
    }
    return clis.map((cli) => {
      // 標題一律用 detect 表的乾淨名稱（runner.label 帶「💻 …（本機）」是舊 quota bar 的格式）
      const fresh0 = byName.get(cli.id);
      const fresh = fresh0 && Object.assign({}, fresh0, { label: cli.label });
      if (fresh && fresh.raw && fresh.ok !== false) {
        // 後端 last-known-good 替換值帶 raw.stale：照樣顯示但亮 ⏳，且不得寫進 lastGood
        if (fresh.raw.stale) return Object.assign({}, fresh, { stale: true });
        lastGood.set(cli.id, fresh);
        return fresh;
      }
      const cached = lastGood.get(cli.id);
      if (cached) return Object.assign({}, cached, { stale: true });
      return fresh || { name: cli.id, label: cli.label, color: cli.color, ok: true, text: "", raw: null };
    });
  }

  function barRow(label, remainPct, resetText, color, metaRight) {
    const warn = remainPct !== null && remainPct < 20;
    const right = metaRight ? esc(metaRight) : resetText ? "Resets in " + esc(resetText) : "No data";
    if (remainPct === null) {
      return `<div class="uv-sec">${esc(label)}</div>
        <div class="uv-bar uv-bar-empty"></div>
        <div class="uv-meta"><span>—</span><span>${right}</span></div>`;
    }
    // 進度條用各家品牌色（warn 紅色由 class 蓋 inline，不另設）
    const fill = warn ? "" : `;background:${esc(color || "#3d9bff")}`;
    return `<div class="uv-sec">${esc(label)}</div>
      <div class="uv-bar"><div class="uv-fill${warn ? " uv-warn" : ""}" style="width:${remainPct}%${fill}"></div></div>
      <div class="uv-meta"><span>${remainPct}% left${warn ? " 🔥" : ""}</span><span>${right}</span></div>`;
  }

  function fmtUsd(v) {
    if (typeof v !== "number" || !Number.isFinite(v)) return "—";
    return "$" + (v >= 100 ? Math.round(v) : v.toFixed(2));
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
    // per-key 明細卡（如 openrouter）沒有標準 % 欄位，副標直接取 raw.plan
    const subtitle = (card.kind !== "none" && card.subtitle) || (runner.raw && runner.raw.plan) || "";
    const plan = subtitle ? `<span class="uv-plan">${esc(subtitle)}</span>` : "";
    const failed = runner.ok === false ? `<span class="uv-err" title="runner 回報失敗">⚠</span>` : "";
    const stale = runner.stale ? `<span class="uv-err uv-stale" title="本輪抓取失敗，顯示上次成功值">⏳</span>` : "";

    // 條色與 icon 同源：PROVIDER_COLORS 優先（深色底可讀性已調過），退回 runner.color
    const barColor =
      (typeof PROVIDER_COLORS !== "undefined" && PROVIDER_COLORS[name]) || runner.color;
    const accounts = runner.raw && Array.isArray(runner.raw.accounts) ? runner.raw.accounts : null;
    let body;
    if (accounts && accounts.length > 0) {
      // per-key 明細（openrouter 多帳號）：一把 key 一條 bar，右側顯示 $剩餘/$總額
      // （跨帳號加總 % 沒資訊量——帳號額度大小差距可達數十倍）
      body = accounts
        .map((a) => {
          const pct = a.total_usd > 0 ? Math.round((a.left_usd / a.total_usd) * 100) : 0;
          return barRow(`Key ${a.key}`, pct, null, barColor, `${fmtUsd(a.left_usd)} / ${fmtUsd(a.total_usd)}`);
        })
        .join("");
    } else if (card.kind === "full" || card.kind === "simple") {
      body = card.windows.map((w) => barRow(w.label, w.remainPct, w.resetText, barColor)).join("");
    } else {
      // 無配額 %：保留骨架列，對齊 OpenUsage 的 No data 樣式
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
      <div class="uv-head">${providerIconHtml(name, 16)}<span class="uv-name">${esc(label)}</span>${failed}${stale}${plan}</div>
      ${body}${trend}${detail}
    </div>`;
  }

  // 「最近完成」清單：hook 事件過濾出本機 CLI 的 Stop/SessionEnd（記憶體 buffer
  // 最近 50 筆，app 重啟即歸零）。OpenAB bot 24/7 loop 會洗版，不列入。
  function renderRecent(events, clis) {
    const root = document.getElementById("uv-recent");
    if (!root) return;
    const byId = new Map(clis.map((c) => [c.id, c.label]));
    const rows = window.QuotaCards.recentCompletions(events, clis.map((c) => c.id));
    if (rows.length === 0) { root.innerHTML = ""; return; }
    const now = Date.now();
    root.innerHTML =
      `<div class="uv-sec uv-recent-head">最近完成</div>` +
      rows.map((r) => {
        const label = byId.get(r.provider) || r.provider;
        const ago = formatRelativeTime(Math.max(0, Math.round((now - r.ts) / 1000)));
        // 專案名 = cwd 最後一段；同 provider 多筆時靠這個區分（不然七列全叫 Codex CLI 無從選）
        const proj = r.cwd ? String(r.cwd).split(/[\\/]/).filter(Boolean).pop() : "";
        return `<div class="uv-recent-row" data-provider="${esc(r.provider)}">${providerIconHtml(r.provider, 13)}
          <span class="uv-recent-name">${esc(label)}</span>
          ${proj ? `<span class="uv-recent-proj">${esc(proj)}</span>` : ""}
          <span class="uv-recent-time">${esc(ago)}</span></div>`;
      }).join("");
  }

  // 點完成紀錄列 → 跳 sessions 視窗並過濾該 provider（同 bot card 的跳轉模式）
  document.addEventListener("click", (e) => {
    const row = e.target.closest(".uv-recent-row");
    if (!row || !row.dataset.provider) return;
    sessionFilter = row.dataset.provider;
    if (typeof lastState !== "undefined" && lastState) renderSessions(lastState);
    showView("expanded");
  });

  async function render() {
    const root = document.getElementById("usage-cards");
    if (!root) return;
    if (renderInFlight) { renderQueued = true; return; }
    renderInFlight = true;
    try {
      const [cliRes, liveRes, statsRes, evRes] = await Promise.allSettled([
        invoke("detect_installed_clis"),
        invoke("get_live_quota_snapshot"),
        invoke("get_claude_daily_stats"),
        invoke("get_recent_completions"),
      ]);
      const clis = cliRes.status === "fulfilled" ? (cliRes.value || []) : [];
      const liveSnap = liveRes.status === "fulfilled" ? liveRes.value : null;
      const dailyStats = statsRes.status === "fulfilled" ? statsRes.value : null;
      const events = evRes.status === "fulfilled" ? (evRes.value || []) : [];

      const runners = cliRunners(clis, liveSnap);
      if (runners.length === 0) {
        root.innerHTML = `<div class="uv-empty">未偵測到本機安裝的 AI CLI</div>`;
      } else {
        root.innerHTML = runners.map((r) => renderCard(r, dailyStats)).join("");
        drawSparks(runners.map((r) => r.name));
      }
      renderRecent(events, clis);
      nextUpdateAt = Date.now() + REFRESH_MS;
      updateCountdown();
    } catch (e) {
      console.warn("[usage-view] render 失敗", e);
    } finally {
      renderInFlight = false;
      if (typeof currentView !== "undefined" && currentView === "usage") fitWindow();
      if (renderQueued) { renderQueued = false; render(); }
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

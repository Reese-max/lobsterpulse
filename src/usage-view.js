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
    // 快取停更時（stats-cache 由 Claude Code 自己寫，實測停過 3 個月）舊值會被
    // 當成今天的數字顯示 → 過期一律顯示 —，並標明停在哪天，不假裝有資料。
    const today = new Date().toLocaleDateString("sv-SE"); // YYYY-MM-DD（本地時區）
    const fresh = window.QuotaCards.statsFreshness(stats.computed_date, today);
    // 今天/昨天走後端直接掃 JSONL 的即時值（live_date 存在即代表算完了）；
    // 30 天與累計花費仍來自可能停更的 stats-cache，過期就標 —。
    const hasLive = stats.live_date === today;
    const cost30 = fmtCost(stats.total_cost_usd);
    const liveTok = (v) => (hasLive ? fmtTok(v) + " tokens" : "—");
    // 30 天無法即時算（實測掃 2.9GB 要 361 秒）→ 改用每輪掃描累積的每日快取。
    // 天數只能從安裝日往後長，所以標題照實寫「近 N 天」，不假裝是 30 天。
    const rDays = Number(stats.range_days);
    const rangeLabel = Number.isFinite(rDays) && rDays > 0 ? `Last ${rDays} Day${rDays > 1 ? "s" : ""}` : "Last 30 Days";
    const rangeVal = Number.isFinite(rDays) && rDays > 0
      ? fmtTok(stats.range_tokens) + " tokens"
      : fresh.stale ? "—" : fmtTok(stats.tokens_30d) + " tokens" + (cost30 ? " · " + cost30 + " 累計" : "");
    const rows = [
      ["Today", liveTok(stats.today_tokens)],
      ["Yesterday", liveTok(stats.yesterday_tokens)],
      [rangeLabel, rangeVal],
    ].map(([k, v]) => `<div class="uv-krow"><span>${k}</span><span>${esc(v)}</span></div>`).join("");
    const age = Number.isFinite(stats.live_age_secs)
      ? `${formatRelativeTime(stats.live_age_secs)}掃描` : "即時掃描";
    const note = !hasLive
      ? `<div class="uv-note">今日統計計算中…（首次約需數秒）</div>`
      : Number.isFinite(rDays) && rDays > 0
      ? `<div class="uv-note">今日/昨日：${esc(age)}；共記錄 ${rDays} 天，最早 ${esc(
          stats.range_since || "?"
        )}（app 關閉期間會有空缺；30 天無法即時算：需掃 2.9GB）</div>`
      : fresh.stale
        ? `<div class="uv-note">今日/昨日：${esc(age)}；30 天統計來自 stats-cache（停在 ${esc(
            stats.computed_date || "未知"
          )}${fresh.days !== null ? `，${fresh.days} 天未更新` : ""}）</div>`
        : `<div class="uv-note">今日/昨日：${esc(age)}；stats-cache 統計日期：${esc(stats.computed_date)}</div>`;
    return rows + note;
  }

  function renderCard(runner, dailyStats, providerDaily) {
    const card = window.QuotaCards.normalizeRunnerCard(runner);
    const name = runner.name || "";
    const label = (runner.label || name).replace(/^[^\w]*\s/, ""); // 去掉開頭 emoji
    // per-key 明細卡（如 openrouter）沒有標準 % 欄位，副標直接取 raw.plan
    const subtitle = (card.kind !== "none" && card.subtitle) || (runner.raw && runner.raw.plan) || "";
    const plan = subtitle ? `<span class="uv-plan">${esc(subtitle)}</span>` : "";
    const basis = renderQuotaBasis(card.basis, "uv-basis");
    const failed = runner.ok === false ? `<span class="uv-err" title="runner 回報失敗">⚠</span>` : "";
    const stale = runner.stale ? `<span class="uv-err uv-stale" title="本輪抓取失敗，顯示上次成功值">⏳</span>` : "";

    // 條色與 icon 同源：PROVIDER_COLORS 優先（深色底可讀性已調過），退回 runner.color
    const barColor =
      (typeof PROVIDER_COLORS !== "undefined" && PROVIDER_COLORS[name]) || runner.color;
    const accounts = runner.raw && Array.isArray(runner.raw.accounts) ? runner.raw.accounts : null;
    // 有 models[]（逐模型額度）時，窗列改為「模型名 · 窗名」逐模型展開。
    const quotaWindows = card.models && card.models.length
      ? card.models.flatMap((model) => model.windows.map((w) => ({ ...w, label: `${model.label} · ${w.label}` })))
      : card.windows || [];
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
    } else if (quotaWindows.length > 0) {
      body = quotaWindows.map((w) => barRow(w.label, w.remainPct, w.resetText, barColor)).join("");
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
    // 非 Claude 的 CLI 沒有可逐筆加總的 log，後端改用「累計計數器的當日差值」。
    // 面板卡片來自 live API 快照那條路，raw 裡不會有這個值，得從 get_provider_daily
    // 撈（膠囊是另一條路，raw 有值——一開始只做了膠囊那條，面板整個看不到）。
    // 用 != null 而非直接判真值：0 是真實用量，不是缺值。
    const pdRaw = providerDaily && providerDaily[name];
    const todayTok = pdRaw != null ? fmtTok(pdRaw) : runner.raw && runner.raw.today_tokens;
    const todayOnly = !stats && todayTok != null;
    // 進度條顯示「剩餘」（快沒了數字變小，餘光掃一眼最直覺），但展開後要能直接
    // 看到「用掉多少」——不然同一張卡片裡 token 是已用量、配額是剩餘量，方向不
    // 一致，得自己在腦中減。
    const winRows = quotaWindows
      .filter((w) => Number.isFinite(w.remainPct))
      .map((w) => {
        const used = Math.max(0, Math.min(100, 100 - Math.round(w.remainPct)));
        return `<div class="uv-krow"><span>${esc(w.label)}</span><span>已用 ${used}% · 剩 ${Math.round(w.remainPct)}%</span></div>`;
      })
      .join("");
    const detailBody = winRows + (stats
      ? detailRows(stats)
      : todayOnly
      ? `<div class="uv-krow"><span>Today</span><span>${esc(String(todayTok))} tokens</span></div>
         <div class="uv-note">app 記錄到的用量（未開啟期間不計入）</div>`
      : "");
    const detail = detailBody
      ? `<button class="uv-chevron" data-uv-toggle="${esc(name)}" title="展開統計">${open ? "︿" : "﹀"}</button>
         <div class="uv-detail${open ? "" : " hidden"}">${detailBody}</div>`
      : "";

    return `<div class="uv-card" data-provider="${esc(name)}">
      <div class="uv-head">${providerIconHtml(name, 16)}<span class="uv-name">${esc(label)}</span>${basis}${failed}${stale}${plan}</div>
      ${body}${trend}${detail}
    </div>`;
  }

  // 「最近完成」清單：hook 事件過濾出本機 CLI 的 Stop/SessionEnd（記憶體 buffer
  // 最近 50 筆，app 重啟即歸零）。OpenAB bot 24/7 loop 會洗版，不列入。
  // 點列 = inline 展開該筆詳情（2026-07-19 使用者反饋：跳去 legacy sessions
  // 視圖是「舊畫面」，改為不離開新面板）。
  const recentData = { rows: [], byId: new Map() };
  let recentOpenKey = null;
  // toast 點擊瞬間清單可能還沒 render 完，先記著要展開誰（5s 內有效，防過期誤展開）
  let pendingOpen = null; // { provider, at }

  // 展開該 provider 最新一筆完成紀錄（toast 點擊入口：一鍵直達詳情）
  function openLatest(provider) {
    const hit = recentData.rows.find((r) => r.provider === provider);
    if (hit) {
      recentOpenKey = hit.provider + "|" + hit.ts;
      drawRecent();
    } else {
      pendingOpen = { provider, at: Date.now() };
    }
  }

  function recentDetailHtml(r) {
    const st = (typeof lastState !== "undefined" && lastState) || null;
    const sess = st && Array.isArray(st.sessions)
      ? st.sessions.find((s) => s.id === r.session_id) : null;
    const when = new Date(r.ts).toLocaleString("zh-TW", { hour12: false });
    const rows = [
      ["完成於", when],
      r.cwd ? ["路徑", String(r.cwd)] : null,
      sess && sess.duration_secs
        ? ["時長", Math.max(1, Math.round(sess.duration_secs / 60)) + " 分鐘"] : null,
    ].filter(Boolean)
      .map(([k, v]) => `<div class="uv-krow"><span>${k}</span><span>${esc(v)}</span></div>`)
      .join("");
    const prompt = sess && sess.last_prompt
      ? `<div class="uv-recent-prompt">${esc(sess.last_prompt)}</div>` : "";
    const actBtn = `<button class="uv-act-btn" data-focus-term="${esc(r.session_id)}"${
      r.cwd ? ` data-open-dir="${esc(String(r.cwd))}"` : ""
    }>⌨ 切到終端機</button>`;
    return `<div class="uv-recent-detail">${rows}${prompt}${actBtn}</div>`;
  }

  // 「等待回應」常駐區塊：等待 toast 只出現 4.5 秒，錯過就沒入口——
  // 這裡從 lastState 常駐列出等待中的 session，點列直接切到終端機。
  // refreshState 的結構變化 hook 會呼叫 drawWaiting()，等待解除即消失。
  let waitKey = null; // 已畫內容的 session id 序列；refreshState 每秒都可能觸發，無變化不重繪
  function drawWaiting(force) {
    const root = document.getElementById("uv-waiting");
    if (!root) return;
    // 非 usage 視圖不畫：區塊隱藏中，重繪＋fitWindow 的 resize IPC 純空轉；
    // 進面板時 render() 會帶 force 補畫
    if (typeof currentView === "undefined" || currentView !== "usage") return;
    const st = (typeof lastState !== "undefined" && lastState) || null;
    const rows = window.QuotaCards.waitingSessions(
      (st && st.sessions) || [],
      Array.from(recentData.byId.keys())
    );
    const key = rows.map((s) => s.id).join("|");
    if (!force && key === waitKey) return;
    waitKey = key;
    if (rows.length === 0) {
      if (root.innerHTML !== "") { root.innerHTML = ""; fitWindow(); }
      return;
    }
    root.innerHTML =
      `<div class="uv-sec uv-recent-head">⏸ 等待回應</div>` +
      rows.map((s) => {
        const label = recentData.byId.get(s.provider) || s.provider;
        const ago = formatRelativeTime(Math.max(0, s.last_event_secs_ago || 0));
        return `<div class="uv-wait-row" data-focus-term="${esc(s.id)}"${
          s.cwd ? ` data-open-dir="${esc(String(s.cwd))}"` : ""
        } title="切到終端機">${providerIconHtml(s.provider, 13)}
          <span class="uv-recent-name">${esc(label)}</span>
          ${s.project_name ? `<span class="uv-recent-proj">${esc(s.project_name)}</span>` : ""}
          <span class="uv-wait-time">${esc(ago)}</span></div>`;
      }).join("");
    fitWindow();
  }

  function renderRecent(events, clis) {
    recentData.byId = new Map(clis.map((c) => [c.id, c.label]));
    recentData.rows = window.QuotaCards.recentCompletions(events, clis.map((c) => c.id));
    if (pendingOpen) {
      if (Date.now() - pendingOpen.at >= 5000) {
        pendingOpen = null; // 逾時作廢，防過期誤展開
      } else {
        const hit = recentData.rows.find((r) => r.provider === pendingOpen.provider);
        if (hit) {
          recentOpenKey = hit.provider + "|" + hit.ts;
          pendingOpen = null;
        } // 沒命中先留著：5s 內下一次 render 再試（完成事件與清單寫入有 race）
      }
    }
    drawRecent();
  }

  function drawRecent() {
    const root = document.getElementById("uv-recent");
    if (!root) return;
    if (recentData.rows.length === 0) { root.innerHTML = ""; return; }
    const now = Date.now();
    root.innerHTML =
      `<div class="uv-sec uv-recent-head">最近完成</div>` +
      recentData.rows.map((r) => {
        const key = r.provider + "|" + r.ts;
        const open = recentOpenKey === key;
        const label = recentData.byId.get(r.provider) || r.provider;
        const ago = formatRelativeTime(Math.max(0, Math.round((now - r.ts) / 1000)));
        // 專案名 = cwd 最後一段；同 provider 多筆時靠這個區分（不然七列全叫 Codex CLI 無從選）
        const proj = r.cwd ? String(r.cwd).split(/[\\/]/).filter(Boolean).pop() : "";
        return `<div class="uv-recent-row${open ? " open" : ""}" data-rkey="${esc(key)}">${providerIconHtml(r.provider, 13)}
          <span class="uv-recent-name">${esc(label)}</span>
          ${proj ? `<span class="uv-recent-proj">${esc(proj)}</span>` : ""}
          <span class="uv-recent-time">${esc(ago)}</span>
          <button class="uv-row-jump" data-focus-term="${esc(r.session_id)}"${
            r.cwd ? ` data-open-dir="${esc(String(r.cwd))}"` : ""
          } title="切到終端機">⌨</button></div>${open ? recentDetailHtml(r) : ""}`;
      }).join("");
    fitWindow();
  }

  // 點完成紀錄列 → 就地展開/收合詳情（不跳視圖）。
  // 列上的 ⌨ 快捷鍵（data-focus-term）點擊不觸發展開，交給下方委派處理。
  document.addEventListener("click", (e) => {
    if (e.target.closest("[data-focus-term]")) return;
    const row = e.target.closest(".uv-recent-row");
    if (!row || !row.dataset.rkey) return;
    recentOpenKey = recentOpenKey === row.dataset.rkey ? null : row.dataset.rkey;
    drawRecent();
  });

  // 詳情裡的「切到終端機」→ 聚焦該 session 的終端機視窗；
  // 失敗（終端機已關 / 舊紀錄無 PID）退回 Explorer 開專案資料夾
  document.addEventListener("click", (e) => {
    const btn = e.target.closest("[data-focus-term]");
    if (!btn) return;
    invoke("focus_terminal", { sessionId: btn.dataset.focusTerm }).catch((err) => {
      console.warn("[usage-view] focus_terminal 失敗，退回開資料夾", err);
      if (btn.dataset.openDir) {
        invoke("open_folder", { path: btn.dataset.openDir })
          .catch((e2) => console.warn("[usage-view] open_folder 也失敗", e2));
      }
    });
  });

  async function render() {
    const root = document.getElementById("usage-cards");
    if (!root) return;
    if (renderInFlight) { renderQueued = true; return; }
    renderInFlight = true;
    try {
      const [cliRes, liveRes, statsRes, evRes, pdRes] = await Promise.allSettled([
        invoke("detect_installed_clis"),
        invoke("get_live_quota_snapshot"),
        invoke("get_claude_daily_stats"),
        invoke("get_recent_completions"),
        invoke("get_provider_daily"),
      ]);
      const clis = cliRes.status === "fulfilled" ? (cliRes.value || []) : [];
      const liveSnap = liveRes.status === "fulfilled" ? liveRes.value : null;
      const dailyStats = statsRes.status === "fulfilled" ? statsRes.value : null;
      const events = evRes.status === "fulfilled" ? (evRes.value || []) : [];

      const runners = cliRunners(clis, liveSnap);
      if (runners.length === 0) {
        root.innerHTML = `<div class="uv-empty">未偵測到本機安裝的 AI CLI</div>`;
      } else {
        const providerDaily = pdRes.status === "fulfilled" ? (pdRes.value || {}) : {};
        root.innerHTML = runners.map((r) => renderCard(r, dailyStats, providerDaily)).join("");
        drawSparks(runners.map((r) => r.name));
      }
      renderRecent(events, clis);
      drawWaiting(true);
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

  window.UsageView = { start, stop, render, openLatest, drawWaiting };
})();

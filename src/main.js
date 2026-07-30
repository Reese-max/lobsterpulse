function invoke(cmd, args = {}) {
  return window.__TAURI_INTERNALS__.invoke(cmd, args);
}

// ─── Provider Icons (inline SVG) ───
const PROVIDER_ICONS = {
  claude: `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd"><path d="M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048-2.698-.073-2.339-.097-2.266-.122-.571-.121L0 11.784l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652.097 2.449.255h.389l.055-.157-.134-.098-.103-.097-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364-.462-.158-1.008.656-.722.881.06.225.061.893.686 1.908 1.476 2.491 1.833.365.304.145-.103.019-.073-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17-.619a2.97 2.97 0 01-.104-.729L6.283.134 6.696 0l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03.456.898.243.832.091.255h.158V9.01l.128-1.706.237-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48.685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212l.243-.242.985-1.306 1.652-2.064.73-.82.85-.904.547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347-.881 1.142-1.264 1.7-.79 1.36.073.11.188-.02 2.856-.606 1.543-.28 1.841-.315.833.388.091.395-.328.807-1.969.486-2.309.462-3.439.813-.042.03.049.061 1.549.146.662.036h1.622l3.02.225.79.522.474.638-.079.485-1.215.62-1.64-.389-3.829-.91-1.312-.329h-.182v.11l1.093 1.068 2.006 1.81 2.509 2.33.127.578-.322.455-.34-.049-2.205-1.657-.851-.747-1.926-1.62h-.128v.17l.444.649 2.345 3.521.122 1.08-.17.353-.608.213-.668-.122-1.374-1.925-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.316.37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924.315-1.53.286-1.9.17-.632-.012-.042-.14.018-1.434 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37.067-.662.401-.589 2.388-3.036 1.44-1.882.93-1.086-.006-.158h-.055L4.132 18.56l-1.13.146-.487-.456.061-.746.231-.243 1.908-1.312-.006.006z"/></svg>`,
  gemini: `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd"><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z"/></svg>`,
  copilot: `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd"><path d="M19.245 5.364c1.322 1.36 1.877 3.216 2.11 5.817.622 0 1.2.135 1.592.654l.73.964c.21.278.323.61.323.955v2.62c0 .339-.173.669-.453.868C20.239 19.602 16.157 21.5 12 21.5c-4.6 0-9.205-2.583-11.547-4.258-.28-.2-.452-.53-.453-.868v-2.62c0-.345.113-.679.321-.956l.73-.963c.392-.517.974-.654 1.593-.654l.029-.297c.25-2.446.81-4.213 2.082-5.52 2.461-2.54 5.71-2.851 7.146-2.864h.198c1.436.013 4.685.323 7.146 2.864zm-7.244 4.328c-.284 0-.613.016-.962.05-.123.447-.305.85-.57 1.108-1.05 1.023-2.316 1.18-2.994 1.18-.638 0-1.306-.13-1.851-.464-.516.165-1.012.403-1.044.996a65.882 65.882 0 00-.063 2.884l-.002.48c-.002.563-.005 1.126-.013 1.69.002.326.204.63.51.765 2.482 1.102 4.83 1.657 6.99 1.657 2.156 0 4.504-.555 6.985-1.657a.854.854 0 00.51-.766c.03-1.682.006-3.372-.076-5.053-.031-.596-.528-.83-1.046-.996-.546.333-1.212.464-1.85.464-.677 0-1.942-.157-2.993-1.18-.266-.258-.447-.661-.57-1.108-.32-.032-.64-.049-.96-.05zm-2.525 4.013c.539 0 .976.426.976.95v1.753c0 .525-.437.95-.976.95a.964.964 0 01-.976-.95v-1.752c0-.525.437-.951.976-.951zm5 0c.539 0 .976.426.976.95v1.753c0 .525-.437.95-.976.95a.964.964 0 01-.976-.95v-1.752c0-.525.437-.951.976-.951zM7.635 5.087c-1.05.102-1.935.438-2.385.906-.975 1.037-.765 3.668-.21 4.224.405.394 1.17.657 1.995.657h.09c.649-.013 1.785-.176 2.73-1.11.435-.41.705-1.433.675-2.47-.03-.834-.27-1.52-.63-1.813-.39-.336-1.275-.482-2.265-.394zm6.465.394c-.36.292-.6.98-.63 1.813-.03 1.037.24 2.06.675 2.47.968.957 2.136 1.104 2.776 1.11h.044c.825 0 1.59-.263 1.995-.657.555-.556.765-3.187-.21-4.224-.45-.468-1.335-.804-2.385-.906-.99-.088-1.875.058-2.265.394zM12 7.615c-.24 0-.525.015-.84.044.03.16.045.336.06.526l-.001.159a2.94 2.94 0 01-.014.25c.225-.022.425-.027.612-.028h.366c.187 0 .387.006.612.028-.015-.146-.015-.277-.015-.409.015-.19.03-.365.06-.526a9.29 9.29 0 00-.84-.044z"/></svg>`,
  codex: `<svg viewBox="0 0 24 24" fill="currentColor" fill-rule="evenodd"><path d="M9.205 8.658v-2.26c0-.19.072-.333.238-.428l4.543-2.616c.619-.357 1.356-.523 2.117-.523 2.854 0 4.662 2.212 4.662 4.566 0 .167 0 .357-.024.547l-4.71-2.759a.797.797 0 00-.856 0l-5.97 3.473zm10.609 8.8V12.06c0-.333-.143-.57-.429-.737l-5.97-3.473 1.95-1.118a.433.433 0 01.476 0l4.543 2.617c1.309.76 2.189 2.378 2.189 3.948 0 1.808-1.07 3.473-2.76 4.163zM7.802 12.703l-1.95-1.142c-.167-.095-.239-.238-.239-.428V5.899c0-2.545 1.95-4.472 4.591-4.472 1 0 1.927.333 2.712.928L8.23 5.067c-.285.166-.428.404-.428.737v6.898zM12 15.128l-2.795-1.57v-3.33L12 8.658l2.795 1.57v3.33L12 15.128zm1.796 7.23c-1 0-1.927-.332-2.712-.927l4.686-2.712c.285-.166.428-.404.428-.737v-6.898l1.974 1.142c.167.095.238.238.238.428v5.233c0 2.545-1.974 4.472-4.614 4.472zm-5.637-5.303l-4.544-2.617c-1.308-.761-2.188-2.378-2.188-3.948A4.482 4.482 0 014.21 6.327v5.423c0 .333.143.571.428.738l5.947 3.449-1.95 1.118a.432.432 0 01-.476 0zm-.262 3.9c-2.688 0-4.662-2.021-4.662-4.519 0-.19.024-.38.047-.57l4.686 2.71c.286.167.571.167.856 0l5.97-3.448v2.26c0 .19-.07.333-.237.428l-4.543 2.616c-.619.357-1.356.523-2.117.523zm5.899 2.83a5.947 5.947 0 005.827-4.756C22.287 18.339 24 15.84 24 13.296c0-1.665-.713-3.282-1.998-4.448.119-.5.19-.999.19-1.498 0-3.401-2.759-5.947-5.946-5.947-.642 0-1.26.095-1.88.31A5.962 5.962 0 0010.205 0a5.947 5.947 0 00-5.827 4.757C1.713 5.447 0 7.945 0 10.49c0 1.666.713 3.283 1.998 4.448-.119.5-.19 1-.19 1.499 0 3.401 2.759 5.946 5.946 5.946.642 0 1.26-.095 1.88-.309a5.96 5.96 0 004.162 1.713z"/></svg>`,
};

// OpenAB bot 繼承各自底層 CLI 的圖示（視覺一致），配色改成 bot 人設色。
PROVIDER_ICONS.cicx = PROVIDER_ICONS.claude;
PROVIDER_ICONS.gitx = PROVIDER_ICONS.copilot;
PROVIDER_ICONS.giminix = PROVIDER_ICONS.gemini;
PROVIDER_ICONS.codex_bot = PROVIDER_ICONS.codex;
// OPENX 專屬 icon：terminal 風格（矩形 + 尖括號 prompt + 底線），辨識 OpenCode = polymorphic CLI
PROVIDER_ICONS.openx = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="4" width="20" height="16" rx="2" ry="2"/><polyline points="7 10 10 12 7 14"/><line x1="12" y1="14" x2="18" y2="14"/></svg>`;
// IRISX 專屬 icon：虹膜/眼睛（IRIS = 虹膜），辨識 IRISX = 經由 hermes 的 Claude API
PROVIDER_ICONS.irisx_bot = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="4"/><circle cx="12" cy="12" r="1.5" fill="currentColor"/></svg>`;
PROVIDER_ICONS.grokx = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 18L18 4"/><path d="M9 4h9v9"/><path d="M5 7l12 10"/></svg>`;
PROVIDER_ICONS.lpbot = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12h4l2-6 4 12 2-6h6"/><circle cx="12" cy="12" r="10"/></svg>`;
PROVIDER_ICONS.mimo = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 18V6l8 6 8-6v12"/><path d="M4 6l8 12L20 6"/></svg>`;
// 本機 CLI 卡（usage 面板）：grok=xAI 式不對稱 X、agy=懸浮球+地平弧（反重力）、devin=D 字+節點眼
PROVIDER_ICONS.grok = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M5 5l14 14"/><path d="M19 5l-5.6 5.6"/><path d="M10.6 13.4L5 19"/></svg>`;
PROVIDER_ICONS.agy = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="7.5" r="3.2"/><path d="M4.5 19c2.2-2.8 4.7-4.2 7.5-4.2s5.3 1.4 7.5 4.2"/></svg>`;
PROVIDER_ICONS.devin = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M8 4v16"/><path d="M8 4h4a8 8 0 010 16H8"/><circle cx="11.5" cy="12" r="1.3" fill="currentColor" stroke="none"/></svg>`;
PROVIDER_ICONS.minimax = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 17V7l4.5 6L12 7l4.5 6L21 7v10"/></svg>`;
PROVIDER_ICONS.openrouter = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="5" cy="12" r="2"/><circle cx="19" cy="6" r="2"/><circle cx="19" cy="18" r="2"/><path d="M7 11l10-4"/><path d="M7 13l10 4"/></svg>`;
PROVIDER_ICONS.unknown = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.8 2.8 0 015 1.8c0 1.9-2.5 2.1-2.5 3.7"/><circle cx="12" cy="17.5" r=".6" fill="currentColor"/></svg>`;

const PROVIDER_COLORS = {
  claude: "#d97757",
  gemini: "#4285f4",
  copilot: "#6e40c9",
  codex: "#10a37f",
  // OpenAB bot 人設色
  cicx: "#ff8c42",      // CICX 橘
  gitx: "#7c3aed",      // GITX 紫
  giminix: "#3b82f6",   // GIMINIX 藍
  codex_bot: "#22c55e", // CODEX bot 亮綠
  openx: "#f472b6",     // OPENX 粉紅（OpenCode 辨識色）
  irisx_bot: "#06b6d4", // IRISX 青（hermes IRISX 辨識色）
  grokx: "#111827",
  lpbot: "#ef4444",
  mimo: "#f59e0b",
  // 本機 CLI 卡（usage 面板）—— grok 品牌黑/灰在深色半透明底看不清，改飽和青
  grok: "#22d3ee",
  agy: "#f59e0b",
  devin: "#2ea3ff",
  minimax: "#ec4899",
  openrouter: "#6366f1",
  unknown: "#888888",
};

const APP_NAME = "額度監控";
// 設定 UI 顯示用：助手/音效/規則清單現在只含本機 CLI，「💻」與「（本機）」
// 是冗餘噪音（還讓 300px 下名字難看地換行），顯示時修掉；config 內名稱不動。
const cleanProviderName = (n) => String(n || "").replace(/^💻\s*/, "").replace(/（本機）$/, "").trim();
const OPENAB_BOTS = ["cicx", "gitx", "giminix", "codex_bot", "openx", "irisx_bot", "grokx", "lpbot", "mimo"];
const LOCAL_PROVIDERS = ["claude", "codex", "copilot", "gemini"];
// OpenAB bot 優先顯示，本機 CLI 接在後面。codex_bot=OpenAB CODEX，codex=本機 CLI（獨立 id）。
const PROVIDER_ORDER = [...OPENAB_BOTS, ...LOCAL_PROVIDERS];
const QUOTA_STALE_SECONDS = 3600;

// ─── State ───
const COLORS = {
  purple: "rgb(217,128,255)", cyan: "rgb(77,217,255)",
  green: "rgb(77,242,153)", orange: "rgb(255,153,51)", pink: "rgb(255,102,153)",
};
const SCALES = { small: 0.85, medium: 1, large: 1.15 };
const DEFAULT_CAPSULE_W = 300;
const DEFAULT_EXPANDED_W = 300;
// 依當前視圖選寬度（capsule 是小方塊，其他視圖用 expanded_width）
function currentW() {
  if (!appConfig) return DEFAULT_EXPANDED_W;
  return currentView === "capsule"
    ? (appConfig.appearance.capsule_width || DEFAULT_CAPSULE_W)
    : (appConfig.appearance.expanded_width || DEFAULT_EXPANDED_W);
}

let currentView = "capsule";
let serverPort = 0;
let appConfig = null;
let collapsedAt = 0;
let sessionFilter = null; // bot card 點選後過濾 session list 的 provider id
let notifiedLongSessions = new Set(); // 避免同一 long session 重複發 Telegram
let sessionExpanded = new Set(); // 被展開的 session id（預設 compact）
let idleClusterExpanded = false; // R168 session clustering: idle 群組折疊開關 (≥2 idle 時自動啟用)
const STATE_PRIORITY = { working: 0, waiting_for_user: 1, stale: 2, idle: 3 }; // R168 排序:active 在前, idle 在後
function renderSessionRow(s, aid) {
  const sel = s.id === aid ? " selected" : "";
  const sc = ({ working: "working", waiting_for_user: "waiting_for_user", stale: "stale" })[s.state] || "idle";
  const sl = ({ working: "執行中", waiting_for_user: "等待中", stale: "閒置過久" })[s.state] || "";
  const cwdShort = shortenCwd(s.cwd);
  const meta = buildSessionMetaHtml(s);
  const expanded = sessionExpanded.has(s.id) ? " expanded" : "";
  const hasDetails = !!(cwdShort || s.last_prompt || meta);
  return `<div class="session-row${sel}${expanded}" data-id="${s.id}">
      <div class="session-row-head">
        <div class="session-provider-icon">${providerIconHtml(s.provider, 14)}</div>
        <span class="session-name">${esc(s.project_name)}</span>
        <span class="status-dot ${sc}"></span>${sl ? `<span class="session-state-label ${sc}">${sl}</span>` : ""}
        <span class="session-row-spacer"></span>
        ${s.is_active ? `<span class="session-time">${s.formatted_time}</span>` : ""}
        ${hasDetails ? `<span class="session-toggle">${expanded ? "▾" : "▸"}</span>` : ""}
        <button class="session-remove" data-rid="${s.id}" title="移除">&times;</button>
      </div>
      ${hasDetails ? `<div class="session-details">
        ${cwdShort ? `<div class="session-cwd">${esc(cwdShort)}</div>` : ""}
        ${s.last_prompt ? `<div class="session-prompt">${esc(s.last_prompt)}</div>` : ""}
        ${meta ? `<div class="session-meta">${meta}</div>` : ""}
      </div>` : ""}
    </div>`;
}
let dashboardCollapsed = { "bot-grid": false, "local-grid": false }; // 兩區塊都預設展開，讓用戶一眼看到 OpenAB 和本機兩條路徑
let quotaCollapsed = false;
let refreshStateInFlight = false;
let refreshStateQueued = false;
let refreshQuotasInFlight = false;
let refreshQuotasQueued = false;
let recentFailuresInFlight = false;
let recentFailuresQueued = false;
let eventsRenderInFlight = false;
let eventsRenderQueued = false;
const notifyDedupeTs = new Map();
const NOTIFY_DEDUPE_MAX_KEYS = 256;

const $ = (id) => document.getElementById(id);

// ─── Window resize ───
async function fitWindow() {
  await new Promise(r => requestAnimationFrame(r));
  // 螢幕剩餘可視空間（CSS px；resize_window 走 LogicalSize 同單位）。
  // 視窗頂在螢幕中段時內容常超出螢幕底——超出的部分（最近完成清單、footer）
  // 看得到截圖卻點不到，2026-07-19 使用者回報「點不進去」的根因。
  const availBelow = Math.floor(screen.availHeight - Math.max(window.screenY, 0)) - 8;
  // usage 視圖：卡片區上限動態縮到「可視空間 - 其他固定區塊」，卡片內捲、
  // 清單與 footer 永遠留在螢幕內（620 仍是原上限，只會更小不會更大）
  const cards = document.getElementById("usage-cards");
  if (cards && currentView === "usage" && availBelow > 200) {
    const rec = document.getElementById("uv-recent");
    const waiting = document.getElementById("uv-waiting");
    const foot = document.querySelector("#view-usage .uv-footer");
    const toast = document.getElementById("lp-toast");
    // 卡片區與「最近完成」清單**兩個都要**可壓縮。原本只壓卡片區，清單長起來
    // （實測 303px）就把 footer 連同齒輪推出畫面外，使用者回報「點不到齒輪」。
    // 真正固定的只有膠囊、等待區、footer、toast。
    const fixed =
      document.getElementById("capsule").offsetHeight +
      (waiting ? waiting.offsetHeight : 0) +
      (foot ? foot.offsetHeight : 0) +
      (toast && !toast.classList.contains("hidden") ? toast.offsetHeight : 0) +
      24;
    // 可分配給「卡片 + 清單」的空間；下限訂得夠低，確保 footer 一定留在畫面內
    const flexible = Math.max(availBelow - fixed, 180);
    if (rec) rec.style.maxHeight = ""; // 先解除舊上限再量自然高度
    await new Promise(r => requestAnimationFrame(r));
    const recWant = rec ? rec.offsetHeight : 0;
    // 卡片優先（那是主角），但清單至少留 80px，兩者都內捲
    const cardsMax = Math.min(Math.max(flexible - Math.min(recWant, 160), 100), 620);
    cards.style.maxHeight = cardsMax + "px";
    if (rec) rec.style.maxHeight = Math.max(flexible - cardsMax, 80) + "px";
    await new Promise(r => requestAnimationFrame(r));
    // 上面的 24 是留白的估計值，實際 margin/gap 會多出十幾 px——不修正的話
    // footer 仍會被切掉半個齒輪。量真實溢出量再從卡片區扣回去（一次就夠）。
    const overflow = document.getElementById("app").scrollHeight - availBelow;
    if (overflow > 0) {
      cards.style.maxHeight = Math.max(cardsMax - overflow, 100) + "px";
      await new Promise(r => requestAnimationFrame(r));
    }
  }
  // expanded（sessions）視圖同款：session-list 內捲，footer 留在螢幕內
  const slist = document.querySelector("#view-expanded .session-list");
  if (slist && currentView === "expanded" && availBelow > 200) {
    const others = Array.from(document.querySelectorAll(
      "#capsule, #view-expanded .filter-bar, #view-expanded .quota-bar-wrap, #view-expanded .action-bar"
    )).reduce((sum, el) => sum + (el && el.offsetHeight ? el.offsetHeight : 0), 0);
    slist.style.maxHeight = Math.max(availBelow - others - 40, 160) + "px";
    await new Promise(r => requestAnimationFrame(r));
  }
  const h = Math.max(Math.ceil(document.getElementById("app").scrollHeight) + 2, 46);
  await invoke("resize_window", { width: currentW(), height: Math.min(h, Math.max(availBelow, 200)) });
}

// ─── View switching ───
function showView(view) {
  const wasExpanded = currentView !== "capsule";
  const prevView = currentView;
  currentView = view;
  $("view-usage").classList.toggle("hidden", view !== "usage");
  $("view-expanded").classList.toggle("hidden", view !== "expanded");
  $("view-settings").classList.toggle("hidden", view !== "settings");
  $("view-dashboard").classList.toggle("hidden", view !== "dashboard");
  $("view-events-log").classList.toggle("hidden", view !== "events");
  $("view-timeline").classList.toggle("hidden", view !== "timeline");
  $("capsule").classList.toggle(
    "has-panel-below",
    view === "usage" || view === "expanded" || view === "settings" || view === "dashboard" || view === "events" || view === "timeline"
  );
  // PUA R112: 離開 capsule view 一定要收掉 brief（避免 brief 飄在 expanded view 上面）
  if (view !== "capsule") showCapsuleBrief(false);
  fitWindow();
  if (view === "capsule" && wasExpanded) {
    collapsedAt = Date.now();
    // CSS-based bounce (replaces the Rust bounce_window shim). Brief
    // forced reflow lets the animation re-fire if we're still in the
    // "bouncing" state from a prior collapse.
    const cap = $("capsule");
    cap.classList.remove("bouncing");
    void cap.offsetWidth;
    cap.classList.add("bouncing");
    setTimeout(() => cap.classList.remove("bouncing"), 300);
  }
  if (view === "events" && prevView !== "events") {
    startEventsAutoRefresh();
  } else if (view !== "events" && prevView === "events") {
    stopEventsAutoRefresh();
  }
  if (view === "timeline" && prevView !== "timeline") {
    startTimelineAutoRefresh();
  } else if (view !== "timeline" && prevView === "timeline") {
    stopTimelineAutoRefresh();
  }
  if (view === "usage" && prevView !== "usage") {
    window.UsageView?.start();
  } else if (view !== "usage" && prevView === "usage") {
    window.UsageView?.stop();
  }
}

// ─── Provider icon HTML ───
function providerIconHtml(providerId, size = 16) {
  const iconKey = PROVIDER_ICONS[providerId] ? providerId : "unknown";
  const svg = PROVIDER_ICONS[iconKey];
  const color = PROVIDER_COLORS[iconKey] || PROVIDER_COLORS.unknown;
  return `<span class="provider-icon" data-provider="${esc(providerId || "unknown")}" style="width:${size}px;height:${size}px;color:${color}">${svg}</span>`;
}

// ─── R115 規則引擎 UI ───
// 載入 + render 規則清單, 綁定 toggle / 新增 / 刪除按鈕
async function initRulesUI() {
  const listEl = $("rules-list");
  const countEl = $("rules-count");
  const toggleEl = $("toggle-rules-enabled");
  const newBtn = $("btn-add-rule");
  if (!listEl) return;

  // 從 config 讀 rules_enabled 旗標, 綁定變更 → 存回 config
  toggleEl.checked = !!appConfig.rules_enabled;
  toggleEl.addEventListener("change", async () => {
    appConfig.rules_enabled = toggleEl.checked;
    try {
      await invoke("save_app_config", { newConfig: appConfig });
    } catch (e) {
      console.warn("[R115] save rules_enabled failed:", e);
    }
  });

  // 載入 provider dropdown：只列有本機設定檔的真 CLI（與助手清單一致，隱藏 OpenAB bot），
  // 顯示人類可讀名而非內部 id。
  const provSel = $("new-rule-provider");
  const knownProviders = (appConfig.providers && typeof appConfig.providers === "object")
    ? Object.keys(appConfig.providers).filter(p => appConfig.providers[p] && appConfig.providers[p].settings_path)
    : LOCAL_PROVIDERS;
  for (const p of knownProviders) {
    const opt = document.createElement("option");
    opt.value = p;
    opt.textContent = cleanProviderName((appConfig.providers[p] && appConfig.providers[p].name) || p);
    provSel.appendChild(opt);
  }

  // 事件/狀態的內部值 → 人類可讀（對齊 index.html 的下拉選項）
  const EVENT_LABELS = {
    SessionStart: "開始", UserPromptSubmit: "送出提問", PreToolUse: "工具執行前",
    PostToolUse: "工具執行後", PostToolUseFailure: "工具失敗", Notification: "通知",
    Stop: "停止回應", SessionEnd: "結束",
  };
  const STATE_LABELS = { Completed: "已完成", StartedWaiting: "開始等待", None: "無狀態" };
  const provName = (id) => cleanProviderName((appConfig.providers[id] && appConfig.providers[id].name) || id);

  async function refresh() {
    let rules = [];
    try {
      rules = await invoke("list_rules");
    } catch (e) {
      console.warn("[R115] list_rules failed:", e);
      listEl.innerHTML = `<div class="rule-empty">載入失敗: ${e}</div>`;
      return;
    }
    countEl.textContent = `(${rules.length} 條)`;
    if (rules.length === 0) {
      listEl.innerHTML = `<div class="rule-empty">尚無規則, 點下方「➕ 新增」建立第一條</div>`;
      return;
    }
    listEl.innerHTML = "";
    for (const r of rules) {
      const whenParts = [
        r.when?.provider ? provName(r.when.provider) : "任何助手",
        r.when?.event ? (EVENT_LABELS[r.when.event] || r.when.event) : "任何事件",
      ];
      if (r.when?.state_to) whenParts.push("→ " + (STATE_LABELS[r.when.state_to] || r.when.state_to));
      const whenDesc = whenParts.join(" · ");
      const actionsDesc = (r.then || []).map(a => {
        if (a.Toast) return "跳通知";
        if (a.Sound) return "播音效";
        if (a.Log) return "寫記錄";
        return "?";
      }).join("、");
      const row = document.createElement("div");
      row.className = "rule-item";
      row.innerHTML = `
        <label class="toggle"><input type="checkbox" ${r.enabled ? "checked" : ""} data-id="${r.id}" class="rule-toggle"/><span class="toggle-slider"></span></label>
        <div style="flex:1">
          <div class="rule-item-desc">${escapeHtml(r.description || r.id)}</div>
          <div class="rule-item-when">${escapeHtml(whenDesc)} → ${escapeHtml(actionsDesc)}</div>
        </div>
        <button class="rule-item-del" data-id="${r.id}" title="刪除此規則">🗑</button>
      `;
      listEl.appendChild(row);
    }
    listEl.querySelectorAll(".rule-toggle").forEach(el => {
      el.addEventListener("change", async () => {
        try { await invoke("toggle_rule", { ruleId: el.dataset.id }); }
        catch (e) { console.warn("[R115] toggle_rule failed:", e); el.checked = !el.checked; }
      });
    });
    listEl.querySelectorAll(".rule-item-del").forEach(el => {
      el.addEventListener("click", async () => {
        try { await invoke("remove_rule", { ruleId: el.dataset.id }); await refresh(); }
        catch (e) { console.warn("[R115] remove_rule failed:", e); }
      });
    });
  }

  newBtn.addEventListener("click", async () => {
    const desc = ($("new-rule-desc").value || "").trim() || "(未命名規則)";
    const provider = $("new-rule-provider").value || null;
    const event = $("new-rule-event").value || null;
    const stateTo = $("new-rule-state").value || null;
    const rule = {
      id: `r115-user-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
      enabled: true,
      description: desc,
      when: { provider, event, state_to: stateTo },
      then: [{
        Toast: { title: `⚡ ${desc}`, body: `${provider || "any"} ${event || ""} → ${stateTo || "any"}` },
      }],
    };
    try {
      await invoke("add_rule", { rule });
      $("new-rule-desc").value = "";
      await refresh();
    } catch (e) { console.warn("[R115] add_rule failed:", e); }
  });

  await refresh();
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

// ─── Init ───
async function init() {
  if (!window.__TAURI_INTERNALS__) { setTimeout(init, 200); return; }

  // 強制 resize — 繞開 Tauri 2.10 + transparent=true + decorations=false 下 setup。
  // init 先執行一次（此時 appConfig 可能未載入），用 DEFAULT；後續 showView/fitWindow 會按 appConfig 重算。
  // 的 set_size 被 Windows DWM 合成器忽略導致 window 卡 14×14 的 bug
  try { await invoke("resize_window", { width: 300, height: 46 }); } catch (e) {}

  try {
    serverPort = await invoke("get_server_port");
    appConfig = await invoke("get_config");
  } catch (e) { return; }

  // 快速路徑：先套 CSS var / 字型（synchronous），capsule 立刻渲染
  applyAccentColor(appConfig.appearance.accent_color);
  applyTextSize(appConfig.appearance.text_size);
  applyFontFamily(appConfig.appearance.font_family);
  applyBgOpacity(appConfig.appearance.bg_opacity);
  // 背景 async 延遲 150ms 跑（capsule 先露面，避免 bg video/image 卡住首屏）
  setTimeout(() => {
    applyBackground(
      appConfig.appearance.background_type,
      appConfig.appearance.background_path,
      appConfig.appearance.background_blur,
      appConfig.appearance.background_image_opacity
    );
  }, 150);
  // 載完 config 後立刻套用使用者偏好寬度（capsule 起始視圖）
  try { await invoke("resize_window", { width: currentW(), height: 46 }); } catch (e) {}
  applyTheme(appConfig.appearance.theme || "dark");
  $("toggle-sound").checked = appConfig.appearance.sound_enabled;
  $("toggle-pin").checked = appConfig.appearance.pin_expanded;
  $("toggle-theme").checked = (appConfig.appearance.theme || "dark") === "light";
  if (appConfig.appearance.sound_enabled) $("sound-picker").classList.remove("hidden");

  // First launch → open settings automatically
  const firstLaunch = !appConfig.setup_done;
  if (firstLaunch) {
    await renderProviders();
    showView("settings");
  } else {
    await fitWindow();
    if (appConfig.appearance.pin_expanded) {
      $("btn-pin").classList.add("active");
      showView("usage");
    }
  }

  // Drag：面板任何空白處都能拖，不再只限膠囊——只綁膠囊時面板一展開就
  // 抓不到窗，移動很費勁（使用者實測反映）。互動元素與可點列不搶拖曳；
  // scrollbar 上的 mousedown（target=捲動容器本身且點位超出 clientWidth）也放行給捲動。
  const NO_DRAG = "button,input,select,textarea,label,a,[contenteditable],.custom-dropdown,.uv-recent-row,.uv-wait-row,.session-row,.uv-chevron,#capsule-quota";
  $("app").addEventListener("mousedown", (e) => {
    if (e.buttons !== 1) return;
    if (e.target.closest(NO_DRAG)) return;
    const t = e.target;
    if (t.clientWidth && t.scrollHeight > t.clientHeight && e.offsetX > t.clientWidth) return;
    invoke("plugin:window|start_dragging", { label: "main" }).catch(() => {});
  });

  // Ctrl+滾輪連續縮放（0.7~1.6，5% 步進），即存 config；S/M/L 按鈕仍是快速檔位
  let scaleSaveTimer = null;
  let scaleFitTimer = null;
  window.addEventListener("wheel", (e) => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    const cur = appConfig.appearance.text_scale ?? SCALES[appConfig.appearance.text_size] ?? 1;
    const next = Math.round(Math.min(1.6, Math.max(0.7, cur + (e.deltaY < 0 ? 0.05 : -0.05))) * 100) / 100;
    if (next === cur) return;
    appConfig.appearance.text_scale = next;
    applyTextSize(appConfig.appearance.text_size);
    // fitWindow 是 async（內部串多個 rAF 才 resize_window），快速滾動時重疊呼叫
    // 會競態閃爍——尾隨 debounce 讓最後一次滾動決定最終尺寸
    clearTimeout(scaleFitTimer);
    scaleFitTimer = setTimeout(fitWindow, 90);
    clearTimeout(scaleSaveTimer);
    scaleSaveTimer = setTimeout(saveConfig, 400);
  }, { passive: false });

  // Hover expand（主視圖改為 OpenUsage 風格 usage 面板；session 列表經 footer 按鈕可達）
  $("capsule").addEventListener("mouseenter", () => {
    if (currentView === "capsule" && !appConfig.appearance.pin_expanded && (Date.now() - collapsedAt > 500)) {
      showView("usage");
    }
  });

  // PUA R112 Capsule Brief: 膠囊本體 hover 時 toggle brief 面板
  // - 與主面板切換解耦：usage 面板由上面 mouseenter 觸發, brief 這裡
  //   獨立控制, 這樣 brief 也能在面板之外的時機用
  // - 300ms debounce: 避免快速 hover 進出時 brief 閃爍
  let briefHoverTimer = null;
  $("capsule").addEventListener("mouseenter", () => {
    if (currentView !== "capsule") return;
    clearTimeout(briefHoverTimer);
    briefHoverTimer = setTimeout(() => showCapsuleBrief(true), 300);
  });
  $("capsule").addEventListener("mouseleave", () => {
    clearTimeout(briefHoverTimer);
    showCapsuleBrief(false);
  });
  // brief 本身也要 support hover（讓使用者移到 brief 讀內容時不收掉）
  $("capsule-brief").addEventListener("mouseenter", () => {
    clearTimeout(briefHoverTimer);
  });
  $("capsule-brief").addEventListener("mouseleave", () => {
    showCapsuleBrief(false);
  });

  // Collapse via cursor-left
  const collapseCallbackId = window.__TAURI_INTERNALS__.transformCallback(() => {
    if ((currentView === "expanded" || currentView === "usage") && !appConfig.appearance.pin_expanded) showView("capsule");
  });
  invoke("plugin:event|listen", { event: "cursor-left", target: { kind: "Any" }, handler: collapseCallbackId }).catch(() => {});

  setInterval(() => {
    if ((currentView !== "expanded" && currentView !== "usage") || appConfig.appearance.pin_expanded) return;
    if (!document.getElementById("app").matches(":hover")) showView("capsule");
  }, 200);

  // Listen for tray → Open Settings
  const openSettingsCb = window.__TAURI_INTERNALS__.transformCallback(async () => {
    await renderProviders();
    await renderProviderSounds();
    await renderProviderSounds("waiting");
    showView("settings");
  });
  invoke("plugin:event|listen", { event: "open-settings", target: { kind: "Any" }, handler: openSettingsCb }).catch(() => {});

  // #8 Configurator live sync — Rust watcher emit 後即時套用
  const appearanceSyncedCb = window.__TAURI_INTERNALS__.transformCallback(async () => {
    try {
      appConfig = await invoke("get_config");
      applyAccentColor(appConfig.appearance.accent_color);
      applyTextSize(appConfig.appearance.text_size);
      applyFontFamily(appConfig.appearance.font_family);
      applyBgOpacity(appConfig.appearance.bg_opacity);
      applyBackground(
        appConfig.appearance.background_type,
        appConfig.appearance.background_path,
        appConfig.appearance.background_blur,
        appConfig.appearance.background_image_opacity
      );
      applyTheme(appConfig.appearance.theme || "dark");
    } catch (e) { console.error("appearance-synced:", e); }
  });
  invoke("plugin:event|listen", { event: "appearance-synced", target: { kind: "Any" }, handler: appearanceSyncedCb }).catch(() => {});

  // Tray → Bot Dashboard
  const openDashboardCb = window.__TAURI_INTERNALS__.transformCallback(() => {
    renderDashboard(lastState);
    showView("dashboard");
  });
  invoke("plugin:event|listen", { event: "open-dashboard", target: { kind: "Any" }, handler: openDashboardCb }).catch(() => {});

  // Tray → Events Log
  const openEventsCb = window.__TAURI_INTERNALS__.transformCallback(async () => {
    await renderEventsLog();
    showView("events");
    startEventsAutoRefresh();
  });
  invoke("plugin:event|listen", { event: "open-events-log", target: { kind: "Any" }, handler: openEventsCb }).catch(() => {});

  // ESC 鍵清除 session filter
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && sessionFilter) {
      sessionFilter = null;
      if (lastState) renderSessions(lastState);
      fitWindow();
    }
  });

  // Listen for tray → Toggle Theme
  const toggleThemeCb = window.__TAURI_INTERNALS__.transformCallback(() => {
    const newTheme = (appConfig.appearance.theme || "dark") === "dark" ? "light" : "dark";
    appConfig.appearance.theme = newTheme;
    applyTheme(newTheme);
    $("toggle-theme").checked = newTheme === "light";
    saveConfig();
  });
  invoke("plugin:event|listen", { event: "toggle-theme", target: { kind: "Any" }, handler: toggleThemeCb }).catch(() => {});

  // Re-register after delay
  setTimeout(() => {
    const cb2 = window.__TAURI_INTERNALS__.transformCallback(() => {
      if ((currentView === "expanded" || currentView === "usage") && !appConfig.appearance.pin_expanded) showView("capsule");
    });
    invoke("plugin:event|listen", { event: "cursor-left", target: { kind: "Any" }, handler: cb2 }).catch(() => {});

    // Listen for task-completed → play sound + optional Windows toast
    const soundCb = window.__TAURI_INTERNALS__.transformCallback((evt) => {
      const provider = (evt && evt.payload) || "unknown";
      const label = PROVIDER_LABEL?.[provider] || provider;
      if (appConfig.appearance.sound_enabled) playProviderSound(provider, "completion");
      if (appConfig.appearance.system_notifications) {
        systemNotify(`${label} ✅ 完成`, "任務結束，可回覆或收尾。", {
          dedupeKey: `task-completed:${provider}`,
          windowMs: 5000,
        });
      }
      showTaskToast(provider, "✅ 完成", "done");
      window.UsageView?.render?.(); // 「最近完成」清單即時插入
    });
    invoke("plugin:event|listen", { event: "task-completed", target: { kind: "Any" }, handler: soundCb }).catch(() => {});

    // Listen for task-waiting → play sound + optional Windows toast
    const waitingCb = window.__TAURI_INTERNALS__.transformCallback((evt) => {
      const provider = (evt && evt.payload) || "unknown";
      const label = PROVIDER_LABEL?.[provider] || provider;
      if (appConfig.appearance.sound_enabled) playProviderSound(provider, "waiting");
      if (appConfig.appearance.system_notifications) {
        systemNotify(`${label} ⏸ 等待你處理`, "agent 需要你回應。", {
          dedupeKey: `task-waiting:${provider}`,
          windowMs: 5000,
        });
      }
      showTaskToast(provider, "⏸ 等待處理・點我跳過去", "wait");
    });
    invoke("plugin:event|listen", { event: "task-waiting", target: { kind: "Any" }, handler: waitingCb }).catch(() => {});
  }, 2000);

  // Pin
  $("btn-pin").addEventListener("click", () => {
    appConfig.appearance.pin_expanded = !appConfig.appearance.pin_expanded;
    $("toggle-pin").checked = appConfig.appearance.pin_expanded;
    $("btn-pin").classList.toggle("active", appConfig.appearance.pin_expanded);
    if (!appConfig.appearance.pin_expanded && (currentView === "expanded" || currentView === "usage")) showView("capsule");
    saveConfig();
  });

  // Settings
  $("btn-settings").addEventListener("click", async () => {
    showView("settings");
    await renderProviders();
    await renderProviderSounds();
    await renderProviderSounds("waiting");
    fitWindow(); // re-measure after async renders complete
  });
  $("btn-close-settings").addEventListener("click", () => {
    appConfig.setup_done = true; saveConfig();
    showView(appConfig.appearance.pin_expanded ? "usage" : "capsule");
  });

  $("toggle-pin").addEventListener("change", (e) => {
    appConfig.appearance.pin_expanded = e.target.checked;
    $("btn-pin").classList.toggle("active", appConfig.appearance.pin_expanded);
    if (!appConfig.appearance.pin_expanded) showView("capsule"); else showView("usage");
    saveConfig();
  });

  $("toggle-theme").addEventListener("change", (e) => {
    const theme = e.target.checked ? "light" : "dark";
    appConfig.appearance.theme = theme;
    applyTheme(theme);
    saveConfig();
  });

  // Autostart toggle — 初始化為實際狀態（plugin 才是 source of truth）
  getAutostart().then(enabled => { $("toggle-autostart").checked = enabled; });
  $("toggle-autostart").addEventListener("change", async (e) => {
    const ok = await setAutostart(e.target.checked);
    if (!ok) e.target.checked = !e.target.checked; // revert on failure
  });

  $("toggle-notify").checked = !!appConfig.appearance.system_notifications;
  $("btn-test-toast")?.addEventListener("click", async () => {
    try { await invoke("test_toast"); } catch (e) { alert("toast 失敗: " + e); }
  });
  $("toggle-notify").addEventListener("change", (e) => {
    appConfig.appearance.system_notifications = e.target.checked;
    saveConfig();
    if (e.target.checked) systemNotify("額度監控", "系統通知已啟用");
  });

  $("toggle-sound").addEventListener("change", (e) => {
    appConfig.appearance.sound_enabled = e.target.checked;
    $("sound-picker").classList.toggle("hidden", !e.target.checked);
    fitWindow();
    saveConfig();
  });

  await renderProviderSounds();
  await renderProviderSounds("waiting");
  $("btn-open-sounds").addEventListener("click", () => invoke("open_sounds_folder").catch(() => {}));

  document.querySelectorAll(".color-dot").forEach(d => d.addEventListener("click", () => {
    appConfig.appearance.accent_color = d.dataset.color;
    applyAccentColor(d.dataset.color);
    saveConfig();
  }));

  document.querySelectorAll(".size-btn").forEach(b => b.addEventListener("click", () => {
    appConfig.appearance.text_size = b.dataset.size;
    appConfig.appearance.text_scale = null; // 回到檔位，清掉滾輪自訂值
    applyTextSize(b.dataset.size);
    fitWindow();
    saveConfig();
  }));

  $("btn-github").addEventListener("click", () => {
    invoke("open_app_config").catch(() => {});
  });

  // Usage 面板 footer 按鈕
  // 舊主視圖（expanded session 清單）入口已全數移除（使用者要求刪除舊版面）：
  // footer 📋 按鈕已拆、膠囊徽章與 dashboard 卡片跳轉改導向 usage 面板。
  // view-expanded 的 DOM/渲染碼保留 dormant——拆掉會踩無 null 防護的 init。
  $("btn-usage-settings")?.addEventListener("click", () => $("btn-settings").click());

  $("btn-hide").addEventListener("click", () => {
    invoke("hide_window").catch(() => {});
  });

  // 強制 snapshot — 立即寫一筆 quota-history.csv
  $("btn-snap-now")?.addEventListener("click", async () => {
    const btn = $("btn-snap-now");
    const orig = btn.innerHTML;
    try {
      const n = await invoke("manual_snapshot_once");
      btn.innerHTML = `<span style="font-size:10px;font-weight:700">✓${n}</span>`;
    } catch (e) {
      btn.innerHTML = `<span style="font-size:10px;color:#ff5050">✗</span>`;
      console.error("snap failed:", e);
    }
    setTimeout(() => { btn.innerHTML = orig; }, 2200);
  });

  // 一鍵 rebuild + relaunch（背景 cargo build → LP exit → 新 exe 自動起）
  $("btn-rebuild")?.addEventListener("click", async () => {
    if (!confirm("確定要 cargo build --release 並重啟 LP？\n（背景跑約 1 分鐘，完成會自動 relaunch）")) return;
    const btn = $("btn-rebuild");
    btn.innerHTML = `<span style="font-size:10px;font-weight:700">⏳</span>`;
    try { await invoke("rebuild_and_relaunch"); }
    catch (e) { alert("rebuild 失敗: " + e); btn.innerHTML = "⟳"; }
  });

  // ⏸/▶️ 自動化主開關
  const updateAutoIcon = () => {
    const on = appConfig?.appearance?.auto_actions?.master_enabled !== false;
    const btn = $("btn-auto-toggle"), ic = $("btn-auto-icon");
    if (!btn || !ic) return;
    ic.textContent = on ? "▶️" : "⏸";
    btn.title = on ? "自動化：運作中（點擊暫停）" : "自動化：已暫停（點擊恢復）";
    btn.style.opacity = on ? "1" : "0.5";
  };
  $("btn-auto-toggle").addEventListener("click", () => {
    if (!appConfig.appearance.auto_actions) return;
    appConfig.appearance.auto_actions.master_enabled = !appConfig.appearance.auto_actions.master_enabled;
    updateAutoIcon();
    saveConfig();
  });
  setTimeout(updateAutoIcon, 500);

  // 清空所有 session（保留 recent_events + provider_totals 歷史累計）
  $("btn-clear-sessions").addEventListener("click", async () => {
    try { await invoke("remove_all_sessions"); } catch (e) {}
    sessionExpanded.clear();
    sessionFilter = null;
    refreshState();
  });

  // 事件診斷手動刷新
  $("btn-refresh-events").addEventListener("click", () => renderEventsLog());

  $("btn-close-dashboard").addEventListener("click", () => {
    showView(appConfig.appearance.pin_expanded ? "usage" : "capsule");
  });
  $("btn-close-events").addEventListener("click", () => {
    showView(appConfig.appearance.pin_expanded ? "usage" : "capsule");
  });

  // ─── R-CPT M1 T-CPT10 — Timeline 視圖 (6th view) ───
  $("btn-timeline").addEventListener("click", () => {
    showView("timeline");
  });
  $("btn-close-timeline").addEventListener("click", () => {
    showView(appConfig.appearance.pin_expanded ? "usage" : "capsule");
  });
  $("btn-timeline-refresh").addEventListener("click", () => renderTimeline());
  $("btn-timeline-toggle-resolution").addEventListener("click", async () => {
    const next = timelineCurrentResolution === "24h" ? "7d" : "24h";
    try {
      const res = await invoke("timeline_toggle_resolution", { resolution: next });
      timelineCurrentResolution = res;
      $("btn-timeline-toggle-resolution").textContent = res;
    } catch (e) {
      console.warn("[timeline] toggle_resolution failed:", e);
    }
  });
  // Tray / shortcut → Timeline (R-CPT 預備, 等 owner M 補 tray menu 條目或
  // 快捷鍵, 前端 listener 先 hook 起來 — 對齊 open-dashboard / open-events-log pattern)
  const openTimelineCb = window.__TAURI_INTERNALS__.transformCallback(() => {
    showView("timeline");
  });
  invoke("plugin:event|listen", { event: "open-timeline", target: { kind: "Any" }, handler: openTimelineCb }).catch(() => {});

  // Settings tab switching
  document.querySelectorAll(".settings-tab").forEach(tab => {
    tab.addEventListener("click", () => {
      const tabId = tab.dataset.tab;
      document.querySelectorAll(".settings-tab").forEach(t => t.classList.toggle("active", t === tab));
      document.querySelectorAll(".settings-tab-panel").forEach(p =>
        p.classList.toggle("active", p.dataset.panel === tabId)
      );
      if (tabId === "keys") renderApiKeys();
      fitWindow();
    });
  });

  // ── 🔑 金鑰一鍵複製 ────────────────────────────────────────
  // 值永遠不主動進畫面：清單只有名稱與遮罩，複製是後端直接寫剪貼簿。
  // 唯一會把值送進 webview 的是「按住顯示」，且放開就消失。
  async function renderApiKeys() {
    const box = document.getElementById("apikey-list");
    if (!box) return;
    let keys = [];
    try {
      keys = (await invoke("list_api_keys")) || [];
    } catch (e) {
      box.innerHTML = `<div class="setting-sub-label">讀取失敗：${esc(String(e))}</div>`;
      return;
    }
    if (keys.length === 0) {
      box.innerHTML = `<div class="setting-sub-label">找不到 AI 服務的環境變數金鑰</div>`;
      return;
    }
    box.innerHTML = keys
      .map(
        (k) => `<div class="apikey-row">
          <span class="apikey-name">${esc(k.name)}</span>
          <span class="apikey-mask" data-mask="${esc(k.masked)}">${esc(k.masked)}</span>
          <button class="icon-btn apikey-eye" data-reveal="${esc(k.name)}" title="按住顯示">👁</button>
          <button class="icon-btn apikey-copy" data-copy="${esc(k.name)}" title="複製（30 秒後自動清除）">複製</button>
        </div>`
      )
      .join("");
  }

  document.getElementById("apikey-list")?.addEventListener("click", async (e) => {
    const btn = e.target.closest("[data-copy]");
    if (!btn) return;
    const old = btn.textContent;
    try {
      await invoke("copy_api_key", { name: btn.dataset.copy });
      btn.textContent = "已複製";
    } catch (err) {
      btn.textContent = "失敗";
      console.warn("[api-keys] 複製失敗", err);
    }
    setTimeout(() => { btn.textContent = old; }, 2000);
  });

  // 按住顯示：pointerdown 取值、放開／離開即還原遮罩
  {
    const box = document.getElementById("apikey-list");
    let shownEl = null;
    const hide = () => {
      if (!shownEl) return;
      shownEl.textContent = shownEl.dataset.mask;
      shownEl = null;
    };
    box?.addEventListener("pointerdown", async (e) => {
      const eye = e.target.closest("[data-reveal]");
      if (!eye) return;
      e.preventDefault();
      const cell = eye.parentElement.querySelector(".apikey-mask");
      try {
        const v = await invoke("reveal_api_key", { name: eye.dataset.reveal });
        if (cell) { cell.textContent = v; shownEl = cell; }
      } catch (err) {
        console.warn("[api-keys] 顯示失敗", err);
      }
    });
    ["pointerup", "pointercancel", "pointerleave"].forEach((ev) =>
      box?.addEventListener(ev, hide)
    );
    // 視窗失焦（Alt-Tab / 螢幕分享切走）也要收起來
    window.addEventListener("blur", hide);
  }

  // ── R115 規則引擎 UI ───────────────────────────────────────
  // 載入現有規則, render list, 綁定 toggle / 新增 / 刪除按鈕
  await initRulesUI();

  // 監聽 rule-fired 事件, MVP 階段: 命中時 console.log + 視覺提示
  window.__TAURI_INTERNALS__.event
    ? window.__TAURI_INTERNALS__.event.listen("rule-fired", (e) => {
        const p = e.payload;
        console.log(`[R115 rule-fired] ${p.rule_id} (${p.provider}/${p.event_name} → ${p.state_to})`, p.action);
      })
    : null;

  // 初始化 3 個 threshold input
  $("idle-secs").value = appConfig.appearance.idle_threshold_secs ?? 30;
  $("stale-secs").value = appConfig.appearance.stale_threshold_secs ?? 600;
  $("remove-secs").value = appConfig.appearance.remove_threshold_secs ?? 1800;
  $("tg-token").value = appConfig.appearance.telegram_bot_token ?? "";
  $("tg-chat").value = appConfig.appearance.telegram_chat_id ?? "";
  $("tg-threshold").value = appConfig.appearance.telegram_notify_threshold_secs ?? 600;

  const bindNumInput = (id, key) => $(id).addEventListener("change", (e) => {
    const n = parseInt(e.target.value, 10);
    if (!isNaN(n)) { appConfig.appearance[key] = n; saveConfig(); }
  });
  bindNumInput("idle-secs", "idle_threshold_secs");
  bindNumInput("stale-secs", "stale_threshold_secs");
  bindNumInput("remove-secs", "remove_threshold_secs");
  bindNumInput("tg-threshold", "telegram_notify_threshold_secs");
  // 視窗寬度（修改後立即 resize + persist）
  $("capsule-width").value = appConfig.appearance.capsule_width ?? DEFAULT_CAPSULE_W;
  $("expanded-width").value = appConfig.appearance.expanded_width ?? DEFAULT_EXPANDED_W;
  const bindWidthInput = (id, key) => $(id).addEventListener("change", (e) => {
    const n = parseInt(e.target.value, 10);
    if (!isNaN(n) && n >= 140 && n <= 600) {
      appConfig.appearance[key] = n;
      saveConfig();
      fitWindow();
    }
  });
  bindWidthInput("capsule-width", "capsule_width");
  bindWidthInput("expanded-width", "expanded_width");

  // 背景照片/影片
  const bgType = $("background-type");
  const bgPath = $("background-path");
  const bgBlur = $("bg-blur");
  const bgBlurVal = $("bg-blur-val");
  const bgImgOp = $("bg-image-opacity");
  const bgImgOpVal = $("bg-image-opacity-val");
  const btnPickBg = $("btn-pick-bg");
  const refreshBg = () => applyBackground(
    appConfig.appearance.background_type,
    appConfig.appearance.background_path,
    appConfig.appearance.background_blur,
    appConfig.appearance.background_image_opacity
  );
  if (bgType) {
    bgType.value = appConfig.appearance.background_type || "none";
    bgType.addEventListener("change", e => { appConfig.appearance.background_type = e.target.value; refreshBg(); saveConfig(); });
  }
  if (bgPath) {
    bgPath.value = appConfig.appearance.background_path || "";
    bgPath.addEventListener("change", e => { appConfig.appearance.background_path = e.target.value; refreshBg(); saveConfig(); });
  }
  if (bgBlur) {
    const v = appConfig.appearance.background_blur || 0;
    bgBlur.value = v; if (bgBlurVal) bgBlurVal.textContent = v + " px";
    bgBlur.addEventListener("input", e => {
      const n = parseInt(e.target.value, 10);
      appConfig.appearance.background_blur = n;
      if (bgBlurVal) bgBlurVal.textContent = n + " px";
      refreshBg(); saveConfig();
    });
  }
  if (bgImgOp) {
    const v = appConfig.appearance.background_image_opacity || 60;
    bgImgOp.value = v; if (bgImgOpVal) bgImgOpVal.textContent = v + "%";
    bgImgOp.addEventListener("input", e => {
      const n = parseInt(e.target.value, 10);
      appConfig.appearance.background_image_opacity = n;
      if (bgImgOpVal) bgImgOpVal.textContent = n + "%";
      refreshBg(); saveConfig();
    });
  }
  if (btnPickBg) {
    btnPickBg.addEventListener("click", async () => {
      try {
        const picked = await invoke("pick_background_file");
        if (picked) {
          appConfig.appearance.background_path = picked;
          bgPath.value = picked;
          // 自動偵測 type
          const ext = picked.split(".").pop().toLowerCase();
          if (["mp4","webm","mov","mkv"].includes(ext)) {
            appConfig.appearance.background_type = "video";
            bgType.value = "video";
          } else if (["jpg","jpeg","png","gif","webp","bmp","svg"].includes(ext)) {
            appConfig.appearance.background_type = "image";
            bgType.value = "image";
          }
          refreshBg(); saveConfig();
        }
      } catch (e) { console.error("pick bg failed:", e); }
    });
  }

  // 外部調整器按鈕：開 configurator.html / 匯入 appearance.json
  const openBtn = $("btn-open-configurator");
  if (openBtn) openBtn.addEventListener("click", async () => {
    try { await invoke("open_configurator"); } catch (e) { console.error(e); }
  });
  const helpBtn = $("btn-open-help");
  if (helpBtn) helpBtn.addEventListener("click", async () => {
    try { await invoke("open_help_page"); } catch (e) { alert("開啟說明失敗: " + e); }
  });
  const importBtn = $("btn-import-appearance");
  if (importBtn) importBtn.addEventListener("click", async () => {
    try {
      const result = await invoke("import_appearance_json");
      if (result && result.ok) {
        // reload config + re-apply visuals
        appConfig = await invoke("get_config");
        applyAccentColor(appConfig.appearance.accent_color);
        applyTextSize(appConfig.appearance.text_size);
        applyFontFamily(appConfig.appearance.font_family);
        applyBgOpacity(appConfig.appearance.bg_opacity);
        applyTheme(appConfig.appearance.theme);
        fitWindow();
        importBtn.textContent = "✓ 已匯入";
        setTimeout(() => importBtn.textContent = "⬆ 匯入 JSON", 1800);
      }
    } catch (e) { alert("匯入失敗: " + e); }
  });

  // 自訂 accent hex（即時套 + persist；清空時回預設 color dot）
  const accentInput = $("accent-custom");
  if (accentInput) {
    accentInput.value = appConfig.appearance.accent_custom_hex || "#d980ff";
    accentInput.addEventListener("input", (e) => {
      const v = e.target.value;
      appConfig.appearance.accent_custom_hex = v;
      applyAccentColor(appConfig.appearance.accent_color);
      saveConfig();
    });
  }

  // 字型 family 下拉
  const ff = $("font-family-select");
  if (ff) {
    ff.value = appConfig.appearance.font_family || "";
    ff.addEventListener("change", (e) => {
      appConfig.appearance.font_family = e.target.value;
      applyFontFamily(e.target.value);
      saveConfig();
    });
  }

  // 背景不透明度 slider
  const bgOp = $("bg-opacity");
  const bgOpVal = $("bg-opacity-val");
  if (bgOp) {
    const cur = appConfig.appearance.bg_opacity || 100;
    bgOp.value = cur;
    if (bgOpVal) bgOpVal.textContent = cur;
    bgOp.addEventListener("input", (e) => {
      const v = parseInt(e.target.value, 10);
      appConfig.appearance.bg_opacity = v;
      applyBgOpacity(v);
      if (bgOpVal) bgOpVal.textContent = v;
      saveConfig();
    });
  }
  const bindTextInput = (id, key) => $(id).addEventListener("change", (e) => {
    appConfig.appearance[key] = e.target.value;
    saveConfig();
  });
  bindTextInput("tg-token", "telegram_bot_token");
  bindTextInput("tg-chat", "telegram_chat_id");

  // openab_restart_command 文字欄位
  $("openab-restart-cmd").value = appConfig.appearance.openab_restart_command ?? "";
  bindTextInput("openab-restart-cmd", "openab_restart_command");

  // Telegram 測試發送
  $("tg-test").addEventListener("click", async () => {
    const result = $("tg-test-result");
    result.textContent = "送出中...";
    try {
      await invoke("send_telegram", { text: "🦞 LobsterPulse 測試訊息（external 按鈕觸發）" });
      result.textContent = "✅ 已送出，檢查 Telegram";
    } catch (e) {
      result.textContent = `❌ ${String(e)}`;
    }
  });

  // ─── 🔔 自動化 Tab：Discord + 3 條 auto-action 規則 ───
  // 保底 default（若 config 舊版無此欄位）
  appConfig.appearance.discord = appConfig.appearance.discord || { bot_token: "", channel_id: "", enabled: false };
  appConfig.appearance.auto_actions = appConfig.appearance.auto_actions || {
    quota_low_enabled: true, quota_low_threshold_pct: 5,
    session_idle_enabled: true, session_idle_trigger_secs: 1800, confirm_timeout_secs: 600,
    hook_failure_burst_enabled: true, failure_window_secs: 600, failure_count_threshold: 3,
    dedup_window_secs: 300,
    daily_summary_enabled: true, daily_summary_hour: 9,
  };
  const dc = appConfig.appearance.discord;
  const aa = appConfig.appearance.auto_actions;
  $("toggle-discord").checked = dc.enabled;
  $("dc-token").value = dc.bot_token || "";
  $("dc-channel").value = dc.channel_id || "";
  $("toggle-rule-quota").checked = aa.quota_low_enabled;
  $("rule-quota-pct").value = aa.quota_low_threshold_pct;
  $("toggle-rule-idle").checked = aa.session_idle_enabled;
  $("rule-idle-secs").value = aa.session_idle_trigger_secs;
  $("rule-confirm-secs").value = aa.confirm_timeout_secs;
  $("toggle-rule-burst").checked = aa.hook_failure_burst_enabled;
  $("rule-burst-window").value = aa.failure_window_secs;
  $("rule-burst-count").value = aa.failure_count_threshold;
  $("rule-dedup").value = aa.dedup_window_secs;
  $("toggle-rule-summary").checked = aa.daily_summary_enabled ?? true;
  $("rule-summary-hour").value = aa.daily_summary_hour ?? 9;

  const bindDiscord = (id, field) => $(id).addEventListener("change", (e) => {
    appConfig.appearance.discord[field] = (typeof e.target.checked === "boolean" && e.target.type === "checkbox")
      ? e.target.checked : e.target.value;
    saveConfig();
  });
  bindDiscord("toggle-discord", "enabled");
  bindDiscord("dc-token", "bot_token");
  bindDiscord("dc-channel", "channel_id");
  const bindAuto = (id, field, isBool, isNum) => $(id).addEventListener("change", (e) => {
    let v = e.target.value;
    if (isBool) v = e.target.checked;
    else if (isNum) v = parseInt(v, 10) || 0;
    appConfig.appearance.auto_actions[field] = v;
    saveConfig();
  });
  bindAuto("toggle-rule-quota", "quota_low_enabled", true);
  bindAuto("rule-quota-pct", "quota_low_threshold_pct", false, true);
  bindAuto("toggle-rule-idle", "session_idle_enabled", true);
  bindAuto("rule-idle-secs", "session_idle_trigger_secs", false, true);
  bindAuto("rule-confirm-secs", "confirm_timeout_secs", false, true);
  bindAuto("toggle-rule-burst", "hook_failure_burst_enabled", true);
  bindAuto("rule-burst-window", "failure_window_secs", false, true);
  bindAuto("rule-burst-count", "failure_count_threshold", false, true);
  bindAuto("rule-dedup", "dedup_window_secs", false, true);
  bindAuto("toggle-rule-summary", "daily_summary_enabled", true);
  bindAuto("rule-summary-hour", "daily_summary_hour", false, true);

  // ─── 📦 Runners Tab 邏輯 ───
  let editingRunnerIdx = -1; // -1 = 新增，其他 = 編輯該 index
  const renderRunners = () => {
    const list = $("runner-list");
    const runners = appConfig.appearance.usage_runners || [];
    list.innerHTML = runners.length === 0
      ? `<div class="threshold-label">（尚無查詢，點「➕ 新增」加一個）</div>`
      : runners.map((r, i) => `
        <div class="provider-row">
          <div class="provider-meta">
            <span class="provider-dot" style="background:${r.color || "#888"}"></span>
            <div><div>${escHtml(r.label || r.name)}</div>
            <div class="setting-sub-label" style="font-size:11px">${escHtml(r.command)} ${escHtml((r.args||[]).join(" "))}</div></div>
          </div>
          <div>
            <button class="mini-btn" data-edit="${i}">✏️</button>
            <button class="mini-btn" data-del="${i}">🗑</button>
          </div>
        </div>
      `).join("");
    // bind edit/delete
    list.querySelectorAll("[data-edit]").forEach(b => b.addEventListener("click", () => openRunnerEdit(parseInt(b.dataset.edit,10))));
    list.querySelectorAll("[data-del]").forEach(b => b.addEventListener("click", () => {
      const i = parseInt(b.dataset.del, 10);
      if (!confirm(`刪除查詢「${appConfig.appearance.usage_runners[i]?.name}」?`)) return;
      appConfig.appearance.usage_runners.splice(i, 1);
      saveConfig(); renderRunners();
    }));
  };
  const escHtml = (s) => String(s).replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
  const openRunnerEdit = (idx) => {
    editingRunnerIdx = idx;
    const r = idx === -1 ? { name:"", label:"", color:"#888", command:"", args:[], template:"" }
                         : appConfig.appearance.usage_runners[idx];
    $("runner-edit-idx").textContent = idx === -1 ? "（新增）" : `(index ${idx})`;
    $("runner-name").value = r.name || "";
    $("runner-label").value = r.label || "";
    $("runner-color").value = r.color || "#888";
    $("runner-command").value = r.command || "";
    $("runner-args").value = (r.args || []).join("\n");
    $("runner-template").value = r.template || "";
    $("runner-test-result").textContent = "";
    $("runner-edit").classList.remove("hidden");
  };
  $("btn-add-runner").addEventListener("click", () => openRunnerEdit(-1));
  $("btn-cancel-runner").addEventListener("click", () => $("runner-edit").classList.add("hidden"));
  $("btn-save-runner").addEventListener("click", () => {
    const r = {
      name: $("runner-name").value.trim(),
      label: $("runner-label").value.trim(),
      color: $("runner-color").value.trim() || "#888",
      command: $("runner-command").value.trim(),
      args: $("runner-args").value.split("\n").map(s => s.trim()).filter(Boolean),
      env: {},
      timeout_secs: 15,
      template: $("runner-template").value.trim() || null,
      cwd: null,
    };
    if (!r.name || !r.command) { alert("name + command 必填"); return; }
    appConfig.appearance.usage_runners = appConfig.appearance.usage_runners || [];
    if (editingRunnerIdx === -1) appConfig.appearance.usage_runners.push(r);
    else appConfig.appearance.usage_runners[editingRunnerIdx] = r;
    saveConfig();
    renderRunners();
    $("runner-edit").classList.add("hidden");
  });
  $("btn-test-runner").addEventListener("click", async () => {
    const result = $("runner-test-result");
    result.textContent = "試跑中...";
    const r = {
      name: $("runner-name").value.trim() || "test",
      label: $("runner-label").value.trim() || "test",
      color: $("runner-color").value.trim() || "#888",
      command: $("runner-command").value.trim(),
      args: $("runner-args").value.split("\n").map(s => s.trim()).filter(Boolean),
      env: {},
      timeout_secs: 15,
      template: $("runner-template").value.trim() || null,
      cwd: null,
    };
    try {
      const out = await invoke("test_usage_runner", { runner: r });
      result.textContent = out;
    } catch (e) {
      result.textContent = "❌ " + String(e);
    }
  });
  renderRunners();

  // Discord 測試發送
  $("dc-test").addEventListener("click", async () => {
    const r = $("dc-test-result");
    r.textContent = "送出中...";
    try {
      const msg = await invoke("send_discord_test");
      r.textContent = `✅ ${msg}`;
    } catch (e) {
      r.textContent = `❌ ${String(e)}`;
    }
  });

  // Dashboard 區塊折疊
  document.querySelectorAll(".dashboard-section-title[data-toggle]").forEach(h => {
    h.addEventListener("click", () => {
      const id = h.dataset.toggle;
      const grid = document.getElementById(id);
      if (!grid) return;
      dashboardCollapsed[id] = !dashboardCollapsed[id];
      grid.classList.toggle("collapsed", dashboardCollapsed[id]);
      h.querySelector(".section-caret").textContent = dashboardCollapsed[id] ? "▸" : "▾";
      fitWindow();
    });
  });

  // Quota bar 折疊
  $("quota-bar-header").addEventListener("click", () => {
    quotaCollapsed = !quotaCollapsed;
    $("quota-bar").classList.toggle("collapsed", quotaCollapsed);
    $("quota-bar-header").querySelector(".section-caret").textContent = quotaCollapsed ? "▸" : "▾";
    fitWindow();
  });

  // Quota 卡片個別收合（click delegation，#quota-bar 只掛一次）
  document.getElementById("quota-bar").addEventListener("click", (e) => {
    const t = e.target.closest("[data-qc-toggle]");
    if (!t) return;
    const name = t.getAttribute("data-qc-toggle");
    qcSetCollapsed(name, !qcCollapsed(name));
    refreshQuotas();
  });

  refreshState();
  setInterval(refreshState, 1000);
  refreshQuotas();
  setInterval(refreshQuotas, 15000);
}

// ─── Bot quota runner filter ───
// OpenAB 各 bot 共用一套 /usage runner 配置（每個 snapshot 都 N 份相同 runner），
// 按 backend 關鍵字過濾讓 bot card 只顯示自己的：CICX→Claude、GITX→Copilot、GIMINIX→Gemini、CODEX→Codex、OPENX→OpenCode。
// null = 明確跳過（該 bot 無對應 /usage runner）
// 空陣列 = 全部顯示（預留給未來新 bot）
// 有值 = 按關鍵字過濾
const BOT_RUNNER_KEYWORDS = {
  cicx: ["claude"],
  gitx: ["copilot"],
  giminix: ["gemini"],
  codex_bot: ["codex", "openai"],
  // OPENX (OpenCode) 有自己的 API provider（Zen/OpenRouter/Z.AI），
  // 不等於那 4 個基礎 CLI runner 的 aggregate → 跳過 snapshot，只顯示 runtime totals
  // 預備：OpenAB 未來加 OpenCode quota runner（e.g. Zen API）時自動接上，不用改 code
  openx: ["opencode", "zen"],
  // IRISX (hermes-agent) 接 Claude backend，顯示 claude runner 用量
  irisx_bot: ["claude", "hermes"],
  // No verified quota runner mapping yet. Showing all runners here would be fake data.
  grokx: null,
  lpbot: null,
  mimo: null,
};

function filterRunnersForBot(botId, runners) {
  if (!runners) return [];
  const kws = BOT_RUNNER_KEYWORDS[botId];
  if (kws === null) return []; // 明確跳過（該 bot 沒有對應的 /usage runner）
  if (!kws || kws.length === 0) return runners; // undefined/空陣列 = 顯示全部
  return runners.filter(r => {
    const label = (r.label || "").toLowerCase();
    return kws.some(kw => label.includes(kw));
  });
}

// ─── Sparkline canvas ───
function drawSparkline(canvasId, samples) {
  const cv = document.getElementById(canvasId);
  if (!cv || !samples || samples.length < 2) return;
  const ctx = cv.getContext("2d");
  const W = cv.width, H = cv.height;
  ctx.clearRect(0, 0, W, H);
  const vals = samples.map(s => (s.tokens_input || 0) + (s.tokens_output || 0));
  const min = Math.min(...vals);
  const max = Math.max(...vals);
  const range = Math.max(1, max - min);
  const style = getComputedStyle(document.documentElement);
  const accent = style.getPropertyValue("--accent").trim() || "#ff8c42";
  ctx.strokeStyle = accent;
  ctx.fillStyle = accent + "33";
  ctx.lineWidth = 1.2;
  ctx.beginPath();
  samples.forEach((s, i) => {
    const x = (i / (samples.length - 1)) * W;
    const y = H - ((vals[i] - min) / range) * (H - 4) - 2;
    if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
  });
  ctx.stroke();
  // area
  ctx.lineTo(W, H);
  ctx.lineTo(0, H);
  ctx.closePath();
  ctx.fill();
}

// ─── 失敗警示 ───
let lastFailureNotifiedAt = 0;
async function checkRecentFailures() {
  if (recentFailuresInFlight) {
    recentFailuresQueued = true;
    return;
  }
  recentFailuresInFlight = true;
  let events = [];
  try {
    try {
      events = await invoke("get_recent_events");
    } catch (e) {
      const dot = $("capsule-error-dot");
      if (dot) {
        dot.classList.add("hidden");
        dot.textContent = "";
      }
      return;
    }
    const cutoff = Date.now() - 10 * 60 * 1000;
    const failures = events.filter(e =>
      e.event_name === "PostToolUseFailure" &&
      new Date(e.timestamp).getTime() > cutoff
    );
    const dot = $("capsule-error-dot");
    if (!dot) return;
    if (failures.length > 0) {
      dot.classList.remove("hidden");
      dot.textContent = failures.length > 9 ? "9+" : String(failures.length);
      // 新失敗 → 彈 toast（不看 system_notifications 設定，失敗永遠推）
      const latest = failures[failures.length - 1];
      const latestTs = new Date(latest.timestamp).getTime();
      if (latestTs > lastFailureNotifiedAt) {
        lastFailureNotifiedAt = latestTs;
        const label = PROVIDER_LABEL[latest.provider] || latest.provider;
        systemNotify(`${label} ❌ 工具失敗`, `${latest.tool_name || "tool"} — 檢查事件診斷`, {
          dedupeKey: `tool-failure:${latest.provider}:${latest.tool_name || "tool"}`,
          windowMs: 10000,
        });
      }
    } else {
      dot.classList.add("hidden");
      dot.textContent = "";
    }
  } finally {
    recentFailuresInFlight = false;
    if (recentFailuresQueued) {
      recentFailuresQueued = false;
      checkRecentFailures();
    }
  }
}

// ─── Telegram 長任務推播 ───
async function maybeSendTelegramLongTask(sessions) {
  if (!appConfig.appearance.telegram_bot_token || !appConfig.appearance.telegram_chat_id) return;
  const threshold = appConfig.appearance.telegram_notify_threshold_secs ?? 600;
  for (const s of sessions) {
    // 只在完成（state=idle 但剛結束 working）且持續時間 >= threshold 的 session 推一次
    if (s.is_active) continue;
    if ((s.duration_secs || 0) < threshold) continue;
    if (notifiedLongSessions.has(s.id)) continue;
    notifiedLongSessions.add(s.id);
    const label = PROVIDER_LABEL[s.provider] || s.provider;
    const mins = Math.floor(s.duration_secs / 60);
    const text = `*${label}* 長任務完成 ⏱ ${mins} 分鐘\n${s.project_name || ""}`;
    try { await invoke("send_telegram", { text }); } catch (e) {}
  }
}

// ─── Autostart / Notifications ───
async function getAutostart() {
  try { return await invoke("plugin:autostart|is_enabled"); }
  catch (e) { return false; }
}
async function setAutostart(enabled) {
  try {
    if (enabled) await invoke("plugin:autostart|enable");
    else await invoke("plugin:autostart|disable");
    return true;
  } catch (e) { return false; }
}

function shouldSuppressSystemNotify(dedupeKey, windowMs) {
  if (!dedupeKey || !Number.isFinite(windowMs) || windowMs <= 0) return false;
  const now = Date.now();
  const lastTs = notifyDedupeTs.get(dedupeKey);
  if (typeof lastTs === "number" && now - lastTs < windowMs) {
    return true;
  }
  notifyDedupeTs.set(dedupeKey, now);
  if (notifyDedupeTs.size > NOTIFY_DEDUPE_MAX_KEYS) {
    const removeCount = notifyDedupeTs.size - Math.floor(NOTIFY_DEDUPE_MAX_KEYS * 0.75);
    let i = 0;
    for (const key of notifyDedupeTs.keys()) {
      notifyDedupeTs.delete(key);
      i += 1;
      if (i >= removeCount) break;
    }
  }
  return false;
}

// Windows toast / macOS banner。Fallback 到 Notification API（webview）
async function systemNotify(title, body, opts = {}) {
  const dedupeKey = typeof opts.dedupeKey === "string" && opts.dedupeKey
    ? opts.dedupeKey
    : `${title}|${body}`;
  const windowMs = Number.isFinite(opts.windowMs) ? opts.windowMs : 0;
  if (shouldSuppressSystemNotify(dedupeKey, windowMs)) return;
  try {
    await invoke("plugin:notification|notify", { options: { title, body } });
  } catch (e) {
    // 靜默失敗——通知是錦上添花，不該中斷主流程
  }
}

// ─── Dashboard (OpenAB bot 6 + 本機 CLI 4 = 10 卡片) ───
// 以 bot 為單位聚合所有 session 資訊，即使沒 active session 也能看到 quota / 最近活動
function formatRelativeTime(secs) {
  if (secs < 0) return "剛剛";
  if (secs < 60) return `${secs} 秒前`;
  if (secs < 3600) return `${Math.floor(secs / 60)} 分鐘前`;
  if (secs < 86400) return `${Math.floor(secs / 3600)} 小時前`;
  return `${Math.floor(secs / 86400)} 天前`;
}

function renderDashboard(st) {
  const sessions = st?.sessions || [];
  renderDashboardGrid("bot-grid", OPENAB_BOTS, sessions);
  renderDashboardGrid("local-grid", LOCAL_PROVIDERS, sessions);
  renderTrendGrid();
}

function quotaRunnerCurrentPct(r) {
  if (!r?.ok || !r.raw) return null;
  const raw = r.raw;
  const candidates = [
    raw.session_5h_remaining, raw.week_7d_remaining,
    raw.h5_remaining, raw.wk_remaining,
    raw.remaining_pct,
  ].filter(v => typeof v === "number" && v >= 0 && v <= 100);
  if (candidates.length === 0) return null;
  return Math.round(Math.min(...candidates));
}

async function loadCurrentTrendQuotaPctByRunner() {
  let snapshots = window.__lastQuotaSnapshots || {};
  try {
    const freshSnapshots = await invoke("read_usage_snapshots");
    snapshots = { ...snapshots, ...freshSnapshots };
  } catch (e) {
    // Trend current can still use the last in-memory quota snapshot.
  }
  const selectedQuota = selectQuotaSnapshot(snapshots);
  const snap = selectedQuota?.snap || null;
  if (!snap || isQuotaSnapshotStale(snap)) return {};
  const out = {};
  for (const r of snap.runners || []) {
    const pct = quotaRunnerCurrentPct(r);
    if (pct !== null) out[r.name] = pct;
  }
  return out;
}

async function renderTrendGrid() {
  const grid = document.getElementById("trend-grid");
  if (!grid) return;
  let hist = {};
  try { hist = await invoke("get_quota_history"); } catch (e) { return; }
  const currentPctByRunner = await loadCurrentTrendQuotaPctByRunner();

  // 過濾最近 N 天（7/30 可切）
  const rangeDays = window.__trendRangeDays || 7;
  const now = Math.floor(Date.now() / 1000);
  const cutoff = now - rangeDays * 86400;
  const filtered = {};
  for (const [name, series] of Object.entries(hist)) {
    filtered[name] = series.filter(([ts]) => ts >= cutoff);
  }

  // runner name → 顏色（對齊 dashboard 既有 PROVIDER_COLORS；usage runner name 是 claude/copilot/gemini/codex）
  const TREND_COLORS = {
    claude: "rgba(217,119,87,1)",      // #d97757
    copilot: "rgba(100,100,110,1)",    // github 黑灰
    gemini: "rgba(66,133,244,1)",      // #4285f4
    codex: "rgba(16,163,127,1)",       // #10a37f
  };

  const names = Object.keys(filtered).sort();
  const hasAny = names.some(n => (filtered[n] || []).length >= 2);

  // 頂部控制列：7/30 天切換 + CSV 匯出
  const ctrlHtml = `<div class="trend-ctrl" style="display:flex;gap:6px;align-items:center;padding:4px 2px 8px;font-size:11px">
    <span style="opacity:0.7">區間：</span>
    <button class="mini-btn" data-range="7" style="${rangeDays===7?'background:var(--accent);color:#111':''}">7 天</button>
    <button class="mini-btn" data-range="30" style="${rangeDays===30?'background:var(--accent);color:#111':''}">30 天</button>
    <span style="flex:1"></span>
    <button class="mini-btn" id="btn-trend-csv" title="下載完整 quota-history.csv">📥 CSV</button>
  </div>`;
  if (!hasAny) {
    grid.innerHTML = ctrlHtml + `<div class="bot-card"><div class="bot-card-name">（${rangeDays} 天內至少需 2 筆才畫圖，等 1~2 小時累積）</div></div>`;
    wireTrendCtrls(grid);
    return;
  }

  grid.innerHTML = ctrlHtml + names.map(name => {
    const series = filtered[name] || [];
    const pcts = series.map(([_, p]) => p);
    if (pcts.length === 0) return "";
    const historyLast = pcts[pcts.length - 1];
    const current = currentPctByRunner[name] ?? historyLast;
    const min = Math.min(...pcts);
    const max = Math.max(...pcts);
    const color = TREND_COLORS[name] || "rgba(150,150,150,1)";
    return `<div class="bot-card" style="border-left:3px solid ${color}">
      <div class="bot-card-name">${escHtmlT(name)} <span style="opacity:0.5;font-size:10px">${series.length} 點 · ${rangeDays} 天</span></div>
      <div style="font-size:11px;opacity:0.7">當前 <b>${current}%</b> · 低 ${min}% · 高 ${max}%</div>
      <canvas data-trend="${escHtmlT(name)}" width="280" height="40" style="width:100%;height:40px;display:block;margin-top:4px;cursor:crosshair"></canvas>
      <div data-tooltip="${escHtmlT(name)}" style="font-size:10px;opacity:0.6;min-height:12px"></div>
    </div>`;
  }).join("");
  wireTrendCtrls(grid);

  // 畫 sparkline + tooltip
  for (const name of names) {
    const cv = grid.querySelector(`canvas[data-trend="${cssEsc(name)}"]`);
    const tt = grid.querySelector(`[data-tooltip="${cssEsc(name)}"]`);
    if (!cv) continue;
    const ctx = cv.getContext("2d");
    const w = cv.width, h = cv.height;
    const series = filtered[name] || [];
    ctx.clearRect(0, 0, w, h);
    if (series.length < 2) continue;
    const color = TREND_COLORS[name] || "rgba(150,150,150,1)";

    // 畫 100% 基準線
    ctx.strokeStyle = "rgba(128,128,128,0.15)";
    ctx.lineWidth = 1;
    ctx.beginPath(); ctx.moveTo(0, 1); ctx.lineTo(w, 1); ctx.stroke();

    // 填色下層
    ctx.fillStyle = color.replace(",1)", ",0.12)");
    ctx.beginPath();
    ctx.moveTo(0, h);
    series.forEach(([_, pct], i) => {
      const x = (i / (series.length - 1)) * w;
      const y = h - (pct / 100) * h;
      ctx.lineTo(x, y);
    });
    ctx.lineTo(w, h);
    ctx.closePath();
    ctx.fill();

    // 線
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    series.forEach(([_, pct], i) => {
      const x = (i / (series.length - 1)) * w;
      const y = h - (pct / 100) * h;
      i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
    });
    ctx.stroke();

    // Hover tooltip + 垂直游標線 —— 滑鼠 x 映射到最近 data point
    cv.addEventListener("mousemove", (e) => {
      const rect = cv.getBoundingClientRect();
      const ratio = (e.clientX - rect.left) / rect.width;
      const idx = Math.min(series.length - 1, Math.max(0, Math.round(ratio * (series.length - 1))));
      const [ts, pct] = series[idx];
      const d = new Date(ts * 1000);
      const mm = String(d.getMonth() + 1).padStart(2, "0");
      const dd = String(d.getDate()).padStart(2, "0");
      const hh = String(d.getHours()).padStart(2, "0");
      const mi = String(d.getMinutes()).padStart(2, "0");
      if (tt) tt.textContent = `${mm}/${dd} ${hh}:${mi} → ${pct}%`;
      // 重畫加游標線（idx 對應的 x 座標）
      ctx.clearRect(0, 0, w, h);
      // 重畫底層
      ctx.strokeStyle = "rgba(128,128,128,0.15)";
      ctx.lineWidth = 1;
      ctx.beginPath(); ctx.moveTo(0, 1); ctx.lineTo(w, 1); ctx.stroke();
      ctx.fillStyle = color.replace(",1)", ",0.12)");
      ctx.beginPath(); ctx.moveTo(0, h);
      series.forEach(([_, p], i) => { const x = (i/(series.length-1))*w; const y = h - (p/100)*h; ctx.lineTo(x, y); });
      ctx.lineTo(w, h); ctx.closePath(); ctx.fill();
      ctx.strokeStyle = color; ctx.lineWidth = 1.5;
      ctx.beginPath();
      series.forEach(([_, p], i) => { const x = (i/(series.length-1))*w; const y = h - (p/100)*h; i===0?ctx.moveTo(x,y):ctx.lineTo(x,y); });
      ctx.stroke();
      // 垂直游標
      const cx = (idx/(series.length-1))*w;
      const cy = h - (pct/100)*h;
      ctx.strokeStyle = "rgba(255,255,255,0.4)";
      ctx.setLineDash([2, 2]);
      ctx.beginPath(); ctx.moveTo(cx, 0); ctx.lineTo(cx, h); ctx.stroke();
      ctx.setLineDash([]);
      // 高亮點
      ctx.fillStyle = color;
      ctx.beginPath(); ctx.arc(cx, cy, 3, 0, Math.PI*2); ctx.fill();
    });
    cv.addEventListener("mouseleave", () => {
      if (tt) tt.textContent = "";
      // 重畫清除游標
      ctx.clearRect(0, 0, w, h);
      ctx.strokeStyle = "rgba(128,128,128,0.15)"; ctx.lineWidth = 1;
      ctx.beginPath(); ctx.moveTo(0, 1); ctx.lineTo(w, 1); ctx.stroke();
      ctx.fillStyle = color.replace(",1)", ",0.12)");
      ctx.beginPath(); ctx.moveTo(0, h);
      series.forEach(([_, p], i) => { const x = (i/(series.length-1))*w; const y = h - (p/100)*h; ctx.lineTo(x, y); });
      ctx.lineTo(w, h); ctx.closePath(); ctx.fill();
      ctx.strokeStyle = color; ctx.lineWidth = 1.5;
      ctx.beginPath();
      series.forEach(([_, p], i) => { const x = (i/(series.length-1))*w; const y = h - (p/100)*h; i===0?ctx.moveTo(x,y):ctx.lineTo(x,y); });
      ctx.stroke();
    });
  }
}

function escHtmlT(s) { return String(s).replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c])); }
function cssEsc(s) { return String(s).replace(/[^a-zA-Z0-9_-]/g, "\\$&"); }

// Ring progress：從 r.raw 的多個 %-欄位取最小值（最緊配額）；無 → null
function runnerPct(r) {
  if (!r || r.ok === false || !r.raw) return null;
  const raw = r.raw;
  const cand = [
    raw.session_5h_remaining, raw.week_7d_remaining,
    raw.h5_remaining, raw.wk_remaining, raw.remaining_pct,
  ].filter(v => typeof v === "number" && v >= 0 && v <= 100);
  if (cand.length === 0) return null;
  return Math.round(Math.min(...cand));
}

function quotaSnapshotAgeSeconds(snap) {
  const ts = Number(snap?.updated_at || 0);
  if (!Number.isFinite(ts) || ts <= 0) return null;
  return Math.max(0, Math.floor(Date.now() / 1000 - ts));
}

function isQuotaSnapshotStale(snap) {
  const ageSec = quotaSnapshotAgeSeconds(snap);
  return ageSec !== null && ageSec >= QUOTA_STALE_SECONDS;
}

function selectQuotaSnapshot(snapshots = {}) {
  const local = snapshots.__local__;
  const live = snapshots.__live__;
  const localRunners = local?.runners || [];
  const localNames = new Set(localRunners.map(r => r?.name).filter(Boolean));
  const liveOnlyRunners = (live?.runners || []).filter(r => r?.name && !localNames.has(r.name));
  const localWithLiveOnly = localRunners.length > 0 && liveOnlyRunners.length > 0
    ? {
        ...local,
        runners: [...localRunners, ...liveOnlyRunners],
        source: "local+live",
        updated_at: Math.max(Number(local.updated_at) || 0, Number(live.updated_at) || 0),
      }
    : null;
  const candidates = [
    { snap: localWithLiveOnly, title: "💻 本機額度", source: "local+live" },
    { snap: snapshots.__local__, title: "💻 本機額度", source: "local" },
    { snap: snapshots.__live__, title: "💻 本機額度 (live)", source: "live" },
    { snap: snapshots.cicx, title: "☁️ OpenAB 額度", source: "cicx" },
    { snap: snapshots.gitx, title: "☁️ OpenAB 額度", source: "gitx" },
    { snap: snapshots.giminix, title: "☁️ OpenAB 額度", source: "giminix" },
    { snap: snapshots.codex_bot, title: "☁️ OpenAB 額度", source: "codex_bot" },
    { snap: snapshots.openx, title: "☁️ OpenAB 額度", source: "openx" },
    { snap: snapshots.irisx_bot, title: "☁️ OpenAB 額度", source: "irisx_bot" },
  ];
  return candidates.find(c => (c.snap?.runners || []).length > 0) || null;
}

function quotaSnapshotAgeText(ageSec) {
  if (ageSec === null) return "";
  if (ageSec < 60) return "剛剛";
  if (ageSec < 3600) return `${Math.floor(ageSec / 60)} 分鐘前`;
  if (ageSec < 86400) return `${Math.floor(ageSec / 3600)} 小時前`;
  return `${Math.floor(ageSec / 86400)} 天前`;
}

function renderQuotaRunner(r, { stale = false, includeProvider = false } = {}) {
  let cls = r.ok === false ? "quota-runner err" : "quota-runner";
  if (stale) cls += " stale";
  const pct = stale ? null : runnerPct(r);
  if (pct !== null) {
    if (pct < 10) cls += " crit";
    else if (pct < 20) cls += " warn";
  }
  const ring = pct !== null ? `<span class="percent-value">${pct}</span>` : "";
  const style = pct !== null ? ` style="--pct:${pct}"` : "";
  const text = (r.text || "")
    .replace(/\*\*/g, "")
    .replace(/`/g, "")
    .replace(/\bnull%/gi, "--")
    .replace(/\bnull\b/gi, "--");
  const errBadge = r.ok === false ? `<span class="quota-runner-err" title="Runner 執行失敗">⚠</span>` : "";
  const staleBadge = stale ? `<span class="quota-runner-stale" title="Snapshot 已超過 1 小時未更新，百分比已停用">舊</span>` : "";
  const providerAttr = includeProvider ? ` data-provider="${esc(r.name || "")}"` : "";
  const titleAttr = stale ? ` title="Snapshot 已超過 1 小時未更新，百分比已停用；請確認 quota runner 是否仍在寫入"` : "";
  return `<div class="${cls}"${providerAttr}${titleAttr}${style}>${ring}<span class="quota-runner-label">${esc(r.label || "")}${errBadge}${staleBadge}</span><span class="quota-runner-text">${esc(text)}</span></div>`;
}

// ===== Quota 卡片面板（spec: docs/superpowers/specs/2026-07-11-quota-cards-design.md）=====
const QC_COLLAPSE_PREFIX = "lp-qc-collapsed:";

function qcCollapsed(name) {
  try { return localStorage.getItem(QC_COLLAPSE_PREFIX + name) === "1"; }
  catch (_) { return false; }
}

function qcSetCollapsed(name, v) {
  try {
    if (v) localStorage.setItem(QC_COLLAPSE_PREFIX + name, "1");
    else localStorage.removeItem(QC_COLLAPSE_PREFIX + name);
  } catch (_) { /* localStorage 不可用時收合僅存活於當次 render */ }
}

function renderQcWindow(w) {
  const warn = w.remainPct < 20;
  const reset = w.resetText ? `Resets in ${w.resetText}` : "—";
  return `
    <div class="qc-window${warn ? " qc-warn" : ""}">
      <div class="qc-window-label">${w.label}${warn ? " 🔥" : ""}</div>
      <div class="qc-bar"><div class="qc-bar-fill" style="width:${w.remainPct}%"></div></div>
      <div class="qc-window-meta"><span>${w.remainPct}% left</span><span>${reset}</span></div>
    </div>`;
}

function renderQuotaCard(card, { stale = false } = {}) {
  const collapsed = qcCollapsed(card.name);
  const sub = card.subtitle ? `<span class="qc-subtitle">${esc(card.subtitle)}</span>` : "";
  const staleCls = stale ? " stale" : "";
  const titleText = (card.failed ? "⚠ " : "") + esc(card.label);
  const staleBadge = stale ? `<span class="qc-stale-badge">舊</span>` : "";
  const titleMsgs = [];
  if (card.failed) titleMsgs.push("runner 回報失敗，顯示最後已知值");
  if (stale) titleMsgs.push("快照過舊，數字可能非即時");
  const rootTitleAttr = titleMsgs.length ? ` title="${esc(titleMsgs.join("；"))}"` : "";
  if (collapsed) {
    return `<div class="quota-card qc-collapsed${staleCls}" data-provider="${esc(card.name)}"${rootTitleAttr}>
      <div class="qc-header" data-qc-toggle="${esc(card.name)}">
        <span class="qc-title">${titleText}</span>${staleBadge}${sub}
        <span class="qc-summary">${card.pct}%</span>
        <span class="qc-chevron">▸</span>
      </div>
    </div>`;
  }
  const spark = card.kind === "full"
    ? `<div class="qc-trend">
        <span class="qc-window-label">Usage Trend</span>
        <canvas class="qc-spark" data-qc-spark="${esc(card.name)}" width="240" height="28"></canvas>
      </div>`
    : "";
  return `<div class="quota-card${staleCls}" data-provider="${esc(card.name)}"${rootTitleAttr}>
    <div class="qc-header" data-qc-toggle="${esc(card.name)}">
      <span class="qc-title">${titleText}</span>${staleBadge}${sub}
      <span class="qc-chevron">▾</span>
    </div>
    ${card.windows.map(renderQcWindow).join("")}
    ${spark}
  </div>`;
}

// Task 4: 7d sparkline（Canvas + 5min 快取）
let __qcHistCache = { ts: 0, data: null };

async function drawQuotaCardSparks(names) {
  if (!names || names.length === 0) return;
  try {
    const now = Date.now();
    if (!__qcHistCache.data || now - __qcHistCache.ts > 300000) {
      __qcHistCache = { ts: now, data: await invoke("get_quota_history") };
    }
    const hist = __qcHistCache.data || {};
    const cutoff = Math.floor(now / 1000) - 7 * 86400;
    for (const name of names) {
      const canvas = document.querySelector(`canvas[data-qc-spark="${cssEsc(name)}"]`);
      if (!canvas) continue;
      const series = (hist[name] || []).filter((pt) => pt[0] >= cutoff);
      if (series.length === 0) {
        const wrap = canvas.closest(".qc-trend");
        if (wrap) wrap.classList.add("hidden");   // spec §4：無歷史 -> 該列隱藏
        continue;
      }
      drawQcSpark(canvas, series);
    }
  } catch (e) {
    console.warn("quota card sparkline 失敗", e);
  }
}

function drawQcSpark(canvas, series) {
  if (!series || series.length === 0) return;
  const ctx = canvas.getContext("2d");
  const cssW = Math.round(canvas.clientWidth || 0);
  if (cssW > 0 && canvas.width !== cssW) canvas.width = cssW;
  const W = canvas.width;
  const H = canvas.height;
  ctx.clearRect(0, 0, W, H);
  const accent = getComputedStyle(document.documentElement)
    .getPropertyValue("--accent").trim() || "#f93";
  const n = Math.min(series.length, 60);
  const pts = series.slice(-n);
  const slot = W / n;
  const bw = Math.max(2, Math.floor(slot) - 1);
  ctx.fillStyle = accent;
  ctx.globalAlpha = 0.85;
  pts.forEach((pt, i) => {
    const used = 100 - pt[1];   // 畫「使用量」高度，對齊截圖語意（用越多柱越高）
    const h = Math.max(1, Math.round((used / 100) * (H - 2)));
    ctx.fillRect(Math.round(i * slot), H - h, bw, h);
  });
  ctx.globalAlpha = 1;
}

// Trend 控制列：7/30 切換 + CSV export
function wireTrendCtrls(grid) {
  grid.querySelectorAll("button[data-range]").forEach(btn => {
    btn.addEventListener("click", () => {
      window.__trendRangeDays = parseInt(btn.dataset.range, 10) || 7;
      renderTrendGrid();
    });
  });
  const csvBtn = grid.querySelector("#btn-trend-csv");
  if (csvBtn) csvBtn.addEventListener("click", async () => {
    try {
      const hist = await invoke("get_quota_history");
      // 轉 CSV: ts_iso, provider, pct
      const rows = [["timestamp", "datetime_local", "provider", "pct_remaining"]];
      for (const [name, series] of Object.entries(hist)) {
        for (const [ts, pct] of series) {
          rows.push([ts, new Date(ts * 1000).toISOString(), name, pct]);
        }
      }
      rows.sort((a, b) => a === rows[0] ? -1 : a[0] - b[0]);
      const csv = rows.map(r => r.join(",")).join("\n");
      const blob = new Blob([csv], { type: "text/csv" });
      const a = document.createElement("a");
      a.href = URL.createObjectURL(blob);
      a.download = `lp-quota-history-${new Date().toISOString().slice(0,10)}.csv`;
      a.click();
    } catch (e) { alert("CSV 匯出失敗: " + e); }
  });
}

function renderDashboardGrid(gridId, dashboardBots, sessions) {
  const grid = document.getElementById(gridId);
  if (!grid) return;
  const latestByProvider = {};

  grid.innerHTML = dashboardBots.map(pid => {
    const p = appConfig.providers[pid];
    if (!p) return "";
    const related = sessions.filter(s => s.provider === pid);
    const active = related.filter(s => s.is_active).length;
    const hasAny = related.length > 0;
    // 以 is_active 優先、其次 last_event_secs_ago 最小（最近才有事件）排最前
    const latest = related.slice().sort((a, b) => {
      if (a.is_active !== b.is_active) return a.is_active ? -1 : 1;
      return (a.last_event_secs_ago ?? 9999) - (b.last_event_secs_ago ?? 9999);
    })[0];
    latestByProvider[pid] = latest || null;

    let stateLabel, stateCls;
    if (active > 0) {
      stateLabel = `${active} 執行中`;
      stateCls = "active";
    } else if (hasAny) {
      stateLabel = "閒置";
      stateCls = "idle";
    } else {
      stateLabel = "尚無事件";
      stateCls = "offline";
    }

    const lastActivity = latest
      ? formatRelativeTime(latest.last_event_secs_ago ?? 0)
      : "—";

    const toolChip = latest && latest.tool_calls && latest.tool_calls.length > 0
      ? `<span class="bot-card-tool">🔧 ${esc(latest.tool_calls[latest.tool_calls.length - 1].title || "?")}</span>`
      : "";
    const thinkingDot = latest && latest.thinking
      ? `<span class="bot-card-thinking-dot"></span>`
      : "";
    const tokens = latest && (latest.tokens_input > 0 || latest.tokens_output > 0)
      ? `<span class="bot-card-tokens">${formatTokens(latest.tokens_input)}·${formatTokens(latest.tokens_output)} tok</span>`
      : "";

    const cardCls = hasAny ? "bot-card" : "bot-card bot-card-empty";
    return `<div class="${cardCls}" data-pid="${pid}">
      <div class="bot-card-header">
        ${providerIconHtml(pid, 16)}
        <span class="bot-card-name">${esc(p.name)}</span>
        <span class="bot-card-state ${stateCls}">${stateLabel}${thinkingDot}</span>
      </div>
      <div class="bot-card-row">
        <span class="bot-card-meta">${related.length} 場</span>
        <span class="bot-card-meta bot-card-time">${lastActivity}</span>
      </div>
      ${toolChip || tokens ? `<div class="bot-card-row">${toolChip}${tokens}</div>` : ""}
      <canvas class="bot-card-spark" id="bot-card-spark-${pid}" width="120" height="20"></canvas>
      <div class="bot-card-quota" id="bot-card-quota-${pid}">—</div>
    </div>`;
  }).filter(Boolean).join("");

  // 點 bot card → 過濾 session 到該 provider
  grid.querySelectorAll(".bot-card").forEach(card => {
    card.addEventListener("click", () => {
      const pid = card.dataset.pid;
      sessionFilter = pid;
      if (lastState) renderSessions(lastState);
      showView("usage");
    });
  });

  // Sparkline draw（近 60 token_samples）
  for (const pid of dashboardBots) {
    const latest = latestByProvider[pid];
    drawSparkline(`bot-card-spark-${pid}`, latest?.token_samples || []);
  }

  // Bot card quota 顯示：runtime totals + **按 bot backend 過濾後**的 snapshot runner
  const totals = lastState?.provider_totals || {};
  const snapshots = window.__lastQuotaSnapshots || {};
  for (const pid of dashboardBots) {
    const el = document.getElementById(`bot-card-quota-${pid}`);
    if (!el) continue;
    const t = totals[pid];
    const snap = snapshots[pid];
    const chips = [];
    if (t && (t.tokens_input > 0 || t.tokens_output > 0 || t.session_count > 0)) {
      chips.push(`<span class="quota-chip" title="累計 input/output tokens">${formatTokens(t.tokens_input)}·${formatTokens(t.tokens_output)} tok</span>`);
      chips.push(`<span class="quota-chip" title="session 次數">${t.session_count}s</span>`);
      if (t.failure_count > 0) chips.push(`<span class="quota-chip err" title="失敗次數">${t.failure_count}❌</span>`);
    }
    // 只顯示跟這個 bot backend 相關的 runner（CICX→Claude、GITX→Copilot、GIMINIX→Gemini、CODEX→Codex）
    const relevantRunners = snap ? filterRunnersForBot(pid, snap.runners) : [];
    for (const r of relevantRunners) {
      chips.push(renderQuotaRunner(r, { stale: isQuotaSnapshotStale(snap) }));
    }
    el.innerHTML = chips.length > 0 ? chips.join("") : "—";
  }
}

// ─── Events log view ───
// R110: 對齊 R78 補齊事件診斷 view OpenAB bot 列表 (6→9, 含 grokx/lpbot/mimo),
// 跟 hook_server.rs KNOWN_PROVIDERS 13 個保持一致。Tab 數: 1 all + 1 errors +
// 9 OpenAB + 4 本機 = 15 (4 本機 count=0 時動態隱藏)。
let eventsFilter = "all"; // all | errors | cicx | gitx | giminix | codex_bot | openx | irisx_bot | grokx | lpbot | mimo | claude | codex | copilot | gemini
let eventsRefreshTimer = null;

const EVENT_CLASS = {
  SessionStart: "event-start",
  SessionEnd: "event-end",
  Stop: "event-stop",
  UserPromptSubmit: "event-prompt",
  PreToolUse: "event-tool",
  PostToolUse: "event-tool-done",
  PostToolUseFailure: "event-tool-fail",
  PermissionRequest: "event-permission",
  Notification: "event-notify",
  ThinkingDelta: "event-thinking",
  TokenUpdate: "event-token",
};

// provider id → 顯示 label（OpenAB bot 大寫，本機 CLI 小寫）
const PROVIDER_LABEL = {
  all: "全部",
  cicx: "CICX",
  gitx: "GITX",
  giminix: "GIMINIX",
  codex_bot: "CODEX",
  openx: "OPENX",
  irisx_bot: "IRISX",
  // R110: 對齊 R78 補齊 R78 T-BOT11 (grokx) / T-BOT12 (lpbot) / T-BOT5 (mimo) 標籤,
  // 避免事件診斷 tab 走 `PROVIDER_LABEL[p] || p` fallback 顯示 raw id
  grokx: "GROKX",
  lpbot: "LPBOT",
  mimo: "MIMO",
  claude: "claude",
  codex: "codex",
  copilot: "copilot",
  gemini: "gemini",
  unknown: "未知來源",
};

async function renderEventsLog() {
  if (eventsRenderInFlight) {
    eventsRenderQueued = true;
    return;
  }
  eventsRenderInFlight = true;
  const list = $("events-list");
  if (!list) {
    eventsRenderInFlight = false;
    return;
  }
  let events = [];
  try {
    try {
      events = await invoke("get_recent_events");
    } catch (e) {
      const header = $("events-filter");
      if (header) header.innerHTML = "";
      list.innerHTML = `<div class="event-empty">（事件資料來源中斷，暫停顯示舊 event）</div>`;
      return;
    }

  // Header with filter tabs + count
  const header = $("events-filter");
  if (header) {
    // OpenAB bot 永遠顯示（即使 count=0，讓用戶知道 bot 存在但尚無事件）；
    // 本機 CLI 只在有事件時顯示，避免 tab 列過長。
    // 「errors」專 tab 匯集所有 provider 的 PostToolUseFailure
    // R110: 對齊 R78 KNOWN_PROVIDERS 13 個補齊 grokx (T-BOT11) / lpbot (T-BOT12) / mimo (T-BOT5),
    // 避免事件診斷 view 漏接 R78 後新增的 3 個 OpenAB bot (grokx/lpbot enabled, mimo disabled 但仍
    // 應顯示 tab 跟 R78 「known 13」一致)。Total tab 數: 1 all + 1 errors + 9 OpenAB + 4 本機 = 15。
    const openabBots = OPENAB_BOTS;
    const localClis = LOCAL_PROVIDERS;
    const counts = {};
    for (const e of events) counts[e.provider] = (counts[e.provider] || 0) + 1;
    counts.all = events.length;
    counts.errors = events.filter(e => e.event_name === "PostToolUseFailure").length;

    const ordered = ["all", "errors", ...openabBots, ...localClis];
    header.innerHTML = ordered.map(p => {
      const n = counts[p] || 0;
      if (localClis.includes(p) && n === 0) return "";
      if (p === "errors" && n === 0) return "";
      const label = p === "errors" ? "❌ 失敗" : (PROVIDER_LABEL[p] || p);
      const active = p === eventsFilter ? "active" : "";
      const countBadge = n > 0 ? ` ${n}` : "";
      const cls = p === "errors" ? "events-tab events-tab-errors" : "events-tab";
      return `<button class="${cls} ${active}" data-filter="${p}">${label}${countBadge}</button>`;
    }).filter(Boolean).join("");
    header.querySelectorAll(".events-tab").forEach(btn => {
      btn.addEventListener("click", () => {
        eventsFilter = btn.dataset.filter;
        renderEventsLog();
      });
    });
  }

  const filtered = eventsFilter === "all"
    ? events
    : eventsFilter === "errors"
      ? events.filter(e => e.event_name === "PostToolUseFailure")
      : events.filter(e => e.provider === eventsFilter);

    if (!filtered.length) {
      list.innerHTML = `<div class="event-empty">（${eventsFilter === "all" ? "尚未收到任何 hook event" : `${eventsFilter} 尚無事件`}）</div>`;
      return;
    }
    // 最新的排上面；provider 欄改顯示 LABEL（CODEX 比 codex_bot 好讀）
    const rows = filtered.slice().reverse().map(e => {
      const t = new Date(e.timestamp);
      const hh = String(t.getHours()).padStart(2, "0");
      const mm = String(t.getMinutes()).padStart(2, "0");
      const ss = String(t.getSeconds()).padStart(2, "0");
      const tool = e.tool_name ? ` · ${esc(e.tool_name)}` : "";
      const cls = EVENT_CLASS[e.event_name] || "event-other";
      const providerLabel = PROVIDER_LABEL[e.provider] || e.provider;
      return `<div class="event-row">
      <span class="event-time">${hh}:${mm}:${ss}</span>
      <span class="event-provider" title="${esc(e.session_id)} (${esc(e.provider)})">${esc(providerLabel)}</span>
      <span class="event-detail ${cls}">${esc(e.event_name)}${tool}</span>
    </div>`;
    }).join("");
    list.innerHTML = rows;
    // 自動捲到頂（最新）
    list.scrollTop = 0;
  } finally {
    eventsRenderInFlight = false;
    if (eventsRenderQueued) {
      eventsRenderQueued = false;
      renderEventsLog();
    }
  }
}

function startEventsAutoRefresh() {
  if (eventsRefreshTimer) return;
  eventsRefreshTimer = setInterval(() => {
    if (currentView === "events") renderEventsLog();
  }, 2000);
}

function stopEventsAutoRefresh() {
  if (!eventsRefreshTimer) return;
  clearInterval(eventsRefreshTimer);
  eventsRefreshTimer = null;
}

// ─── Cross-Provider Timeline (R-CPT M1 T-CPT10) — 6th view ───
// 4 state SSoT 對齊 session.rs / timeline.rs: 0=Idle / 1=Working /
// 2=WaitingForUser / 3=Stale (R122 TimelineRing state_to_u8 順序)。
const TIMELINE_STATE_CLASSES = ["timeline-cell-idle", "timeline-cell-working", "timeline-cell-waiting", "timeline-cell-stale"];
const TIMELINE_STATE_LABELS = ["Idle", "Working", "WaitingForUser", "Stale"];
const TIMELINE_KNOWN_PROVIDERS = PROVIDER_ORDER;
const TIMELINE_AXIS_HOURS = ["00:00", "04:00", "08:00", "12:00", "16:00", "20:00", "24:00"];
let timelineRefreshTimer = null;
let timelineRenderInFlight = false;
let timelineCurrentResolution = "24h";

function timelineBuildAxis() {
  const axis = $("timeline-axis");
  if (!axis) return;
  axis.innerHTML = TIMELINE_AXIS_HOURS.map(h => `<span>${h}</span>`).join("");
}

async function renderTimeline() {
  if (timelineRenderInFlight) return;
  timelineRenderInFlight = true;
  const strip = $("timeline-strip");
  const stats = $("timeline-stats");
  if (!strip) { timelineRenderInFlight = false; return; }
  let snap = [];
  try {
    const recordedEvents = await invoke("timeline_recorded_event_count");
    if (!recordedEvents) {
      if (stats) stats.textContent = "(尚未收到任何 timeline event)";
      strip.innerHTML = "";
      timelineRenderInFlight = false;
      return;
    }
    snap = await invoke("timeline_snapshot_24h");
  } catch (e) {
    if (stats) stats.textContent = `載入失敗: ${e}`;
    strip.innerHTML = "";
    timelineRenderInFlight = false;
    return;
  }
  if (!Array.isArray(snap) || snap.length === 0) {
    if (stats) stats.textContent = "(空 snapshot — 尚未收到任何 event)";
    strip.innerHTML = "";
    timelineRenderInFlight = false;
    return;
  }
  // 對齊 R-CPT-1 Scenario: 13 row × 1440 cell 24h strip
  const rows = Math.min(snap.length, TIMELINE_KNOWN_PROVIDERS.length);
  const totalCells = rows > 0 ? snap[0].length : 0;
  // 統計 4 state 分布
  const counts = [0, 0, 0, 0];
  for (let r = 0; r < rows; r++) {
    const row = snap[r];
    for (let c = 0; c < row.length; c++) {
      const s = row[c];
      if (s >= 0 && s <= 3) counts[s]++;
    }
  }
  const total = rows * totalCells;
  if (stats) {
    const pct = (n) => total > 0 ? `${(n / total * 100).toFixed(1)}%` : "0%";
    stats.textContent = `${rows}×${totalCells} · Idle ${pct(counts[0])} · Working ${pct(counts[1])} · Wait ${pct(counts[2])} · Stale ${pct(counts[3])}`;
  }
  // 建 13 row (label + 1440 cell track)
  const frag = document.createDocumentFragment();
  for (let r = 0; r < rows; r++) {
    const providerId = TIMELINE_KNOWN_PROVIDERS[r];
    const row = snap[r];
    const rowEl = document.createElement("div");
    rowEl.className = "timeline-row";
    const label = document.createElement("span");
    label.className = "timeline-row-label";
    label.textContent = providerId;
    const track = document.createElement("div");
    track.className = "timeline-row-track";
    track.dataset.provider = providerId;
    track.title = `${providerId} · ${row.length} cells`;
    // 1440 cells, 直接用 flex 1 平均分配
    const cellFrag = document.createDocumentFragment();
    for (let c = 0; c < row.length; c++) {
      const s = row[c];
      const cell = document.createElement("div");
      cell.className = "timeline-cell " + (TIMELINE_STATE_CLASSES[s] || "timeline-cell-idle");
      cell.dataset.provider = providerId;
      cell.dataset.minute = String(c);
      cell.title = `${providerId} · ${TIMELINE_STATE_LABELS[s] || "Idle"} · minute ${c}`;
      cellFrag.appendChild(cell);
    }
    track.appendChild(cellFrag);
    rowEl.appendChild(label);
    rowEl.appendChild(track);
    frag.appendChild(rowEl);
  }
  strip.innerHTML = "";
  strip.appendChild(frag);
  // 綁定 cell click → timeline_jump_to_event
  strip.querySelectorAll(".timeline-cell").forEach(cell => {
    cell.addEventListener("click", async () => {
      const provider = cell.dataset.provider;
      const minute = parseInt(cell.dataset.minute || "0", 10);
      try {
        await invoke("timeline_jump_to_event", { provider, minute });
        // 跳到 events view (對齊 design.md §5 開放問題 #3 簡化版)
        showView("events");
        // 設 filter 鎖定 provider
        if (typeof eventsFilter !== "undefined") {
          eventsFilter = provider;
          try { await renderEventsLog(); } catch (e) {}
        }
      } catch (e) { console.warn("[timeline] jump failed:", e); }
    });
  });
  timelineRenderInFlight = false;
}

function startTimelineAutoRefresh() {
  if (timelineRefreshTimer) return;
  timelineBuildAxis();
  // 立即跑一次
  renderTimeline();
  timelineRefreshTimer = setInterval(() => {
    if (currentView === "timeline") renderTimeline();
  }, 5000);
}

function stopTimelineAutoRefresh() {
  if (!timelineRefreshTimer) return;
  clearInterval(timelineRefreshTimer);
  timelineRefreshTimer = null;
}

// ─── /usage quota (雙資料源) ───
async function refreshQuotas() {
  if (refreshQuotasInFlight) {
    refreshQuotasQueued = true;
    return;
  }
  refreshQuotasInFlight = true;
  // 雙資料源：OpenAB snapshot 檔（加分） + runtime provider_totals（即時累計，必備）
  // R90: 加第 3 條 live API（Tauri IPC `get_live_quota_snapshot`,R89 暴露）,
  // 與 snapshot 平行抓, 任一失敗不擋另一條；live 注入 `__live__` key, `__local__`
  // 缺/stale 時 fallback 用。
  try {
    const [snapshotsRes, liveRes] = await Promise.allSettled([
      invoke("read_usage_snapshots"),
      invoke("get_live_quota_snapshot"),
    ]);
    let snapshots = snapshotsRes.status === "fulfilled" ? snapshotsRes.value : {};
    const liveSnap = liveRes.status === "fulfilled" ? liveRes.value : null;
    const quotaSourcesUnavailable = snapshotsRes.status !== "fulfilled" && liveRes.status !== "fulfilled";
    window.__lastLiveQuota = liveSnap;
    // 轉成跟 snapshot envelope 同形（runners / source / updated_at）,
    // 至少 1 runner ok 才視為可用, 避免滿版錯誤蓋掉其它來源。
    snapshots.__live__ = (liveSnap?.runners || []).some(r => r.ok)
      ? { runners: liveSnap.runners.filter(r => r.ok), source: liveSnap.source || "live_api", updated_at: liveSnap.updated_at || 0 }
      : null;
    window.__lastQuotaSnapshots = snapshots;
    // snapshot 更新時同步刷新 capsule quota 提示
    updateCapsuleQuota();

    const totals = lastState?.provider_totals || {};
    const bar = $("quota-bar");
    if (!bar) return;

    if (quotaSourcesUnavailable) {
      window.__lastQuotaSnapshots = {};
      updateCapsuleQuota();
      const wrap = document.getElementById("quota-bar-wrap");
      if (wrap) wrap.classList.remove("hidden");
      bar.innerHTML = `<div class="quota-empty">quota 資料來源中斷：暫停顯示舊額度資料</div>`;
      if (currentView === "expanded") fitWindow();
      return;
    }

    // 每個 bot 的本地 totals row（活動/失敗），snapshot runner 改到底下全域區去重顯示
    const rows = PROVIDER_ORDER.map(pid => {
      const t = totals[pid];
      if (!t || (t.tokens_input === 0 && t.tokens_output === 0 && t.session_count === 0)) return "";
      const badges = [
        `<span class="quota-badge" title="累計輸入 token">⬇ ${formatTokens(t.tokens_input)}</span>`,
        `<span class="quota-badge" title="累計輸出 token">⬆ ${formatTokens(t.tokens_output)}</span>`,
        `<span class="quota-badge" title="session 次數">${t.session_count} 場</span>`,
        ...(t.failure_count > 0 ? [`<span class="quota-badge err" title="失敗次數">${t.failure_count} ❌</span>`] : []),
      ].join("");
      const nameShort = cleanProviderName(appConfig.providers[pid]?.name || pid);
      return `<div class="quota-row">
      ${providerIconHtml(pid, 14)}
      <span class="quota-row-bot">${esc(nameShort)}</span>
      <div class="quota-badges">${badges}</div>
    </div>`;
    }).filter(Boolean).join("");

    // 全域額度區——**LobsterPulse 自跑的 local runner 優先**，若無才用 live API 補，
    // 兩者皆缺才 fallback 到 OpenAB snapshot。來源選擇集中在 helper，避免 capsule
    // 和 expanded quota 列各自挑資料源導致顯示不一致。
    // 註: 上面 quota fetch 區已宣告 `liveSnap` (line 1797), 這裡直接讀 `snapshots.__live__`
    // 避免重複宣告 syntax error。R180 修 R179 透明化的 R13 防護漏洞。
    const selectedQuota = selectQuotaSnapshot(snapshots);
    const representativeSnap = selectedQuota?.snap || null;
    const sectionTitle = selectedQuota?.title || "";

    // freshness badge: 計算 snapshot 年齡
    let freshnessBadge = "";
    const ageSec = quotaSnapshotAgeSeconds(representativeSnap);
    if (ageSec !== null) {
      const ageText = quotaSnapshotAgeText(ageSec);
      const freshCls = ageSec < 300 ? "fresh" : ageSec < QUOTA_STALE_SECONDS ? "" : "stale";
      freshnessBadge = `<span class="quota-freshness ${freshCls}" title="snapshot 更新時間">${ageText}</span>`;
    }

    // 本機 runner 需要各自獨立顯示（每個 runner 有獨立 ring + 狀態）
    const globalRunners = representativeSnap?.runners || [];
    const globalStale = isQuotaSnapshotStale(representativeSnap);
    const cards = globalRunners
      .map((r) => window.QuotaCards.normalizeRunnerCard(r))
      .filter((c) => c.kind !== "none");
    const globalRow = cards.length > 0
      ? `<div class="quota-row-global">
        <div class="quota-section-title">${sectionTitle}${freshnessBadge}</div>
        ${cards.map((c) => renderQuotaCard(c, { stale: globalStale })).join("")}
      </div>`
      : "";

    const finalRows = rows + globalRow;

    const wrap = document.getElementById("quota-bar-wrap");
    if (wrap) wrap.classList.remove("hidden"); // 永遠顯示 wrap，沒資料時給 hint
    bar.innerHTML = finalRows || `<div class="quota-empty">暫無 quota 資料：等 OpenAB 寫 <code>~/.lobsterpulse/usage-*.json</code> 或 session 送 TokenUpdate event</div>`;
    drawQuotaCardSparks(cards.filter((c) => c.kind === "full").map((c) => c.name));
    if (currentView === "dashboard" && lastState) {
      renderDashboard(lastState);
    }
    if (currentView === "expanded") fitWindow();
  } finally {
    refreshQuotasInFlight = false;
    if (refreshQuotasQueued) {
      refreshQuotasQueued = false;
      refreshQuotas();
    }
  }
}

async function renderProviders() {
  const detected = await invoke("detect_installed_providers");
  const list = $("provider-list");

  // Fixed order instead of HashMap random order
  // 只列有 settings_path 的「真・本機 CLI」：本分頁用途是設定本機 CLI 的 hook
  // （勾選＝install/remove_provider_hooks）。OpenAB bot 是 settings_path:None、
  // 由 OpenAB 推事件、沒有本機 hook 可設，不屬於這個「設定 CLI」清單。
  const entries = PROVIDER_ORDER
    .filter(id => appConfig.providers[id] && appConfig.providers[id].settings_path)
    .map(id => [id, appConfig.providers[id]]);

  list.innerHTML = entries.map(([id, p]) => {
    const found = detected[id] || false;
    // settings_path=None 在 OpenAB 模式下是「由 OpenAB 推送事件」而非「不支援」。
    const isOpenAbBot = !p.settings_path;
    const checked = p.enabled ? "checked" : "";
    const statusText = isOpenAbBot
      ? (found ? "OpenAB 驅動" : "尚未偵測到 OpenAB")
      : (found ? "已偵測" : "");
    const statusClass = isOpenAbBot
      ? (found ? "provider-found" : "provider-pending")
      : (found ? "provider-found" : "");

    return `<div class="provider-item">
      <input type="checkbox" class="provider-check" data-provider="${id}" ${checked}>
      ${providerIconHtml(id, 18)}
      <span class="provider-name">${esc(cleanProviderName(p.name))}</span>
      ${statusText ? `<span class="${statusClass}">${statusText}</span>` : ""}
      ${!isOpenAbBot ? `<button class="provider-open" data-provider="${id}" title="開啟 ${esc(cleanProviderName(p.name))} 設定檔"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="9" y1="13" x2="15" y2="13"/><line x1="9" y1="17" x2="15" y2="17"/></svg></button>` : ""}
    </div>`;
  }).join("");

  // Listen for toggle changes
  list.querySelectorAll(".provider-check").forEach(cb => {
    cb.addEventListener("change", async () => {
      const pid = cb.dataset.provider;
      const p = appConfig.providers[pid];
      const isOpenAbBot = p && !p.settings_path;
      if (isOpenAbBot) {
        // OpenAB 模式：只切 enabled 旗標，不動 CLI 原生 hook
        appConfig.providers[pid].enabled = cb.checked;
        appConfig.setup_done = true;
        await saveConfig();
      } else if (cb.checked) {
        try { await invoke("install_provider_hooks", { providerId: pid }); } catch (e) {}
        appConfig = await invoke("get_config");
        appConfig.setup_done = true; saveConfig();
      } else {
        try { await invoke("remove_provider_hooks", { providerId: pid }); } catch (e) {}
        appConfig = await invoke("get_config");
        appConfig.setup_done = true; saveConfig();
      }
    });
  });

  // Open settings file buttons
  list.querySelectorAll(".provider-open").forEach(btn => {
    btn.addEventListener("click", async (e) => {
      e.preventDefault();
      e.stopPropagation();
      try { await invoke("open_provider_settings", { providerId: btn.dataset.provider }); } catch (e) {}
    });
  });
}

// ─── Dropdown ───
// kind: "completion" (container #provider-sounds-list, config.provider_sounds)
//     | "waiting"    (container #provider-waiting-sounds-list, config.provider_waiting_sounds)
async function renderProviderSounds(kind = "completion") {
  const configKey = kind === "waiting" ? "provider_waiting_sounds" : "provider_sounds";
  const containerId = kind === "waiting" ? "provider-waiting-sounds-list" : "provider-sounds-list";
  const filenameSuffix = kind === "waiting" ? "-waiting." : ".";
  const container = $(containerId);
  if (!container) return;

  let sounds = [];
  try { sounds = await invoke("list_sounds"); } catch(e) {}

  if (sounds.length === 0) {
    container.innerHTML = `<div class="dropdown-empty">音效資料夾目前沒有檔案。點 📁 後放入 MP3/WAV/OGG 即可。</div>`;
    return;
  }

  if (!appConfig.appearance[configKey]) appConfig.appearance[configKey] = {};

  // Auto-match: only if user has never set this provider's sound
  // Use "__none__" as explicit "no sound" marker (empty string would be ambiguous)
  PROVIDER_ORDER.forEach(pid => {
    if (!(pid in appConfig.appearance[configKey])) {
      const match = sounds.find(s => s.toLowerCase().startsWith(pid + filenameSuffix));
      if (match) appConfig.appearance[configKey][pid] = match;
    }
  });

  container.innerHTML = PROVIDER_ORDER
    .filter(pid => appConfig.providers[pid] && appConfig.providers[pid].settings_path)
    .map(pid => {
      const p = appConfig.providers[pid];
      const stored = appConfig.appearance[configKey][pid];
      // Treat both "__none__" and "" as None
      const isNone = stored === "__none__" || stored === "";
      const display = isNone || !stored ? "(不播放)" : stored;
      return `<div class="provider-sound-row">
        ${providerIconHtml(pid, 16)}
        <span class="provider-sound-name">${esc(cleanProviderName(p.name))}</span>
        <div class="custom-dropdown sound-dd" data-provider="${pid}">
          <div class="dropdown-selected">${esc(display)}</div>
          <div class="dropdown-options hidden">
            <div class="dropdown-option${isNone ? ' active' : ''}" data-value="__none__">(不播放)</div>
            ${sounds.map(s => `<div class="dropdown-option${s === stored ? ' active' : ''}" data-value="${esc(s)}">${esc(s)}</div>`).join("")}
          </div>
        </div>
        <button class="icon-btn play-btn" data-sound="${esc(isNone || !stored ? "" : stored)}" title="試聽">
          <svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z"/></svg>
        </button>
      </div>`;
    }).join("");

  // Wire dropdowns
  container.querySelectorAll(".sound-dd").forEach(dd => {
    const selected = dd.querySelector(".dropdown-selected");
    const options = dd.querySelector(".dropdown-options");
    const pid = dd.dataset.provider;

    selected.addEventListener("click", async (e) => {
      e.stopPropagation();
      // Close all other dropdowns
      container.querySelectorAll(".dropdown-options").forEach(o => o !== options && o.classList.add("hidden"));
      // Decide direction: open up if not enough space below
      const rect = selected.getBoundingClientRect();
      const spaceBelow = window.innerHeight - rect.bottom;
      options.classList.toggle("up", spaceBelow < 180);
      // Rescan sounds folder before opening
      if (options.classList.contains("hidden")) {
        const freshSounds = await invoke("list_sounds");
        const stored = appConfig.appearance[configKey][pid];
        const isNone = stored === "__none__" || stored === "";
        options.innerHTML =
          `<div class="dropdown-option${isNone ? ' active' : ''}" data-value="__none__">(不播放)</div>` +
          freshSounds.map(s => `<div class="dropdown-option${s === stored ? ' active' : ''}" data-value="${esc(s)}">${esc(s)}</div>`).join("");
        // Rewire click handlers for new options
        options.querySelectorAll(".dropdown-option").forEach(opt => {
          opt.addEventListener("click", (ev) => {
            ev.stopPropagation();
            const val = opt.dataset.value;
            const optIsNone = val === "__none__";
            selected.textContent = optIsNone ? "(不播放)" : val;
            appConfig.appearance[configKey][pid] = val;
            if (!optIsNone) playSound(val);
            options.classList.add("hidden");
            options.querySelectorAll(".dropdown-option").forEach(o => o.classList.toggle("active", o.dataset.value === val));
            const playBtn = dd.parentElement.querySelector(".play-btn");
            if (playBtn) playBtn.dataset.sound = optIsNone ? "" : val;
            saveConfig();
          });
        });
      }
      options.classList.toggle("hidden");
    });

    options.querySelectorAll(".dropdown-option").forEach(opt => {
      opt.addEventListener("click", (e) => {
        e.stopPropagation();
        const val = opt.dataset.value; // "" never, either filename or "__none__"
        const isNone = val === "__none__";
        selected.textContent = isNone ? "(不播放)" : val;
        appConfig.appearance[configKey][pid] = val;
        if (!isNone) playSound(val);
        options.classList.add("hidden");
        options.querySelectorAll(".dropdown-option").forEach(o => o.classList.toggle("active", o.dataset.value === val));
        const playBtn = dd.parentElement.querySelector(".play-btn");
        if (playBtn) playBtn.dataset.sound = isNone ? "" : val;
        saveConfig();
      });
    });
  });

  // Wire preview buttons
  container.querySelectorAll(".play-btn").forEach(btn => {
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      if (btn.dataset.sound) playSound(btn.dataset.sound);
    });
  });
}

// One global click handler closes all sound dropdowns across both sections
document.addEventListener("click", () => {
  document.querySelectorAll("#provider-sounds-list .dropdown-options, #provider-waiting-sounds-list .dropdown-options")
    .forEach(o => o.classList.add("hidden"));
});

// ─── Config save ───
async function saveConfig() {
  try { await invoke("save_app_config", { newConfig: appConfig }); } catch (e) {}
}

// ─── State ───
let lastStructureJson = ""; // tracks session add/remove/state changes (excludes timer)
let lastState = null;

function renderStateUnavailable(error) {
  lastState = null;
  lastStructureJson = "__state_unavailable__";
  const project = $("capsule-project");
  const status = $("capsule-status");
  const time = $("capsule-time");
  const icons = $("capsule-icons");
  const count = $("capsule-count");
  const quota = $("capsule-quota");
  const errorDot = $("capsule-error-dot");
  if (project) project.textContent = APP_NAME;
  if (status) {
    status.textContent = "資料來源中斷";
    status.className = "capsule-status stale";
  }
  if (time) time.style.display = "none";
  if (icons) icons.innerHTML = "";
  if (count) count.classList.add("hidden");
  if (quota) {
    quota.classList.add("hidden");
    quota.classList.remove("warn", "crit");
    delete quota.dataset.provider;
  }
  if (errorDot) {
    errorDot.classList.add("hidden");
    errorDot.textContent = "";
  }
  const sessionList = $("session-list");
  if (sessionList) {
    sessionList.innerHTML = `<div class="event-empty">（資料來源中斷，暫停顯示舊 session）</div>`;
  }
  const filterBar = $("filter-bar");
  if (filterBar) {
    filterBar.classList.add("hidden");
    filterBar.innerHTML = "";
  }
  // 資料來源斷線：等待回應區塊一併清空，不殘留可能已不存在的 session
  const uvWaiting = $("uv-waiting");
  if (uvWaiting) uvWaiting.innerHTML = "";
  for (const gridId of ["bot-grid", "local-grid"]) {
    const grid = $(gridId);
    if (grid) grid.innerHTML = `<div class="event-empty">（資料來源中斷）</div>`;
  }
  if (error) console.warn("[state] get_state failed; cleared stale UI", error);
}

async function refreshState() {
  if (refreshStateInFlight) {
    refreshStateQueued = true;
    return;
  }
  refreshStateInFlight = true;
  try {
    const st = await invoke("get_state");
    lastState = st;

    // Build a structure key that ignores formatted_time but includes active session
    const activeId = st.active_session?.id || "";
    const structureKey = JSON.stringify({
      active: activeId,
      sessions: st.sessions.map(s =>
        s.id + s.state + s.provider + (s.last_prompt || "") + (s.cwd || "") +
        (s.thinking ? "T" : "") +
        (s.tool_calls || []).map(t => t.id + t.status).join("|") +
        "|" + (s.tokens_input || 0) + ":" + (s.tokens_output || 0)
      )
    });

    if (structureKey !== lastStructureJson) {
      // Sessions changed — full re-render
      lastStructureJson = structureKey;
      renderCapsule(st);
      renderSessions(st);
      if (currentView === "dashboard") renderDashboard(st);
      if (currentView === "expanded" || currentView === "dashboard" || currentView === "events") fitWindow();
      window.UsageView?.drawWaiting?.(); // 等待回應區塊跟著 session 狀態即時增減
      maybeSendTelegramLongTask(st.sessions || []);
    } else {
      // Only timers changed — update in place
      renderCapsule(st);
      updateTimers(st);
      if (currentView === "dashboard") {
        renderDashboard(st);
      }
    }
  } catch (e) {
    renderStateUnavailable(e);
  } finally {
    refreshStateInFlight = false;
    if (refreshStateQueued) {
      refreshStateQueued = false;
      refreshState();
    }
  }
}

function updateTimers(st) {
  // Update timer text without destroying DOM (preserves hover state)
  st.sessions.forEach(s => {
    const row = document.querySelector(`.session-row[data-id="${s.id}"] .session-time`);
    if (row && s.is_active) {
      row.textContent = s.formatted_time;
    }
  });
}

function renderCapsule(st) {
  // manual provider override（#10 multi-provider tab）已隨 capsule chips 一併退役
  const s = st.active_session;
  window.__lastSt = st;  // 給 click handler 用

  // 2026-07-17 產品轉向額度監控：capsule 不再顯示 session 舊資訊
  // （專案名/執行中/計時/計數），改為固定 app 名 + per-CLI 額度 chips
  // （updateCapsuleQuota 渲染）。session 細節仍在 expanded view。
  $("capsule").classList.remove("multi-active");
  $("capsule-project").textContent = APP_NAME;
  $("capsule-status").textContent = "";
  $("capsule-status").className = "capsule-status idle";
  $("capsule-time").style.display = "none";
  $("capsule-count").classList.add("hidden");

  // 近 10 分鐘失敗計數 → 紅點
  checkRecentFailures();

  // capsule 額度 chips（snapshots.__local__ 由 refreshQuotas 填）
  updateCapsuleQuota();

  // PUA R112 Capsule Brief: 同步更新 brief 內容（hover 才顯示，但內容隨 state 持續更新）
  updateCapsuleBrief(s);
}

// PUA R112: 把當前 active session 的 last_prompt 摘要渲染到 #capsule-brief
// - 純前端 transform, 0 後端改（session.last_prompt 已在 hook_event.rs:69-70 capture）
// - 多行 prompt 壓平成單行；>80 字截斷到 77 + "…"
// - 工具名 + cwd + duration 拼到 meta 行
function updateCapsuleBrief(s) {
  const briefEl = $("capsule-brief");
  const textEl = $("capsule-brief-text");
  const metaEl = $("capsule-brief-meta");
  const timeEl = $("capsule-brief-time");
  if (!briefEl || !textEl || !metaEl || !timeEl) return;

  if (!s) {
    textEl.textContent = "（無 active session）";
    textEl.classList.add("empty");
    metaEl.textContent = "";
    timeEl.textContent = "";
    return;
  }

  // prompt 截斷：去掉多餘換行 + 截到 80 字
  const rawPrompt = (s.last_prompt || "").replace(/\s+/g, " ").trim();
  const MAX_PROMPT = 80;
  if (rawPrompt) {
    textEl.textContent = rawPrompt.length > MAX_PROMPT
      ? rawPrompt.slice(0, MAX_PROMPT - 1) + "…"
      : rawPrompt;
    textEl.classList.remove("empty");
  } else {
    textEl.textContent = "（此 session 還沒有 prompt）";
    textEl.classList.add("empty");
  }

  // meta: provider · state · tool · cwd short · duration
  const parts = [];
  parts.push(s.provider);
  if (s.state) {
    const stateMap = { working: "執行中", waiting_for_user: "等你回", idle: "閒置", stale: "過期" };
    parts.push(stateMap[s.state] || s.state);
  }
  if (s.last_tool_name) parts.push(`🔧 ${s.last_tool_name}`);
  if (s.cwd) {
    const cwdShort = s.cwd.length > 24 ? "…" + s.cwd.slice(-22) : s.cwd;
    parts.push(`📁 ${cwdShort}`);
  }
  if (s.is_active && s.formatted_time) parts.push(`⏱ ${s.formatted_time}`);
  metaEl.innerHTML = parts.map((p, i) =>
    (i > 0 ? '<span class="meta-sep">·</span>' : "") +
    `<span>${esc(p)}</span>`
  ).join("");

  // head time 顯示最近事件時間（精簡, optional）
  if (s.is_active && s.formatted_time) {
    timeEl.textContent = s.formatted_time;
  } else {
    timeEl.textContent = "";
  }

  // 同步 CSS var 給 brief 寬度（讓 brief 跟 capsule 同寬）
  const w = appConfig.appearance.capsule_width || 300;
  briefEl.style.setProperty("--capsule-w", `${w}px`);
}

// PUA R112: toggle Capsule Brief 顯示（hover 觸發）
// ─── 完成/等待即時 toast（app 內小卡，Windows toast 之外的介面內提示）───
let lpToastTimer = null;
const lpToastLast = new Map(); // "provider|text" -> 上次顯示 ts（5s 去重防洗版）
function showTaskToast(provider, text, kind) {
  const el = $("lp-toast");
  if (!el) return;
  const key = `${provider}|${text}`;
  const now = Date.now();
  if (now - (lpToastLast.get(key) || 0) < 5000) return;
  lpToastLast.set(key, now);
  const label = PROVIDER_LABEL?.[provider] || provider;
  // --capsule-w 只被 brief 設在自己身上（sibling 繼承不到），toast 自帶一份
  el.style.setProperty("--capsule-w", `${appConfig?.appearance?.capsule_width || DEFAULT_CAPSULE_W}px`);
  el.dataset.provider = provider;
  el.dataset.kind = kind || ""; // "done" 才會在點擊時展開完成詳情
  el.innerHTML = `${providerIconHtml(provider, 14)}<span class="lp-toast-label">${esc(label)}</span><span class="lp-toast-text">${esc(text)}</span>`;
  el.classList.remove("hidden");
  el.setAttribute("aria-hidden", "false");
  fitWindow();
  clearTimeout(lpToastTimer);
  lpToastTimer = setTimeout(() => {
    el.classList.add("hidden");
    el.setAttribute("aria-hidden", "true");
    fitWindow();
  }, 4500);
}

// 點 toast → 收掉 toast。完成 toast：開額度面板＋直接展開該 provider 最新
// 完成紀錄詳情；等待 toast：直接切到該 session 的終端機（要跳回去回話），
// 切不過去才退回開面板。
document.addEventListener("click", (e) => {
  const t = e.target.closest("#lp-toast");
  if (!t || t.classList.contains("hidden")) return;
  clearTimeout(lpToastTimer);
  t.classList.add("hidden");
  t.setAttribute("aria-hidden", "true");
  if (t.dataset.kind === "wait" && t.dataset.provider) {
    invoke("focus_provider_terminal", { provider: t.dataset.provider }).catch((err) => {
      console.warn("[main] focus_provider_terminal 失敗，退回開面板", err);
      showView("usage");
    });
    return;
  }
  showView("usage");
  if (t.dataset.kind === "done" && t.dataset.provider) window.UsageView?.openLatest?.(t.dataset.provider);
});

function showCapsuleBrief(visible) {
  const el = $("capsule-brief");
  if (!el) return;
  if (visible) {
    el.classList.remove("hidden");
    el.classList.add("visible");
    el.setAttribute("aria-hidden", "false");
  } else {
    el.classList.remove("visible");
    el.classList.add("hidden");
    el.setAttribute("aria-hidden", "true");
  }
}

// 掃目前採用的 quota source 找「最緊」配額（<100 的最小值），顯示在 capsule
// 2026-07-17 使用者定案：capsule 極簡——品牌章 + app 名，不放任何額度資訊
// （chips 與最緊徽章都撤），額度細節一律在 hover 展開的 usage 面板看。
function updateCapsuleQuota() {
  const badge = $("capsule-quota");
  if (badge) {
    badge.classList.add("hidden");
    badge.classList.remove("warn", "crit");
    delete badge.dataset.provider;
  }
  const iconsEl = $("capsule-icons");
  if (iconsEl) iconsEl.innerHTML = "";
}

let capsuleInteractionsBound = false;

// Capsule-quota 點擊跳展開面板 + scroll 到該 provider row
function bindCapsuleInteractions() {
  if (capsuleInteractionsBound) return;
  capsuleInteractionsBound = true;

  const cq = document.getElementById("capsule-quota");
  if (!cq) return;
  cq.addEventListener("click", (e) => {
    e.stopPropagation();
    const prov = cq.dataset.provider;
    if (!prov) return;
    showView("usage");
    // 等 fitWindow 完成再 scroll
    setTimeout(() => {
      const row = document.querySelector(`.quota-card[data-provider="${cssEsc(prov)}"]`)
        || document.querySelector(`.quota-runner[data-provider="${cssEsc(prov)}"]`);
      if (row) {
        row.scrollIntoView({ behavior: "smooth", block: "center" });
        row.classList.add("flash");
        setTimeout(() => row.classList.remove("flash"), 1400);
      }
    }, 120);
  });
}

function renderSessions(st) {
  const aid = st.active_session?.id;
  // Filter bar: 若 sessionFilter 啟用，顯示 "篩選中: CICX ✕"
  const fbar = $("filter-bar");
  if (sessionFilter) {
    const label = PROVIDER_LABEL[sessionFilter] || sessionFilter;
    fbar.classList.remove("hidden");
    fbar.innerHTML = `<span class="filter-badge">篩選中：${esc(label)}</span><button id="filter-clear" class="icon-btn" title="清除篩選（ESC）">✕</button>`;
    fbar.querySelector("#filter-clear").addEventListener("click", () => {
      sessionFilter = null;
      renderSessions(st);
      fitWindow();
    });
  } else {
    fbar.classList.add("hidden");
    fbar.innerHTML = "";
  }

  const visible = sessionFilter
    ? st.sessions.filter(s => s.provider === sessionFilter)
    : st.sessions;

  $("session-list").innerHTML = (() => {
    // R168 session clustering: active 狀態 (working/waiting_for_user/stale) 排序在前,
    // idle ≥2 時自動收成可折疊 cluster。解決「5 session 折疊看不到 active」痛點。
    const sorted = visible.slice().sort((a, b) =>
      (STATE_PRIORITY[a.state] ?? 3) - (STATE_PRIORITY[b.state] ?? 3)
    );
    const active = sorted.filter(s => s.state !== "idle");
    const idle = sorted.filter(s => s.state === "idle");
    const html = [active.map(s => renderSessionRow(s, aid)).join("")];
    if (idle.length >= 2) {
      html.push(
        `<div class="session-cluster" data-cluster="idle">
          <span class="session-cluster-toggle">${idleClusterExpanded ? "▾" : "▸"}</span>
          <span class="session-cluster-label">閒置 · ${idle.length} 場</span>
          <span class="session-row-spacer"></span>
          <span class="session-cluster-hint">點擊展開</span>
        </div>`
      );
      if (idleClusterExpanded) {
        html.push(idle.map(s => renderSessionRow(s, aid)).join(""));
      }
    } else {
      html.push(idle.map(s => renderSessionRow(s, aid)).join(""));
    }
    return html.join("");
  })();

  $("session-list").querySelectorAll(".session-row").forEach(r => {
    r.addEventListener("mouseenter", () => r.classList.add("hovered"));
    r.addEventListener("mouseleave", () => r.classList.remove("hovered"));
    // Click row head → toggle compact/expanded + select
    r.addEventListener("click", (e) => {
      if (e.target.closest(".session-remove")) return;
      const id = r.dataset.id;
      if (sessionExpanded.has(id)) sessionExpanded.delete(id);
      else sessionExpanded.add(id);
      invoke("select_session", { id });
      refreshState();
    });
  });

  $("session-list").querySelectorAll(".session-cluster").forEach(c => {
    c.addEventListener("click", () => {
      idleClusterExpanded = !idleClusterExpanded;
      refreshState();
    });
  });

  $("session-list").querySelectorAll(".session-remove").forEach(btn => {
    btn.addEventListener("mouseenter", () => { btn.style.color = "rgb(255,80,80)"; });
    btn.addEventListener("mouseleave", () => { btn.style.color = ""; });
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      invoke("remove_session", { id: btn.dataset.rid });
      refreshState();
    });
  });
}

// ─── Session meta (tools / thinking / tokens) ───
const TOOL_STATUS_ICON = {
  running: "⏳",
  completed: "✓",
  failed: "✗",
};

// 跨平台 home 路徑縮短：Linux / Mac `/home/user`, Windows `C:\Users\user` / MSYS `/c/Users/user`
function shortenCwd(cwd) {
  if (!cwd) return "";
  return cwd
    .replace(/^\/home\/[^/]+/, "~")
    .replace(/^\/Users\/[^/]+/, "~")
    .replace(/^[A-Z]:\\Users\\[^\\]+/i, "~")
    .replace(/^\/[a-z]\/[Uu]sers\/[^/]+/, "~");
}

function formatTokens(n) {
  if (!n || n <= 0) return "";
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "k";
  return String(n);
}

function buildSessionMetaHtml(s) {
  const parts = [];

  if (s.thinking) {
    parts.push(`<span class="meta-thinking"><span class="meta-thinking-dot"></span>思考中</span>`);
  }

  if (Array.isArray(s.tool_calls) && s.tool_calls.length > 0) {
    const chips = s.tool_calls.slice(-3).map(tc => {
      const icon = TOOL_STATUS_ICON[tc.status] || "·";
      const cls = `meta-tool meta-tool-${tc.status || "running"}`;
      const title = tc.title || tc.id || "tool";
      return `<span class="${cls}" title="${esc(tc.status || "")}">${esc(title)} ${icon}</span>`;
    }).join("");
    parts.push(`<span class="meta-tools">${chips}</span>`);
  }

  const ti = s.tokens_input || 0;
  const to = s.tokens_output || 0;
  if (ti > 0 || to > 0) {
    parts.push(`<span class="meta-tokens" title="in / out tokens">${formatTokens(ti)} · ${formatTokens(to)} tok</span>`);
  }

  return parts.join("");
}

// ─── Apply ───
function applyAccentColor(n) {
  // 自訂 hex 優先於預設 5 色
  const customHex = appConfig?.appearance?.accent_custom_hex || "";
  const resolved = /^#[0-9a-fA-F]{6}$/.test(customHex) ? customHex : (COLORS[n] || COLORS.purple);
  document.documentElement.style.setProperty("--accent", resolved);
  document.querySelectorAll(".color-dot").forEach(d => d.classList.toggle("active", d.dataset.color === n && !customHex));
}
function applyTextSize(s) {
  // text_scale（Ctrl+滾輪連續值）優先；null 時沿用 S/M/L 檔位
  const sc = appConfig?.appearance?.text_scale ?? SCALES[s] ?? 1;
  document.documentElement.style.setProperty("--scale", sc);
  document.querySelectorAll(".size-btn").forEach(b =>
    b.classList.toggle("active", Math.abs((SCALES[b.dataset.size] ?? 1) - sc) < 0.001));
}

function applyTheme(t) {
  document.documentElement.setAttribute("data-theme", t);
}

function applyFontFamily(ff) {
  document.documentElement.style.setProperty("--font-family-custom", ff || "");
  document.body.style.fontFamily = ff || "";
}

function applyBgOpacity(pct) {
  const v = Math.max(30, Math.min(100, parseInt(pct, 10) || 100));
  document.documentElement.style.setProperty("--bg-opacity", (v / 100).toFixed(2));
}

// 背景照片/影片：根據 type 切換 img / video / hidden
// 本機路徑走 Rust get_background_data_url (base64 data URL)；網路 URL 直接用
async function resolveBgSrc(raw) {
  if (!raw) return "";
  const s = raw.trim();
  if (/^(https?:|data:|blob:)/.test(s)) return s;
  try {
    return await invoke("get_background_data_url", { path: s });
  } catch (e) {
    console.error("bg data url failed:", e);
    return "";
  }
}
// 背景：圖片走 CSS var `--bg-image-url`（styles.css 的 body.has-bg::before 吃這個）
// 影片走 <video>。統一到單一路徑，移除舊 #lp-bg-div 避免 2 套機制打架。
async function applyBackground(type, path, blur, imageOpacity) {
  const bgImg = document.getElementById("bg-fullscreen");
  const bgVid = document.getElementById("bg-fullscreen-video");
  // 務實：用半透 overlay 達到視覺背景效果（Tauri WebView2 底層 z-index:-1 被透明處理吃掉）
  // image_opacity 100 = 很透(0.25)，看 UI；50 = 中等(0.5)；越小越明顯圖 = opacity 反向映射
  const userPct = parseInt(imageOpacity, 10) || 60;
  const alpha = (1 - userPct / 100 * 0.6).toFixed(2);  // userPct=100→0.4, 60→0.64, 10→0.94
  const blurPx = (parseInt(blur, 10) || 0) + "px";
  const clear = () => {
    document.body.classList.remove("has-bg");
    if (bgImg) { bgImg.style.display = "none"; bgImg.src = ""; }
    if (bgVid) { bgVid.style.display = "none"; try { bgVid.pause(); } catch {} bgVid.removeAttribute("src"); }
  };
  if (!type || type === "none" || !path) { clear(); return; }
  const src = await resolveBgSrc(path);
  if (!src) { clear(); return; }
  document.body.classList.add("has-bg");
  // 實際 opacity = userPct/100 的 0.3-0.5 倍 — 太不透明 UI 看不清
  const visAlpha = (userPct / 100 * 0.4).toFixed(2);
  if (type === "image" && bgImg) {
    bgImg.src = src;
    bgImg.style.display = "block";
    bgImg.style.opacity = visAlpha;
    bgImg.style.filter = "blur(" + blurPx + ")";
    if (bgVid) { bgVid.style.display = "none"; try { bgVid.pause(); } catch {} bgVid.removeAttribute("src"); }
  } else if (type === "video" && bgVid) {
    if (bgVid.getAttribute("src") !== src) { bgVid.src = src; bgVid.load(); }
    try { bgVid.play(); } catch {}
    bgVid.style.display = "block";
    bgVid.style.opacity = visAlpha;
    bgVid.style.filter = "blur(" + blurPx + ")";
    if (bgImg) { bgImg.style.display = "none"; bgImg.src = ""; }
  }
}

// ─── Sounds ───
async function playSound(name) {
  if (!name) return;
  try { await invoke("play_sound_file", { name }); } catch (e) {}
}

/// Play sound for a provider — uses user-configured per-provider sound
/// kind: "completion" (provider_sounds) | "waiting" (provider_waiting_sounds)
async function playProviderSound(provider, kind = "completion") {
  const key = kind === "waiting" ? "provider_waiting_sounds" : "provider_sounds";
  const sound = appConfig.appearance[key]?.[provider];
  if (sound && sound !== "__none__") await playSound(sound);
}

function esc(s) { const d = document.createElement("div"); d.textContent = s; return d.innerHTML.replace(/"/g, "&quot;"); }

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", () => {
    bindCapsuleInteractions();
    init();
  });
} else {
  bindCapsuleInteractions();
  init();
}

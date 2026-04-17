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

const PROVIDER_COLORS = {
  claude: "#d97757",
  gemini: "#4285f4",
  copilot: "#6e40c9",
  codex: "#10a37f",
};

const APP_NAME = "龍蝦監控";

// ─── State ───
const COLORS = {
  purple: "rgb(217,128,255)", cyan: "rgb(77,217,255)",
  green: "rgb(77,242,153)", orange: "rgb(255,153,51)", pink: "rgb(255,102,153)",
};
const SCALES = { small: 0.85, medium: 1, large: 1.15 };
const W = 300;

let currentView = "capsule";
let serverPort = 0;
let appConfig = null;
let collapsedAt = 0;

const $ = (id) => document.getElementById(id);

// ─── Window resize ───
async function fitWindow() {
  await new Promise(r => requestAnimationFrame(r));
  const h = Math.max(Math.ceil(document.getElementById("app").scrollHeight) + 2, 46);
  await invoke("resize_window", { width: W, height: h });
}

// ─── View switching ───
function showView(view) {
  const wasExpanded = currentView !== "capsule";
  currentView = view;
  $("view-expanded").classList.toggle("hidden", view !== "expanded");
  $("view-settings").classList.toggle("hidden", view !== "settings");
  $("capsule").classList.toggle("has-panel-below", view === "expanded" || view === "settings");
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
}

// ─── Provider icon HTML ───
function providerIconHtml(providerId, size = 16) {
  const svg = PROVIDER_ICONS[providerId] || PROVIDER_ICONS.claude;
  const color = PROVIDER_COLORS[providerId] || "#888";
  return `<span class="provider-icon" style="width:${size}px;height:${size}px;color:${color}">${svg}</span>`;
}

// ─── Init ───
async function init() {
  if (!window.__TAURI_INTERNALS__) { setTimeout(init, 200); return; }

  try {
    serverPort = await invoke("get_server_port");
    appConfig = await invoke("get_config");
  } catch (e) { return; }

  applyAccentColor(appConfig.appearance.accent_color);
  applyTextSize(appConfig.appearance.text_size);
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
      showView("expanded");
    }
  }

  // Drag
  $("capsule").addEventListener("mousedown", (e) => {
    if (e.buttons === 1) invoke("plugin:window|start_dragging", { label: "main" }).catch(() => {});
  });

  // Hover expand
  $("capsule").addEventListener("mouseenter", () => {
    if (currentView === "capsule" && !appConfig.appearance.pin_expanded && (Date.now() - collapsedAt > 500)) {
      showView("expanded");
    }
  });

  // Collapse via cursor-left
  const collapseCallbackId = window.__TAURI_INTERNALS__.transformCallback(() => {
    if (currentView === "expanded" && !appConfig.appearance.pin_expanded) showView("capsule");
  });
  invoke("plugin:event|listen", { event: "cursor-left", target: { kind: "Any" }, handler: collapseCallbackId }).catch(() => {});

  setInterval(() => {
    if (currentView !== "expanded" || appConfig.appearance.pin_expanded) return;
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
      if (currentView === "expanded" && !appConfig.appearance.pin_expanded) showView("capsule");
    });
    invoke("plugin:event|listen", { event: "cursor-left", target: { kind: "Any" }, handler: cb2 }).catch(() => {});

    // Listen for task-completed → play provider-specific completion sound
    const soundCb = window.__TAURI_INTERNALS__.transformCallback((evt) => {
      if (!appConfig.appearance.sound_enabled) return;
      const provider = (evt && evt.payload) || "claude";
      playProviderSound(provider, "completion");
    });
    invoke("plugin:event|listen", { event: "task-completed", target: { kind: "Any" }, handler: soundCb }).catch(() => {});

    // Listen for task-waiting → play provider-specific waiting sound
    const waitingCb = window.__TAURI_INTERNALS__.transformCallback((evt) => {
      if (!appConfig.appearance.sound_enabled) return;
      const provider = (evt && evt.payload) || "claude";
      playProviderSound(provider, "waiting");
    });
    invoke("plugin:event|listen", { event: "task-waiting", target: { kind: "Any" }, handler: waitingCb }).catch(() => {});
  }, 2000);

  // Pin
  $("btn-pin").addEventListener("click", () => {
    appConfig.appearance.pin_expanded = !appConfig.appearance.pin_expanded;
    $("toggle-pin").checked = appConfig.appearance.pin_expanded;
    $("btn-pin").classList.toggle("active", appConfig.appearance.pin_expanded);
    if (!appConfig.appearance.pin_expanded && currentView === "expanded") showView("capsule");
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
    showView(appConfig.appearance.pin_expanded ? "expanded" : "capsule");
  });

  $("toggle-pin").addEventListener("change", (e) => {
    appConfig.appearance.pin_expanded = e.target.checked;
    $("btn-pin").classList.toggle("active", appConfig.appearance.pin_expanded);
    if (!appConfig.appearance.pin_expanded) showView("capsule"); else showView("expanded");
    saveConfig();
  });

  $("toggle-theme").addEventListener("change", (e) => {
    const theme = e.target.checked ? "light" : "dark";
    appConfig.appearance.theme = theme;
    applyTheme(theme);
    saveConfig();
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
    applyTextSize(b.dataset.size);
    fitWindow();
    saveConfig();
  }));

  $("btn-github").addEventListener("click", () => {
    invoke("open_app_config").catch(() => {});
  });

  // Settings tab switching
  document.querySelectorAll(".settings-tab").forEach(tab => {
    tab.addEventListener("click", () => {
      const tabId = tab.dataset.tab;
      document.querySelectorAll(".settings-tab").forEach(t => t.classList.toggle("active", t === tab));
      document.querySelectorAll(".settings-tab-panel").forEach(p =>
        p.classList.toggle("active", p.dataset.panel === tabId)
      );
      fitWindow();
    });
  });

  refreshState();
  setInterval(refreshState, 1000);
}

// ─── Providers in settings ───
const PROVIDER_ORDER = ["claude", "codex", "copilot", "gemini"];

async function renderProviders() {
  const detected = await invoke("detect_installed_providers");
  const list = $("provider-list");

  // Fixed order instead of HashMap random order
  const entries = PROVIDER_ORDER
    .filter(id => appConfig.providers[id])
    .map(id => [id, appConfig.providers[id]]);

  list.innerHTML = entries.map(([id, p]) => {
    const found = detected[id] || false;
    const canEnable = !!p.settings_path;
    const checked = p.enabled ? "checked" : "";
    const statusText = !canEnable ? "尚未支援"
                     : found ? "已偵測"
                     : "";
    const statusClass = !canEnable ? "provider-pending"
                      : found ? "provider-found"
                      : "";

    return `<div class="provider-item ${!canEnable ? 'disabled' : ''}">
      <input type="checkbox" class="provider-check" data-provider="${id}" ${checked} ${!canEnable ? 'disabled' : ''}>
      ${providerIconHtml(id, 18)}
      <span class="provider-name">${esc(p.name)}</span>
      ${statusText ? `<span class="${statusClass}">${statusText}</span>` : ""}
      ${canEnable ? `<button class="provider-open" data-provider="${id}" title="開啟 ${esc(p.name)} 設定檔"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="9" y1="13" x2="15" y2="13"/><line x1="9" y1="17" x2="15" y2="17"/></svg></button>` : ""}
    </div>`;
  }).join("");

  // Listen for toggle changes
  list.querySelectorAll(".provider-check").forEach(cb => {
    cb.addEventListener("change", async () => {
      const pid = cb.dataset.provider;
      if (cb.checked) {
        try { await invoke("install_provider_hooks", { providerId: pid }); } catch (e) {}
      } else {
        // Remove hooks from CLI's settings file too
        try { await invoke("remove_provider_hooks", { providerId: pid }); } catch (e) {}
      }
      appConfig = await invoke("get_config");
      appConfig.setup_done = true; saveConfig();
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
    .filter(pid => appConfig.providers[pid])
    .map(pid => {
      const p = appConfig.providers[pid];
      const stored = appConfig.appearance[configKey][pid];
      // Treat both "__none__" and "" as None
      const isNone = stored === "__none__" || stored === "";
      const display = isNone || !stored ? "(不播放)" : stored;
      return `<div class="provider-sound-row">
        ${providerIconHtml(pid, 16)}
        <span class="provider-sound-name">${esc(p.name)}</span>
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

async function refreshState() {
  try {
    const st = await invoke("get_state");
    lastState = st;

    // Build a structure key that ignores formatted_time but includes active session
    const activeId = st.active_session?.id || "";
    const structureKey = JSON.stringify({
      active: activeId,
      sessions: st.sessions.map(s => s.id + s.state + s.provider + (s.last_prompt || "") + (s.cwd || ""))
    });

    if (structureKey !== lastStructureJson) {
      // Sessions changed — full re-render
      lastStructureJson = structureKey;
      renderCapsule(st);
      renderSessions(st);
      if (currentView === "expanded") fitWindow();
    } else {
      // Only timers changed — update in place
      renderCapsule(st);
      updateTimers(st);
    }
  } catch (e) {}
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
  const s = st.active_session;

  // Capsule icons: show active provider icons
  const providers = st.active_providers.length > 0 ? st.active_providers : (s ? [s.provider] : ["claude"]);
  $("capsule-icons").innerHTML = providers.map(p => providerIconHtml(p, 16)).join('<span class="icon-sep">|</span>');

  if (s) {
    $("capsule-project").textContent = s.project_name;
    const stMap = { working: "執行中...", waiting_for_user: "等待處理", stale: "過久未更新" };
    $("capsule-status").textContent = stMap[s.state] || "閒置";
    const stClass = ({ working: "working", waiting_for_user: "waiting_for_user", stale: "stale" })[s.state] || "idle";
    $("capsule-status").className = "capsule-status " + stClass;
    $("capsule-time").textContent = s.is_active ? s.formatted_time : "";
    $("capsule-time").style.display = s.is_active ? "" : "none";
  } else {
    $("capsule-project").textContent = APP_NAME;
    $("capsule-status").textContent = "";
    $("capsule-status").className = "capsule-status";
    $("capsule-time").style.display = "none";
  }

  if (st.session_count > 1) {
    $("capsule-count").classList.remove("hidden");
    let h = "";
    if (st.active_count > 0) {
      h += `<span class="count-active">${st.active_count}</span>`;
      h += `<span class="count-sep">/</span>`;
    }
    h += `<span class="count-total">${st.session_count}</span>`;
    $("capsule-count").innerHTML = h;
  } else $("capsule-count").classList.add("hidden");
}

function renderSessions(st) {
  const aid = st.active_session?.id;
  $("session-list").innerHTML = st.sessions.map(s => {
    const sel = s.id === aid ? " selected" : "";
    const sc = ({ working: "working", waiting_for_user: "waiting_for_user", stale: "stale" })[s.state] || "idle";
    const sl = ({ working: "執行中", waiting_for_user: "等待中", stale: "過舊" })[s.state] || "";
    const cwdShort = s.cwd ? s.cwd.replace(/^\/home\/[^/]+/, "~") : "";
    return `<div class="session-row${sel}" data-id="${s.id}">
      <div class="session-provider-icon">${providerIconHtml(s.provider, 16)}</div>
      <div class="session-info">
        <div class="session-header">
          <span class="session-name">${esc(s.project_name)}</span>
          <span class="status-dot ${sc}"></span>${sl ? `<span class="session-state-label ${sc}">${sl}</span>` : ""}
        </div>
        ${cwdShort ? `<div class="session-cwd">${esc(cwdShort)}</div>` : ""}
        ${s.last_prompt ? `<div class="session-prompt">${esc(s.last_prompt)}</div>` : ""}
      </div>
      ${s.is_active ? `<span class="session-time">${s.formatted_time}</span>` : ""}
      <button class="session-remove" data-rid="${s.id}" title="移除">&times;</button>
    </div>`;
  }).join("");

  $("session-list").querySelectorAll(".session-row").forEach(r => {
    // Show X on row hover
    r.addEventListener("mouseenter", () => r.classList.add("hovered"));
    r.addEventListener("mouseleave", () => r.classList.remove("hovered"));
    // Click row to focus window
    r.addEventListener("click", (e) => {
      if (e.target.closest(".session-remove")) return;
      invoke("select_session", { id: r.dataset.id });
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

// ─── Apply ───
function applyAccentColor(n) {
  document.documentElement.style.setProperty("--accent", COLORS[n] || COLORS.purple);
  document.querySelectorAll(".color-dot").forEach(d => d.classList.toggle("active", d.dataset.color === n));
}
function applyTextSize(s) {
  document.documentElement.style.setProperty("--scale", SCALES[s] || 1);
  document.querySelectorAll(".size-btn").forEach(b => b.classList.toggle("active", b.dataset.size === s));
}

function applyTheme(t) {
  document.documentElement.setAttribute("data-theme", t);
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

function esc(s) { const d = document.createElement("div"); d.textContent = s; return d.innerHTML; }

if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init);
else init();

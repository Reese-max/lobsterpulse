(function () {
  const nowIso = () => new Date().toISOString();
  const providerOrder = ["claude", "codex", "copilot", "gemini"];

  const state = {
    port: 19280,
    config: {
      setup_done: true,
      appearance: {
        accent_color: "orange",
        text_size: "medium",
        theme: "dark",
        pin_expanded: false,
        sound_enabled: true,
        provider_sounds: {
          claude: "claude.mp3",
          codex: "codex.mp3",
          copilot: "copilot.mp3",
          gemini: "__none__",
        },
        provider_waiting_sounds: {
          claude: "claude-waiting.mp3",
          codex: "codex-waiting.mp3",
          copilot: "copilot-waiting.mp3",
          gemini: "__none__",
        },
        sound_name: "",
      },
      providers: {
        claude: { enabled: true, name: "Claude Code", settings_path: "~/.claude/settings.json" },
        codex: { enabled: true, name: "Codex CLI", settings_path: "~/.codex/hooks.json" },
        copilot: { enabled: true, name: "GitHub Copilot CLI", settings_path: "~/.copilot/config.json" },
        gemini: { enabled: false, name: "Gemini CLI", settings_path: "~/.gemini/settings.json" },
      },
    },
    sessions: {
      "claude-demo": {
        id: "claude-demo",
        provider: "claude",
        state: "working",
        start_time: nowIso(),
        last_event_time: nowIso(),
        cwd: "~/Desktop/監控",
        last_tool_name: "Edit",
        last_prompt: "把監控器改成龍蝦監控",
      },
      "codex-demo": {
        id: "codex-demo",
        provider: "codex",
        state: "waiting_for_user",
        start_time: nowIso(),
        last_event_time: nowIso(),
        cwd: "~/projects/duty-scheduler",
        last_tool_name: "shell",
        last_prompt: "檢查 viewer 路由和個人班表資料",
      },
    },
    activeId: "claude-demo",
  };

  const listeners = new Map();
  function emit(event, payload) {
    const handlers = listeners.get(event) || [];
    handlers.forEach((fn) => {
      try { fn({ payload }); } catch (err) { console.warn(err); }
    });
  }

  function formatDuration(startIso) {
    const totalSec = Math.floor((Date.now() - new Date(startIso).getTime()) / 1000);
    const h = Math.floor(totalSec / 3600);
    const m = Math.floor((totalSec % 3600) / 60);
    const s = totalSec % 60;
    return h > 0
      ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
      : `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }

  function sessionInfo(session) {
    const project = session.cwd ? session.cwd.split("/").pop() : session.id;
    const isActive = session.state === "working" || session.state === "waiting_for_user";
    return {
      id: session.id,
      provider: session.provider,
      state: session.state,
      project_name: project,
      cwd: session.cwd,
      is_active: isActive,
      formatted_time: formatDuration(session.start_time),
      last_tool_name: session.last_tool_name,
      last_prompt: session.last_prompt,
    };
  }

  function buildState() {
    const sessions = Object.values(state.sessions).map(sessionInfo);
    sessions.sort((a, b) => {
      if (a.is_active !== b.is_active) return a.is_active ? -1 : 1;
      return providerOrder.indexOf(a.provider) - providerOrder.indexOf(b.provider);
    });

    const active = state.sessions[state.activeId];
    const activeSession = active ? sessionInfo(active) : sessions[0] || null;
    const activeProviders = [...new Set(
      Object.values(state.sessions)
        .filter((s) => s.state === "working" || s.state === "waiting_for_user")
        .map((s) => s.provider)
    )];

    return {
      active_session: activeSession,
      sessions,
      session_count: sessions.length,
      active_count: activeProviders.length,
      active_providers: activeProviders,
    };
  }

  function ensureSession(id, provider, cwd, prompt) {
    if (state.sessions[id]) return state.sessions[id];
    state.sessions[id] = {
      id,
      provider,
      state: "idle",
      start_time: nowIso(),
      last_event_time: nowIso(),
      cwd,
      last_tool_name: null,
      last_prompt: prompt,
    };
    return state.sessions[id];
  }

  function transition(sessionId, newState, extra = {}) {
    const session = state.sessions[sessionId];
    if (!session) return;
    const prev = session.state;
    session.state = newState;
    session.last_event_time = nowIso();
    Object.assign(session, extra);

    if (prev === "working" && newState === "idle") emit("task-completed", session.provider);
    if (prev !== "waiting_for_user" && newState === "waiting_for_user") emit("task-waiting", session.provider);
    emit("session-update");
  }

  const handlers = {
    get_config: () => JSON.parse(JSON.stringify(state.config)),
    save_app_config: ({ newConfig }) => {
      state.config = newConfig;
      return null;
    },
    get_server_port: () => state.port,
    get_state: () => buildState(),
    resize_window: () => null,
    bounce_window: () => null,
    is_cursor_inside: () => false,
    "plugin:window|start_dragging": () => null,
    "plugin:event|unlisten": () => null,
    "plugin:event|listen": ({ event, handler }) => {
      if (!listeners.has(event)) listeners.set(event, []);
      listeners.get(event).push(handler);
      return Date.now();
    },
    detect_installed_providers: () => ({
      claude: true,
      codex: true,
      copilot: true,
      gemini: true,
    }),
    check_provider_setup: () => false,
    install_provider_hooks: ({ providerId }) => {
      if (state.config.providers[providerId]) state.config.providers[providerId].enabled = true;
      return null;
    },
    remove_provider_hooks: ({ providerId }) => {
      if (state.config.providers[providerId]) state.config.providers[providerId].enabled = false;
      return null;
    },
    open_provider_settings: ({ providerId }) => {
      alert(`${providerId} 的設定檔在網頁 demo 不會真的打開。下載桌面版後才會直接跳到本機設定。`);
      return null;
    },
    open_app_config: () => {
      alert("這是網頁 demo。桌面版才會直接打開 ~/.config/lobsterpulse/config.json。");
      return null;
    },
    open_sounds_folder: () => {
      alert("這是網頁 demo。桌面版才會直接打開 lobsterpulse 的音效資料夾。");
      return null;
    },
    list_sounds: () => [
      "claude.mp3",
      "codex.mp3",
      "copilot.mp3",
      "gemini.mp3",
      "claude-waiting.mp3",
      "codex-waiting.mp3",
      "copilot-waiting.mp3",
      "gemini-waiting.mp3",
    ],
    play_sound_file: ({ name }) => {
      try {
        const audio = new Audio(`../../sounds/${name}`);
        audio.volume = 0.55;
        audio.play().catch(() => {});
      } catch (_) {}
      return null;
    },
    select_session: ({ id }) => {
      if (id && state.sessions[id]) state.activeId = id;
      return null;
    },
    remove_session: ({ id }) => {
      if (!id) return null;
      delete state.sessions[id];
      if (state.activeId === id) state.activeId = Object.keys(state.sessions)[0] || null;
      emit("session-update");
      return null;
    },
  };

  window.__TAURI_INTERNALS__ = {
    invoke(cmd, args = {}) {
      const fn = handlers[cmd];
      if (!fn) return Promise.resolve(null);
      try {
        return Promise.resolve(fn(args));
      } catch (err) {
        return Promise.reject(err);
      }
    },
    transformCallback(fn) {
      return fn;
    },
  };

  const timeline = [
    [0, () => transition("claude-demo", "working", { last_tool_name: "Edit", last_prompt: "把設定面板改成繁中" })],
    [8500, () => transition("claude-demo", "idle")],
    [12000, () => transition("codex-demo", "working", { last_tool_name: "shell", last_prompt: "跑一次 release build 檢查 icon" })],
    [17000, () => transition("codex-demo", "waiting_for_user")],
    [22500, () => {
      ensureSession("copilot-demo", "copilot", "~/workspace/openab", "整理 Copilot MCP 清單");
      transition("copilot-demo", "working", { last_tool_name: "Read" });
      state.activeId = "copilot-demo";
    }],
    [29000, () => transition("copilot-demo", "idle")],
    [34000, () => {
      ensureSession("gemini-demo", "gemini", "~/experiments", "整理備用實驗指令");
      transition("gemini-demo", "working");
    }],
    [38000, () => transition("gemini-demo", "idle")],
    [43000, () => {
      transition("claude-demo", "working", { last_tool_name: "Edit", last_prompt: "把 README 和 docs 全部改成品牌版" });
      state.activeId = "claude-demo";
    }],
    [50000, () => transition("claude-demo", "idle")],
  ];

  function scheduleCycle() {
    timeline.forEach(([delay, fn]) => setTimeout(fn, delay));
    setTimeout(scheduleCycle, 56000);
  }

  scheduleCycle();
})();

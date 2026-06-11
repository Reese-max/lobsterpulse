import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

function requireFromNpmExecPath(pkg) {
  const pathParts = (process.env.PATH || "").split(process.platform === "win32" ? ";" : ":");
  for (const part of pathParts) {
    if (!part.endsWith(`${process.platform === "win32" ? "\\" : "/"}node_modules${process.platform === "win32" ? "\\" : "/"}.bin`)) continue;
    const nodeModules = dirname(part);
    try {
      return createRequire(resolve(nodeModules, pkg, "package.json"))(pkg);
    } catch (_) {
      // Keep scanning PATH entries from npm exec.
    }
  }
  throw new Error(`Cannot resolve ${pkg}; run via npm exec --package ${pkg}`);
}

const { chromium } = requireFromNpmExecPath("playwright");

const appUrl = (scenario) => {
  const url = pathToFileURL(resolve("src", "index.html"));
  url.searchParams.set("scenario", scenario);
  return url.toString();
};

const baseConfig = {
  setup_done: true,
  appearance: {
    accent_color: "orange",
    text_size: "medium",
    theme: "dark",
    pin_expanded: false,
    sound_enabled: false,
    system_notifications: false,
    rules_enabled: false,
    provider_sounds: {},
    provider_waiting_sounds: {},
    usage_runners: [],
    background_type: "none",
    background_path: "",
    background_blur: 0,
    background_image_opacity: 60,
    idle_threshold_secs: 300,
    stale_threshold_secs: 1800,
    remove_threshold_secs: 3600,
    capsule_width: 300,
    expanded_width: 300,
  },
  providers: {
    cicx: { enabled: true, name: "CICX" },
    gitx: { enabled: true, name: "GITX" },
    giminix: { enabled: true, name: "GIMINIX" },
    codex_bot: { enabled: true, name: "CODEX" },
    openx: { enabled: true, name: "OPENX" },
    irisx_bot: { enabled: true, name: "IRISX" },
    grokx: { enabled: true, name: "GROKX" },
    lpbot: { enabled: true, name: "LPBOT" },
    mimo: { enabled: false, name: "MIMO" },
    claude: { enabled: true, name: "Claude" },
    codex: { enabled: true, name: "Codex" },
    copilot: { enabled: true, name: "Copilot" },
    gemini: { enabled: true, name: "Gemini" },
  },
  rules: [],
};

async function installMockTauri(page) {
  await page.addInitScript((config) => {
    window.__LP_TEST_CONFIG__ = config;
  }, baseConfig);

  await page.addInitScript(() => {
    const scenario = new URL(location.href).searchParams.get("scenario") || "empty";
    const now = Math.floor(Date.now() / 1000);
    const staleTs = now - 7200;
    const freshTs = now - 30;
    const calls = [];
    const config = JSON.parse(JSON.stringify(window.__LP_TEST_CONFIG__));
    const baseState = {
      active_session: null,
      sessions: [],
      session_count: 0,
      active_count: 0,
      active_providers: [],
      provider_totals: {},
    };
    const mysterySession = {
      id: "mystery-1",
      provider: "mystery_provider",
      state: "working",
      project_name: "未知專案",
      cwd: "C:/tmp",
      is_active: true,
      formatted_time: "00:03",
      last_tool_name: "tool",
      last_prompt: "unknown provider smoke",
    };
    const idleSession = {
      id: "idle-1",
      provider: "claude",
      state: "idle",
      project_name: "Old Project",
      cwd: "C:/old",
      is_active: false,
      formatted_time: "00:00",
      last_event_secs_ago: 120,
      tool_calls: [],
      tokens_input: 0,
      tokens_output: 0,
    };
    const states = {
      empty: baseState,
      idleOnly: {
        ...baseState,
        sessions: [idleSession],
        session_count: 1,
      },
      staleQuota: baseState,
      freshQuota: baseState,
      openabQuota: baseState,
      unknown: {
        ...baseState,
        active_session: mysterySession,
        sessions: [mysterySession],
        session_count: 1,
        active_count: 1,
        active_providers: ["mystery_provider"],
      },
    };
    const quotaRunner = {
      name: "claude",
      label: "Claude quota",
      ok: true,
      text: "session 5h remaining 7%",
      raw: { session_5h_remaining: 7 },
    };
    const handlers = {
      get_config: () => config,
      save_app_config: ({ newConfig }) => Object.assign(config, newConfig),
      get_server_port: () => 19280,
      get_state: () => {
        if (scenario === "stateFailure") throw new Error("mock get_state unavailable");
        return JSON.parse(JSON.stringify(states[scenario] || baseState));
      },
      list_rules: () => [],
      list_sounds: () => [],
      detect_installed_providers: () => Object.fromEntries(Object.keys(config.providers).map((p) => [p, true])),
      check_provider_setup: () => false,
      is_cursor_inside: () => false,
      get_recent_events: () => {
        if (scenario === "stateFailure" || scenario === "eventFailure") throw new Error("mock recent events unavailable");
        return [];
      },
      get_quota_history: () => ({}),
      read_usage_snapshots: () => {
        if (scenario === "quotaFailure") throw new Error("mock usage snapshots unavailable");
        if (scenario === "staleQuota") return { __local__: { source: "local", updated_at: staleTs, runners: [quotaRunner] } };
        if (scenario === "freshQuota") return { __local__: { source: "local", updated_at: freshTs, runners: [quotaRunner] } };
        if (scenario === "openabQuota") return { cicx: { source: "openab", updated_at: freshTs, runners: [quotaRunner] } };
        return {};
      },
      get_live_quota_snapshot: () => {
        if (scenario === "quotaFailure") throw new Error("mock live quota unavailable");
        return { source: "live_api", updated_at: freshTs, runners: [] };
      },
      timeline_recorded_event_count: () => {
        if (scenario === "timelineFailure") throw new Error("mock timeline unavailable");
        return 0;
      },
      timeline_snapshot_24h: () => {
        throw new Error("timeline_snapshot_24h must not be called when no events were recorded");
      },
      resize_window: () => null,
      bounce_window: () => null,
      play_sound_file: () => null,
      "plugin:event|listen": () => Date.now(),
      "plugin:event|unlisten": () => null,
      "plugin:window|start_dragging": () => null,
      "plugin:notification|notify": () => null,
    };
    window.__lpMockCalls = calls;
    window.__TAURI_INTERNALS__ = {
      invoke(cmd, args = {}) {
        calls.push(cmd);
        if (Object.prototype.hasOwnProperty.call(handlers, cmd)) {
          try {
            return Promise.resolve(handlers[cmd](args));
          } catch (err) {
            return Promise.reject(err);
          }
        }
        return Promise.resolve(null);
      },
      transformCallback(fn) {
        return fn;
      },
      event: {
        listen() {
          return Promise.resolve(() => {});
        },
      },
    };
  });
}

async function openScenario(browser, scenario) {
  const page = await browser.newPage({ viewport: { width: 390, height: 720 } });
  await installMockTauri(page);
  await page.goto(appUrl(scenario));
  await page.waitForSelector("#capsule-project");
  return page;
}

async function run() {
  const browser = await chromium.launch();
  try {
    {
      const page = await openScenario(browser, "empty");
      assert.equal(await page.locator("#capsule-icons .provider-icon").count(), 0, "empty capsule must not synthesize provider icons");
      await page.evaluate(() => window.showView("timeline"));
      await page.waitForSelector("#timeline-stats");
      assert.match(await page.locator("#timeline-stats").innerText(), /尚未收到任何 timeline event/);
      assert.equal(await page.locator("#timeline-strip .timeline-row").count(), 0, "empty timeline must not render idle-filled rows");
      const snapshotCalls = await page.evaluate(() => window.__lpMockCalls.filter((cmd) => cmd === "timeline_snapshot_24h").length);
      assert.equal(snapshotCalls, 0, "timeline_snapshot_24h must not be called before any recorded events exist");
      await page.close();
    }

    {
      const page = await openScenario(browser, "idleOnly");
      await page.waitForFunction(() => document.querySelector("#capsule-status")?.textContent.includes("尚無執行中"));
      assert.equal(await page.locator("#capsule-icons .provider-icon").count(), 0, "idle-only state must not synthesize an active provider icon");
      assert.match(await page.locator("#capsule-project").innerText(), /龍蝦監控|LobsterPulse/);
      assert.match(await page.locator("#capsule-status").innerText(), /尚無執行中/);
      await page.close();
    }

    {
      const page = await openScenario(browser, "stateFailure");
      await page.waitForFunction(() => document.querySelector("#capsule-status")?.textContent.includes("資料來源中斷"));
      assert.equal(await page.locator("#capsule-icons .provider-icon").count(), 0, "state failure must not keep stale provider icons");
      assert.match(await page.locator("#capsule-status").innerText(), /資料來源中斷/);
      assert.match(await page.locator("#session-list").innerText(), /暫停顯示舊 session/);
      assert.match(await page.locator("#capsule-error-dot").getAttribute("class"), /\bhidden\b/, "state failure must clear stale failure dot");
      assert.match(await page.locator("#capsule-quota").getAttribute("class"), /\bhidden\b/, "state failure must clear stale quota capsule");
      await page.close();
    }

    {
      const page = await openScenario(browser, "eventFailure");
      await page.evaluate(() => window.showView("events"));
      await page.waitForFunction(() => document.querySelector("#events-list")?.textContent.includes("事件資料來源中斷"));
      assert.match(await page.locator("#events-list").innerText(), /事件資料來源中斷/);
      assert.equal(await page.locator("#events-filter .events-tab").count(), 0, "event source failure must clear stale event tabs");
      await page.close();
    }

    {
      const page = await openScenario(browser, "timelineFailure");
      await page.evaluate(() => window.showView("timeline"));
      await page.waitForFunction(() => document.querySelector("#timeline-stats")?.textContent.includes("載入失敗"));
      assert.match(await page.locator("#timeline-stats").innerText(), /載入失敗/);
      assert.equal(await page.locator("#timeline-strip .timeline-row").count(), 0, "timeline failure must clear stale rows");
      await page.close();
    }

    {
      const page = await openScenario(browser, "quotaFailure");
      await page.waitForFunction(() => document.querySelector("#quota-bar")?.textContent.includes("quota 資料來源中斷"));
      assert.match(await page.locator("#quota-bar").innerText(), /quota 資料來源中斷/);
      assert.match(await page.locator("#capsule-quota").getAttribute("class"), /\bhidden\b/, "quota source failure must hide stale capsule quota");
      await page.close();
    }

    {
      const page = await openScenario(browser, "staleQuota");
      await page.waitForFunction(() => document.querySelectorAll(".quota-runner.stale").length === 1);
      assert.equal(await page.locator(".quota-runner.stale").count(), 1, "stale quota runner should be visibly downgraded");
      assert.match(await page.locator(".quota-runner-stale").innerText(), /舊/);
      assert.equal(await page.locator(".quota-runner.stale .percent-value").count(), 0, "stale quota must not show fresh percent ring");
      const cls = await page.locator(".quota-runner.stale").first().getAttribute("class");
      assert.ok(!/\bcrit\b|\bwarn\b/.test(cls || ""), `stale quota must not carry fresh severity class, got ${cls}`);
      const capsuleClass = await page.locator("#capsule-quota").getAttribute("class");
      assert.match(capsuleClass || "", /\bhidden\b/, "stale quota must not show a capsule percent alert");
      await page.close();
    }

    {
      const page = await openScenario(browser, "freshQuota");
      await page.waitForFunction(() => document.querySelectorAll(".quota-runner .percent-value").length === 1);
      assert.equal(await page.locator(".quota-runner.stale").count(), 0, "fresh quota must not be downgraded");
      assert.match(await page.locator(".quota-runner .percent-value").innerText(), /7/);
      assert.match(await page.locator(".quota-runner").first().getAttribute("class"), /\bcrit\b/);
      await page.close();
    }

    {
      const page = await openScenario(browser, "openabQuota");
      await page.waitForFunction(() => document.querySelectorAll(".quota-runner .percent-value").length === 1);
      assert.match(await page.locator(".quota-section-title").innerText(), /OpenAB 額度/);
      assert.match(await page.locator(".quota-runner .percent-value").innerText(), /7/);
      await page.waitForFunction(() => !document.querySelector("#capsule-quota")?.classList.contains("hidden"));
      assert.match(await page.locator("#capsule-quota").innerText(), /7%/);
      assert.equal(await page.locator("#capsule-quota").getAttribute("data-provider"), "claude");
      await page.close();
    }

    {
      const page = await openScenario(browser, "unknown");
      const icon = page.locator("#capsule-icons .provider-icon").first();
      await page.waitForFunction(() => document.querySelectorAll("#capsule-icons .provider-icon").length === 1);
      assert.equal(await icon.getAttribute("data-provider"), "mystery_provider");
      assert.equal(await icon.evaluate((el) => getComputedStyle(el).color), "rgb(136, 136, 136)");
      assert.equal(await icon.locator("path").count(), 1, "unknown icon should use question-mark path, not Claude logo");
      assert.equal(await icon.locator("circle").count(), 2, "unknown icon should use generic circles, not Claude logo");
      await page.close();
    }
  } finally {
    await browser.close();
  }
  console.log("[ui-no-fake-data] browser smoke passed");
}

run().catch((err) => {
  console.error(err);
  process.exit(1);
});

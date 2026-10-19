const { test } = require("node:test");
const assert = require("node:assert");
const { parseBoundedInt, RESET_KEYS, resetKeysFor, applyDefaults } = require("../src/settings-lib.js");

test("resetKeysFor：只留額度模式下一般頁連系統通知一起還原", () => {
  assert.deepStrictEqual(resetKeysFor("general", false), ["pin_expanded"]);
  assert.deepStrictEqual(resetKeysFor("general", true), ["pin_expanded", "system_notifications"]);
  assert.deepStrictEqual(resetKeysFor("advanced", true), RESET_KEYS.advanced, "其他頁不受影響");
  assert.deepStrictEqual(resetKeysFor("keys", true), [], "沒有還原清單的頁回空");
  const appearance = { pin_expanded: true, system_notifications: true };
  const changed = applyDefaults(appearance, { pin_expanded: false, system_notifications: false }, "general", resetKeysFor("general", true));
  assert.deepStrictEqual(changed.sort(), ["pin_expanded", "system_notifications"]);
});

test("parseBoundedInt：範圍內的整數才算數，其餘回原因", () => {
  assert.deepStrictEqual(parseBoundedInt("300", 60, Infinity), { ok: true, value: 300, message: "" });
  assert.strictEqual(parseBoundedInt(" 23 ", 0, 23).value, 23, "前後空白可接受");
  const neg = parseBoundedInt("-5", 60, Infinity);
  assert.strictEqual(neg.ok, false, "負數不可存（原本照存）");
  assert.match(neg.message, /≥ 60/);
  const empty = parseBoundedInt("", 1, 100);
  assert.strictEqual(empty.ok, false, "空白不可存成 0（原本 parseInt||0）");
  assert.match(empty.message, /1–100/);
  assert.strictEqual(parseBoundedInt("1.5", 1, 100).ok, false, "小數不收");
  assert.strictEqual(parseBoundedInt("abc", 1, 100).ok, false);
  assert.strictEqual(parseBoundedInt("601", 140, 600).ok, false);
  assert.strictEqual(parseBoundedInt("140", 140, 600).ok, true, "邊界值可用");
  assert.strictEqual(parseBoundedInt("9007199254740993", 60, Infinity).ok, false, "超過安全整數會被捨入，不收");
  assert.strictEqual(parseBoundedInt("9".repeat(400), 60, Infinity).ok, false, "轉成 Infinity 的長字串不收");
});

test("applyDefaults：只動這一頁的鍵，金鑰與指令不在任何一頁", () => {
  const appearance = {
    theme: "light", accent_color: "cyan", accent_custom_hex: "#123456", text_scale: 1.3,
    telegram_bot_token: "secret", openab_restart_command: "schtasks /run",
    idle_threshold_secs: 99, capsule_width: 400, provider_sounds: { claude: "a.mp3" },
  };
  const defaults = {
    theme: "dark", accent_color: "purple", accent_custom_hex: "", text_scale: null,
    telegram_bot_token: "", openab_restart_command: "",
    idle_threshold_secs: 30, capsule_width: 300, provider_sounds: { claude: "b.mp3" },
  };
  const changed = applyDefaults(appearance, defaults, "appearance");
  assert.deepStrictEqual(changed.sort(), ["accent_color", "accent_custom_hex", "text_scale", "theme"]);
  assert.strictEqual(appearance.theme, "dark");
  assert.strictEqual(appearance.text_scale, null);
  assert.strictEqual(appearance.idle_threshold_secs, 99, "別頁的設定不動");
  assert.strictEqual(appearance.telegram_bot_token, "secret", "token 不被還原清掉");
  assert.strictEqual(appearance.openab_restart_command, "schtasks /run");

  applyDefaults(appearance, defaults, "notify");
  assert.deepStrictEqual(appearance.provider_sounds, { claude: "b.mp3" });
  defaults.provider_sounds.claude = "mutated";
  assert.strictEqual(appearance.provider_sounds.claude, "b.mp3", "深拷貝，不和預設物件共用參照");

  for (const keys of Object.values(RESET_KEYS)) {
    for (const k of keys) assert.ok(!/token|chat_id|channel|command|usage_runners/.test(k), `${k} 不該被還原`);
  }
  assert.deepStrictEqual(applyDefaults(appearance, null, "general"), [], "讀不到預設值就什麼都不做");
  assert.deepStrictEqual(applyDefaults(appearance, defaults, "keys"), [], "沒有還原清單的頁不動");
});

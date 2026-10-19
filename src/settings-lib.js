// 設定頁純邏輯層（無 DOM）：瀏覽器掛 window.SettingsLib；node --test 走 module.exports。
(function (root) {
  "use strict";

  // 數字欄位驗證：整數且落在 [min, max]。原本 parseInt 失敗就存 0、負數也照存，
  // 規則門檻變 0 會讓通知每次都觸發。
  function parseBoundedInt(raw, min, max) {
    const text = String(raw ?? "").trim();
    const lo = Number.isFinite(min) ? min : -Infinity;
    const hi = Number.isFinite(max) ? max : Infinity;
    const range = lo > -Infinity && hi < Infinity ? `${lo}–${hi}`
      : lo > -Infinity ? `≥ ${lo}` : hi < Infinity ? `≤ ${hi}` : "整數";
    const n = Number(text);
    // 超過安全整數會被默默捨入（9007199254740993 → …992），一律當不合法
    if (!/^-?\d+$/.test(text) || !Number.isSafeInteger(n)) return { ok: false, value: null, message: `請輸入整數（${range}）` };
    if (n < lo || n > hi) return { ok: false, value: null, message: `超出範圍，允許 ${range}` };
    return { ok: true, value: n, message: "" };
  }

  // 「還原這一頁的預設值」會動到的欄位（appearance 底下的鍵）。
  // 刻意不含：金鑰／token／Chat ID／Channel ID／OpenAB 指令（清掉要重新申請或找回）、
  // 開機自動啟動（作業系統層的狀態，不在設定檔裡）、執行器與規則（使用者自建的資料）。
  const RESET_KEYS = {
    general: ["pin_expanded"],
    appearance: [
      "theme", "accent_color", "accent_custom_hex", "text_size", "text_scale", "font_family",
      "bg_opacity", "background_type", "background_path", "background_blur", "background_image_opacity",
    ],
    notify: [
      "system_notifications", "sound_enabled", "provider_sounds", "provider_waiting_sounds",
      "telegram_notify_threshold_secs",
    ],
    advanced: [
      "idle_threshold_secs", "stale_threshold_secs", "remove_threshold_secs",
      "capsule_width", "expanded_width",
    ],
  };

  // 把 defaults 裡屬於這一頁的鍵複製進 appearance（深拷貝，避免和預設物件共用參照）。
  // 回傳實際有變動的鍵；defaults 缺的鍵不碰。
  function applyDefaults(appearance, defaults, tab) {
    const keys = RESET_KEYS[tab];
    if (!keys || !appearance || !defaults) return [];
    const changed = [];
    for (const k of keys) {
      if (!Object.prototype.hasOwnProperty.call(defaults, k)) continue;
      const next = JSON.parse(JSON.stringify(defaults[k]));
      if (JSON.stringify(appearance[k]) !== JSON.stringify(next)) changed.push(k);
      appearance[k] = next;
    }
    return changed;
  }

  const api = { parseBoundedInt, RESET_KEYS, applyDefaults };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.SettingsLib = api;
})(typeof window !== "undefined" ? window : globalThis);

# Tasks: OpenAB bot 監控同步（openclaw→hermes agent 對齊）

> 來源：operator（人類 operator）2026-06-03 餵入的外部方向 —— openclaw 已全面遷為 hermes agent，LobsterPulse 的 bot 監控清單需對齊。經 Spectra 餵入，由 loop 實作。**非 loop 自加 task**。
>
> 對應 spec capability：`openab-bot-registry`（見 `specs/openab-bot-registry/spec.md`）。
> 每個 Phase 內的 task 同時對應到 spec.md 的 Requirement 行（以「↪ R-XXX」標註）。

## Phase 1: 加 hermes agent（IRISX）監控 — 核心

- [x] **T-BOT1: 加 irisx_bot 到 default_providers()** — R70 ab4b134 落地：config.rs `default_providers()` 加 ProviderConfig，name `"🤖 IRISX · OpenAB Hermes"`、enabled=true (covers: OpenAB bot registration is a 4-point sync)
  - 驗證：R70 commit 過 `cargo test --lib = 364/364 綠` + R67 護欄自動通過
- [x] **T-BOT2: 加 irisx_bot 到 sounds + waiting_sounds + usage poller** — R70 ab4b134 落地：4 同步點齊加 irisx_bot (covers: OpenAB bot registration is a 4-point sync)
  - 驗證：R70 commit 過
- [x] **T-BOT3: irisx 音效缺檔 fallback** — R71 ab4b134 落地 `sounds/irisx_bot.mp3` + `sounds/irisx_bot-waiting.mp3` silent placeholder (1.5s/1.0s) + `lib.rs:120-153` `seed_default_sounds` defaults list 對稱補齊；R74 補 `r74_play_sound_file_safe_when_file_missing` + `r74_seed_default_sounds_is_idempotent_and_seeds_irisx_bot` 2 條 fallback 守護 test, `play_sound_file` 缺檔早 return (lib.rs:202-204) 不 panic 路徑正式 deterministic 化 (covers: Missing sound file MUST NOT panic playback)
  - 驗證：`cargo test --lib` 367/367 綠 (R73 365 + R74 +2) + `seed_default_sounds` idempotent (2 次呼叫 file count 相同) + irisx_bot.mp3 / irisx_bot-waiting.mp3 確實 seeded 到目標 dir

## Phase 2: 修正既有 drift

- [x] **T-BOT4: 釐清並修 cicx2 ID 漂移** — R78 commit 97aea24 落地 `parse_provider()` 加 alias `cicx2 → cicx`（比照 `bot → openx` 模式），CICX2 POST `/hook/cicx2` 自動 rewrite 到 cicx bucket。R78 spec 漏勾 [x]（同 R75 T-BOT9 模式），R80 修。`r78_t_bot4_cicx2_alias_rewrites_to_cicx` test 守住。 (covers: Provider id alias resolves legacy / drift ids)
  - 驗證：cargo test --lib = 368 passed；CICX2 事件不漏接；本來就對齊（openab 已 normalize）則 alias 為 no-op（訊息記在 commit body）
- [x] **T-BOT5: 補 mimo provider（disabled）** — R78 commit 1a2c900 落地 4 同步點齊：default_providers `mimo` (name `"🤖 MIMO · OpenAB MIMO"`, enabled=false) + default_provider_sounds `mimo→mimo.mp3` + default_provider_waiting_sounds `mimo→mimo-waiting.mp3` + hook_server.rs `KNOWN_PROVIDERS` 12→13 + lib.rs seed_default_sounds `include_bytes!` 內嵌 2 條 mimo mp3 (1.5s/1.0s silent placeholder 沿 R71 irisx 模式)。R78 spec 漏勾 [x]，R80 修。 (covers: OpenAB bot registration is a 4-point sync)
  - 驗證：cargo test --lib = 369 passed；`default_providers()` 含 mimo；手動 enable 後可監控 mimo；`KNOWN_PROVIDERS` size 13 守住

## Phase 3: 防再漂 + docs

- [x] **T-BOT6: 新增 OpenAB bot 同步 SOP** — R78 commit 68fd164 落地 CONTRIBUTING.md (54 行) 完整 checklist：音效檔建立 / config.rs 4 同步點 (default_providers / sounds / waiting_sounds / detect) / hook_server.rs KNOWN_PROVIDERS / lib.rs seed_default_sounds include_bytes / 測試數量更新 / 命名慣例 (🤖 OpenAB / 💻 本機 CLI)。R78 spec 漏勾 [x]，R80 修。 (covers: OpenAB bot registration is a 4-point sync)
  - 驗證：CONTRIBUTING.md 含 5 步驟 checklist + 命名慣例章節
- [x] **T-BOT7: drift 守護測試** — R67 ff4b0cb 落地：跨 3 同步點 (`default_providers` / `default_provider_sounds` / `default_provider_waiting_sounds`) 一致性護欄 chain 第 16 條 + 撞 id 守護 + 命名 convention 守護 (covers: Drift guard prevents silent provider re-drift)
  - 驗證：R67 commit 過 + R70 T-BOT1/T-BOT2 落地後護欄自動通過驗證 `(a)(b)(c)` 對稱 + 撞 id 守衛
- [x] **T-BOT8: 更新 docs bot inventory** — R78 commit 0e29573 落地 README.md (OpenAB 6→10 bot，總 10→14) + CLAUDE.md (Bot 總覽 5→9 卡、事件診斷 11→14 tabs、OpenAB bot 清單補 grokx/lpbot/mimo)。R78 spec 漏勾 [x]，R80 修。 (covers: OpenAB bot registration is a 4-point sync)
  - 驗證：README/CLAUDE.md 列齊 9 OpenAB bot + hermes 後端遷移標註

## Phase 4: 後端標籤對齊（bot 換後端但 LobsterPulse 標籤過時）

- [x] **T-BOT9: GIMINIX 後端標籤 gemini→agy（Antigravity）** — R75 commit b9f36ab 落地：config.rs:379 name `"🤖 GIMINIX · OpenAB Gemini"` → `"🤖 GIMINIX · OpenAB Antigravity"` 對齊 openab `config-gemini.toml` 第 1 行「後端: agy-acp-wrapper (Antigravity)」source of truth；新 mod `r75_giminix_backend_label_tests` + 1 條 deterministic 護欄 test `r75_giminix_name_reflects_antigravity_backend_not_gemini`（3 sub-assertion: 含 Antigravity / 不含 Gemini / 仍 enabled）守住不再回退。R75 spec commit 15a6c54 加 `(covers: ...)` reference 時漏勾 `[x]`，R77 修此 spec/實作 drift 解 Spectra ship blocker。bot_id 維持 `giminix` 不變 (covers: Backend label reflects actual backend engine)
  - ⚠️ **勿動** config.rs line ~420 的本機 `gemini` CLI provider（`"💻 Gemini CLI（本機）"`，`~/.gemini/settings.json`）—— 那是獨立的本機 Gemini CLI，仍是 gemini
  - 驗證：膠囊/UI 的 GIMINIX 標籤顯示 Antigravity/agy 後端；本機 gemini CLI provider 不受影響、仍存在
- [x] **T-BOT10: 全 bot 後端標籤稽核** — R78 commit b26c551 落地 `r78_t_bot10_all_openab_backend_labels_match_config` 護欄 (config.rs +44 行)：斷言 9 隻 OpenAB provider 的 display name 後端字樣與 openab config-*.toml 一致，對照表 cicx→Claude / gitx→Copilot / giminix→Antigravity / codex_bot→Codex / openx→OpenCode / irisx_bot→Hermes / grokx→Grok / lpbot→Claude / mimo→MIMO，額外守 giminix 不含 Gemini（R75 護欄泛化）。R78 spec 漏勾 [x]，R80 修。 (covers: Backend label reflects actual backend engine)
  - 涵蓋 design.md 「後端對照表（name 應反映的後端，來源 = openab config-*.toml 第 1 行）」9 行對齊表（cicx/gitx/giminix/codex_bot/openx/irisx_bot/grokx/lpbot/mimo → 對應 openab config-*.toml 第 1 行「後端: X」字樣）
  - 驗證：cargo test --lib = 368 passed；護欄 test pass；9 隻 OpenAB bot name 後端字樣與 openab config 一致；無其他過時標籤殘留

## Phase 5: 拆撞 id + 納管（openab 端已於 2026-06-04 由 operator 修好，LP 端補 provider）

- [x] **T-BOT11: 加 grokx provider（GROKX 已拆獨立 id）** — R78 commit 落地：4 同步點齊 (a) `config.rs` default_providers 加 grokx（name `"🤖 GROKX · OpenAB Grok"`, enabled=true，line ~421）/ (b) `default_provider_sounds` 加 `grokx→grokx.mp3` (line ~340) / (c) `default_provider_waiting_sounds` 加 `grokx→grokx-waiting.mp3` (line ~356) / (d) `lib.rs` seed_default_sounds 內嵌 2 條 grokx mp3 (`include_bytes!` from `sounds/`)；`hook_server.rs` `KNOWN_PROVIDERS` 10→11（line ~328, parse_provider 護欄 size 11 守住）；`r74_play_sound_file_fallback_tests` mp3 seeded 12→14 守住。`sounds/grokx.mp3` (9596 B silent) + `sounds/grokx-waiting.mp3` (6572 B silent) 沿 R71 irisx 模式 1.5s/1.0s。R78 (f) 護欄（line ~942）擴充 R67 chain #16：enabled 🤖 OpenAB bot name「· OpenAB X」後段 X 集合兩兩不同，防「撞後端視覺標籤」drift (covers: OpenAB bot registration is a 4-point sync, Drift guard prevents silent provider re-drift)
  - 撞 id 守護: Rust `HashMap<String, _>` key 唯一由 type system 編譯期保證（`default_providers.insert("grokx", ...)` 重複 insert 自動 dedup，無法表達「兩個 openab bot 寫到同一個 LP provider id」），故 line 44 字面「runtime 偵測撞 id」在 Rust 表達層不可能 — R78 (f) 守護撞**後端視覺標籤**（name 後段）為撞 id 視覺後果的 runtime 補強；編譯期撞 id 仍由 type system 擋。R79 觀察輪可考慮把 line 44 文字收斂成「撞 id 由 type system 擋 + (f) 擋撞標籤」
  - 驗證：膠囊出現 GROKX；POST `/hook/grokx` 被接住；撞 id 由 type system 編譯期保證；撞後端標籤由 (f) runtime 護欄擋
- [x] **T-BOT12: 加 lpbot provider（LPBOT 已納管）** — R78 commit b0ad9f1 落地 4 同步點齊：default_providers `lpbot` (name `"🤖 LPBOT · OpenAB Claude（quota 監控）"`, enabled=true) + default_provider_sounds `lpbot→lpbot.mp3` + default_provider_waiting_sounds `lpbot→lpbot-waiting.mp3` + hook_server.rs `KNOWN_PROVIDERS` 11→12 + lib.rs seed_default_sounds `include_bytes!` 內嵌 2 條 lpbot mp3 (1.5s/1.0s silent placeholder 沿 R71 irisx 模式)。operator 已於 2026-06-04 在 openab `config-lpbot.toml` 加 `[lobsterpulse] bot_id="lpbot" enabled=true`。R78 spec 漏勾 [x]，R80 修。 (covers: OpenAB bot registration is a 4-point sync)
  - 驗證：cargo test --lib = 368 passed；膠囊出現 LPBOT；`usage-lpbot.json` 被讀
  - ⚠️ T-BOT12 描述原文「docs 記錄刻意保留/未監控清單」未獨立 commit 落地（與 T-BOT8 合併在 0e29573 中）— R80 不拆 spec，若 owner 要單獨追可 R81+ 拆

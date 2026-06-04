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

- [ ] **T-BOT4: 釐清並修 cicx2 ID 漂移** — config-cicx2.toml 宣告 bot_id=`cicx2`，LobsterPulse default 用 `cicx`。先確認 CICX2 實際 POST `/hook/{cicx|cicx2}`，再對齊（加 provider 或 `parse_provider()` alias，比照 `"bot"→"openx"`） (covers: Provider id alias resolves legacy / drift ids)
  - 驗證：CICX2 事件不被漏接 / 誤路由；若本來就對齊則 docs 註記、no-op（勿盲改）
- [ ] **T-BOT5: 補 mimo provider（disabled）** — config.rs 加 mimo（enabled=false，對應 config-mimo.toml）+ sounds (covers: OpenAB bot registration is a 4-point sync)
  - 驗證：`default_providers()` 含 mimo；手動 enable 後可監控 mimo

## Phase 3: 防再漂 + docs

- [ ] **T-BOT6: 新增 OpenAB bot 同步 SOP** — CONTRIBUTING.md / docs 加 checklist，列 config.rs 的 4 個同步點 + 原則「bot_id 以 openab config-*.toml `[lobsterpulse]` 為準」 (covers: OpenAB bot registration is a 4-point sync)
  - 驗證：checklist 存在、4 個同步點齊全
- [x] **T-BOT7: drift 守護測試** — R67 ff4b0cb 落地：跨 3 同步點 (`default_providers` / `default_provider_sounds` / `default_provider_waiting_sounds`) 一致性護欄 chain 第 16 條 + 撞 id 守護 + 命名 convention 守護 (covers: Drift guard prevents silent provider re-drift)
  - 驗證：R67 commit 過 + R70 T-BOT1/T-BOT2 落地後護欄自動通過驗證 `(a)(b)(c)` 對稱 + 撞 id 守衛
- [ ] **T-BOT8: 更新 docs bot inventory** — CLAUDE.md / README 的 provider 清單補 IRISX(hermes)，9→10 provider，標注 openclaw→hermes 遷移 (covers: OpenAB bot registration is a 4-point sync)
  - 驗證：docs 列出 IRISX + hermes 後端

## Phase 4: 後端標籤對齊（bot 換後端但 LobsterPulse 標籤過時）

- [ ] **T-BOT9: GIMINIX 後端標籤 gemini→agy（Antigravity）** — GIMINIX bot 後端已從 gemini 換成 agy-acp-wrapper（Antigravity，見 openab/config-gemini.toml 第 1 行「後端: agy-acp-wrapper (Antigravity)」），但 config.rs line 374 仍標 `"🤖 GIMINIX · OpenAB Gemini"`。改成反映 agy/Antigravity 後端（如 `"🤖 GIMINIX · OpenAB Antigravity"`）。bot_id 維持 `giminix` 不變 (covers: Backend label reflects actual backend engine)
  - ⚠️ **勿動** config.rs line ~420 的本機 `gemini` CLI provider（`"💻 Gemini CLI（本機）"`，`~/.gemini/settings.json`）—— 那是獨立的本機 Gemini CLI，仍是 gemini
  - 驗證：膠囊/UI 的 GIMINIX 標籤顯示 Antigravity/agy 後端；本機 gemini CLI provider 不受影響、仍存在
- [ ] **T-BOT10: 全 bot 後端標籤稽核** — 對每個 OpenAB provider 的 display name，比對 openab `config-*.toml` 第 1 行的「後端: X」，確保 LobsterPulse 標籤反映實際後端。對照表（同步 design.md「後端對照表（name 應反映的後端，來源 = openab config-*.toml 第 1 行）」）：cicx=Claude(claude-agent-acp) / gitx=Copilot（GITX；GROKX 已於 2026-06-04 拆為獨立 `grokx`，見 T-BOT11）/ grokx=Grok(hermes -p grokx) / giminix=**Antigravity(agy-acp-wrapper)** / codex_bot=Codex(codex-acp) / openx=OpenCode(opencode) / irisx_bot=Hermes(hermes→gpt-5.5) / lpbot=Claude(claude-agent-acp, quota 監控) (covers: Backend label reflects actual backend engine)
  - 驗證：每個 provider name 的後端字樣與 openab config 一致；無其他過時標籤殘留

## Phase 5: 拆撞 id + 納管（openab 端已於 2026-06-04 由 operator 修好，LP 端補 provider）

- [ ] **T-BOT11: 加 grokx provider（GROKX 已拆獨立 id）** — GROKX（後端 hermes -p grokx）原與 GITX 撞 `bot_id="gitx"`；operator 已於 2026-06-04 在 openab `config-copilot-native.toml` 拆為 `bot_id="grokx"`。加 grokx 到 default_providers（name `"🤖 GROKX · OpenAB Grok"`、enabled=true）+ sounds + waiting_sounds + usage poller（line 547） (covers: OpenAB bot registration is a 4-point sync, Drift guard prevents silent provider re-drift)
  - 並把 T-BOT7 drift 守護測試擴充：偵測「多個 openab enabled bot 映射到同一 LP provider id」→ fail，防未來再撞 id
  - 驗證：膠囊出現 GROKX；POST `/hook/grokx` 被接住；撞 id 守護測試能抓到人為製造的撞 id
- [ ] **T-BOT12: 加 lpbot provider（LPBOT 已納管）** — operator 已於 2026-06-04 在 openab `config-lpbot.toml` 加 `[lobsterpulse] bot_id="lpbot" enabled=true`。加 lpbot 到 default_providers（name `"🤖 LPBOT · OpenAB Claude（quota 監控）"`、enabled=true）+ sounds + usage poller (covers: OpenAB bot registration is a 4-point sync)
  - 並在 docs 記錄「刻意保留 / 未監控」清單：本機 CLI provider（claude/codex/copilot/gemini 本機，enabled=false）為刻意保留、非孤兒
  - 驗證：膠囊出現 LPBOT；`usage-lpbot.json` 被讀；docs 含「刻意保留」清單

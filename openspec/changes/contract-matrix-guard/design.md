# Design: Provider Contract Matrix Guard (13×3)

## 13 row × 3 attribute 期望值表（single source of truth）

> Code-level 落地: `src-tauri/src/config.rs` module `provider_contract_matrix_tests`
> 內 `const CONTRACT: &[(&str, &str, bool, &str, &str)]`
> 護衛: 1 條 `r106_provider_contract_13_by_3_matrix` test
> 跨 4 同步點對齊: `default_providers` (config.rs:370) / `default_provider_sounds`
> (config.rs:329) / `default_provider_waiting_sounds` (config.rs:351) /
> `OPENAB_BOT_IDS` (lib.rs:39)

### 3 attribute 設計理由

1. **name prefix + emoji (🤖 OpenAB / 💻 本機 CLI)**
   - 對齊 CLAUDE.md naming convention
   - 防新 provider 漏加前綴 (e.g. `cicx_bot` 寫成 `bot: cicx` 不帶 🤖)
2. **enabled_default** (9 OpenAB bot 預設 enabled, mimo 例外 disabled; 4 本機 CLI 預設 disabled)
   - 對齊 K0 即時性: enabled 才能進 K0 量化 (K0-A1/A2 端點 emit 維度)
   - 防單隻 enabled 被誤關成 disabled (退回 disabled = 監控盲區)
3. **sound file mapping** (filename 對齊 default_provider_sounds / default_provider_waiting_sounds)
   - 對齊 R67 護欄 (a)(c) 但細到 filename
   - 防「key 對 value 錯」: e.g. cicx 預期 `"cicx.mp3"` 但 code 寫成 `"cicx.MP3"`

### 13 row 完整表

| id | name prefix | enabled_default | sound_file | waiting_sound_file | OPENAB_BOT_IDS member |
|---|---|---|---|---|---|
| `cicx` | 🤖 | true | `cicx.mp3` | `cicx-waiting.mp3` | ✅ |
| `gitx` | 🤖 | true | `gitx.mp3` | `gitx-waiting.mp3` | ✅ |
| `giminix` | 🤖 | true | `giminix.mp3` | `giminix-waiting.mp3` | ✅ |
| `codex_bot` | 🤖 | true | `codex.mp3` | `codex-waiting.mp3` | ✅ |
| `openx` | 🤖 | true | `openx.mp3` | `openx-waiting.mp3` | ✅ |
| `irisx_bot` | 🤖 | true | `irisx_bot.mp3` | `irisx_bot-waiting.mp3` | ✅ |
| `grokx` | 🤖 | true | `grokx.mp3` | `grokx-waiting.mp3` | ✅ |
| `lpbot` | 🤖 | true | `lpbot.mp3` | `lpbot-waiting.mp3` | ✅ |
| `mimo` | 🤖 | false | `mimo.mp3` | `mimo-waiting.mp3` | ✅ |
| `claude` | 💻 | false | (無 default) | (無 default) | ❌ |
| `codex` | 💻 | false | `codex.mp3` | `codex-waiting.mp3` | ❌ |
| `copilot` | 💻 | false | (無 default) | (無 default) | ❌ |
| `gemini` | 💻 | false | (無 default) | (無 default) | ❌ |

總計: 4 本機 CLI + 9 OpenAB bot = 13 provider (對齊 R78 spec 13)

### 特殊情況說明

- **mimo disabled**: R78 T-BOT5 決策 (M3 環境 disabled 處理)
- **codex / codex_bot 共用 sound**: R70 spec 決策 (codex 跟 codex_bot 共用
  `codex.mp3` / `codex-waiting.mp3`)
- **claude / copilot / gemini 無 default sound**: 留 user 自訂 (R67 spec)

## 護衛 test 設計（`config.rs` `#[cfg(test)] mod provider_contract_matrix_tests`）

```rust
#[test]
fn r106_provider_contract_13_by_3_matrix() {
    let providers = default_providers();
    let sounds = default_provider_sounds();
    let waiting_sounds = default_provider_waiting_sounds();
    let openab_set: HashSet<&str> = OPENAB_BOT_IDS.iter().copied().collect();

    // 矩陣完整性: CONTRACT 必須 13 row
    assert_eq!(CONTRACT.len(), 13, "...");

    for &(id, prefix, enabled, sound_file, waiting_sound_file) in CONTRACT {
        let p = providers.get(id).unwrap_or_else(|| panic!("..."));

        // Attribute 1: name prefix
        assert!(p.name.starts_with(&format!("{prefix} ")), "...");

        // Attribute 2: enabled default
        assert_eq!(p.enabled, enabled, "...");

        // Attribute 3a: sound file
        if sound_file.is_empty() {
            assert!(!sounds.contains_key(id), "...");
        } else {
            assert_eq!(sounds.get(id).map(|s| s.as_str()), Some(sound_file), "...");
        }

        // Attribute 3b: waiting sound file
        if waiting_sound_file.is_empty() {
            assert!(!waiting_sounds.contains_key(id), "...");
        } else {
            assert_eq!(waiting_sounds.get(id).map(|s| s.as_str()),
                       Some(waiting_sound_file), "...");
        }

        // Cross-attribute: OpenAB bot 必須在 OPENAB_BOT_IDS
        let expect_in_openab = prefix == "🤖";
        assert_eq!(openab_set.contains(id), expect_in_openab, "...");
    }
}
```

## 跟 R67 護欄差異對照

| 護衛維度 | R67 (config.rs:909) | R106 (config.rs:1010) |
|---|---|---|
| 範疇 | set 對稱 | value 對齊 |
| 驗 name prefix | ✅ (e) | ✅ (Attribute 1, 強化到 value 對齊) |
| 驗 enabled | ⚠️ 總量 ≥ 5 | ✅ 每 row 對齊 enabled_default |
| 驗 sounds keys | ✅ (a) | ✅ (Attribute 3a, 強化到 value 對齊) |
| 驗 sounds value | ❌ | ✅ (filename 對齊) |
| 驗 waiting_sounds keys | ✅ (b) | ✅ (Attribute 3b, 強化到 value 對齊) |
| 驗 waiting_sounds value | ❌ | ✅ (filename 對齊) |
| 驗 sounds ≡ waiting_sounds | ✅ (c) | ⚠️ 退到 (d) — 3a + 3b 各自 value 對齊, 隱含 set 對稱 |
| 驗 OPENAB_BOT_IDS membership | ❌ | ✅ (Cross-attribute) |
| 護衛總條數 | 5 條斷言 | 1 條 test × 6 斷言/row × 13 row |

## K42 chain 飽和評估

> R50 freeze: 護衛 chain 17 條凍結不擴張。
>
> R106 屬 **chain 16 護衛 (provider_registration_guard)** 對稱面延伸, 跟 R67 同 chain,
> 不算新 chain 18。`護衛 chain 17 條不擴張` 紅線守住。

## 改 4 同步點時 R106 護衛行為

| 改動 | R106 fail 訊息 | 修正路徑 |
|---|---|---|
| 加新 provider 但漏加 CONTRACT row | `[matrix size] CONTRACT 應有 13 row, 觀察 = N` | 同步加 CONTRACT row |
| 改 cicx.enabled = false | `[enabled_default] "cicx" 預期 enabled=true, 觀察=false` | 同步改 CONTRACT enabled |
| 改 sounds.get("cicx") = "cicx-v2.mp3" | `[sound file] "cicx" 預期 "cicx.mp3", 觀察 Some("cicx-v2.mp3")` | 同步改 CONTRACT sound_file |
| 漏加 OPENAB_BOT_IDS "cicx" | `[OPENAB_BOT_IDS membership] "cicx" prefix="🤖" 預期 in_openab=true, 觀察 OPENAB_BOT_IDS = {...}` | 同步加 OPENAB_BOT_IDS |
| 漏加 default_providers "cicx" | `[provider missing] "cicx" 不在 default_providers() 內, 觀察 keys = [...]` | 同步加 default_providers |

## 不在本 change scope（列為 follow-up）

- ❌ 改既有 R67 護欄 (config.rs:909) — R67 護 keys 對稱, R106 護 value 對齊, 兩條並存
- ❌ 動 4 同步點本體 — R106 只驗對齊, 不修對齊源
- ❌ 加新 provider — 走 MISSION.md 納入標準 review
- ❌ 接 OTel SDK / 6 條 counter 命名 — 留 R103+ follow-up

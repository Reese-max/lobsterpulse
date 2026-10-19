//! 「一鍵複製 API key」——只讀環境變數，app 自己不持有任何金鑰。
//!
//! 為什麼不做「儲存」：金鑰已經存在環境變數裡，再存一份到 app 就是多一個外洩
//! 點，而且會被 runtime/ 同步、備份、設定掃描一起帶走。改成每次即時讀、用完
//! 就算了。
//!
//! 值不進前端（除非使用者主動按住顯示）：複製是後端直接寫剪貼簿，webview 從頭
//! 到尾只拿得到名稱與遮罩，截圖／螢幕分享／devtools 都看不到。
//!
//! 「即時讀」指的是登錄檔，不是本程式的環境變數：程序的環境變數是啟動那一刻的
//! 快照，使用者之後改了金鑰它也不會變（2026-10-19 使用者回報「複製到的不會自動
//! 更新」）。見 `live_entries`。

use crate::quota::codex::jwt_exp;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 只收 AI 推論服務的金鑰。用前綴白名單而非「排除非 AI」：後者會 fail-open，
/// 以後多一個無關的 *_TOKEN 就自動被收進來。要加新家就往這裡加一個前綴。
const AI_PREFIXES: &[&str] = &[
    "ANTHROPIC",
    "CLAUDE",
    "COHERE",
    "DEEPSEEK",
    "FIREWORKS",
    "GEMINI",
    "GLM",
    "GOOGLE_AI",
    "GROQ",
    "MIMO",
    "MINIMAX",
    "MISTRAL",
    "MOONSHOT",
    "OPENAI",
    "OPENROUTER",
    "PERPLEXITY",
    "QWEN",
    "TOGETHER",
    "XAI",
    "ZAI",
];

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ApiKeyEntry {
    /// 環境變數名稱（前端只拿得到這個與 masked）
    pub name: String,
    /// 遮罩後的樣子，供辨識用（例如 OpenRouter 五把要分辨是哪一把）
    pub masked: String,
    /// 使用者自訂備註（存 config.api_key_aliases）。名稱如 OPENROUTER_API_KEY_A
    /// 只有字母沒有意義，帳號歸屬只有使用者知道 → 讓他自己標。
    #[serde(default)]
    pub alias: Option<String>,
    /// 值相同的金鑰共用同一組號（None = 沒有重複）。只給組號不給值——
    /// 實測本機有兩組隱形重複（GLM=ZAI、MINIMAX 兩把），畫面上看不出來。
    #[serde(default)]
    pub dup_group: Option<usize>,
    /// LP 哪張額度卡實際會用這把（None = 只是放著供複製）
    #[serde(default)]
    pub used_by: Option<&'static str>,
    /// 值實際取自哪裡：user／machine（環境變數）、process（退回程序環境）、codex
    pub source: &'static str,
    /// JWT 形狀的 token 才有：到期時間（epoch 秒）。一般 API key 沒有到期日。
    #[serde(default)]
    pub expires_at: Option<u64>,
}

pub const SRC_USER: &str = "user";
pub const SRC_MACHINE: &str = "machine";
pub const SRC_PROCESS: &str = "process";
pub const SRC_CODEX: &str = "codex";

/// 一把金鑰的名稱、值與來源。只在後端流動，不序列化給前端。
#[derive(Clone, Debug)]
pub struct KeyValue {
    pub name: String,
    pub value: String,
    pub source: &'static str,
}

impl From<(String, String)> for KeyValue {
    fn from((name, value): (String, String)) -> Self {
        KeyValue { name, value, source: SRC_PROCESS }
    }
}

/// 這把金鑰是否被 LP 自己的 quota fetcher 讀取。對照來源是 fetcher 實作：
/// openrouter.rs 收 `OPENROUTER_API_KEY*`、minimax.rs 讀 `MINIMAX_API_KEY`。
/// 其餘 CLI 走 OAuth／本機憑證檔，不吃環境變數。
pub fn used_by(name: &str) -> Option<&'static str> {
    if name.starts_with("OPENROUTER_API_KEY") {
        return Some("OpenRouter 額度卡");
    }
    if name == "MINIMAX_API_KEY" {
        return Some("MiniMax 額度卡");
    }
    None
}

/// 名稱是否為「AI 服務的金鑰」。大小寫敏感（環境變數慣例全大寫）。
pub fn is_ai_key_name(name: &str) -> bool {
    let key_shaped = name.ends_with("_KEY")
        || name.contains("API_KEY")
        || name.contains("APIKEY")
        || name.ends_with("_TOKEN");
    key_shaped && AI_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// 遮罩：只露末 4 碼。太短的一律全遮——短到只剩幾碼時露 4 碼等於露一半。
pub fn mask(value: &str) -> String {
    let v = value.trim();
    let n = v.chars().count();
    if n <= 8 {
        return "•".repeat(n.max(1));
    }
    let tail: String = v.chars().skip(n - 4).collect();
    format!("••••{tail}")
}

/// 從一組 (名稱, 值) 篩出可複製的金鑰，依名稱排序。空值視為未設定。
/// 同時標出「值相同」的重複組——分組只在函式內看值，回傳結構仍不含原值。
pub fn collect<T: Into<KeyValue>, I: IntoIterator<Item = T>>(vars: I) -> Vec<ApiKeyEntry> {
    let mut kept: Vec<KeyValue> = vars
        .into_iter()
        .map(Into::into)
        .filter(|k: &KeyValue| is_ai_key_name(&k.name) && !k.value.trim().is_empty())
        .collect();
    kept.sort_by(|a, b| a.name.cmp(&b.name));

    // 值 → 出現次數，只有 >1 的才配組號（組號按首次出現順序，從 1 起）
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for k in &kept {
        *seen.entry(k.value.trim()).or_insert(0) += 1;
    }
    let mut group_of: HashMap<&str, usize> = HashMap::new();
    let mut next_group = 1usize;
    for k in &kept {
        let val = k.value.trim();
        if seen.get(val).copied().unwrap_or(0) > 1 && !group_of.contains_key(val) {
            group_of.insert(val, next_group);
            next_group += 1;
        }
    }

    kept.iter()
        .map(|k| ApiKeyEntry {
            masked: mask(&k.value),
            dup_group: group_of.get(k.value.trim()).copied(),
            used_by: used_by(&k.name),
            alias: None,
            source: k.source,
            expires_at: jwt_exp(k.value.trim()),
            name: k.name.clone(),
        })
        .collect()
}

/// 使用者層蓋系統層（Windows 組環境變數的規則）；名稱不分大小寫。
#[cfg_attr(not(windows), allow(dead_code))]
fn merge_env(machine: Vec<(String, String)>, user: Vec<(String, String)>) -> Vec<KeyValue> {
    let mut out: Vec<KeyValue> = machine
        .into_iter()
        .map(|(name, value)| KeyValue { name, value, source: SRC_MACHINE })
        .collect();
    for (name, value) in user {
        out.retain(|k| !k.name.eq_ignore_ascii_case(&name));
        out.push(KeyValue { name, value, source: SRC_USER });
    }
    out
}

/// REG_EXPAND_SZ 的 %NAME% 展開。不用 ExpandEnvironmentStringsW：它查的是本程式
/// 啟動時的環境快照，被引用的變數改過也看不到。找不到的 %NAME% 照 Windows 慣例原樣保留；
/// 只展開一層。
#[cfg_attr(not(windows), allow(dead_code))]
fn expand_with(s: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match lookup(name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// 直讀 HKCU／HKLM 的 Environment 機碼。先只列名稱，通過白名單的才讀值——
/// 其他環境變數（密碼、憑證路徑）的值不會被讀進來。
#[cfg(windows)]
mod registry {
    use super::{expand_with, merge_env, KeyValue};
    use windows_sys::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_READ, REG_EXPAND_SZ, REG_SZ,
    };

    const MACHINE_ENV: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn until_nul(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }

    /// 開著的機碼；離開作用域一定關（每條路徑都不漏 RegCloseKey）。
    struct EnvKey(HKEY);

    impl Drop for EnvKey {
        fn drop(&mut self) {
            // SAFETY: self.0 是 open() 成功開啟的機碼，只關一次
            unsafe { RegCloseKey(self.0) };
        }
    }

    impl EnvKey {
        fn open(root: HKEY, path: &str) -> Option<Self> {
            let path_w = wide(path);
            let mut hkey: HKEY = std::ptr::null_mut();
            // SAFETY: path_w 以 NUL 結尾；hkey 由系統寫入
            let rc = unsafe { RegOpenKeyExW(root, path_w.as_ptr(), 0, KEY_READ, &mut hkey) };
            (rc == ERROR_SUCCESS).then(|| EnvKey(hkey))
        }

        /// 只列值的名稱（不傳資料緩衝，系統不會回傳任何值）。
        fn names(&self) -> Vec<String> {
            let mut out = Vec::new();
            let mut buf = vec![0u16; 16384]; // 值名稱上限 16383 字元
            for index in 0u32.. {
                let mut len = buf.len() as u32;
                // SAFETY: 名稱緩衝長度如實傳入；資料與型別指標皆為 null
                let rc = unsafe {
                    RegEnumValueW(
                        self.0,
                        index,
                        buf.as_mut_ptr(),
                        &mut len,
                        std::ptr::null(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                };
                if rc == ERROR_NO_MORE_ITEMS {
                    break;
                }
                if rc == ERROR_SUCCESS {
                    out.push(String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]));
                }
            }
            out
        }

        /// 單一值：(型別, 字串)。不是字串型別或不存在回 None。
        fn get(&self, name: &str) -> Option<(u32, String)> {
            let name_w = wide(name);
            let mut kind = 0u32;
            let mut size = 0u32;
            // SAFETY: 第一次只問型別與所需位元組數
            let rc = unsafe {
                RegQueryValueExW(
                    self.0,
                    name_w.as_ptr(),
                    std::ptr::null(),
                    &mut kind,
                    std::ptr::null_mut(),
                    &mut size,
                )
            };
            if rc != ERROR_SUCCESS || (kind != REG_SZ && kind != REG_EXPAND_SZ) {
                return None;
            }
            // 多配一個 u16：登錄檔不保證字串以 NUL 結尾
            let mut buf = vec![0u16; size as usize / 2 + 1];
            let mut bytes = (buf.len() * 2) as u32;
            // SAFETY: 緩衝長度（位元組）如實傳入
            let rc = unsafe {
                RegQueryValueExW(
                    self.0,
                    name_w.as_ptr(),
                    std::ptr::null(),
                    &mut kind,
                    buf.as_mut_ptr().cast(),
                    &mut bytes,
                )
            };
            if rc != ERROR_SUCCESS || (kind != REG_SZ && kind != REG_EXPAND_SZ) {
                return None;
            }
            let units = (bytes as usize / 2).min(buf.len());
            Some((kind, until_nul(&buf[..units])))
        }
    }

    /// 兩層都打不開才回 None（呼叫端退回程序環境）。
    pub fn read(keep: &dyn Fn(&str) -> bool) -> Option<Vec<KeyValue>> {
        let machine = EnvKey::open(HKEY_LOCAL_MACHINE, MACHINE_ENV);
        let user = EnvKey::open(HKEY_CURRENT_USER, "Environment");
        if machine.is_none() && user.is_none() {
            return None;
        }
        // %NAME% 依當下登錄檔解析（使用者層優先），登錄檔沒有的（USERPROFILE 這類
        // 登入時才產生的）才看程序環境
        let lookup = |n: &str| -> Option<String> {
            [&user, &machine]
                .into_iter()
                .flatten()
                .find_map(|k| k.get(n).map(|(_, v)| v))
                .or_else(|| std::env::var(n).ok())
        };
        let pick = |key: &Option<EnvKey>| -> Vec<(String, String)> {
            let Some(k) = key else { return Vec::new() };
            k.names()
                .into_iter()
                .filter(|n| keep(n.as_str()))
                .filter_map(|n| {
                    let (kind, v) = k.get(&n)?;
                    let v = if kind == REG_EXPAND_SZ { expand_with(&v, &lookup) } else { v };
                    Some((n, v))
                })
                .collect()
        };
        Some(merge_env(pick(&machine), pick(&user)))
    }
}

/// 現在這一刻的金鑰。Windows 上直讀登錄檔：改了環境變數（系統內容、setx）
/// 不必重開本程式就生效，刪掉的也會跟著消失。登錄檔讀不到才退回程序環境；
/// 測試一律走程序環境（不碰本機真實金鑰）。
pub fn live_entries() -> Vec<KeyValue> {
    #[cfg(windows)]
    {
        if !cfg!(test) {
            if let Some(v) = registry::read(&is_ai_key_name) {
                return v;
            }
        }
    }
    std::env::vars()
        .filter(|(k, _)| is_ai_key_name(k))
        .map(KeyValue::from)
        .collect()
}

/// 額度卡用：同 `live_entries`，只取 (名稱, 值)。
pub fn live_vars() -> Vec<(String, String)> {
    live_entries().into_iter().map(|k| (k.name, k.value)).collect()
}

/// 額度卡用：單一名稱的現值。
pub fn live_var(name: &str) -> Option<String> {
    live_entries().into_iter().find(|k| k.name == name).map(|k| k.value)
}

/// 環境變數裡的 ChatGPT access token 是某次手動貼進去的快照（2026-10-19 實測：
/// 07-03 就過期了），Codex CLI 則會自己刷新 ~/.codex/auth.json。兩者是同一種
/// token（同 iss／client_id／帳號），到期日較晚的那份才是能用的。
const CODEX_TOKEN_NAME: &str = "OPENAI_ACCESS_TOKEN";

/// JWT 的身分三件套：(iss, client_id, ChatGPT 帳號)。缺任一項回 None。
fn token_identity(token: &str) -> Option<(String, String, String)> {
    let p = crate::quota::codex::decode_jwt_payload(token)?;
    let s = |v: Option<&serde_json::Value>| v.and_then(|x| x.as_str()).map(String::from);
    Some((
        s(p.get("iss"))?,
        s(p.get("client_id"))?,
        s(p.get("https://api.openai.com/auth").and_then(|a| a.get("chatgpt_account_id")))?,
    ))
}

fn apply_codex_token(keys: &mut Vec<KeyValue>, token: String) {
    let take = match keys.iter().find(|k| k.name == CODEX_TOKEN_NAME) {
        // 沒設這個環境變數就不憑空多一列
        None => false,
        Some(k) => {
            let (env, codex) = (k.value.trim(), token.trim());
            // 只換「同一個帳號、同一種 token」：Codex 改登別的帳號時不可把別人的憑證
            // 塞進這個名字；環境變數不是 JWT（使用者自己放的別種 token）也不動
            let same = token_identity(env).is_some() && token_identity(env) == token_identity(codex);
            same && matches!((jwt_exp(env), jwt_exp(codex)), (Some(e), Some(c)) if c > e)
        }
    };
    if take {
        keys.retain(|k| k.name != CODEX_TOKEN_NAME);
        keys.push(KeyValue {
            name: CODEX_TOKEN_NAME.to_string(),
            value: token,
            source: SRC_CODEX,
        });
    }
}

/// 列表／複製／顯示／檢查共用的唯一來源。
pub fn current_keys() -> Vec<KeyValue> {
    let mut keys = live_entries();
    let home = if cfg!(test) { None } else { dirs::home_dir() };
    if let Some(token) = home.and_then(|h| crate::quota::codex::chatgpt_access_token(&h)) {
        apply_codex_token(&mut keys, token);
    }
    keys
}

/// 取值前一律再驗一次名稱：前端傳什麼字串都可能，不可讓它變成「讀任意環境
/// 變數」的通道（PATH、憑證路徑、其他服務的密碼都在同一個環境裡）。
/// 回傳去掉前後空白的值——貼上時多一個換行就是一把「錯的」金鑰。
pub fn value_of(name: &str) -> Result<String, String> {
    if !is_ai_key_name(name) {
        return Err("不是可複製的 API key 名稱".into());
    }
    current_keys()
        .into_iter()
        .find(|k| k.name == name)
        .map(|k| k.value.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("{name} 未設定或為空"))
}

/// 有效性檢查結果：只有名稱與判定，沒有值。
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct KeyCheck {
    pub name: String,
    /// valid／invalid／blocked／expired／error／unchecked
    pub status: &'static str,
    /// tooltip 用的短說明（HTTP 狀態碼等），不含金鑰
    pub detail: String,
}

/// 各家的唯讀端點（列模型／查 key 本身，不產生費用）。金鑰只送往它所屬供應商
/// 的官方網域。第一個是主端點；主端點回「無效」時才試後面的（同一家另一個區域）。
/// 沒列的家回空 → unchecked，不亂打。
fn probe_urls(name: &str) -> &'static [&'static str] {
    if name.starts_with("OPENROUTER") {
        &["https://openrouter.ai/api/v1/key"]
    } else if name.starts_with("GLM") || name.starts_with("ZAI") {
        &[
            "https://api.z.ai/api/paas/v4/models",
            "https://open.bigmodel.cn/api/paas/v4/models",
        ]
    } else if name.starts_with("GROQ") {
        &["https://api.groq.com/openai/v1/models"]
    } else if name.starts_with("MINIMAX") {
        &["https://api.minimax.io/v1/models"]
    } else if name.starts_with("MIMO") {
        &[
            "https://api.xiaomimimo.com/v1/models",
            "https://token-plan-sgp.xiaomimimo.com/v1/models",
        ]
    } else {
        &[]
    }
}

/// HTTP 結果 → 判定。只有 401 算「無效」：2026-10-19 拿假金鑰實測，上面每一家
/// 對壞金鑰都回 401。403 原因很多（權限、地區、WAF），判不出金鑰好壞——其中 Groq
/// 擋 VPN／地區時回「Access denied. Please check your network settings.」，單獨標出來。
fn classify(status: u16, body: &str) -> (&'static str, String) {
    match status {
        200..=299 => ("valid", String::new()),
        401 => ("invalid", "HTTP 401：供應商不認這把金鑰".into()),
        403 => {
            let b = body.to_ascii_lowercase();
            if b.contains("access denied") && b.contains("network") {
                ("blocked", "HTTP 403：供應商擋了目前的網路（VPN／地區），不是金鑰的問題".into())
            } else {
                ("error", "HTTP 403：被拒絕，原因不明（不一定是金鑰）".into())
            }
        }
        _ => ("error", format!("HTTP {status}")),
    }
}

/// 主端點＋備援區的結果合併：任一區有效就有效；每一區都明確回「無效」才算無效；
/// 有任何一區判不出來（被擋／錯誤），就回那個「判不出來」的結果，不冒然說無效。
fn combine(results: Vec<(&'static str, String)>) -> (&'static str, String) {
    if results.iter().any(|r| r.0 == "valid") {
        return ("valid", String::new());
    }
    let undecided = results.iter().find(|r| r.0 != "invalid").cloned();
    undecided
        .or_else(|| results.into_iter().next())
        .unwrap_or(("error", "沒有結果".into()))
}

/// JWT 到期日本機就看得出來，不必連線，也不可被快取蓋掉。
fn expired_check(name: &str, value: &str) -> Option<KeyCheck> {
    jwt_exp(value).is_some_and(|exp| exp <= now_secs()).then(|| KeyCheck {
        name: name.to_string(),
        status: "expired",
        detail: "token 已過期".into(),
    })
}

async fn probe(client: &reqwest::Client, url: &str, key: &str) -> (&'static str, String) {
    match client.get(url).bearer_auth(key).send().await {
        Ok(r) if r.status().is_success() => classify(r.status().as_u16(), ""),
        Ok(r) => {
            let code = r.status().as_u16();
            let body = r.text().await.unwrap_or_default();
            classify(code, &body)
        }
        Err(e) if e.is_timeout() => ("error", "逾時".into()),
        Err(_) => ("error", "連線失敗".into()),
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

async fn check_one(client: reqwest::Client, name: String, value: String) -> KeyCheck {
    if let Some(r) = expired_check(&name, &value) {
        return r;
    }
    let Some((first, rest)) = probe_urls(&name).split_first() else {
        return KeyCheck { name, status: "unchecked", detail: String::new() };
    };
    let mut results = vec![probe(&client, first, &value).await];
    // 主端點明確說「無效」才試同一家的另一區（金鑰可能只在那一區有效）
    if results[0].0 == "invalid" {
        for url in rest {
            results.push(probe(&client, url, &value).await);
        }
    }
    let (status, detail) = combine(results);
    KeyCheck { name, status, detail }
}

/// 同一個值 10 分鐘內不重打；值一換（指紋不同）就重查。失敗（error）不快取。
const CHECK_TTL: Duration = Duration::from_secs(600);
type CheckCache = Mutex<HashMap<(String, u64), (KeyCheck, Instant)>>;

fn check_cache() -> &'static CheckCache {
    static CACHE: OnceLock<CheckCache> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// 值的指紋：只放在記憶體裡當快取鍵，不落地、不送出。
fn fingerprint(value: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}

/// 檢查目前每一把金鑰。回傳依名稱排序。
pub async fn check_all() -> Vec<KeyCheck> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut set = tokio::task::JoinSet::new();
    for k in current_keys() {
        let value = k.value.trim().to_string();
        if value.is_empty() {
            continue;
        }
        // 先看到期：快取裡的「有效」可能是 token 到期前查的
        if let Some(r) = expired_check(&k.name, &value) {
            out.push(r);
            continue;
        }
        let cache_key = (k.name.clone(), fingerprint(&value));
        let hit = check_cache().lock().ok().and_then(|c| {
            c.get(&cache_key)
                .filter(|(_, at)| at.elapsed() < CHECK_TTL)
                .map(|(r, _)| r.clone())
        });
        if let Some(r) = hit {
            out.push(r);
            continue;
        }
        let client = client.clone();
        set.spawn(async move { (cache_key, check_one(client, k.name, value).await) });
    }
    while let Some(joined) = set.join_next().await {
        let Ok((cache_key, r)) = joined else { continue };
        // 只記名稱與判定，永遠不記值
        log::info!("[api_keys] 檢查 {}：{}", r.name, r.status);
        if r.status != "error" {
            if let Ok(mut c) = check_cache().lock() {
                c.retain(|_, (_, at)| at.elapsed() < CHECK_TTL);
                c.insert(cache_key, (r.clone(), Instant::now()));
            }
        }
        out.push(r);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ai_provider_keys_are_listed() {
        assert!(is_ai_key_name("GROQ_API_KEY"));
        assert!(is_ai_key_name("GROQ_API_KEY_2"), "帶編號的也要收");
        assert!(is_ai_key_name("OPENROUTER_API_KEY_A"));
        assert!(is_ai_key_name("MINIMAX_DIRECT_KEY"));
        assert!(is_ai_key_name("OPENAI_ACCESS_TOKEN"));
        // 使用者明確選擇不收非 AI 的基礎設施憑證
        assert!(!is_ai_key_name("KAGGLE_API_TOKEN"));
        assert!(!is_ai_key_name("N8N_MCP_TOKEN"));
        assert!(!is_ai_key_name("TWINKLE_HUB_TOKEN"));
        assert!(!is_ai_key_name("GITHUB_TOKEN"));
        // 非金鑰形狀的一律不收，別把整個環境變數表倒出來
        assert!(!is_ai_key_name("PATH"));
        assert!(!is_ai_key_name("OPENAI_BASE_URL"));
    }

    #[test]
    fn duplicate_values_share_a_group_and_uniques_get_none() {
        // 實測本機情境：GLM 與 ZAI 是同一把、MINIMAX 兩個名字也是同一把，
        // 畫面上末四碼相同但使用者無從得知 → 必須標出來。
        let same = "sk-samesecret-9999".to_string();
        let vars = vec![
            ("GLM_API_KEY".to_string(), same.clone()),
            ("ZAI_API_KEY".to_string(), same.clone()),
            ("MINIMAX_API_KEY".to_string(), "sk-cp-minimax-1111".to_string()),
            ("MINIMAX_DIRECT_KEY".to_string(), "sk-cp-minimax-1111".to_string()),
            ("GROQ_API_KEY".to_string(), "gsk-unique-2222".to_string()),
        ];
        let got = collect(vars);
        let by = |n: &str| got.iter().find(|e| e.name == n).expect("entry").clone();
        assert_eq!(by("GLM_API_KEY").dup_group, by("ZAI_API_KEY").dup_group);
        assert!(by("GLM_API_KEY").dup_group.is_some());
        assert_eq!(
            by("MINIMAX_API_KEY").dup_group,
            by("MINIMAX_DIRECT_KEY").dup_group
        );
        // 兩組重複必須是不同組號，不可混為一談
        assert_ne!(by("GLM_API_KEY").dup_group, by("MINIMAX_API_KEY").dup_group);
        assert_eq!(by("GROQ_API_KEY").dup_group, None, "沒重複的不給組號");
    }

    #[test]
    fn used_by_marks_only_keys_lp_actually_reads() {
        assert_eq!(used_by("OPENROUTER_API_KEY_A"), Some("OpenRouter 額度卡"));
        assert_eq!(used_by("OPENROUTER_API_KEY"), Some("OpenRouter 額度卡"));
        assert_eq!(used_by("MINIMAX_API_KEY"), Some("MiniMax 額度卡"));
        // MINIMAX_DIRECT_KEY 名字很像但 fetcher 沒讀它，不可謊報「正在使用」
        assert_eq!(used_by("MINIMAX_DIRECT_KEY"), None);
        assert_eq!(used_by("GROQ_API_KEY"), None);
    }

    #[test]
    fn mask_reveals_only_last_four() {
        assert_eq!(mask("sk-abcdefghijklmnop"), "••••mnop");
        assert_eq!(mask("12345678"), "••••••••", "太短一律全遮");
        assert_eq!(mask("abc"), "•••");
        assert_eq!(mask(""), "•");
        // 遮罩結果不得含有原值的前綴（避免洩漏可辨識的開頭）
        assert!(!mask("sk-verylongsecretvalue").contains("sk-"));
    }

    #[test]
    fn collect_filters_sorts_and_drops_empty() {
        let vars = vec![
            ("PATH".to_string(), "C:\\bin".to_string()),
            ("ZAI_API_KEY".to_string(), "zzzz-1111".to_string()),
            ("GROQ_API_KEY".to_string(), "gsk-abcdefgh9999".to_string()),
            ("GLM_API_KEY".to_string(), "   ".to_string()), // 空值＝未設定
            ("KAGGLE_API_TOKEN".to_string(), "kkkk".to_string()),
        ];
        let got = collect(vars);
        assert_eq!(
            got.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            vec!["GROQ_API_KEY", "ZAI_API_KEY"]
        );
        assert_eq!(got[0].masked, "••••9999");
        // 回傳結構裡不得有任何欄位存放原值
        let json = serde_json::to_string(&got[0]).unwrap();
        assert!(!json.contains("gsk-abcdefgh"), "序列化後不可含原值: {json}");
    }

    #[test]
    fn value_of_rejects_arbitrary_env_names() {
        assert!(value_of("PATH").is_err(), "不可變成讀任意環境變數的通道");
        assert!(value_of("USERPROFILE").is_err());
        assert!(value_of("GROQ_API_KEY_DOES_NOT_EXIST_9").is_err(), "沒設定就是錯誤");
    }

    #[test]
    fn value_of_trims_whitespace_so_pasted_key_is_exact() {
        let _env_guard = crate::quota::copilot::ENV_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        std::env::set_var("GROQ_API_KEY_ZTRIM", "  gsk-trim-test-1234 \r\n");
        assert_eq!(value_of("GROQ_API_KEY_ZTRIM").unwrap(), "gsk-trim-test-1234");
        std::env::remove_var("GROQ_API_KEY_ZTRIM");
    }

    #[test]
    fn merge_env_user_overrides_machine_case_insensitively() {
        let got = merge_env(
            vec![
                ("GROQ_API_KEY".into(), "machine-old".into()),
                ("ZAI_API_KEY".into(), "machine-only".into()),
            ],
            vec![("groq_api_key".into(), "user-new".into())],
        );
        let groq: Vec<_> = got.iter().filter(|k| k.name.eq_ignore_ascii_case("GROQ_API_KEY")).collect();
        assert_eq!(groq.len(), 1, "同名只留一份");
        assert_eq!((groq[0].value.as_str(), groq[0].source), ("user-new", SRC_USER));
        let zai = got.iter().find(|k| k.name == "ZAI_API_KEY").unwrap();
        assert_eq!((zai.value.as_str(), zai.source), ("machine-only", SRC_MACHINE));
    }

    // payload {"exp":100} / {"exp":200}
    const JWT_EXP_100: &str = "h.eyJleHAiOjEwMH0.s";
    const JWT_EXP_200: &str = "h.eyJleHAiOjIwMH0.s";

    #[test]
    fn collect_reports_source_and_jwt_expiry() {
        let got = collect(vec![
            KeyValue { name: "OPENAI_ACCESS_TOKEN".into(), value: JWT_EXP_100.into(), source: SRC_USER },
            KeyValue { name: "GROQ_API_KEY".into(), value: "gsk-plain-key-0000".into(), source: SRC_MACHINE },
        ]);
        assert_eq!(got[0].name, "GROQ_API_KEY");
        assert_eq!((got[0].source, got[0].expires_at), (SRC_MACHINE, None), "一般金鑰沒有到期日");
        assert_eq!((got[1].source, got[1].expires_at), (SRC_USER, Some(100)));
    }

    // payload：exp + iss=https://auth.openai.com + client_id=app_TEST + chatgpt_account_id
    const ACC1_EXP_100: &str = "h.eyJleHAiOjEwMCwiaXNzIjoiaHR0cHM6Ly9hdXRoLm9wZW5haS5jb20iLCJjbGllbnRfaWQiOiJhcHBfVEVTVCIsImh0dHBzOi8vYXBpLm9wZW5haS5jb20vYXV0aCI6eyJjaGF0Z3B0X2FjY291bnRfaWQiOiJhY2MtMSJ9fQ.s";
    const ACC1_EXP_200: &str = "h.eyJleHAiOjIwMCwiaXNzIjoiaHR0cHM6Ly9hdXRoLm9wZW5haS5jb20iLCJjbGllbnRfaWQiOiJhcHBfVEVTVCIsImh0dHBzOi8vYXBpLm9wZW5haS5jb20vYXV0aCI6eyJjaGF0Z3B0X2FjY291bnRfaWQiOiJhY2MtMSJ9fQ.s";
    const ACC2_EXP_200: &str = "h.eyJleHAiOjIwMCwiaXNzIjoiaHR0cHM6Ly9hdXRoLm9wZW5haS5jb20iLCJjbGllbnRfaWQiOiJhcHBfVEVTVCIsImh0dHBzOi8vYXBpLm9wZW5haS5jb20vYXV0aCI6eyJjaGF0Z3B0X2FjY291bnRfaWQiOiJhY2MtMiJ9fQ.s";

    #[test]
    fn codex_token_replaces_only_an_older_token_of_the_same_account() {
        let env = |v: &str| vec![KeyValue { name: CODEX_TOKEN_NAME.into(), value: v.into(), source: SRC_USER }];
        // 同帳號、環境變數的較舊 → 換成 Codex 的
        let mut keys = env(ACC1_EXP_100);
        apply_codex_token(&mut keys, ACC1_EXP_200.into());
        assert_eq!(keys.len(), 1);
        assert_eq!((keys[0].value.as_str(), keys[0].source), (ACC1_EXP_200, SRC_CODEX));
        // Codex 登的是別的帳號 → 即使較新也不可換（那是別人的憑證）
        let mut keys = env(ACC1_EXP_100);
        apply_codex_token(&mut keys, ACC2_EXP_200.into());
        assert_eq!(keys[0].value, ACC1_EXP_100);
        // 環境變數的反而較新 → 不動
        let mut keys = env(ACC1_EXP_200);
        apply_codex_token(&mut keys, ACC1_EXP_100.into());
        assert_eq!(keys[0].source, SRC_USER);
        // 缺身分欄位的 JWT 無從核對 → 不動
        let mut keys = env(JWT_EXP_100);
        apply_codex_token(&mut keys, JWT_EXP_200.into());
        assert_eq!(keys[0].value, JWT_EXP_100);
        // 環境變數不是 JWT（使用者自己放的別種 token）→ 不動
        let mut keys = env("sk-not-a-jwt-0000");
        apply_codex_token(&mut keys, ACC1_EXP_200.into());
        assert_eq!(keys[0].value, "sk-not-a-jwt-0000");
        // 沒設這個環境變數 → 不憑空多一列
        let mut keys = Vec::new();
        apply_codex_token(&mut keys, ACC1_EXP_200.into());
        assert!(keys.is_empty());
    }

    #[test]
    fn classify_only_401_means_invalid() {
        assert_eq!(classify(200, "").0, "valid");
        assert_eq!(classify(401, "").0, "invalid");
        let groq_vpn = r#"{"error":{"message":"Access denied. Please check your network settings."}}"#;
        assert_eq!(classify(403, groq_vpn).0, "blocked", "被擋網路不是金鑰壞掉");
        assert_eq!(classify(403, r#"{"error":"forbidden"}"#).0, "error", "原因不明的 403 不可說成無效");
        assert_eq!(classify(500, "").0, "error");
        assert_eq!(classify(429, "").0, "error", "限流判不出金鑰好壞");
    }

    #[test]
    fn combine_needs_every_region_to_reject_before_saying_invalid() {
        let r = |s: &'static str| (s, String::new());
        assert_eq!(combine(vec![r("invalid"), r("valid")]).0, "valid", "只在備援區有效也算有效");
        assert_eq!(combine(vec![r("invalid"), r("invalid")]).0, "invalid");
        assert_eq!(combine(vec![r("invalid"), r("blocked")]).0, "blocked", "備援區判不出來就不說無效");
        assert_eq!(combine(vec![r("invalid"), r("error")]).0, "error");
        assert_eq!(combine(vec![r("blocked")]).0, "blocked");
        assert_eq!(combine(Vec::new()).0, "error");
    }

    #[test]
    fn expand_with_resolves_from_lookup_and_keeps_unknown() {
        let lookup = |n: &str| match n {
            "KEY_STORE" => Some("sk-from-store".to_string()),
            _ => None,
        };
        assert_eq!(expand_with("%KEY_STORE%", &lookup), "sk-from-store");
        assert_eq!(expand_with("a-%KEY_STORE%-b", &lookup), "a-sk-from-store-b");
        assert_eq!(expand_with("%NOPE%", &lookup), "%NOPE%", "找不到的照 Windows 慣例原樣保留");
        assert_eq!(expand_with("100%", &lookup), "100%");
        assert_eq!(expand_with("%%", &lookup), "%%");
        assert_eq!(expand_with("plain", &lookup), "plain");
    }

    #[test]
    fn expired_check_runs_locally_for_jwt_only() {
        assert_eq!(expired_check("OPENAI_ACCESS_TOKEN", JWT_EXP_100).unwrap().status, "expired");
        assert!(expired_check("GROQ_API_KEY", "gsk-plain-0000").is_none());
    }

    #[test]
    fn probe_urls_only_hit_the_keys_own_provider() {
        let host = |u: &str| u.split('/').nth(2).unwrap().to_string();
        let owns = |name: &str, domains: &[&str]| {
            for u in probe_urls(name) {
                let h = host(u);
                assert!(domains.iter().any(|d| h == *d || h.ends_with(&format!(".{d}"))), "{name} 不可送往 {h}");
            }
            assert!(!probe_urls(name).is_empty(), "{name} 應有檢查端點");
        };
        owns("OPENROUTER_API_KEY_A", &["openrouter.ai"]);
        owns("GLM_API_KEY", &["z.ai", "bigmodel.cn"]);
        owns("ZAI_API_KEY", &["z.ai", "bigmodel.cn"]);
        owns("GROQ_API_KEY_2", &["groq.com"]);
        owns("MINIMAX_TOKEN_PLAN_KEY", &["minimax.io"]);
        owns("MIMO_API_KEY", &["xiaomimimo.com"]);
        // 沒驗證過端點的家：不檢查，不亂打
        assert!(probe_urls("OPENAI_ACCESS_TOKEN").is_empty());
        assert!(probe_urls("DEEPSEEK_API_KEY").is_empty());
    }

    #[tokio::test]
    async fn check_one_flags_expired_jwt_and_unchecked_without_network() {
        let client = reqwest::Client::new();
        let r = check_one(client.clone(), "OPENAI_ACCESS_TOKEN".into(), JWT_EXP_100.into()).await;
        assert_eq!(r.status, "expired");
        let r = check_one(client, "DEEPSEEK_API_KEY".into(), "sk-no-endpoint-0000".into()).await;
        assert_eq!(r.status, "unchecked");
    }

    /// 手動診斷：cargo test --lib live_registry -- --ignored --nocapture
    /// 只印名稱、來源與遮罩，不印值。
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn live_registry_lists_ai_keys_masked() {
        let got = registry::read(&is_ai_key_name).expect("讀不到登錄檔");
        for k in &got {
            println!("{} [{}] {}", k.name, k.source, mask(&k.value));
        }
        assert!(!got.is_empty());
    }
}

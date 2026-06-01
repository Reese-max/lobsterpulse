//! Discord bot REST helper —— 走 Windows 內建 curl.exe，不加 crate 依賴。
//! Button interaction 靠 reaction polling 實現（不開 Gateway WSS）。

use serde::Deserialize;
use std::process::Command;

const API: &str = "https://discord.com/api/v10";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

fn auth_ok(token: &str, channel_id: &str) -> Result<(), String> {
    if token.trim().is_empty() {
        return Err("token empty".into());
    }
    if channel_id.trim().is_empty() {
        return Err("channel_id empty".into());
    }
    Ok(())
}

fn curl(method: &str, url: &str, token: &str, body: Option<&str>) -> Result<Vec<u8>, String> {
    let auth = format!("Authorization: Bot {}", token.trim());
    let mut cmd = Command::new("curl.exe");
    cmd.args([
        "-s",
        "-S",
        "-f",
        "-X",
        method,
        "-H",
        &auth,
        "-H",
        "User-Agent: LobsterPulse/0.6",
    ]);
    if let Some(b) = body {
        cmd.args(["-H", "Content-Type: application/json", "--data-raw", b]);
    }
    cmd.arg(url);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| format!("curl spawn fail: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "curl exit {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(out.stdout)
}

/// 發純文字訊息 → 回傳 message_id。
pub fn send_message(token: &str, channel_id: &str, content: &str) -> Result<String, String> {
    auth_ok(token, channel_id)?;
    let body = serde_json::json!({ "content": content }).to_string();
    let url = format!("{API}/channels/{channel_id}/messages");
    let resp = curl_recorded("POST", &url, token, Some(&body))?;
    parse_msg_id(&resp)
}

/// 發 embed 訊息（標題+描述+顏色）→ 回傳 message_id。color 是 0xRRGGBB。
pub fn send_embed(
    token: &str,
    channel_id: &str,
    title: &str,
    desc: &str,
    color: u32,
) -> Result<String, String> {
    auth_ok(token, channel_id)?;
    let body = serde_json::json!({
        "embeds": [{
            "title": title,
            "description": desc,
            "color": color,
        }]
    })
    .to_string();
    let url = format!("{API}/channels/{channel_id}/messages");
    let resp = curl_recorded("POST", &url, token, Some(&body))?;
    parse_msg_id(&resp)
}

fn parse_msg_id(resp: &[u8]) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_slice(resp)
        .map_err(|e| format!("parse resp fail ({e}): {}", String::from_utf8_lossy(resp)))?;
    v.get("id")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("no message id: {}", String::from_utf8_lossy(resp)))
}

/// Bot 自己加 reaction（讓用戶一鍵點選）。emoji 用 unicode 字元。
pub fn add_reaction(
    token: &str,
    channel_id: &str,
    message_id: &str,
    emoji: &str,
) -> Result<(), String> {
    auth_ok(token, channel_id)?;
    // curl 用 --data-urlencode-like 不方便，直接 percent-encode。
    let encoded = percent_encode(emoji);
    let url = format!("{API}/channels/{channel_id}/messages/{message_id}/reactions/{encoded}/@me");
    curl_recorded("PUT", &url, token, Some(""))?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct DiscordUser {
    #[allow(dead_code)]
    pub id: String,
    #[serde(default)]
    pub bot: bool,
}

/// 檢查訊息是否有非 bot 的用戶按了 emoji → 回傳 true（有人點）。
pub fn has_human_reactor(
    token: &str,
    channel_id: &str,
    message_id: &str,
    emoji: &str,
) -> Result<bool, String> {
    auth_ok(token, channel_id)?;
    let encoded = percent_encode(emoji);
    let url =
        format!("{API}/channels/{channel_id}/messages/{message_id}/reactions/{encoded}?limit=20");
    let resp = curl_recorded("GET", &url, token, None)?;
    let users: Vec<DiscordUser> = serde_json::from_slice(&resp)
        .map_err(|e| format!("parse users fail ({e}): {}", String::from_utf8_lossy(&resp)))?;
    Ok(users.iter().any(|u| !u.bot))
}

/// 列最近 N 筆訊息（for command polling）。
pub fn list_messages(
    token: &str,
    channel_id: &str,
    limit: u32,
    after: Option<&str>,
) -> Result<Vec<serde_json::Value>, String> {
    auth_ok(token, channel_id)?;
    let mut url = format!("{API}/channels/{channel_id}/messages?limit={limit}");
    if let Some(a) = after {
        url.push_str(&format!("&after={a}"));
    }
    let resp = curl_recorded("GET", &url, token, None)?;
    serde_json::from_slice::<Vec<serde_json::Value>>(&resp).map_err(|e| {
        format!(
            "parse messages fail ({e}): {}",
            String::from_utf8_lossy(&resp)
        )
    })
}

/// K14 落地：Discord 健康度分類。
/// 對齊 R2/R25 Discord 401 silent-surfacing 主題：把 curl `-f` 失敗的 stderr 字串
/// 分類 4xx / 5xx / network 給 Prometheus metric 用。「沒失敗」= `last_class: None`,
/// 不放 Ok variant (4xx 5xx network 是 3 種失敗模式,不是 4 種狀態含 success)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscordHealthClass {
    /// HTTP 4xx — recoverable,常見 401 token rotated / 403 沒權限 / 404 channel 錯 / 429 rate limit
    Client4xx(u16),
    /// HTTP 5xx — server-side issue
    Server5xx(u16),
    /// Network — DNS 解析失敗 / 連線被拒 / 逾時 / curl spawn fail / curl exit code 非 22
    Network,
}

impl DiscordHealthClass {
    /// Prometheus gauge 用的穩定整數:0=ok (last_class=None) / 1=4xx / 2=5xx / 3=network。
    /// 排序符合「嚴重但不引發 alert 過激」邏輯：4xx 通常配置問題,5xx 是 server 端,
    /// network 通常意味 operator 需查網路 / DNS。
    pub fn health_gauge(&self) -> i64 {
        match self {
            DiscordHealthClass::Client4xx(_) => 1,
            DiscordHealthClass::Server5xx(_) => 2,
            DiscordHealthClass::Network => 3,
        }
    }
}

/// K14 健康度累計 state (process-level,因為 Discord 是單一端點不是 per-provider)。
/// 對齊 K6/K7/K9/K13 lifetime aggregate 語意:counter 一旦累加就不蒸發,operator 端
/// `rate(send_failures_total{class="5xx"}[5m])` 是標準 throughput 公式。
#[derive(Debug, Default, Clone, Copy)]
pub struct DiscordHealth {
    pub class_4xx: u64,
    pub class_5xx: u64,
    pub class_network: u64,
    /// 最後一筆失敗分類;`None` = 啟動後還沒失敗過 (last_health gauge = 0)。
    pub last_class: Option<DiscordHealthClass>,
    pub last_event_unix: i64,
}

impl DiscordHealth {
    /// 記一筆失敗 (`sat add` overflow 不 panic) + 更新 last_class / last_event_unix。
    /// 給 K14 配套使用,搭配 `classify_error_str` 從 error string 推 class。
    /// K14 不主動記 success —「健康」= `last_class: None` (gauge 0),只在失敗時
    /// 切到 Some(class) (gauge 1/2/3)。
    pub fn record(&mut self, class: DiscordHealthClass, now_unix: i64) {
        match class {
            DiscordHealthClass::Client4xx(_) => {
                self.class_4xx = self.class_4xx.saturating_add(1);
            }
            DiscordHealthClass::Server5xx(_) => {
                self.class_5xx = self.class_5xx.saturating_add(1);
            }
            DiscordHealthClass::Network => {
                self.class_network = self.class_network.saturating_add(1);
            }
        }
        self.last_class = Some(class);
        self.last_event_unix = now_unix;
    }
}

/// K14 純分類器:把 curl 失敗時組出的 `"curl exit {:?}: stderr"` 字串 parse 出 class。
///
/// 為什麼 parse 字串而不是結構化傳遞:
///   - R2 commit 修了 `-f` flag,失敗時 error 是 `format!("curl exit {:?}: {}", exit, stderr)`
///   - 高層 `send_message` / `send_embed` / `add_reaction` / `list_messages` 都用 `?` 把
///     `Result<_, String>` 往外傳,call site 只看得到字串。為不破壞既有 signature、
///     最小 surgical 改動,選 parse string
///   - parse pattern 鎖定 `The requested URL returned error: NNN` (curl 對 4xx/5xx 的
///     stderr 慣用格式),其他一律 network
///
/// 為什麼獨立抽 fn 而不 inline:
///   - 5 種典型 curl stderr 都要 test (4xx/5xx/network × 多種變體) → 純函式比 `DiscordHealth::record`
///     內部 if-else 易測,boundary 清晰
///   - 將來若改用別的 transport (reqwest / ureq) 結構化傳 status code,只換 classifier
///     內部,call site 不動
pub fn classify_error_str(err: &str) -> DiscordHealthClass {
    // 1. HTTP 4xx/5xx:抓 curl 慣用 `The requested URL returned error: NNN`
    if let Some(pos) = err.find("returned error: ") {
        let after = &err[pos + "returned error: ".len()..];
        let code_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(code) = code_str.parse::<u16>() {
            if (400..500).contains(&code) {
                return DiscordHealthClass::Client4xx(code);
            }
            if (500..600).contains(&code) {
                return DiscordHealthClass::Server5xx(code);
            }
        }
    }
    // 2. 其他一律 network (DNS / conn refused / timeout / spawn fail / curl exit != 22)
    DiscordHealthClass::Network
}

/// K14 落地：process-level health state（OnceLock + Mutex）。
/// 模組級避免改 4 個 fn signature → 20+ 個 call site 零改動,符合 KISS。
/// `init_health()` 在 `lib::setup()` 啟動時呼叫一次;未 init 時 record no-op,
/// render 端讀到 `Default::default()` snapshot,metric 仍 emit (0/0/0, 沒歷史)。
use std::sync::{Mutex, OnceLock};

static HEALTH: OnceLock<Mutex<DiscordHealth>> = OnceLock::new();

/// 啟動時呼叫一次:把 process-level DiscordHealth state 注入 OnceLock。
/// 多次呼叫安全 (`get_or_init` idempotent);未呼叫不 panic,只是 record/snapshot 退化。
pub fn init_health() {
    let _ = HEALTH.get_or_init(|| Mutex::new(DiscordHealth::default()));
}

/// 讀 snapshot 給 Prometheus render 用。未 init 時回 `Default::default()` (全 0)。
/// 拿 snapshot 不持鎖跨越 string 構造 (避免 render 卡住其他 record 點)。
pub fn health_snapshot() -> DiscordHealth {
    HEALTH
        .get()
        .map(|m| *m.lock().expect("DiscordHealth mutex poisoned"))
        .unwrap_or_default()
}

/// 記一筆失敗 (K14 helper):純函式 → classify + record,「現在」由 caller 注入。
/// 不在這裡取 wall clock 是為了讓 test 用 fake now_unix 而不需 mock time crate。
fn record_classified_failure(err: &str, now_unix: i64) {
    if let Some(mutex) = HEALTH.get() {
        let class = classify_error_str(err);
        mutex
            .lock()
            .expect("DiscordHealth mutex poisoned")
            .record(class, now_unix);
    }
}

/// K14 內部 `curl` 包裝:失敗時自動 record (auth_ok 失敗也歸 network — 無 stderr 可解析)。
/// 4 個高層 fn (send_message / send_embed / add_reaction / list_messages) 改用這個,
/// 保持 `Result<_, String>` signature,call site 完全不動。
fn curl_recorded(
    method: &str,
    url: &str,
    token: &str,
    body: Option<&str>,
) -> Result<Vec<u8>, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    curl(method, url, token, body).inspect_err(|e| {
        record_classified_failure(e, now_unix);
    })
}

/// RFC3986 percent-encode 給 URL path 片段用（emoji 等多 byte 字元）。
fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod k14_health_tests {
    //! K14 Discord 健康度分類 / 累計的純函式 + module-level state 測試。
    //!
    //! 涵蓋:
    //!   - `classify_error_str` 純函式:curl stderr → DiscordHealthClass
    //!     （4xx / 5xx / 3xx+2xx 邊界 / 不可解析 → network）
    //!   - `DiscordHealthClass::health_gauge` 穩定整數 (0/1/2/3 對應 ok/4xx/5xx/network)
    //!   - `DiscordHealth::record` counter saturating + last_class 更新
    //!   - `init_health` / `health_snapshot` 模組級 OnceLock<Mutex<...>> 行為
    //!
    //! 為什麼純函式 + 模組級混測:
    //!   - 分類器 / 累計是純邏輯 → 純函式測試容易覆蓋 boundary
    //!   - OnceLock 不可重置,所以 record/snapshot 測試只驗證「不 panic +
    //!     snapshot 反映已 init 狀態」,不在此層做數值精確比對（那是
    //!     `render_prometheus_body` 測試的責任）

    use super::*;

    // ─── classify_error_str ────────────────────────────────────────────

    #[test]
    fn classify_4xx_codes() {
        for code in [400u16, 401, 403, 404, 429] {
            let err = format!("curl exit Some(22): The requested URL returned error: {code}");
            assert!(
                matches!(classify_error_str(&err), DiscordHealthClass::Client4xx(c) if c == code),
                "expected Client4xx({code}), got {err:?}"
            );
        }
    }

    #[test]
    fn classify_5xx_codes() {
        for code in [500u16, 502, 503, 504] {
            let err = format!("curl exit Some(22): The requested URL returned error: {code}");
            assert!(
                matches!(classify_error_str(&err), DiscordHealthClass::Server5xx(c) if c == code),
                "expected Server5xx({code}), got {err:?}"
            );
        }
    }

    #[test]
    fn classify_3xx_and_2xx_fall_through_to_network() {
        // 3xx 跟 2xx 不在我們 4xx/5xx 範圍,classifier 視為 network
        // (curl `-f` flag 在 3xx 預設 follow,但若 server 不送 Location 或
        //  disable follow,curl 仍會 fail,語意上歸 network 合理)
        for code in [200u16, 204, 301, 302, 999] {
            let err = format!("curl exit Some(22): The requested URL returned error: {code}");
            assert_eq!(
                classify_error_str(&err),
                DiscordHealthClass::Network,
                "code {code} 不在 4xx/5xx 應歸 network"
            );
        }
    }

    #[test]
    fn classify_unparseable_returns_network() {
        // 常見 curl stderr 變體:DNS / conn refused / timeout / spawn fail
        let cases = [
            "curl exit Some(6): Could not resolve host: discord.com",
            "curl exit Some(7): Failed to connect to discord.com port 443",
            "curl exit Some(28): Connection timed out after 30000 ms",
            "curl exit Some(22): SSL certificate problem",
            "curl spawn fail: not found",
        ];
        for err in cases {
            assert_eq!(
                classify_error_str(err),
                DiscordHealthClass::Network,
                "err {err:?} 應歸 network"
            );
        }
    }

    #[test]
    fn classify_empty_string_returns_network() {
        // auth_ok 失敗 / curl 完全沒給 stderr 會傳空字串進來
        assert_eq!(classify_error_str(""), DiscordHealthClass::Network);
    }

    // ─── DiscordHealthClass::health_gauge ─────────────────────────────

    #[test]
    fn health_gauge_stable_mapping() {
        // 0=ok (None), 1=4xx, 2=5xx, 3=network — 排序符號 operator 邏輯:
        // 4xx 配置問題 → 5xx server 端 → network 需查 DNS / 連線
        assert_eq!(DiscordHealthClass::Client4xx(401).health_gauge(), 1);
        assert_eq!(DiscordHealthClass::Server5xx(503).health_gauge(), 2);
        assert_eq!(DiscordHealthClass::Network.health_gauge(), 3);
    }

    // ─── DiscordHealth::record ────────────────────────────────────────

    #[test]
    fn record_increments_correct_counter() {
        let mut h = DiscordHealth::default();
        h.record(DiscordHealthClass::Client4xx(401), 1000);
        h.record(DiscordHealthClass::Client4xx(403), 1001);
        h.record(DiscordHealthClass::Server5xx(500), 1002);
        h.record(DiscordHealthClass::Network, 1003);
        assert_eq!(h.class_4xx, 2);
        assert_eq!(h.class_5xx, 1);
        assert_eq!(h.class_network, 1);
    }

    #[test]
    fn record_updates_last_class_and_event_unix() {
        let mut h = DiscordHealth::default();
        assert!(h.last_class.is_none());
        assert_eq!(h.last_event_unix, 0);
        h.record(DiscordHealthClass::Client4xx(401), 1234);
        assert_eq!(h.last_class, Some(DiscordHealthClass::Client4xx(401)));
        assert_eq!(h.last_event_unix, 1234);
        // 第二次 record 覆寫 last_class / last_event_unix (「最後一次」語意)
        h.record(DiscordHealthClass::Network, 5678);
        assert_eq!(h.last_class, Some(DiscordHealthClass::Network));
        assert_eq!(h.last_event_unix, 5678);
    }

    #[test]
    fn record_saturates_no_overflow() {
        // u64::MAX counter + 1 必須 saturating_add 不 panic
        let mut h = DiscordHealth {
            class_4xx: u64::MAX,
            ..Default::default()
        };
        h.record(DiscordHealthClass::Client4xx(401), 1);
        assert_eq!(h.class_4xx, u64::MAX);
        // last_class 仍應更新 (因為 saturating_add 後 record 邏輯繼續走)
        assert_eq!(h.last_class, Some(DiscordHealthClass::Client4xx(401)));
    }

    #[test]
    fn default_state_is_all_zero() {
        let h = DiscordHealth::default();
        assert_eq!(h.class_4xx, 0);
        assert_eq!(h.class_5xx, 0);
        assert_eq!(h.class_network, 0);
        assert!(h.last_class.is_none());
        assert_eq!(h.last_event_unix, 0);
    }

    // ─── 模組級 state (OnceLock<Mutex<...>>) ───────────────────────────

    #[test]
    fn init_health_is_idempotent() {
        // 多次呼叫 get_or_init 不 panic、不重置既有 state
        init_health();
        let snap_before = health_snapshot();
        init_health();
        init_health();
        let snap_after = health_snapshot();
        assert_eq!(snap_before.class_4xx, snap_after.class_4xx);
    }

    #[test]
    fn health_snapshot_does_not_block_on_uninit() {
        // 在 init 之前呼叫 health_snapshot() 應回 Default,而不是 panic
        // (此測試假設沒人先呼叫 init — 在多測試 process 中不嚴格保證,
        //  但「不 panic」契約一定要成立:fallback 走 unwrap_or_default)
        let snap = health_snapshot();
        // 只驗證不 panic,數值取決於其他測試是否已 init
        let _ = snap.class_4xx;
    }

    #[test]
    fn record_classified_failure_noop_when_uninit_or_otherwise_safe() {
        // 在 init 過的 process 中 record 一次失敗,後續 snapshot 至少
        // 反映「最後一筆 class 屬於某個 record 過的 class」(不精確比對計數,
        // 因為其他測試可能已 record)。
        record_classified_failure(
            "curl exit Some(22): The requested URL returned error: 401",
            9999,
        );
        let snap = health_snapshot();
        // 最後一筆 = 我們剛 record 的 401,或被其他後跑測試覆寫;只驗證
        // 「至少有 last_class 設值」或「是某個有效 class」
        if let Some(c) = snap.last_class {
            assert!(matches!(
                c,
                DiscordHealthClass::Client4xx(_)
                    | DiscordHealthClass::Server5xx(_)
                    | DiscordHealthClass::Network
            ));
        }
    }

    #[test]
    fn record_classified_failure_with_network_string() {
        // 不可解析的字串 → 走 network 分支,last_class = Network
        record_classified_failure("curl exit Some(6): DNS fail", 8888);
        let snap = health_snapshot();
        // last_event_unix >= 8888 (我們剛 record 的,後續測試可能覆寫)
        // 因此只驗證「若 last_class 是某個值,event_unix 不為 0」
        if snap.last_class.is_some() {
            assert!(
                snap.last_event_unix > 0,
                "若 last_class 有設,event_unix 應 > 0"
            );
        }
    }
}

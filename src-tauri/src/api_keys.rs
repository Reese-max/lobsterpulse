//! 「一鍵複製 API key」——只讀環境變數，app 自己不持有任何金鑰。
//!
//! 為什麼不做「儲存」：金鑰已經存在環境變數裡，再存一份到 app 就是多一個外洩
//! 點，而且會被 runtime/ 同步、備份、設定掃描一起帶走。改成每次即時讀、用完
//! 就算了。
//!
//! 值不進前端（除非使用者主動按住顯示）：複製是後端直接寫剪貼簿，webview 從頭
//! 到尾只拿得到名稱與遮罩，截圖／螢幕分享／devtools 都看不到。

use serde::Serialize;

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
pub fn collect<I: IntoIterator<Item = (String, String)>>(vars: I) -> Vec<ApiKeyEntry> {
    let mut out: Vec<ApiKeyEntry> = vars
        .into_iter()
        .filter(|(k, v)| is_ai_key_name(k) && !v.trim().is_empty())
        .map(|(k, v)| ApiKeyEntry {
            masked: mask(&v),
            name: k,
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 取值前一律再驗一次名稱：前端傳什麼字串都可能，不可讓它變成「讀任意環境
/// 變數」的通道（PATH、憑證路徑、其他服務的密碼都在同一個環境裡）。
pub fn value_of(name: &str) -> Result<String, String> {
    if !is_ai_key_name(name) {
        return Err("不是可複製的 API key 名稱".into());
    }
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => Ok(v),
        _ => Err(format!("{name} 未設定或為空")),
    }
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
}

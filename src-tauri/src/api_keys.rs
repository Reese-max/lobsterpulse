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
pub fn collect<I: IntoIterator<Item = (String, String)>>(vars: I) -> Vec<ApiKeyEntry> {
    let mut kept: Vec<(String, String)> = vars
        .into_iter()
        .filter(|(k, v)| is_ai_key_name(k) && !v.trim().is_empty())
        .collect();
    kept.sort_by(|a, b| a.0.cmp(&b.0));

    // 值 → 出現次數，只有 >1 的才配組號（組號按首次出現順序，從 1 起）
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (_, v) in &kept {
        *seen.entry(v.trim()).or_insert(0) += 1;
    }
    let mut group_of: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut next_group = 1usize;
    for (_, v) in &kept {
        let val = v.trim();
        if seen.get(val).copied().unwrap_or(0) > 1 && !group_of.contains_key(val) {
            group_of.insert(val, next_group);
            next_group += 1;
        }
    }

    kept.iter()
        .map(|(k, v)| ApiKeyEntry {
            masked: mask(v),
            dup_group: group_of.get(v.trim()).copied(),
            used_by: used_by(k),
            alias: None,
            name: k.clone(),
        })
        .collect()
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
}

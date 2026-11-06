//! 调用 OpenAI 兼容接口（默认 DeepSeek），返回结构化润色结果。

use std::time::Duration;

use serde_json::json;

use crate::config::Config;
use crate::model::PolishResult;

const SYSTEM_PROMPT: &str = r#"你是专业的英语写作润色助手。用户会给你一段文本，可能包含中文、代码、URL、专业术语或 Markdown。

请严格遵守：
1. 只修正英文中的语法、拼写、冠词、介词、时态、单复数、搭配、标点和明显的表达问题；
2. 不改变原意、语气、信息量与语言（原文是中文的部分保持中文）；
3. 原样保留换行、缩进、Markdown 标记、代码块、变量名、URL、邮箱、数字、人名与专有名词；
4. 如果原文没有需要修改的地方，polished 必须原样返回；
5. polished 中不要出现任何解释、前言或后记。

只输出一个 JSON 对象，不要输出其他内容：
{"polished":"润色后的完整文本","issues":[{"type":"grammar|spelling|article|preposition|tense|word-choice|punctuation|style|other","original":"原文片段","corrected":"修改后片段","explain":"一句中文说明"}],"summary":"一句中文总结本次改动"}"#;

fn endpoint(base: &str) -> String {
    let b = base.trim().trim_end_matches('/');
    if b.ends_with("/chat/completions") {
        b.to_string()
    } else if b.ends_with("/v1") {
        format!("{b}/chat/completions")
    } else {
        format!("{b}/v1/chat/completions")
    }
}

pub fn polish(cfg: &Config, text: &str) -> Result<PolishResult, String> {
    if cfg.api_key.trim().is_empty() {
        return Err("还没有配置 API Key，请先在「设置」里填写".to_string());
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

    let body = json!({
        "model": cfg.model,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": format!("请润色下面的文本：\n\n{}", text)}
        ],
        "temperature": cfg.temperature,
        "max_tokens": 4096,
        "stream": false,
        "response_format": {"type": "json_object"}
    });

    let resp = client
        .post(endpoint(&cfg.api_base))
        .bearer_auth(cfg.api_key.trim())
        .json(&body)
        .send()
        .map_err(|e| format!("请求失败: {e}"))?;

    let status = resp.status();
    let raw = resp.text().unwrap_or_default();
    if !status.is_success() {
        let brief: String = raw.chars().take(400).collect();
        return Err(format!("接口返回 {status}: {brief}"));
    }

    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("响应不是合法 JSON: {e}"))?;
    let content = value["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "响应缺少 choices[0].message.content".to_string())?;

    parse_content(content)
}

pub fn parse_content(content: &str) -> Result<PolishResult, String> {
    let cleaned = strip_fence(content);

    if let Ok(mut parsed) = serde_json::from_str::<PolishResult>(cleaned) {
        if parsed.polished.is_empty() {
            parsed.polished = cleaned.to_string();
        }
        return Ok(parsed);
    }

    if let (Some(start), Some(end)) = (cleaned.find('{'), cleaned.rfind('}')) {
        if end > start {
            let slice = &cleaned[start..=end];
            if let Ok(mut parsed) = serde_json::from_str::<PolishResult>(slice) {
                if parsed.polished.is_empty() {
                    parsed.polished = cleaned.to_string();
                }
                return Ok(parsed);
            }
        }
    }

    // 模型没有按 JSON 返回时，退化为纯文本结果
    let fallback = cleaned.trim().trim_matches('"').to_string();
    if fallback.is_empty() {
        return Err("模型返回为空".to_string());
    }
    Ok(PolishResult {
        polished: fallback,
        issues: Vec::new(),
        summary: String::new(),
    })
}

fn strip_fence(s: &str) -> &str {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        let rest = rest.strip_prefix("json").unwrap_or(rest);
        let rest = rest.trim_start_matches('\n');
        if let Some(idx) = rest.rfind("```") {
            return rest[..idx].trim();
        }
        return rest.trim();
    }
    t
}

/// 设置页「测试连接」使用
pub fn test_connection(cfg: &Config) -> Result<String, String> {
    let probe = "This are a test sentence, and i want to check the connection.";
    let result = polish(cfg, probe)?;
    Ok(result.polished)
}
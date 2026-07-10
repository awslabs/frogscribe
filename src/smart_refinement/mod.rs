#![allow(dead_code)]
//! Smart AI refinement via FrogScribe backend API (Bedrock Claude Haiku).
//! Authenticated with Midway cookies from ~/.midway/cookie.
//! Falls back gracefully to raw text if unavailable.

use anyhow::Result;
use std::time::Duration;

const API_BASE: &str = "https://frogscribe.rajmohr.people.amazon.dev";
const TIMEOUT_SECS: u64 = 5;

/// Refine text using the FrogScribe LLM API (Bedrock Claude Haiku).
/// Returns original text on any failure (timeout, auth, network).
pub async fn refine(text: &str, vocabulary: &[String]) -> String {
    match refine_inner(text, vocabulary).await {
        Ok(refined) => guard_against_injection(&refined, text),
        Err(e) => {
            tracing::warn!("Smart refinement failed, using raw text: {}", e);
            text.to_string()
        }
    }
}

async fn refine_inner(text: &str, vocabulary: &[String]) -> Result<String> {
    let token = read_midway_token()
        .ok_or_else(|| anyhow::anyhow!("Midway token not available. Run 'mwinit' in terminal."))?;

    let mut payload = serde_json::json!({
        "text": text,
        "preset": "clean"
    });
    if !vocabulary.is_empty() {
        payload["vocabulary"] = serde_json::json!(vocabulary);
    }

    let client = reqwest::Client::new();
    let response = tokio::time::timeout(
        Duration::from_secs(TIMEOUT_SECS),
        client
            .post(format!("{}/refine", API_BASE))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Timeout after {}s", TIMEOUT_SECS))?
    .map_err(|e| anyhow::anyhow!("Request failed: {}", e))?;

    let status = response.status().as_u16();
    if status == 401 || status == 403 {
        // Try refresh and retry once
        if refresh_aea() {
            if let Some(new_token) = read_midway_token() {
                let retry = client
                    .post(format!("{}/refine", API_BASE))
                    .header("Authorization", format!("Bearer {}", new_token))
                    .header("Content-Type", "application/json")
                    .json(&payload)
                    .send()
                    .await?;
                if retry.status().is_success() {
                    let body: serde_json::Value = retry.json().await?;
                    return Ok(body["refined_text"].as_str().unwrap_or(text).to_string());
                }
            }
        }
        anyhow::bail!("Midway session expired. Run 'mwinit' to refresh.");
    }

    if !response.status().is_success() {
        anyhow::bail!("API returned HTTP {}", status);
    }

    let body: serde_json::Value = response.json().await?;
    Ok(body["refined_text"].as_str().unwrap_or(text).to_string())
}

/// Read Midway JWT from ~/.midway/cookie (Netscape cookie format).
/// Looks for amzn_sso_token or amazon_enterprise_access cookies.
fn read_midway_token() -> Option<String> {
    let path = dirs::home_dir()?.join(".midway/cookie");
    let content = std::fs::read_to_string(&path).ok()?;
    parse_midway_token(&content)
}

fn parse_midway_token(content: &str) -> Option<String> {
    let accepted = ["amzn_sso_token", "amazon_enterprise_access"];

    for token_name in &accepted {
        for line in content.lines() {
            let line = line.trim_start_matches("#HttpOnly_");
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 7 {
                continue;
            }
            let name = fields[5];
            let value = fields[6];
            if name == *token_name && !value.is_empty() && value.split('.').count() == 3 {
                // Check JWT expiry
                if !is_jwt_expired(value) {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Check if JWT exp claim is expired or within 5 minutes of expiry.
fn is_jwt_expired(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return true;
    }
    // Decode base64url payload
    let mut payload = parts[1].replace('-', "+").replace('_', "/");
    while payload.len() % 4 != 0 {
        payload.push('=');
    }
    let decoded = match base64_decode(&payload) {
        Some(d) => d,
        None => return true,
    };
    let json: serde_json::Value = match serde_json::from_slice(&decoded) {
        Ok(v) => v,
        Err(_) => return true,
    };
    let exp = match json.get("exp").and_then(|v| v.as_f64()) {
        Some(e) => e,
        None => return true,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    exp - now < 300.0 // 5 minute threshold
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    // Simple base64 decoder
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in input.as_bytes() {
        if b == b'=' {
            break;
        }
        let val = TABLE.iter().position(|&c| c == b)? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(output)
}

/// Attempt non-interactive AEA token refresh via mwinit --refresh-aea
fn refresh_aea() -> bool {
    tracing::info!("Attempting AEA token refresh via mwinit --refresh-aea");
    std::process::Command::new("mwinit")
        .args(["--refresh-aea"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if Midway token is available (for UI status display)
pub fn has_valid_token() -> bool {
    read_midway_token().is_some()
}

/// Guard against prompt injection in LLM output
fn guard_against_injection(refined: &str, original: &str) -> String {
    let refined_lower = refined.to_lowercase();
    let original_lower = original.to_lowercase();
    let injection_phrases = [
        "speech-to-text post-processor",
        "never respond conversationally",
        "i am an ai",
        "i'm an ai",
        "as an ai language model",
        "i cannot fulfill",
        "i can't help with",
    ];
    for phrase in &injection_phrases {
        if refined_lower.contains(phrase) && !original_lower.contains(phrase) {
            return original.to_string();
        }
    }
    refined.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_against_injection_clean() {
        let result = guard_against_injection("Hello world.", "hello world");
        assert_eq!(result, "Hello world.");
    }

    #[test]
    fn test_guard_against_injection_detected() {
        let result = guard_against_injection(
            "I am an AI language model and cannot help.",
            "please fix my text",
        );
        assert_eq!(result, "please fix my text");
    }

    #[test]
    fn test_parse_midway_token_valid() {
        // Fake JWT with 3 parts (header.payload.signature)
        let fake_jwt = "eyJhbGciOiJSUzI1NiJ9.eyJleHAiOjk5OTk5OTk5OTl9.signature";
        let cookie_content = format!(
            ".amazon.com\tTRUE\t/\tTRUE\t9999999999\tamzn_sso_token\t{}",
            fake_jwt
        );
        let token = parse_midway_token(&cookie_content);
        assert_eq!(token, Some(fake_jwt.to_string()));
    }

    #[test]
    fn test_parse_midway_token_expired() {
        // JWT with exp=0 (expired)
        // payload: {"exp":0} -> base64url: eyJleHAiOjB9
        let expired_jwt = "eyJhbGciOiJSUzI1NiJ9.eyJleHAiOjB9.signature";
        let cookie_content = format!(
            ".amazon.com\tTRUE\t/\tTRUE\t0\tamzn_sso_token\t{}",
            expired_jwt
        );
        let token = parse_midway_token(&cookie_content);
        assert_eq!(token, None);
    }

    #[test]
    fn test_parse_midway_token_empty_file() {
        let token = parse_midway_token("");
        assert_eq!(token, None);
    }

    #[test]
    fn test_is_jwt_expired_invalid() {
        assert!(is_jwt_expired("not.a.jwt"));
        assert!(is_jwt_expired(""));
    }
}

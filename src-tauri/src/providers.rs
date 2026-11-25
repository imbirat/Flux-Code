use serde_json::{json, Value};

pub struct ProviderDef {
    pub id: &'static str,
    pub label: &'static str,
    /// wire format: "openai" (OpenAI-compatible), "anthropic", "gemini"
    pub format: &'static str,
    pub base_url: &'static str,
    /// where users create an API key for this provider
    pub help_url: &'static str,
}

pub static PROVIDERS: &[ProviderDef] = &[
    ProviderDef {
        id: "groq",
        label: "Groq",
        format: "openai",
        base_url: "https://api.groq.com/openai/v1",
        help_url: "https://console.groq.com/keys",
    },
    ProviderDef {
        id: "gemini",
        label: "Google Gemini",
        format: "gemini",
        base_url: "https://generativelanguage.googleapis.com/v1beta",
        help_url: "https://aistudio.google.com/apikey",
    },
    ProviderDef {
        id: "anthropic",
        label: "Anthropic",
        format: "anthropic",
        base_url: "https://api.anthropic.com/v1",
        help_url: "https://console.anthropic.com/settings/keys",
    },
    ProviderDef {
        id: "openai",
        label: "OpenAI",
        format: "openai",
        base_url: "https://api.openai.com/v1",
        help_url: "https://platform.openai.com/api-keys",
    },
    ProviderDef {
        id: "openrouter",
        label: "OpenRouter",
        format: "openai",
        base_url: "https://openrouter.ai/api/v1",
        help_url: "https://openrouter.ai/settings/keys",
    },
    ProviderDef {
        id: "nvidia",
        label: "NVIDIA",
        format: "openai",
        base_url: "https://integrate.api.nvidia.com/v1",
        help_url: "https://build.nvidia.com/",
    },
];

pub fn def(id: &str) -> Option<&'static ProviderDef> {
    PROVIDERS.iter().find(|p| p.id == id)
}

pub fn is_builtin(id: &str) -> bool {
    def(id).is_some()
}

/// A resolved provider configuration: everything the adapters need.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub format: String,
    pub base_url: String,
    pub extra_headers: Vec<(String, String)>,
    /// path used for model discovery, e.g. "/models"
    pub models_path: String,
}

/// Resolve configuration for any provider id: built-ins come from the static
/// registry, `custom-*` ids from the custom_providers table.
pub fn resolve(app: &tauri::AppHandle, id: &str) -> Result<Resolved, String> {
    if let Some(d) = def(id) {
        let config = crate::db::provider_config_get(app, id);
        let models_path = config
            .get("modelsPath")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        return Ok(Resolved {
            format: d.format.to_string(),
            base_url: d.base_url.to_string(),
            extra_headers: vec![],
            models_path,
        });
    }
    let row = crate::db::custom_get(app, id).map_err(|_| format!("Unknown provider: {id}"))?;
    Ok(Resolved {
        format: row.format,
        base_url: row.base_url,
        extra_headers: headers_from_value(&row.headers),
        models_path: row.models_path,
    })
}

pub fn headers_from_value(v: &Value) -> Vec<(String, String)> {
    match v {
        Value::Object(map) => map
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect(),
        _ => vec![],
    }
}

/// Generate a unique custom provider id from a name, e.g. "Qwen" ->
/// "custom-qwen", with a numeric suffix when the slug is taken.
pub fn unique_custom_id(name_slug: &str, existing: &[String]) -> String {
    let base = if name_slug.is_empty() {
        "provider".to_string()
    } else {
        name_slug.to_string()
    };
    let mut candidate = format!("custom-{base}");
    let mut n = 1u32;
    while existing.iter().any(|e| e == &candidate) {
        n += 1;
        candidate = format!("custom-{base}-{n}");
    }
    candidate
}

/// Model IDs that are known to be unusable for chat completions. Applied
/// heuristically when the API does not provide capability metadata.
pub fn looks_non_chat(id: &str) -> bool {
    let l = id.to_lowercase();
    const BAD: [&str; 16] = [
        "embedding",
        "embed-",
        "-embed",
        "whisper",
        "tts",
        "dall-e",
        "dalle",
        "moderation",
        "transcribe",
        "transcription",
        "audio",
        "image-gen",
        "stable-diffusion",
        "sdxl",
        "midjourney",
        "/rerank",
    ];
    BAD.iter().any(|b| l.contains(b))
}

/// Sort and de-duplicate a model id list.
pub fn dedupe_sort(mut ids: Vec<String>) -> Vec<String> {
    ids.retain(|s| !s.trim().is_empty());
    ids.sort();
    ids.dedup();
    ids
}

/// Parse an OpenAI-style `{ "data": [ { "id": ... } ] }` listing. When the API
/// provides output-modality metadata (OpenRouter-style `architecture`), only
/// models that can emit text are kept; otherwise a heuristic filters models
/// that cannot serve chat completions.
pub fn parse_openai_models(v: &Value) -> Vec<String> {
    let mut out = vec![];
    if let Some(arr) = v.get("data").and_then(|d| d.as_array()) {
        for m in arr {
            let Some(id) = m.get("id").and_then(|i| i.as_str()) else {
                continue;
            };
            if !supports_text_output(id, m) {
                continue;
            }
            out.push(id.to_string());
        }
    }
    dedupe_sort(out)
}

/// Whether a single OpenAI-style model entry can emit text. Uses the API's
/// own capability metadata when present (OpenRouter's `architecture`
/// object), falling back to the id heuristic.
fn supports_text_output(id: &str, m: &Value) -> bool {
    let modalities = m
        .pointer("/architecture/output_modalities")
        .or_else(|| m.get("output_modalities"))
        .and_then(|x| x.as_array());
    if let Some(mods) = modalities {
        return mods.iter().any(|x| {
            x.as_str()
                .map(|s| s.eq_ignore_ascii_case("text"))
                .unwrap_or(false)
        });
    }
    !looks_non_chat(id)
}

/// Parse a Gemini `{ "models": [ ... ] }` listing page. Keeps models that
/// support generateContent and strips the "models/" name prefix.
pub fn parse_gemini_models(v: &Value) -> Vec<String> {
    let mut out = vec![];
    if let Some(arr) = v.get("models").and_then(|m| m.as_array()) {
        for m in arr {
            let supported = m
                .get("supportedGenerationMethods")
                .and_then(|s| s.as_array())
                .map(|a| {
                    a.iter().any(|x| {
                        x.as_str()
                            .map(|s| s.contains("generateContent"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(true);
            if !supported {
                continue;
            }
            if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                let name = name.strip_prefix("models/").unwrap_or(name);
                out.push(name.to_string());
            }
        }
    }
    dedupe_sort(out)
}

/// Parse an Anthropic `{ "data": [ { "id": ... } ], "has_more": ..., "last_id": ... }` page.
pub fn parse_anthropic_models(v: &Value) -> Vec<String> {
    let mut out = vec![];
    if let Some(arr) = v.get("data").and_then(|d| d.as_array()) {
        for m in arr {
            if let Some(id) = m.get("id").and_then(|i| i.as_str()) {
                out.push(id.to_string());
            }
        }
    }
    dedupe_sort(out)
}

/// The next page token for a Gemini listing, if any.
pub fn gemini_next_page(v: &Value) -> Option<String> {
    v.get("nextPageToken")
        .and_then(|t| t.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// Whether an Anthropic listing has more pages, and the cursor to continue from.
pub fn anthropic_next_page(v: &Value) -> Option<String> {
    if v.get("has_more").and_then(|b| b.as_bool()).unwrap_or(false) {
        v.get("last_id")
            .and_then(|t| t.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from)
    } else {
        None
    }
}

/// The next OpenAI-style cursor for a listing page, per the OpenAI list-API
/// convention (`has_more: bool` + `last_id` cursor). Providers that never
/// paginate simply omit these fields and discovery stops after one page.
pub fn openai_next_page(v: &Value) -> Option<String> {
    if v.get("has_more").and_then(|b| b.as_bool()).unwrap_or(false) {
        v.get("last_id")
            .and_then(|t| t.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from)
    } else {
        None
    }
}

/// URL for one OpenAI-style listing page. The `after` cursor is only appended
/// when the previous page asked for a continuation.
pub fn openai_page_url(base: &str, path: &str, after: Option<&str>) -> String {
    match after {
        Some(cursor) => format!("{base}{path}?after={}", urlencode(cursor)),
        None => format!("{base}{path}"),
    }
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())
}

fn trim_base(base: &str) -> &str {
    base.trim_end_matches('/')
}

/// Apply provider-appropriate authentication headers.
fn auth(req: reqwest::RequestBuilder, format: &str, key: &str) -> reqwest::RequestBuilder {
    if key.is_empty() {
        return req;
    }
    match format {
        "anthropic" => req
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01"),
        "gemini" => req.header("x-goog-api-key", key),
        _ => req.bearer_auth(key),
    }
}

/// Discover models with pagination where the provider supports it.
/// `models_path` overrides the listing path for custom providers.
pub async fn list_models_http(
    format: &str,
    base_url: &str,
    api_key: Option<&str>,
    extra_headers: &[(String, String)],
    models_path: &str,
) -> Result<Vec<String>, String> {
    let base = trim_base(base_url);
    if base.is_empty() {
        return Err("Base URL is not configured for this provider".into());
    }
    let client = client()?;
    let key = api_key.unwrap_or("");
    let path = {
        let p = models_path.trim();
        if p.is_empty() {
            "/models".to_string()
        } else if p.starts_with('/') {
            p.to_string()
        } else {
            format!("/{p}")
        }
    };

    let mut models: Vec<String> = vec![];
    // Hard caps so a misbehaving endpoint can never loop forever. The client
    // also enforces a 45 s overall request timeout.
    const MAX_PAGES: usize = 20;
    match format {
        // Gemini: paginate with nextPageToken until exhausted.
        "gemini" => {
            let mut token: Option<String> = None;
            let mut seen_tokens: Vec<String> = vec![];
            for _ in 0..MAX_PAGES {
                let mut url = format!("{base}{path}?pageSize=1000");
                if let Some(t) = &token {
                    url.push_str(&format!("&pageToken={}", urlencode(t)));
                }
                let req = auth(client.get(&url), format, key);
                let req = apply_headers(req, extra_headers);
                let v = send_json(req).await?;
                models.extend(parse_gemini_models(&v));
                token = gemini_next_page(&v);
                match token.as_deref() {
                    None => break,
                    Some(t) => {
                        // Repeated cursor means the endpoint ignores paging —
                        // stop instead of fetching the same page forever.
                        if seen_tokens.iter().any(|s| s == t) {
                            break;
                        }
                        seen_tokens.push(t.to_string());
                    }
                }
            }
        }
        // Anthropic: paginate with after_id following has_more/last_id.
        "anthropic" => {
            let mut after: Option<String> = None;
            let mut seen_cursors: Vec<String> = vec![];
            for _ in 0..MAX_PAGES {
                let mut url = format!("{base}{path}?limit=100");
                if let Some(a) = &after {
                    url.push_str(&format!("&after_id={}", urlencode(a)));
                }
                let req = auth(client.get(&url), format, key);
                let req = apply_headers(req, extra_headers);
                let v = send_json(req).await?;
                models.extend(parse_anthropic_models(&v));
                after = anthropic_next_page(&v);
                match after.as_deref() {
                    None => break,
                    Some(a) => {
                        if seen_cursors.iter().any(|s| s == a) {
                            break;
                        }
                        seen_cursors.push(a.to_string());
                    }
                }
            }
        }
        // OpenAI-compatible: one page by default; follow the OpenAI list-API
        // cursor (`has_more` + `last_id`, requested back via `?after=`) when
        // the endpoint actually paginates. Endpoints that never paginate omit
        // those fields, so the first response ends the loop unchanged.
        _ => {
            let mut after: Option<String> = None;
            let mut seen_cursors: Vec<String> = vec![];
            for _ in 0..MAX_PAGES {
                let url = openai_page_url(base, &path, after.as_deref());
                let req = auth(client.get(&url), format, key);
                let req = apply_headers(req, extra_headers);
                let v = send_json(req).await?;
                models.extend(parse_openai_models(&v));
                after = openai_next_page(&v);
                match after.as_deref() {
                    None => break,
                    Some(a) => {
                        if seen_cursors.iter().any(|s| s == a) {
                            break;
                        }
                        seen_cursors.push(a.to_string());
                    }
                }
            }
            models.retain(|id| !looks_non_chat(id));
        }
    }

    // An HTTP success with an empty catalog is a *successful discovery with
    // zero results*, not an error: callers decide the connection verdict.
    Ok(dedupe_sort(models))
}

fn apply_headers(
    mut req: reqwest::RequestBuilder,
    headers: &[(String, String)],
) -> reqwest::RequestBuilder {
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    req
}

async fn send_json(req: reqwest::RequestBuilder) -> Result<Value, String> {
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(crate::llm::format_api_error(status.as_u16(), &text));
    }
    serde_json::from_str(&text).map_err(|_| "Provider returned invalid JSON".to_string())
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Minimal chat-completion ping used when a provider has no usable model
/// listing endpoint. Uses the native chat endpoint for the configured wire
/// format, so Anthropic- and Gemini-compatible endpoints are validated with
/// their real request shape, not the OpenAI one.
pub async fn ping_chat(
    format: &str,
    base_url: &str,
    api_key: &str,
    extra_headers: &[(String, String)],
    model: &str,
) -> Result<(), String> {
    let base = trim_base(base_url);
    if base.is_empty() {
        return Err("Base URL is not configured".into());
    }
    if model.trim().is_empty() {
        return Err("No model configured to test with".into());
    }
    let client = client()?;
    let (url, body) = ping_request(format, base, model.trim());
    let req = auth(client.post(&url).json(&body), format, api_key);
    let req = apply_headers(req, extra_headers);
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(crate::llm::format_api_error(status.as_u16(), &text));
    }
    Ok(())
}

/// URL and payload used by the connection ping for each wire format.
pub fn ping_request(format: &str, base: &str, model: &str) -> (String, Value) {
    match format {
        "anthropic" => (
            format!("{base}/messages"),
            json!({
                "model": model,
                "max_tokens": 1,
                "messages": [{ "role": "user", "content": "ping" }],
            }),
        ),
        "gemini" => (
            format!("{base}/models/{model}:generateContent"),
            json!({
                "contents": [{ "parts": [{ "text": "ping" }] }],
                "generationConfig": { "maxOutputTokens": 1 },
            }),
        ),
        _ => (
            format!("{base}/chat/completions"),
            json!({
                "model": model,
                "messages": [{ "role": "user", "content": "ping" }],
                "max_tokens": 1,
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_has_six_providers_with_help_urls() {
        assert_eq!(PROVIDERS.len(), 6);
        for p in PROVIDERS {
            assert!(p.help_url.starts_with("https://"));
            assert!(p.format == "openai" || p.format == "anthropic" || p.format == "gemini");
        }
        assert!(def("groq").is_some());
        assert!(def("gemini").is_some());
        assert!(def("anthropic").is_some());
        assert!(def("openai").is_some());
        assert!(def("openrouter").is_some());
        assert!(def("nvidia").is_some());
        assert!(def("custom").is_none(), "shared custom id must be gone");
    }

    #[test]
    fn unique_custom_id_dedupes() {
        let existing = vec!["custom-qwen".to_string()];
        assert_eq!(unique_custom_id("qwen", &existing), "custom-qwen-2");
        assert_eq!(unique_custom_id("qwen", &[]), "custom-qwen");
        assert_eq!(unique_custom_id("", &[]), "custom-provider");
        let two = vec!["custom-qwen".to_string(), "custom-qwen-2".to_string()];
        assert_eq!(unique_custom_id("qwen", &two), "custom-qwen-3");
    }

    #[test]
    fn parse_openai_models_dedupes_and_sorts() {
        let v = json!({ "data": [
            { "id": "b-model" }, { "id": "a-model" }, { "id": "b-model" }, { "id": 42 }
        ]});
        assert_eq!(
            parse_openai_models(&v),
            vec!["a-model".to_string(), "b-model".to_string()]
        );
        assert_eq!(parse_openai_models(&json!({})), Vec::<String>::new());
    }

    #[test]
    fn parse_gemini_models_filters_and_strips_prefix() {
        let v = json!({ "models": [
            { "name": "models/gemini-2.0-flash", "supportedGenerationMethods": ["generateContent"] },
            { "name": "models/gemini-2.0-flash-lite", "supportedGenerationMethods": ["generateContent"] },
            { "name": "models/embedding-001", "supportedGenerationMethods": ["embedContent"] },
            { "name": "models/aqa", "supportedGenerationMethods": ["generateAnswer"] }
        ]});
        assert_eq!(
            parse_gemini_models(&v),
            vec![
                "gemini-2.0-flash".to_string(),
                "gemini-2.0-flash-lite".to_string()
            ]
        );
    }

    #[test]
    fn gemini_pagination_token() {
        assert_eq!(
            gemini_next_page(&json!({ "nextPageToken": "abc" })),
            Some("abc".to_string())
        );
        assert_eq!(gemini_next_page(&json!({})), None);
        assert_eq!(gemini_next_page(&json!({ "nextPageToken": "" })), None);
    }

    #[test]
    fn parse_anthropic_models_and_pagination() {
        let v = json!({ "data": [ { "id": "claude-sonnet-4" } ], "has_more": true, "last_id": "claude-sonnet-4" });
        assert_eq!(
            parse_anthropic_models(&v),
            vec!["claude-sonnet-4".to_string()]
        );
        assert_eq!(anthropic_next_page(&v), Some("claude-sonnet-4".to_string()));
        assert_eq!(anthropic_next_page(&json!({ "has_more": false })), None);
        assert_eq!(
            anthropic_next_page(&json!({ "has_more": true })),
            None,
            "no cursor means stop"
        );
    }

    #[test]
    fn non_chat_models_are_flagged() {
        assert!(looks_non_chat("text-embedding-3-small"));
        assert!(looks_non_chat("whisper-1"));
        assert!(looks_non_chat("dall-e-3"));
        assert!(!looks_non_chat("gpt-4.1"));
        assert!(!looks_non_chat("llama-3.3-70b-versatile"));
    }

    #[test]
    fn dedupe_sort_removes_empty_and_duplicates() {
        let ids = vec![
            "".to_string(),
            "b".to_string(),
            "a".to_string(),
            "b".to_string(),
        ];
        assert_eq!(dedupe_sort(ids), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn openai_models_respect_output_modality_metadata() {
        let v = json!({ "data": [
            { "id": "vendor/text-model", "architecture": { "output_modalities": ["text"] } },
            { "id": "vendor/img-model", "architecture": { "output_modalities": ["image"] } },
            { "id": "vendor/no-meta" }
        ]});
        assert_eq!(
            parse_openai_models(&v),
            vec![
                "vendor/no-meta".to_string(),
                "vendor/text-model".to_string()
            ]
        );
        // Top-level output_modalities (non-OpenRouter shape) is honored too.
        let v2 = json!({ "data": [
            { "id": "a", "output_modalities": ["text", "image"] },
            { "id": "b", "output_modalities": ["audio"] }
        ]});
        assert_eq!(parse_openai_models(&v2), vec!["a".to_string()]);
    }

    #[test]
    fn ping_request_matches_each_wire_format() {
        let (url, body) = ping_request("anthropic", "https://api.x.com/v1", "m1");
        assert_eq!(url, "https://api.x.com/v1/messages");
        assert_eq!(body["max_tokens"], json!(1));
        let (url, _) = ping_request("gemini", "https://api.x.com/v1beta", "m2");
        assert_eq!(url, "https://api.x.com/v1beta/models/m2:generateContent");
        let (url, body) = ping_request("openai", "https://api.x.com/v1", "m3");
        assert_eq!(url, "https://api.x.com/v1/chat/completions");
        assert_eq!(body["model"], json!("m3"));
    }

    #[test]
    fn headers_from_value_maps_strings() {
        let v = json!({ "X-Custom": "abc", "X-Num": 5 });
        let h = headers_from_value(&v);
        assert_eq!(h, vec![("X-Custom".to_string(), "abc".to_string())]);
        assert!(headers_from_value(&json!("nope")).is_empty());
    }

    #[test]
    fn urlencode_encodes_specials() {
        assert_eq!(urlencode("a b&c"), "a%20b%26c");
        assert_eq!(urlencode("safe-id.1~x"), "safe-id.1~x");
    }

    // ---------- OpenAI-style pagination ----------

    #[test]
    fn openai_next_page_requires_has_more_and_cursor() {
        assert_eq!(
            openai_next_page(&json!({ "has_more": true, "last_id": "b" })),
            Some("b".to_string())
        );
        // No has_more, or no cursor, or an empty cursor all mean "stop".
        assert_eq!(
            openai_next_page(&json!({ "has_more": false, "last_id": "b" })),
            None
        );
        assert_eq!(
            openai_next_page(&json!({ "has_more": true })),
            None,
            "no cursor means stop"
        );
        assert_eq!(
            openai_next_page(&json!({ "has_more": true, "last_id": "" })),
            None
        );
        assert_eq!(openai_next_page(&json!({})), None);
    }

    #[test]
    fn openai_page_url_appends_cursor_only_when_present() {
        assert_eq!(
            openai_page_url("https://api.x.com/v1", "/models", None),
            "https://api.x.com/v1/models"
        );
        assert_eq!(
            openai_page_url("https://api.x.com/v1", "/models", Some("m-b")),
            "https://api.x.com/v1/models?after=m-b"
        );
        assert_eq!(
            openai_page_url("https://api.x.com/v1", "/models", Some("a b&c")),
            "https://api.x.com/v1/models?after=a%20b%26c"
        );
    }

    // ---------- discovery over a real mock HTTP server ----------

    /// Serve a scripted sequence of (status, body) responses, one per accepted
    /// connection, then stop. Pure std, no extra dependencies.
    fn spawn_mock(responses: Vec<(u16, &'static str)>) -> String {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock");
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for (status, body) in responses {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buf = [0u8; 8192];
                let _ = stream.read(&mut buf);
                let resp = format!(
                    "HTTP/1.1 {status} Status\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn openai_discovery_follows_cursor_and_dedupes_pages() {
        let base = spawn_mock(vec![
            (
                200,
                r#"{"data":[{"id":"m-b"},{"id":"m-a"}],"has_more":true,"last_id":"m-b"}"#,
            ),
            (
                200,
                r#"{"data":[{"id":"m-b"},{"id":"m-c"}],"has_more":false}"#,
            ),
        ]);
        let models = list_models_http("openai", &base, Some("k"), &[], "/models")
            .await
            .unwrap();
        assert_eq!(
            models,
            vec!["m-a".to_string(), "m-b".to_string(), "m-c".to_string()]
        );
    }

    #[tokio::test]
    async fn openai_discovery_stops_without_cursor() {
        // Single-page response: no has_more/last_id, exactly one request.
        let base = spawn_mock(vec![(200, r#"{"data":[{"id":"only"}]}"#)]);
        let models = list_models_http("openai", &base, Some("k"), &[], "/models")
            .await
            .unwrap();
        assert_eq!(models, vec!["only".to_string()]);
    }

    #[tokio::test]
    async fn openai_discovery_empty_final_page_is_ok() {
        let base = spawn_mock(vec![
            (
                200,
                r#"{"data":[{"id":"m-a"}],"has_more":true,"last_id":"m-a"}"#,
            ),
            (200, r#"{"data":[]}"#),
        ]);
        let models = list_models_http("openai", &base, Some("k"), &[], "/models")
            .await
            .unwrap();
        assert_eq!(models, vec!["m-a".to_string()]);
    }

    #[tokio::test]
    async fn discovery_empty_catalog_is_success_not_error() {
        // Authenticated HTTP success with zero models: Ok(vec![]) — the caller
        // decides the verdict (connection status is separate from discovery).
        let base = spawn_mock(vec![(200, r#"{"data":[]}"#)]);
        let models = list_models_http("openai", &base, Some("k"), &[], "/models")
            .await
            .unwrap();
        assert!(models.is_empty());
    }

    #[tokio::test]
    async fn discovery_bad_key_is_a_failure_with_status() {
        let base = spawn_mock(vec![(401, r#"{"error":{"message":"invalid key"}}"#)]);
        let err = list_models_http("openai", &base, Some("bad"), &[], "/models")
            .await
            .unwrap_err();
        assert!(err.starts_with("Provider error 401"), "got: {err}");
        assert!(!err.contains("bad"), "the key itself must not appear");
    }

    #[tokio::test]
    async fn discovery_mid_pagination_error_propagates() {
        let base = spawn_mock(vec![
            (
                200,
                r#"{"data":[{"id":"m-a"}],"has_more":true,"last_id":"m-a"}"#,
            ),
            (429, r#"{"error":{"message":"slow down"}}"#),
        ]);
        let err = list_models_http("openai", &base, Some("k"), &[], "/models")
            .await
            .unwrap_err();
        assert!(err.starts_with("Provider error 429"), "got: {err}");
    }

    #[tokio::test]
    async fn ping_chat_succeeds_on_success_status() {
        let base = spawn_mock(vec![(200, r#"{"id":"resp-1"}"#)]);
        assert!(ping_chat("openai", &base, "k", &[], "m-1").await.is_ok());
    }

    #[tokio::test]
    async fn ping_chat_reports_auth_failure() {
        let base = spawn_mock(vec![(403, r#"{"error":{"message":"forbidden"}}"#)]);
        let err = ping_chat("openai", &base, "k", &[], "m-1")
            .await
            .unwrap_err();
        assert!(err.starts_with("Provider error 403"), "got: {err}");
    }

    #[tokio::test]
    async fn ping_chat_reports_unreachable_endpoint() {
        // Port 1 on loopback is never listening: a clean network failure.
        let err = ping_chat("openai", "http://127.0.0.1:1", "k", &[], "m-1")
            .await
            .unwrap_err();
        assert!(err.starts_with("Network error"), "got: {err}");
    }
}

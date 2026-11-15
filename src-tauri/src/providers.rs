use serde_json::Value;

pub struct ProviderDef {
    pub id: &'static str,
    pub label: &'static str,
    /// wire format: "openai" (OpenAI-compatible), "anthropic", "gemini"
    pub format: &'static str,
    pub base_url: &'static str,
}

pub static PROVIDERS: &[ProviderDef] = &[
    ProviderDef { id: "groq", label: "Groq", format: "openai", base_url: "https://api.groq.com/openai/v1" },
    ProviderDef { id: "gemini", label: "Google Gemini", format: "gemini", base_url: "https://generativelanguage.googleapis.com/v1beta" },
    ProviderDef { id: "anthropic", label: "Anthropic", format: "anthropic", base_url: "https://api.anthropic.com/v1" },
    ProviderDef { id: "openai", label: "OpenAI", format: "openai", base_url: "https://api.openai.com/v1" },
    ProviderDef { id: "openrouter", label: "OpenRouter", format: "openai", base_url: "https://openrouter.ai/api/v1" },
    ProviderDef { id: "nvidia", label: "NVIDIA", format: "openai", base_url: "https://integrate.api.nvidia.com/v1" },
    ProviderDef { id: "custom", label: "Custom", format: "openai", base_url: "" },
];

pub fn def(id: &str) -> Option<&'static ProviderDef> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Resolve the wire format for a provider (custom providers can override).
pub fn format_for(id: &str, config: &Value) -> String {
    if id == "custom" {
        config
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("openai")
            .to_string()
    } else {
        def(id).map(|d| d.format.to_string()).unwrap_or("openai".into())
    }
}

/// Resolve the base URL (custom providers define their own).
pub fn base_url_for(id: &str, config: &Value) -> String {
    if id == "custom" {
        config
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string()
    } else {
        def(id)
            .map(|d| d.base_url.to_string())
            .unwrap_or_default()
    }
}

pub fn extra_headers_for(id: &str, config: &Value) -> Vec<(String, String)> {
    if id != "custom" {
        return vec![];
    }
    match config.get("headers") {
        Some(Value::Object(map)) => map
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect(),
        _ => vec![],
    }
}

pub fn get_api_key(id: &str) -> Result<Option<String>, String> {
    crate::creds::get_key(id)
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())
}

fn trim_base(base: &str) -> &str {
    base.trim_end_matches('/')
}

/// Discover models from the provider. Returns sorted model IDs.
pub async fn list_models_http(
    format: &str,
    base_url: &str,
    api_key: Option<&str>,
    extra_headers: &[(String, String)],
) -> Result<Vec<String>, String> {
    let base = trim_base(base_url);
    if base.is_empty() {
        return Err("Base URL is not configured for this provider".into());
    }
    let client = client()?;
    let url = match format {
        "gemini" => format!("{base}/models"),
        _ => format!("{base}/models"),
    };
    let mut req = client.get(&url);
    if let Some(k) = api_key {
        if !k.is_empty() {
            req = match format {
                "anthropic" => req
                    .header("x-api-key", k)
                    .header("anthropic-version", "2023-06-01"),
                "gemini" => req.header("x-goog-api-key", k),
                _ => req.bearer_auth(k),
            };
        }
    }
    for (k, v) in extra_headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let resp = req.send().await.map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(crate::llm::format_api_error(status.as_u16(), &text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|_| "Provider returned invalid JSON".to_string())?;
    let mut models: Vec<String> = vec![];
    match format {
        "gemini" => {
            if let Some(arr) = v.get("models").and_then(|m| m.as_array()) {
                for m in arr {
                    let supported = m
                        .get("supportedGenerationMethods")
                        .and_then(|s| s.as_array())
                        .map(|a| {
                            a.iter()
                                .any(|x| x.as_str().map(|s| s.contains("generateContent")).unwrap_or(false))
                        })
                        .unwrap_or(true);
                    if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                        let name = name.strip_prefix("models/").unwrap_or(name);
                        if supported {
                            models.push(name.to_string());
                        }
                    }
                }
            }
        }
        _ => {
            if let Some(arr) = v.get("data").and_then(|d| d.as_array()) {
                for m in arr {
                    if let Some(id) = m.get("id").and_then(|i| i.as_str()) {
                        models.push(id.to_string());
                    }
                }
            }
        }
    }
    models.sort();
    Ok(models)
}

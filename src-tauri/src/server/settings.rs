//! Telemetry, configuration, cache and provider connectivity tests.

use super::*;

// Handler 5: Agent Telemetry
pub(super) async fn telemetry_handler(State(state): State<AppState>) -> Json<crate::models::TelemetryStats> {
    let stats = state.db.get_telemetry_stats(state.port);
    Json(stats)
}


// Handler 9: Configuration (LLM Keys & Academic Providers)
pub(super) async fn get_config_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    // Secrets never leave the process: stored credentials become a sentinel the
    // UI can echo back unchanged.
    if !matches!(
        crate::agents::authenticate(&state.db, &headers),
        Ok(crate::agents::Principal::Local | crate::agents::Principal::Admin)
    ) {
        return Json(serde_json::json!({"authentication_required":true}));
    }
    let config = crate::config::sanitize_config(state.db.get_all_config());
    Json(config)
}

pub(super) async fn set_config_handler(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let values = match crate::config::validate_patch(payload) {
        Ok(values) => values,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"success":false,"error":error})),
            )
        }
    };
    if let Err(error) = state.db.set_config_patch(&values) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"success":false,"error":error.to_string()})),
        );
    }
    (StatusCode::OK, Json(serde_json::json!({"success":true})))
}

// Handler 10: Clear Cache
pub(super) async fn clear_cache_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.clear_cache() {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "success": false, "error": error.to_string() })),
        ),
    }
}

pub(super) fn provider_secret_key(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" => Some("openai_api_key"),
        "anthropic" => Some("anthropic_api_key"),
        "gemini" => Some("gemini_api_key"),
        "deepseek" => Some("deepseek_api_key"),
        "groq" => Some("groq_api_key"),
        "perplexity" => Some("perplexity_api_key"),
        _ => None,
    }
}

// Handler 11: Test LLM API Key
pub(super) async fn test_llm_handler(
    State(state): State<AppState>,
    Json(mut payload): Json<TestLlmRequest>,
) -> Json<TestLlmResponse> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .unwrap_or_default();

    let provider = payload.provider.to_lowercase();
    // Blank key means "use the saved credential" — it is never sent to the UI.
    if payload.api_key.is_empty() || payload.api_key == crate::config::KEEP_SECRET {
        payload.api_key = provider_secret_key(&provider)
            .and_then(|key| state.db.get_config(key))
            .unwrap_or_default();
    }
    if payload.api_key.is_empty() {
        return Json(TestLlmResponse {
            success: false,
            latency_ms: 0,
            message: format!("No API key stored for {provider}; save the settings before testing."),
        });
    }

    let started = Instant::now();

    match payload.provider.to_lowercase().as_str() {
        "openai" => {
            let base_url = payload
                .base_url
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            let url = format!("{}/models", base_url.trim_end_matches('/'));
            match client.get(&url).bearer_auth(&payload.api_key).send().await {
                Ok(resp) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    if resp.status().is_success() {
                        Json(TestLlmResponse {
                            success: true,
                            latency_ms: elapsed,
                            message: "OpenAI API Key verified successfully!".to_string(),
                        })
                    } else {
                        Json(TestLlmResponse {
                            success: false,
                            latency_ms: elapsed,
                            message: format!("OpenAI returned status: {}", resp.status()),
                        })
                    }
                }
                Err(e) => Json(TestLlmResponse {
                    success: false,
                    latency_ms: started.elapsed().as_millis() as u64,
                    message: format!("Connection error: {}", e),
                }),
            }
        }
        "anthropic" => {
            let url = "https://api.anthropic.com/v1/messages";
            match client
                .post(url)
                .header("x-api-key", &payload.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&serde_json::json!({
                    "model": "claude-3-haiku-20240307",
                    "max_tokens": 1,
                    "messages": [{"role": "user", "content": "ping"}]
                }))
                .send()
                .await
            {
                Ok(resp) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    if resp.status().is_success() {
                        Json(TestLlmResponse {
                            success: true,
                            latency_ms: elapsed,
                            message: "Anthropic Claude API Key verified successfully!".to_string(),
                        })
                    } else {
                        Json(TestLlmResponse {
                            success: false,
                            latency_ms: elapsed,
                            message: format!("Anthropic status: {}", resp.status()),
                        })
                    }
                }
                Err(e) => Json(TestLlmResponse {
                    success: false,
                    latency_ms: started.elapsed().as_millis() as u64,
                    message: format!("Connection error: {}", e),
                }),
            }
        }
        "gemini" => {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models?key={}",
                payload.api_key
            );
            match client.get(&url).send().await {
                Ok(resp) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    if resp.status().is_success() {
                        Json(TestLlmResponse {
                            success: true,
                            latency_ms: elapsed,
                            message: "Google Gemini API Key verified successfully!".to_string(),
                        })
                    } else {
                        Json(TestLlmResponse {
                            success: false,
                            latency_ms: elapsed,
                            message: format!("Gemini returned status: {}", resp.status()),
                        })
                    }
                }
                Err(e) => Json(TestLlmResponse {
                    success: false,
                    latency_ms: started.elapsed().as_millis() as u64,
                    message: format!("Connection error: {}", e),
                }),
            }
        }
        "deepseek" => {
            let url = "https://api.deepseek.com/models";
            match client.get(url).bearer_auth(&payload.api_key).send().await {
                Ok(resp) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    if resp.status().is_success() {
                        Json(TestLlmResponse {
                            success: true,
                            latency_ms: elapsed,
                            message: "DeepSeek API Key verified successfully!".to_string(),
                        })
                    } else {
                        Json(TestLlmResponse {
                            success: false,
                            latency_ms: elapsed,
                            message: format!("DeepSeek returned status: {}", resp.status()),
                        })
                    }
                }
                Err(e) => Json(TestLlmResponse {
                    success: false,
                    latency_ms: started.elapsed().as_millis() as u64,
                    message: format!("Connection error: {}", e),
                }),
            }
        }
        "groq" => {
            let url = "https://api.groq.com/openai/v1/models";
            match client.get(url).bearer_auth(&payload.api_key).send().await {
                Ok(resp) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    Json(TestLlmResponse {
                        success: resp.status().is_success(),
                        latency_ms: elapsed,
                        message: if resp.status().is_success() {
                            "Groq API key verified successfully!".to_string()
                        } else {
                            format!("Groq returned status: {}", resp.status())
                        },
                    })
                }
                Err(e) => Json(TestLlmResponse {
                    success: false,
                    latency_ms: started.elapsed().as_millis() as u64,
                    message: format!("Connection error: {}", e),
                }),
            }
        }
        "ollama" => {
            let base_url = payload
                .base_url
                .unwrap_or_else(|| "http://localhost:11434".to_string());
            let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
            match client.get(&url).send().await {
                Ok(resp) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    if resp.status().is_success() {
                        let configured = payload
                            .model
                            .as_deref()
                            .map(str::trim)
                            .filter(|model| !model.is_empty());
                        let found = match configured {
                            Some(model) => {
                                resp.json::<serde_json::Value>()
                                    .await
                                    .ok()
                                    .is_some_and(|body| {
                                        body["models"].as_array().is_some_and(|models| {
                                            models.iter().any(|item| {
                                                item["name"].as_str().is_some_and(|name| {
                                                    name == model
                                                        || name.strip_suffix(":latest")
                                                            == Some(model)
                                                })
                                            })
                                        })
                                    })
                            }
                            None => true,
                        };
                        Json(TestLlmResponse {
                            success: found,
                            latency_ms: elapsed,
                            message: if found {
                                configured.map_or_else(
                                    || "Local Ollama server is online and responsive.".to_string(),
                                    |model| {
                                        format!(
                                            "Ollama is online and model '{model}' is installed."
                                        )
                                    },
                                )
                            } else {
                                format!(
                                    "Ollama is online, but model '{}' is not installed.",
                                    configured.unwrap()
                                )
                            },
                        })
                    } else {
                        Json(TestLlmResponse {
                            success: false,
                            latency_ms: elapsed,
                            message: format!("Ollama returned status: {}", resp.status()),
                        })
                    }
                }
                Err(e) => Json(TestLlmResponse {
                    success: false,
                    latency_ms: started.elapsed().as_millis() as u64,
                    message: format!(
                        "Could not connect to Ollama (check if running on port 11434): {}",
                        e
                    ),
                }),
            }
        }
        "perplexity" => {
            let url = "https://api.perplexity.ai/chat/completions";
            match client
                .post(url)
                .bearer_auth(&payload.api_key)
                .json(&serde_json::json!({
                    "model": "sonar",
                    "messages": [{"role": "user", "content": "ping"}],
                    "max_tokens": 5
                }))
                .send()
                .await
            {
                Ok(resp) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    if resp.status().is_success() {
                        Json(TestLlmResponse {
                            success: true,
                            latency_ms: elapsed,
                            message: "Perplexity API Key verified successfully!".to_string(),
                        })
                    } else {
                        Json(TestLlmResponse {
                            success: false,
                            latency_ms: elapsed,
                            message: format!("Perplexity returned status: {}", resp.status()),
                        })
                    }
                }
                Err(e) => Json(TestLlmResponse {
                    success: false,
                    latency_ms: started.elapsed().as_millis() as u64,
                    message: format!("Connection error: {}", e),
                }),
            }
        }
        _ => Json(TestLlmResponse {
            success: false,
            latency_ms: 0,
            message: "Unsupported provider".to_string(),
        }),
    }
}

// Handler 11b: Test SearXNG Instance
pub(super) async fn test_searxng_handler(
    Json(payload): Json<crate::models::TestSearxngRequest>,
) -> Json<crate::models::TestSearxngResponse> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .build()
        .unwrap_or_default();

    let started = Instant::now();
    let clean_base = payload.url.trim_end_matches('/');
    let cat = payload.categories.unwrap_or_else(|| "science".to_string());
    let mut url = format!(
        "{}/search?q=machine+learning&categories={}&format=json",
        clean_base, cat
    );
    if let Some(ref eng) = payload.engines {
        if !eng.trim().is_empty() {
            url.push_str(&format!("&engines={}", urlencoding::encode(eng.trim())));
        }
    }

    match client.get(&url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36")
        .header("Accept", "application/json")
        .send().await {
        Ok(resp) => {
            let elapsed = started.elapsed().as_millis() as u64;
            let status = resp.status();
            if status.is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    let count = json.get("results").and_then(|r| r.as_array()).map(|a| a.len()).unwrap_or(0);
                    Json(crate::models::TestSearxngResponse {
                        success: true,
                        latency_ms: elapsed,
                        number_of_results: count,
                        message: format!("SearXNG connected. Received {} results for category '{}'", count, cat),
                    })
                } else {
                    Json(crate::models::TestSearxngResponse {
                        success: false,
                        latency_ms: elapsed,
                        number_of_results: 0,
                        message: "SearXNG responded but not with JSON (check 'formats: [html, json]' in settings.yml)".to_string(),
                    })
                }
            } else if status.as_u16() == 403 || status.as_u16() == 429 {
                Json(crate::models::TestSearxngResponse {
                    success: false,
                    latency_ms: elapsed,
                    number_of_results: 0,
                    message: format!("The SearXNG server refused the query (HTTP {}). The built-in MetaSearch / Europe PMC still works without SearXNG.", status),
                })
            } else {
                Json(crate::models::TestSearxngResponse {
                    success: false,
                    latency_ms: elapsed,
                    number_of_results: 0,
                    message: format!("SearXNG returned HTTP {}", status),
                })
            }
        }
        Err(e) => Json(crate::models::TestSearxngResponse {
            success: false,
            latency_ms: started.elapsed().as_millis() as u64,
            number_of_results: 0,
            message: format!("Cannot connect to SearXNG: {}", e),
        }),
    }
}

//! v1.4：本地 LLM 桥接 tauri 命令

use provider::local::{discover_all, LocalDiscovery};

#[tauri::command]
pub async fn local_discover(
    ollama_url: Option<String>,
    llamacpp_url: Option<String>,
) -> Result<LocalDiscovery, String> {
    Ok(discover_all(ollama_url.as_deref(), llamacpp_url.as_deref()).await)
}

#[derive(serde::Serialize)]
pub struct LocalModelSummary {
    pub backend: String, // "ollama" | "llamacpp"
    pub id: String,
    pub name: String,
    pub size: Option<u64>,
    pub param_size: Option<String>,
}

/// 把 discovery 结果转成统一模型列表（供前端 UI）
#[tauri::command]
pub fn local_list_models(discovery: LocalDiscovery) -> Vec<LocalModelSummary> {
    let mut out = Vec::new();
    for m in discovery.ollama_models {
        out.push(LocalModelSummary {
            backend: "ollama".to_string(),
            id: format!("ollama:{}", m.name),
            name: m.name.clone(),
            size: m.size,
            param_size: m
                .details
                .as_ref()
                .and_then(|d| d.parameter_size.clone()),
        });
    }
    for m in discovery.llamacpp_models {
        out.push(LocalModelSummary {
            backend: "llamacpp".to_string(),
            id: format!("llamacpp:{}", m.id),
            name: m.id.clone(),
            size: None,
            param_size: None,
        });
    }
    out
}

/// 健康检查（用一次最小的 chat 请求测速）
#[tauri::command]
pub async fn local_ping(
    backend: String,
    model: String,
    base_url: Option<String>,
) -> Result<LocalPingResult, String> {
    use std::time::Instant;
    let url = base_url.unwrap_or_else(|| match backend.as_str() {
        "ollama" => "http://127.0.0.1:11434".to_string(),
        _ => "http://127.0.0.1:8080".to_string(),
    });
    let started = Instant::now();
    let mut req = provider::request::ChatRequest::new(model.clone());
    req.messages = vec![provider::request::ChatMessage::user("ping")];
    req.max_tokens = Some(8);
    req.temperature = Some(0.0);
    req.stream = false;
    let result = match backend.as_str() {
        "ollama" => {
            let info = provider::ollama_info(&model);
            let p = provider::OllamaProvider::new(info, url);
            use provider::Model;
            p.chat(req).await
        }
        "llamacpp" => {
            let info = provider::llama_cpp_info(&model);
            let p = provider::LlamaCppProvider::new(info, url);
            use provider::Model;
            p.chat(req).await
        }
        _ => return Err(format!("unknown backend: {backend}")),
    };
    let elapsed = started.elapsed().as_millis() as u64;
    match result {
        Ok(_) => Ok(LocalPingResult {
            ok: true,
            latency_ms: elapsed,
            error: None,
        }),
        Err(e) => Ok(LocalPingResult {
            ok: false,
            latency_ms: elapsed,
            error: Some(e.to_string()),
        }),
    }
}

#[derive(serde::Serialize)]
pub struct LocalPingResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

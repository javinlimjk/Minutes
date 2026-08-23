use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SummaryResult {
    pub summary: String,
    pub action_items: Vec<String>,
    pub key_decisions: Vec<String>,
}

pub async fn generate_summary_local(
    endpoint: &str,
    model: &str,
    transcript: &str,
    cloud_fallback_url: Option<String>,
    cloud_api_key: Option<String>,
    custom_prompt: Option<String>,
) -> Result<SummaryResult, Box<dyn Error + Send + Sync>> {
    if transcript.trim().is_empty() {
        return Ok(SummaryResult {
            summary: "No audio transcript recorded for this session.".to_string(),
            action_items: vec!["Ensure microphone input permissions are granted.".to_string()],
            key_decisions: vec!["Session completed with zero audio input.".to_string()],
        });
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let prompt = if let Some(cp) = custom_prompt.filter(|p| !p.trim().is_empty()) {
        if cp.contains("{transcript}") {
            cp.replace("{transcript}", transcript)
        } else {
            format!("{}\n\nTRANSCRIPT:\n{}", cp, transcript)
        }
    } else {
        format!(
            "You are an executive scribe that writes precise, factually grounded meeting summaries.\n\
            Extract a clear, professional executive summary, key action items, and major decisions strictly from the speech transcript below.\n\n\
            STRICT FACTUAL ACCURACY DIRECTIVES:\n\
            - ZERO FABRICATION: Include ONLY facts, names, numbers, symptoms, and decisions that are EXPLICITLY STATED in the transcript.\n\
            - Under NO circumstances should you guess, assume, or extrapolate unmentioned facts, people, or details.\n\
            - IMPORTANT: Always write the summary, action items, and key decisions in clear, professional ENGLISH regardless of the input transcript language.\n\n\
            TRANSCRIPT:\n{}\n\n\
            Respond in this EXACT format with no extra text:\n\
            ## Summary\n\
            <3-4 concise, professional executive sentences in English summarizing the key topics strictly from the spoken text>\n\n\
            ## Action Items\n\
            <list each concrete action item in English, one per line starting with '-'. If none, write '- None identified.'>\n\n\
            ## Key Decisions\n\
            <list each decision or agreement in English, one per line starting with '-'. If none, write '- None identified.'>",
            transcript
        )
    };

    let request_body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
        "options": {
            "temperature": 0.0,
            "top_p": 0.1,
            "num_predict": 800
        }
    });

    let url = format!("{}/api/generate", endpoint.trim_end_matches('/'));
    
    // 1. Try local Ollama instance
    match client.post(&url).json(&request_body).send().await {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(res_json) = resp.json::<OllamaGenerateResponse>().await {
                let res = parse_summary_text(&res_json.response, transcript);
                return Ok(res);
            }
        }
        _ => {}
    }

    // 2. Try enterprise Cloud LLM API fallback (OpenAI / Groq / Anthropic / Gemini compatible)
    if let (Some(cloud_url), Some(api_key)) = (cloud_fallback_url, cloud_api_key) {
        if !cloud_url.trim().is_empty() && !api_key.trim().is_empty() {
            let cloud_payload = serde_json::json!({
                "model": "gpt-4o-mini",
                "messages": [
                    { "role": "system", "content": "You are an executive assistant that writes precise meeting summaries." },
                    { "role": "user", "content": prompt }
                ]
            });

            if let Ok(resp) = client.post(&cloud_url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&cloud_payload)
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(val) = resp.json::<serde_json::Value>().await {
                        if let Some(content) = val["choices"][0]["message"]["content"].as_str() {
                            return Ok(parse_summary_text(content, transcript));
                        }
                    }
                }
            }
        }
    }

    // If neither Local Ollama nor Cloud LLM generated a valid summary, return a clear error
    Err(format!(
        "Failed to generate summary: Local AI engine (Ollama at '{}') was unreachable or model '{}' is not loaded. Please ensure Ollama is running with '{}' or configure a Cloud API key in Settings.",
        endpoint, model, model
    ).into())
}

fn parse_summary_text(text: &str, _raw_transcript: &str) -> SummaryResult {
    let mut summary = String::new();
    let mut action_items = Vec::new();
    let mut key_decisions = Vec::new();

    let mut current_section = "";

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.to_lowercase().starts_with("## summary") {
            current_section = "summary";
            continue;
        } else if trimmed.to_lowercase().starts_with("## action items") {
            current_section = "action";
            continue;
        } else if trimmed.to_lowercase().starts_with("## key decisions") {
            current_section = "decisions";
            continue;
        }

        match current_section {
            "summary" => {
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    if !summary.is_empty() {
                        summary.push(' ');
                    }
                    summary.push_str(trimmed);
                }
            }
            "action" if is_bullet_item(trimmed) => {
                let item = clean_bullet_item(trimmed);
                if !item.is_empty() && !item.to_lowercase().starts_with("none identified") {
                    action_items.push(item.to_string());
                }
            }
            "decisions" if is_bullet_item(trimmed) => {
                let item = clean_bullet_item(trimmed);
                if !item.is_empty() && !item.to_lowercase().starts_with("none identified") {
                    key_decisions.push(item.to_string());
                }
            }
            _ => {}
        }
    }

    if summary.is_empty() {
        summary = text.trim().to_string();
    }

    SummaryResult {
        summary,
        action_items,
        key_decisions,
    }
}

fn is_bullet_item(trimmed: &str) -> bool {
    trimmed.starts_with('-') || trimmed.starts_with('*') || (trimmed.len() > 2 && trimmed.chars().next().map(|c| c.is_numeric()).unwrap_or(false))
}

fn clean_bullet_item(trimmed: &str) -> &str {
    trimmed.trim_start_matches(|c: char| c == '-' || c == '*' || c.is_numeric() || c == '.' || c == ' ').trim()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OllamaStatus {
    pub online: bool,
    pub installed_models: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DownloadProgressPayload {
    pub model: String,
    pub status: String,
    pub completed: u64,
    pub total: u64,
    pub percentage: f32,
}

pub async fn check_ollama_status(endpoint: &str) -> OllamaStatus {
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(3)).build() {
        Ok(c) => c,
        Err(_) => return OllamaStatus { online: false, installed_models: vec![] },
    };

    let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                let mut models = Vec::new();
                if let Some(arr) = json["models"].as_array() {
                    for item in arr {
                        if let Some(name) = item["name"].as_str() {
                            models.push(name.to_string());
                        }
                    }
                }
                return OllamaStatus { online: true, installed_models: models };
            }
            OllamaStatus { online: true, installed_models: vec![] }
        }
        _ => OllamaStatus { online: false, installed_models: vec![] },
    }
}

pub async fn pull_ollama_model(
    endpoint: &str,
    model_name: &str,
    window: tauri::Window,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    use futures_util::StreamExt;
    use tauri::Emitter;

    // Ensure daemon is started if possible
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600))
        .build()?;

    let url = format!("{}/api/pull", endpoint.trim_end_matches('/'));
    let payload = serde_json::json!({
        "name": model_name,
        "stream": true
    });
    let res_result = client.post(&url).json(&payload).send().await;
    let res = match res_result {
        Ok(r) => r,
        Err(err) => {
            println!("[llm] HTTP pull error: {}. Attempting fallback via CLI command 'ollama pull {}'...", err, model_name);
            let ollama_bin = if std::path::Path::new("/opt/homebrew/bin/ollama").exists() {
                "/opt/homebrew/bin/ollama"
            } else if std::path::Path::new("/usr/local/bin/ollama").exists() {
                "/usr/local/bin/ollama"
            } else {
                "ollama"
            };

            let _ = window.emit("ollama-download-progress", DownloadProgressPayload {
                model: model_name.to_string(),
                status: "Pulling model via CLI...".to_string(),
                completed: 50,
                total: 100,
                percentage: 50.0,
            });

            let status = std::process::Command::new(ollama_bin)
                .arg("pull")
                .arg(model_name)
                .status();

            match status {
                Ok(s) if s.success() => {
                    let _ = window.emit("ollama-download-progress", DownloadProgressPayload {
                        model: model_name.to_string(),
                        status: "Download complete!".to_string(),
                        completed: 100,
                        total: 100,
                        percentage: 100.0,
                    });
                    return Ok(());
                }
                _ => return Err(Box::new(err)),
            }
        }
    };

    let mut stream = res.bytes_stream();
    while let Some(chunk_result) = stream.next().await {
        if let Ok(chunk) = chunk_result {
            if let Ok(text) = String::from_utf8(chunk.to_vec()) {
                for line in text.lines() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                        let status_str = json["status"].as_str().unwrap_or("Downloading...").to_string();
                        let completed = json["completed"].as_u64().unwrap_or(0);
                        let total = json["total"].as_u64().unwrap_or(0);
                        let percentage = if total > 0 {
                            (completed as f32 / total as f32) * 100.0
                        } else {
                            0.0
                        };

                        let _ = window.emit("ollama-download-progress", DownloadProgressPayload {
                            model: model_name.to_string(),
                            status: status_str,
                            completed,
                            total,
                            percentage,
                        });
                    }
                }
            }
        }
    }

    Ok(())
}

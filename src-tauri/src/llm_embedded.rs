use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use futures_util::StreamExt;
use crate::db::get_app_data_dir;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BuiltinModelStatus {
    pub is_downloaded: bool,
    pub model_path: Option<String>,
    pub file_size_mb: u64,
    pub recommended_model: String,
}

pub fn get_models_dir() -> PathBuf {
    let dir = get_app_data_dir().join("models");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn get_default_gguf_path() -> PathBuf {
    get_models_dir().join("qwen2.5-1.5b-instruct-q4_k_m.gguf")
}

pub fn check_builtin_model_status() -> BuiltinModelStatus {
    let target_file = get_default_gguf_path();
    let is_downloaded = target_file.exists() && target_file.metadata().map(|m| m.len() > 10_000_000).unwrap_or(false);
    let file_size_mb = if is_downloaded {
        target_file.metadata().map(|m| m.len() / (1024 * 1024)).unwrap_or(0)
    } else {
        0
    };

    BuiltinModelStatus {
        is_downloaded,
        model_path: if is_downloaded { Some(target_file.to_string_lossy().to_string()) } else { None },
        file_size_mb,
        recommended_model: "qwen2.5-1.5b-instruct-q4_k_m.gguf".to_string(),
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DownloadProgressEvent {
    pub model: String,
    pub status: String,
    pub completed: u64,
    pub total: u64,
    pub percentage: f32,
}

pub async fn download_builtin_model(window: tauri::Window) -> Result<String, String> {
    let target_path = get_default_gguf_path();
    let temp_path = get_models_dir().join("qwen2.5-1.5b-instruct-q4_k_m.gguf.download");

    // HuggingFace direct CDN download URL for quantized Qwen 2.5 1.5B Instruct GGUF
    let download_url = "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf";

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let _ = window.emit("builtin-download-progress", DownloadProgressEvent {
        model: "qwen2.5-1.5b-instruct-q4_k_m.gguf".to_string(),
        status: "Connecting to model repository...".to_string(),
        completed: 0,
        total: 100,
        percentage: 0.0,
    });

    let res = client.get(download_url)
        .send()
        .await
        .map_err(|e| format!("Download request failed: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Download server returned HTTP status {}", res.status()));
    }

    let total_size = res.content_length().unwrap_or(986_000_000);
    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();

    use std::io::Write;
    let mut file = std::fs::File::create(&temp_path)
        .map_err(|e| format!("Failed to create local model file: {}", e))?;

    let mut last_emit_percent = -1.0f32;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Stream error during download: {}", e))?;
        file.write_all(&chunk).map_err(|e| format!("File write error: {}", e))?;
        downloaded += chunk.len() as u64;

        let percentage = (downloaded as f32 / total_size as f32) * 100.0;
        if (percentage - last_emit_percent).abs() >= 1.0 || percentage >= 99.9 {
            last_emit_percent = percentage;
            let mb_downloaded = downloaded / (1024 * 1024);
            let mb_total = total_size / (1024 * 1024);
            let _ = window.emit("builtin-download-progress", DownloadProgressEvent {
                model: "qwen2.5-1.5b-instruct-q4_k_m.gguf".to_string(),
                status: format!("Downloading Qwen 2.5 (1.5B): {}MB / {}MB", mb_downloaded, mb_total),
                completed: downloaded,
                total: total_size,
                percentage,
            });
        }
    }

    file.flush().map_err(|e| format!("Flush error: {}", e))?;
    drop(file);

    std::fs::rename(&temp_path, &target_path)
        .map_err(|e| format!("Failed to rename completed model file: {}", e))?;

    let _ = window.emit("builtin-download-progress", DownloadProgressEvent {
        model: "qwen2.5-1.5b-instruct-q4_k_m.gguf".to_string(),
        status: "Model ready for on-device inference!".to_string(),
        completed: total_size,
        total: total_size,
        percentage: 100.0,
    });

    Ok(target_path.to_string_lossy().to_string())
}

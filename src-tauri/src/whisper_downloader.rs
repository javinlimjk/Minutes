use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use futures_util::StreamExt;
use crate::db::get_app_data_dir;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WhisperModelInfo {
    pub name: String,
    pub filename: String,
    pub label: String,
    pub size: String,
    pub desc: String,
    pub is_downloaded: bool,
    pub is_recommended: bool,
    pub file_size_mb: u64,
}

pub fn get_models_dir() -> PathBuf {
    let dir = get_app_data_dir().join("models");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn find_whisper_file(filename: &str) -> Option<PathBuf> {
    let app_models = get_models_dir().join(filename);
    if app_models.exists() && app_models.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false) {
        return Some(app_models);
    }

    let current_dir = std::env::current_dir().unwrap_or_default();
    let exe_dir = std::env::current_exe().unwrap_or_default().parent().unwrap_or(std::path::Path::new("")).to_path_buf();
    let mut candidate_dirs = vec![
        current_dir.clone(),
        current_dir.join("src-tauri"),
        current_dir.join("src-tauri").join("models"),
        current_dir.join("models"),
        exe_dir.clone(),
        exe_dir.join("models"),
        exe_dir.join("..").join("Resources"),
        exe_dir.join("..").join("Resources").join("models"),
        exe_dir.join("..").join("Resources").join("_up_").join("models"),
    ];

    if let Some(home) = dirs_next::home_dir() {
        candidate_dirs.push(home.join("Projects").join("Minutes").join("models"));
        candidate_dirs.push(home.join(".minutes_data").join("models"));
        candidate_dirs.push(home.join("Library").join("Application Support").join("com.minutes.scribe").join("models"));
    }

    for dir in &candidate_dirs {
        let p = dir.join(filename);
        if p.exists() && p.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false) {
            return Some(p);
        }
    }

    None
}

pub fn get_all_whisper_models_status() -> Vec<WhisperModelInfo> {
    let models = vec![
        (
            "small.en",
            "ggml-small.en.bin",
            "Whisper Small (English)",
            "~487 MB",
            "Highest Clinical Accuracy for English Consultations (Recommended)",
            true,
        ),
        (
            "small",
            "ggml-small.bin",
            "Whisper Small (Multilingual)",
            "~488 MB",
            "Bilingual & Multilingual Consultations (English, Chinese, Spanish, etc.)",
            false,
        ),
        (
            "base.en",
            "ggml-base.en.bin",
            "Whisper Base (English)",
            "~148 MB",
            "Fast Lightweight Transcription for Older Hardware",
            false,
        ),
        (
            "tiny.en",
            "ggml-tiny.en.bin",
            "Whisper Tiny (English)",
            "~77 MB",
            "Ultra-Lightweight Minimal Footprint",
            false,
        ),
    ];

    models.into_iter().map(|(name, filename, label, size, desc, is_rec)| {
        let found = find_whisper_file(filename);
        let is_downloaded = found.is_some();
        let file_size_mb = found.and_then(|p| p.metadata().ok()).map(|m| m.len() / (1024 * 1024)).unwrap_or(0);

        WhisperModelInfo {
            name: name.to_string(),
            filename: filename.to_string(),
            label: label.to_string(),
            size: size.to_string(),
            desc: desc.to_string(),
            is_downloaded,
            is_recommended: is_rec,
            file_size_mb,
        }
    }).collect()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WhisperDownloadProgress {
    pub model: String,
    pub status: String,
    pub completed: u64,
    pub total: u64,
    pub percentage: f32,
}

pub async fn download_whisper_model(window: tauri::Window, model_name: &str) -> Result<String, String> {
    let (filename, download_url) = match model_name {
        "small.en" => (
            "ggml-small.en.bin",
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        ),
        "small" => (
            "ggml-small.bin",
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        ),
        "base.en" => (
            "ggml-base.en.bin",
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
        ),
        "tiny.en" => (
            "ggml-tiny.en.bin",
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        ),
        other => return Err(format!("Unknown Whisper model variant: {}", other)),
    };

    let target_path = get_models_dir().join(filename);
    let temp_path = get_models_dir().join(format!("{}.download", filename));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let _ = window.emit("whisper-download-progress", WhisperDownloadProgress {
        model: model_name.to_string(),
        status: "Connecting to Whisper model repository...".to_string(),
        completed: 0,
        total: 100,
        percentage: 0.0,
    });

    let res = client.get(download_url)
        .send()
        .await
        .map_err(|e| format!("Download request failed: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Server returned HTTP status {}", res.status()));
    }

    let total_size = res.content_length().unwrap_or(487_000_000);
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
            let _ = window.emit("whisper-download-progress", WhisperDownloadProgress {
                model: model_name.to_string(),
                status: format!("Downloading Whisper ({}): {}MB / {}MB", model_name, mb_downloaded, mb_total),
                completed: downloaded,
                total: total_size,
                percentage,
            });
        }
    }

    file.flush().map_err(|e| format!("Flush error: {}", e))?;
    drop(file);

    std::fs::rename(&temp_path, &target_path)
        .map_err(|e| format!("Failed to rename downloaded model: {}", e))?;

    let _ = window.emit("whisper-download-progress", WhisperDownloadProgress {
        model: model_name.to_string(),
        status: "Whisper model ready for transcription!".to_string(),
        completed: total_size,
        total: total_size,
        percentage: 100.0,
    });

    Ok(target_path.to_string_lossy().to_string())
}

pub mod audio;
pub mod crypto;
pub mod db;
pub mod diarization;
pub mod llm;
pub mod llm_embedded;
pub mod soap;
pub mod stt;

#[cfg(target_os = "ios")]
pub mod stt_ios;

use std::sync::{Arc, Mutex};
use rusqlite::Connection;
use tauri::State;
#[cfg(not(target_os = "ios"))]
use tauri::Manager;
use db::{Meeting, ModelSettings, TranscriptSegment};
use soap::SOAPNote;
use chrono::Utc;
use uuid::Uuid;

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub audio_engine: Arc<Mutex<audio::AudioEngine>>,
    pub stt_engine: Arc<Mutex<Option<stt::WhisperEngine>>>,
}

#[tauri::command]
fn get_meetings(state: State<'_, AppState>) -> Result<Vec<Meeting>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::get_all_meetings(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_meeting(state: State<'_, AppState>, id: String) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let meetings = db::get_all_meetings(&conn).map_err(|e| e.to_string())?;
    let meeting = meetings.into_iter().find(|m| m.id == id).ok_or("Meeting not found")?;
    let segments = db::get_meeting_segments(&conn, &id).map_err(|e| e.to_string())?;
    let soap_note = db::get_soap_note(&conn, &id).unwrap_or(None);

    Ok(serde_json::json!({
        "meeting": meeting,
        "segments": segments,
        "soap_note": soap_note
    }))
}

#[tauri::command]
fn start_recording(state: State<'_, AppState>, title: String, specialty: Option<String>) -> Result<Meeting, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let spec = specialty.unwrap_or_else(|| "General Practice".to_string());
    let new_meeting = Meeting {
        id: Uuid::new_v4().to_string(),
        title: if title.trim().is_empty() {
            format!("Consultation {}", Utc::now().format("%Y-%m-%d %H:%M"))
        } else {
            title
        },
        created_at: Utc::now().to_rfc3339(),
        duration_seconds: 0,
        status: "recording".to_string(),
        audio_path: None,
        summary: None,
        action_items: None,
        key_decisions: None,
        transcript_count: 0,
        specialty: Some(spec),
    };

    db::insert_meeting(&conn, &new_meeting).map_err(|e| e.to_string())?;

    if let Ok(mut audio) = state.audio_engine.lock() {
        if let Err(e) = audio.start_capture() {
            eprintln!("[start_recording] Audio capture notice: {}", e);
        }
    }

    Ok(new_meeting)
}

#[tauri::command]
fn start_audio_capture(state: State<'_, AppState>) -> Result<(), String> {
    let mut audio = state.audio_engine.lock().map_err(|e| e.to_string())?;
    audio.start_capture().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_live_transcript(state: State<'_, AppState>) -> Result<Vec<stt::WhisperSegment>, String> {
    let (pcm_chunk, total_duration) = {
        let audio = state.audio_engine.lock().map_err(|e| e.to_string())?;
        if !audio.is_recording {
            return Ok(Vec::new());
        }
        let buf = audio.buffer.lock().unwrap_or_else(|e| e.into_inner());
        if buf.is_empty() {
            return Ok(Vec::new());
        }
        let total_dur = buf.len() as f32 / audio.sample_rate;
        let resampled = audio::resample_to_16k(&buf, audio.sample_rate);
        
        let chunk_size = 16000 * 10;
        let chunk = if resampled.len() > chunk_size {
            resampled[resampled.len() - chunk_size..].to_vec()
        } else {
            resampled
        };
        (chunk, total_dur)
    };

    if pcm_chunk.is_empty() {
        return Ok(Vec::new());
    }

    let stt_guard = state.stt_engine.lock().map_err(|e| e.to_string())?;
    if let Some(stt) = stt_guard.as_ref() {
        let mut segs = stt.transcribe_buffer(&pcm_chunk).map_err(|e| e.to_string())?;
        let offset = (total_duration - (pcm_chunk.len() as f32 / 16000.0)).max(0.0);
        for s in &mut segs {
            s.start_timestamp += offset;
            s.end_timestamp += offset;
        }
        Ok(segs)
    } else {
        Ok(Vec::new())
    }
}

#[tauri::command]
async fn stop_audio_capture_and_transcribe(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<TranscriptSegment>, String> {
    let pcm_data = {
        let mut audio = state.audio_engine.lock().map_err(|e| e.to_string())?;
        audio.stop_capture()
    };

    if pcm_data.is_empty() {
        return Ok(Vec::new());
    }

    let wav_filename = format!("audio_{}.wav", meeting_id);
    let wav_path = db::get_recordings_dir().join(&wav_filename);
    if let Err(e) = audio::save_pcm_to_wav(&pcm_data, 16000, &wav_path) {
        eprintln!("[AudioEngine] Notice saving WAV file: {}", e);
    } else if let Ok(conn) = state.db.lock() {
        let _ = conn.execute(
            "UPDATE meetings SET audio_path = ?1 WHERE id = ?2",
            rusqlite::params![wav_path.to_string_lossy().to_string(), meeting_id],
        );
    }

    let segments = {
        let mut stt_guard = state.stt_engine.lock().map_err(|e| e.to_string())?;
        if stt_guard.is_none() {
            let (pref_model, pref_lang) = {
                let conn = state.db.lock().map_err(|e| e.to_string())?;
                let s = db::get_settings(&conn).unwrap_or_default();
                (s.stt_model, s.stt_language)
            };
            let search_dirs = get_all_candidate_model_dirs();
            if let Some(cand) = find_best_model_in_dirs(&search_dirs, Some(&pref_model), Some(&pref_lang)) {
                println!("[lib.rs] Initialized WhisperEngine on demand: '{}', lang: '{}'", cand.display(), pref_lang);
                *stt_guard = Some(stt::WhisperEngine::new(&cand.to_string_lossy(), &pref_lang, None));
            }
        }
        if let Some(stt) = stt_guard.as_ref() {
            stt.transcribe_buffer(&pcm_data).map_err(|e| e.to_string())?
        } else {
            return Ok(Vec::new());
        }
    };

    let mut db_segments: Vec<TranscriptSegment> = segments
        .into_iter()
        .map(|s| TranscriptSegment {
            id: format!("seg_{}_{}", date_now(), Uuid::new_v4().to_string().chars().take(6).collect::<String>()),
            meeting_id: meeting_id.clone(),
            speaker_label: s.speaker,
            start_time: s.start_timestamp as f64,
            end_time: s.end_timestamp as f64,
            text: s.text,
            confidence: Some(0.95),
        })
        .collect();

    let (endpoint, model, cloud_url, cloud_key, should_diarize) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let settings = db::get_settings(&conn).unwrap_or_default();
        (
            settings.ollama_endpoint,
            settings.ollama_model,
            settings.cloud_fallback_url,
            settings.cloud_api_key,
            settings.enable_speaker_diarization,
        )
    };

    if should_diarize {
        if db_segments.len() > 1 && !pcm_data.is_empty() {
            diarization::diarize_transcript_segments(&pcm_data, &mut db_segments);
        }
        diarization::resolve_clinical_roles(&mut db_segments, &endpoint, &model, cloud_url, cloud_key).await;
    }

    if !db_segments.is_empty() {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::insert_transcript_segments(&conn, &meeting_id, &db_segments).map_err(|e| e.to_string())?;
    }

    Ok(db_segments)
}

fn date_now() -> i64 {
    Utc::now().timestamp_millis()
}

#[tauri::command]
fn transcribe_pcm_buffer(
    state: State<'_, AppState>,
    pcm_data: Vec<f32>,
    sample_rate: Option<f32>,
) -> Result<Vec<stt::WhisperSegment>, String> {
    let sr = sample_rate.unwrap_or(16000.0);
    let pcm_16k = audio::resample_to_16k(&pcm_data, sr);

    let mut stt_guard = state.stt_engine.lock().map_err(|e| e.to_string())?;
    if stt_guard.is_none() {
        let (pref_model, pref_lang) = {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            let s = db::get_settings(&conn).unwrap_or_default();
            (s.stt_model, s.stt_language)
        };
        let search_dirs = get_all_candidate_model_dirs();
        if let Some(cand) = find_best_model_in_dirs(&search_dirs, Some(&pref_model), Some(&pref_lang)) {
            println!("[lib.rs] Initialized WhisperEngine on demand: '{}', lang: '{}'", cand.display(), pref_lang);
            *stt_guard = Some(stt::WhisperEngine::new(&cand.to_string_lossy(), &pref_lang, None));
        }
    }
    if let Some(stt) = stt_guard.as_ref() {
        stt.transcribe_buffer(&pcm_16k).map_err(|e| e.to_string())
    } else {
        Err("Whisper STT engine is not initialized. Please ensure a model is installed.".to_string())
    }
}

#[tauri::command]
fn transcribe_file(
    state: State<'_, AppState>,
    file_path: String,
) -> Result<Vec<stt::WhisperSegment>, String> {
    if !std::path::Path::new(&file_path).exists() {
        return Err(format!("File does not exist at path: '{}'", file_path));
    }

    let stt_guard = state.stt_engine.lock().map_err(|e| e.to_string())?;
    if let Some(stt) = stt_guard.as_ref() {
        stt.transcribe_file(&file_path).map_err(|e| e.to_string())
    } else {
        Err("STT engine is not initialized".to_string())
    }
}

fn consolidate_segments(segments: Vec<TranscriptSegment>) -> Vec<TranscriptSegment> {
    let mut consolidated: Vec<TranscriptSegment> = Vec::new();
    for seg in segments {
        let text = seg.text.trim().to_string();
        if text.is_empty() { continue; }

        if let Some(last) = consolidated.last_mut() {
            let last_text = last.text.trim().to_string();
            let gap = seg.start_time - last.end_time;
            let same_speaker = last.speaker_label == seg.speaker_label;

            if same_speaker && gap <= 5.0 && (last_text.len() + text.len()) < 600 {
                let last_lower = last_text.to_lowercase();
                let seg_lower = text.to_lowercase();

                if last_lower == seg_lower {
                    continue;
                }
                if !last_lower.contains(&seg_lower) {
                    last.text = format!("{} {}", last_text, text);
                    last.end_time = last.end_time.max(seg.end_time);
                }
                continue;
            }
        }
        consolidated.push(seg);
    }
    consolidated
}

#[tauri::command]
fn save_meeting_segments(
    state: State<'_, AppState>,
    meeting_id: String,
    duration_seconds: i64,
    segments: Vec<TranscriptSegment>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let consolidated = consolidate_segments(segments);

    conn.execute(
        "DELETE FROM transcript_segments WHERE meeting_id = ?1",
        rusqlite::params![meeting_id],
    ).map_err(|e| e.to_string())?;

    if !consolidated.is_empty() {
        db::insert_transcript_segments(&conn, &meeting_id, &consolidated).map_err(|e| e.to_string())?;
    }
    conn.execute(
        "UPDATE meetings SET duration_seconds = ?1, status = 'completed' WHERE id = ?2",
        rusqlite::params![duration_seconds, meeting_id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn update_speaker_label(
    state: State<'_, AppState>,
    meeting_id: String,
    segment_id: String,
    new_label: String,
) -> Result<(), String> {
    let _ = meeting_id;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::update_speaker_label(&conn, &segment_id, &new_label).map_err(|e| e.to_string())
}

#[tauri::command]
fn stop_recording(state: State<'_, AppState>, meeting_id: String) -> Result<Meeting, String> {
    if let Ok(mut audio) = state.audio_engine.lock() {
        let pcm_data = audio.stop_capture();
        if !pcm_data.is_empty() {
            let wav_filename = format!("audio_{}.wav", meeting_id);
            let wav_path = db::get_recordings_dir().join(&wav_filename);
            if audio::save_pcm_to_wav(&pcm_data, 16000, &wav_path).is_ok() {
                if let Ok(conn) = state.db.lock() {
                    let _ = conn.execute(
                        "UPDATE meetings SET audio_path = ?1 WHERE id = ?2",
                        rusqlite::params![wav_path.to_string_lossy().to_string(), meeting_id],
                    );
                }
            }

            if let Ok(stt_guard) = state.stt_engine.lock() {
                if let Some(stt) = stt_guard.as_ref() {
                    if let Ok(segs) = stt.transcribe_buffer(&pcm_data) {
                        let db_segs: Vec<TranscriptSegment> = segs
                            .into_iter()
                            .map(|s| TranscriptSegment {
                                id: format!("seg_{}", Uuid::new_v4()),
                                meeting_id: meeting_id.clone(),
                                speaker_label: s.speaker,
                                start_time: s.start_timestamp as f64,
                                end_time: s.end_timestamp as f64,
                                text: s.text,
                                confidence: Some(0.95),
                            })
                            .collect();
                        if let Ok(conn) = state.db.lock() {
                            let _ = db::insert_transcript_segments(&conn, &meeting_id, &db_segs);
                        }
                    }
                }
            }
        }
    }

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let meetings = db::get_all_meetings(&conn).map_err(|e| e.to_string())?;
    let meeting = meetings.into_iter().find(|m| m.id == meeting_id).ok_or("Meeting not found")?;
    Ok(meeting)
}

#[tauri::command]
fn delete_meeting(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::delete_meeting(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn generate_soap_note(
    state: State<'_, AppState>,
    meeting_id: String,
    specialty: Option<String>,
) -> Result<SOAPNote, String> {
    let (endpoint, model, spec, cloud_url, cloud_key, segments) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let settings = db::get_settings(&conn).unwrap_or_default();
        let segments = db::get_meeting_segments(&conn, &meeting_id).map_err(|e| e.to_string())?;
        let chosen_specialty = specialty.unwrap_or(settings.default_specialty);

        (
            settings.ollama_endpoint,
            settings.ollama_model,
            chosen_specialty,
            settings.cloud_fallback_url,
            settings.cloud_api_key,
            segments,
        )
    };

    let transcript_text = segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let note = soap::generate_soap_note_local(
        &endpoint,
        &model,
        &spec,
        &transcript_text,
        cloud_url,
        cloud_key,
    )
    .await
    .map_err(|e| e.to_string())?;

    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let _ = db::save_soap_note(&conn, &meeting_id, &note);
    }

    Ok(note)
}

#[tauri::command]
async fn generate_soap_note_direct(
    state: State<'_, AppState>,
    transcript: String,
    specialty: Option<String>,
) -> Result<SOAPNote, String> {
    let (endpoint, model, cloud_url, cloud_key, spec) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let settings = db::get_settings(&conn).ok();
        let cloud_url = settings.as_ref().and_then(|s| s.cloud_fallback_url.clone());
        let cloud_key = settings.as_ref().and_then(|s| s.cloud_api_key.clone());
        let endpoint = settings.as_ref().map(|s| s.ollama_endpoint.clone()).unwrap_or_else(|| "http://localhost:11434".to_string());
        let model = settings.as_ref().map(|s| s.ollama_model.clone()).unwrap_or_else(|| "qwen2.5:7b".to_string());
        let spec = specialty
            .or_else(|| settings.as_ref().map(|s| s.default_specialty.clone()))
            .unwrap_or_else(|| "General Practice".to_string());
        (endpoint, model, cloud_url, cloud_key, spec)
    };

    soap::generate_soap_note_local(
        &endpoint,
        &model,
        &spec,
        &transcript,
        cloud_url,
        cloud_key,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_soap_note(state: State<'_, AppState>, meeting_id: String) -> Result<Option<SOAPNote>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::get_soap_note(&conn, &meeting_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_always_on_top(window: tauri::Window, always_on_top: bool) -> Result<(), String> {
    window.set_always_on_top(always_on_top).map_err(|e| e.to_string())
}

#[tauri::command]
async fn generate_summary(state: State<'_, AppState>, meeting_id: String) -> Result<llm::SummaryResult, String> {
    let (endpoint, model, segments, cloud_url, cloud_key, custom_prompt) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let segments = db::get_meeting_segments(&conn, &meeting_id).map_err(|e| e.to_string())?;
        let settings = db::get_settings(&conn).ok();
        let cloud_url = settings.as_ref().and_then(|s| s.cloud_fallback_url.clone());
        let cloud_key = settings.as_ref().and_then(|s| s.cloud_api_key.clone());
        let custom_prompt = settings.as_ref().and_then(|s| s.custom_summary_prompt.clone());
        let endpoint = settings.as_ref().map(|s| s.ollama_endpoint.clone()).unwrap_or_else(|| "http://localhost:11434".to_string());
        let model = settings.as_ref().map(|s| s.ollama_model.clone()).unwrap_or_else(|| "qwen2.5:7b".to_string());
        (
            endpoint,
            model,
            segments,
            cloud_url,
            cloud_key,
            custom_prompt,
        )
    };

    if segments.is_empty() {
        let empty_result = llm::SummaryResult {
            summary: "No speech captured during this session.".to_string(),
            action_items: vec!["Ensure microphone input permissions are granted.".to_string()],
            key_decisions: vec!["Session saved in local encrypted database.".to_string()],
        };

        {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            db::update_meeting_summary(
                &conn,
                &meeting_id,
                &empty_result.summary,
                &empty_result.action_items,
                &empty_result.key_decisions,
            ).map_err(|e| e.to_string())?;
        }

        return Ok(empty_result);
    }

    let transcript_text = segments
        .iter()
        .map(|s| format!("{}: {}", s.speaker_label, s.text))
        .collect::<Vec<_>>()
        .join("\n");

    let summary_res = llm::generate_summary_local(&endpoint, &model, &transcript_text, cloud_url, cloud_key, custom_prompt)
        .await
        .map_err(|e| e.to_string())?;

    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::update_meeting_summary(&conn, &meeting_id, &summary_res.summary, &summary_res.action_items, &summary_res.key_decisions)
            .map_err(|e| e.to_string())?;
    }

    Ok(summary_res)
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<ModelSettings, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::get_settings(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: ModelSettings) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::update_settings(&conn, &settings).map_err(|e| e.to_string())?;

    if let Ok(mut stt_guard) = state.stt_engine.lock() {
        let lang = if settings.stt_language.is_empty() { "auto".to_string() } else { settings.stt_language.clone() };
        let initial_prompt = settings.custom_vocabulary.clone();
        let search_dirs = get_all_candidate_model_dirs();

        if let Some(cand) = find_best_model_in_dirs(&search_dirs, Some(&settings.stt_model), Some(&lang)) {
            let current_path = stt_guard.as_ref().map(|s| s.model_path.clone()).unwrap_or_default();
            let cand_str = cand.to_string_lossy().to_string();
            if current_path != cand_str || stt_guard.is_none() {
                println!("[lib.rs] Switching WhisperEngine to model: '{}', lang: '{}'", cand.display(), lang);
                *stt_guard = Some(stt::WhisperEngine::new(&cand_str, &lang, initial_prompt));
            } else if let Some(stt) = stt_guard.as_mut() {
                stt.language = lang;
                stt.initial_prompt = initial_prompt;
            }
        }
    }

    Ok(())
}

#[tauri::command]
fn request_microphone_permission() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        audio::init_macos_audio_permission()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}

#[cfg(not(target_os = "ios"))]
fn get_all_candidate_model_dirs() -> Vec<std::path::PathBuf> {
    let mut search_dirs = Vec::new();
    let current_dir = std::env::current_dir().unwrap_or_default();
    let exe_dir = std::env::current_exe().unwrap_or_default().parent().unwrap_or(std::path::Path::new("")).to_path_buf();
    let app_data_dir = db::get_app_data_dir();

    search_dirs.push(app_data_dir.clone());
    search_dirs.push(app_data_dir.join("models"));
    search_dirs.push(current_dir.clone());
    search_dirs.push(current_dir.join("models"));
    search_dirs.push(current_dir.join("src-tauri").join("models"));
    search_dirs.push(exe_dir.clone());
    search_dirs.push(exe_dir.join("models"));
    search_dirs.push(exe_dir.join("..").join("Resources"));
    search_dirs.push(exe_dir.join("..").join("Resources").join("models"));
    search_dirs.push(exe_dir.join("..").join("Resources").join("_up_").join("models"));

    if let Some(home) = dirs_next::home_dir() {
        search_dirs.push(home.join("Projects").join("Minutes").join("models"));
        search_dirs.push(home.join(".minutes_data").join("models"));
        search_dirs.push(home.join("Library").join("Application Support").join("com.minutes.scribe").join("models"));
    }

    search_dirs
}

#[cfg(not(target_os = "ios"))]
fn find_best_model_in_dirs(
    dirs: &[std::path::PathBuf],
    preferred_model: Option<&str>,
    preferred_lang: Option<&str>,
) -> Option<std::path::PathBuf> {
    let mut model_names: Vec<&str> = Vec::new();

    if let Some(pref) = preferred_model {
        let pref_file = match pref {
            "small" => "ggml-small.bin",
            "small.en" => "ggml-small.en.bin",
            "base" => "ggml-base.bin",
            "base.en" => "ggml-base.en.bin",
            "tiny" => "ggml-tiny.bin",
            "tiny.en" => "ggml-tiny.en.bin",
            "medium" => "ggml-medium.bin",
            "medium.en" => "ggml-medium.en.bin",
            "large" | "large-v3" => "ggml-large-v3.bin",
            "large-v3-turbo" => "ggml-large-v3-turbo.bin",
            other => other,
        };
        model_names.push(pref_file);
    }

    let is_multilingual_requested = preferred_lang
        .map(|l| {
            let low = l.trim().to_lowercase();
            low != "en" && low != "english"
        })
        .unwrap_or(true);

    if is_multilingual_requested {
        model_names.extend_from_slice(&[
            "ggml-small.bin",
            "ggml-medium.bin",
            "ggml-large-v3.bin",
            "ggml-large-v3-turbo.bin",
            "ggml-base.bin",
            "ggml-tiny.bin",
            "ggml-small.en.bin",
            "ggml-medium.en.bin",
            "ggml-base.en.bin",
            "ggml-tiny.en.bin",
        ]);
    } else {
        model_names.extend_from_slice(&[
            "ggml-small.en.bin",
            "ggml-small.bin",
            "ggml-medium.en.bin",
            "ggml-medium.bin",
            "ggml-base.en.bin",
            "ggml-base.bin",
            "ggml-tiny.en.bin",
            "ggml-tiny.bin",
        ]);
    }

    for name in &model_names {
        for dir in dirs {
            let p1 = dir.join(name);
            if p1.exists() {
                return Some(p1);
            }
            let p2 = dir.join("models").join(name);
            if p2.exists() {
                return Some(p2);
            }
            let p3 = dir.join("_up_").join("models").join(name);
            if p3.exists() {
                return Some(p3);
            }
            let p4 = dir.join("..").join("Resources").join(name);
            if p4.exists() {
                return Some(p4);
            }
            let p5 = dir.join("..").join("Resources").join("models").join(name);
            if p5.exists() {
                return Some(p5);
            }
            let p6 = dir.join("..").join("Resources").join("_up_").join("models").join(name);
            if p6.exists() {
                return Some(p6);
            }
        }
    }
    None
}

#[tauri::command]
async fn check_ollama_status(state: State<'_, AppState>) -> Result<llm::OllamaStatus, String> {
    let endpoint = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let settings = db::get_settings(&conn).unwrap_or_default();
        settings.ollama_endpoint
    };
    Ok(llm::check_ollama_status(&endpoint).await)
}

#[tauri::command]
async fn pull_local_model(window: tauri::Window, state: State<'_, AppState>, model_name: String) -> Result<(), String> {
    let endpoint = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let settings = db::get_settings(&conn).unwrap_or_default();
        settings.ollama_endpoint
    };
    llm::pull_ollama_model(&endpoint, &model_name, window)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn check_builtin_model_status() -> Result<llm_embedded::BuiltinModelStatus, String> {
    Ok(llm_embedded::check_builtin_model_status())
}

#[tauri::command]
async fn download_builtin_model(window: tauri::Window) -> Result<String, String> {
    llm_embedded::download_builtin_model(window).await
}

#[tauri::command]
async fn diarize_meeting(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<TranscriptSegment>, String> {
    let (mut segments, audio_path, endpoint, model, cloud_url, cloud_key) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let segs = db::get_meeting_segments(&conn, &meeting_id).map_err(|e| e.to_string())?;
        let meetings = db::get_all_meetings(&conn).map_err(|e| e.to_string())?;
        let meeting = meetings.into_iter().find(|m| m.id == meeting_id).ok_or("Meeting not found")?;
        let settings = db::get_settings(&conn).unwrap_or_default();
        (
            segs,
            meeting.audio_path,
            settings.ollama_endpoint,
            settings.ollama_model,
            settings.cloud_fallback_url,
            settings.cloud_api_key,
        )
    };

    let mut pcm_data = Vec::new();
    if let Some(path_str) = audio_path {
        let path = std::path::Path::new(&path_str);
        if path.exists() {
            if let Ok(pcm) = audio::read_wav_to_16k_pcm(path) {
                pcm_data = pcm;
            }
        }
    }

    // If segments are empty but we have the audio PCM, transcribe it now!
    if segments.is_empty() && !pcm_data.is_empty() {
        let raw_segs = {
            let mut stt_guard = state.stt_engine.lock().map_err(|e| e.to_string())?;
            if stt_guard.is_none() {
                let (pref_model, pref_lang) = {
                    let conn = state.db.lock().map_err(|e| e.to_string())?;
                    let s = db::get_settings(&conn).unwrap_or_default();
                    (s.stt_model, s.stt_language)
                };
                let search_dirs = get_all_candidate_model_dirs();
                if let Some(cand) = find_best_model_in_dirs(&search_dirs, Some(&pref_model), Some(&pref_lang)) {
                    println!("[lib.rs] Initialized WhisperEngine on demand for diarization: '{}', lang: '{}'", cand.display(), pref_lang);
                    *stt_guard = Some(stt::WhisperEngine::new(&cand.to_string_lossy(), &pref_lang, None));
                }
            }
            if let Some(stt) = stt_guard.as_ref() {
                stt.transcribe_buffer(&pcm_data).map_err(|e| e.to_string())?
            } else {
                return Err("Whisper STT engine model not found. Please ensure a model exists.".to_string());
            }
        };

        segments = raw_segs
            .into_iter()
            .map(|s| TranscriptSegment {
                id: format!("seg_{}_{}", date_now(), Uuid::new_v4().to_string().chars().take(6).collect::<String>()),
                meeting_id: meeting_id.clone(),
                speaker_label: s.speaker,
                start_time: s.start_timestamp as f64,
                end_time: s.end_timestamp as f64,
                text: s.text,
                confidence: Some(0.95),
            })
            .collect();

        if !segments.is_empty() {
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            let _ = db::insert_transcript_segments(&conn, &meeting_id, &segments);
        }
    }

    if segments.is_empty() {
        return Ok(Vec::new());
    }

    if !pcm_data.is_empty() {
        diarization::diarize_transcript_segments(&pcm_data, &mut segments);
    } else {
        for (i, seg) in segments.iter_mut().enumerate() {
            seg.speaker_label = if i % 2 == 0 { "Speaker 1".to_string() } else { "Speaker 2".to_string() };
        }
    }

    diarization::resolve_clinical_roles(&mut segments, &endpoint, &model, cloud_url.clone(), cloud_key.clone()).await;

    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        for seg in &segments {
            let _ = db::update_speaker_label(&conn, &seg.id, &seg.speaker_label);
        }
    }

    // Auto-generate or refresh summary and SOAP note if needed
    let transcript_text = segments.iter().map(|s| format!("{}: {}", s.speaker_label, s.text)).collect::<Vec<_>>().join("\n");
    if !transcript_text.trim().is_empty() {
        if let Ok(sum_res) = llm::generate_summary_local(&endpoint, &model, &transcript_text, cloud_url.clone(), cloud_key.clone(), None).await {
            if let Ok(conn) = state.db.lock() {
                let _ = db::update_meeting_summary(&conn, &meeting_id, &sum_res.summary, &sum_res.action_items, &sum_res.key_decisions);
            }
        }
    }

    Ok(segments)
}

#[tauri::command]
fn get_meeting_audio_bytes(state: State<'_, AppState>, meeting_id: String) -> Result<Vec<u8>, String> {
    let audio_path = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let meetings = db::get_all_meetings(&conn).map_err(|e| e.to_string())?;
        let meeting = meetings.into_iter().find(|m| m.id == meeting_id);
        meeting.and_then(|m| m.audio_path)
    };

    if let Some(ref path_str) = audio_path {
        let path = std::path::Path::new(path_str);
        if path.exists() {
            if let Ok(bytes) = std::fs::read(path) {
                return Ok(bytes);
            }
        }
    }

    // Fallback 1: persistent recordings dir audio_<meeting_id>.wav
    let app_wav = db::get_recordings_dir().join(format!("audio_{}.wav", meeting_id));
    if app_wav.exists() {
        if let Ok(bytes) = std::fs::read(&app_wav) {
            return Ok(bytes);
        }
    }

    // Fallback 2: legacy temp dir audio_<meeting_id>.wav
    let temp_wav = std::env::temp_dir().join(format!("audio_{}.wav", meeting_id));
    if temp_wav.exists() {
        if let Ok(bytes) = std::fs::read(&temp_wav) {
            return Ok(bytes);
        }
    }

    Err(format!("Audio file not found for meeting {}", meeting_id))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn import_audio_pcm_and_diarize(
    state: State<'_, AppState>,
    meeting_id: String,
    title: String,
    duration_seconds: i64,
    pcm_data: Vec<f32>,
    sample_rate: f32,
    specialty: Option<String>,
    enable_diarization: Option<bool>,
) -> Result<Meeting, String> {
    let pcm_16k = audio::resample_to_16k(&pcm_data, sample_rate);
    let spec = specialty.unwrap_or_else(|| "General Practice".to_string());

    let wav_filename = format!("audio_{}.wav", meeting_id);
    let wav_path = db::get_recordings_dir().join(&wav_filename);
    let wav_path_str = wav_path.to_string_lossy().to_string();

    if let Err(e) = audio::save_pcm_to_wav(&pcm_16k, 16000, &wav_path) {
        eprintln!("[import_audio_pcm_and_diarize] Notice saving WAV: {}", e);
    }

    let mut meeting = Meeting {
        id: meeting_id.clone(),
        title: if title.trim().is_empty() {
            format!("Consultation {}", Utc::now().format("%Y-%m-%d %H:%M"))
        } else {
            title
        },
        created_at: Utc::now().to_rfc3339(),
        duration_seconds,
        status: "completed".to_string(),
        audio_path: Some(wav_path_str.clone()),
        summary: None,
        action_items: None,
        key_decisions: None,
        transcript_count: 0,
        specialty: Some(spec.clone()),
    };

    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::insert_meeting(&conn, &meeting).map_err(|e| e.to_string())?;
    }

    // 1. Transcribe PCM with Whisper
    let segments = {
        let stt_guard = state.stt_engine.lock().map_err(|e| e.to_string())?;
        if let Some(stt) = stt_guard.as_ref() {
            stt.transcribe_buffer(&pcm_16k).map_err(|e| e.to_string())?
        } else {
            Vec::new()
        }
    };

    let mut db_segments: Vec<TranscriptSegment> = segments
        .into_iter()
        .map(|s| TranscriptSegment {
            id: format!("seg_{}_{}", date_now(), Uuid::new_v4().to_string().chars().take(6).collect::<String>()),
            meeting_id: meeting_id.clone(),
            speaker_label: s.speaker,
            start_time: s.start_timestamp as f64,
            end_time: s.end_timestamp as f64,
            text: s.text,
            confidence: Some(0.95),
        })
        .collect();

    // 2. Perform acoustic diarization ONLY if enabled
    let (endpoint, model, cloud_url, cloud_key, should_diarize) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let settings = db::get_settings(&conn).unwrap_or_default();
        let diarize_flag = enable_diarization.unwrap_or(settings.enable_speaker_diarization);
        (
            settings.ollama_endpoint,
            settings.ollama_model,
            settings.cloud_fallback_url,
            settings.cloud_api_key,
            diarize_flag,
        )
    };

    if should_diarize {
        if db_segments.len() > 1 && !pcm_16k.is_empty() {
            diarization::diarize_transcript_segments(&pcm_16k, &mut db_segments);
        }
        diarization::resolve_clinical_roles(&mut db_segments, &endpoint, &model, cloud_url.clone(), cloud_key.clone()).await;
    } else {
        for s in &mut db_segments {
            s.speaker_label = "Speaker".to_string();
        }
    }

    // 4. Save segments to database
    if !db_segments.is_empty() {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::insert_transcript_segments(&conn, &meeting_id, &db_segments).map_err(|e| e.to_string())?;
    }

    // 5. Generate Summary and SOAP note concurrently for maximum speed
    let transcript_text = db_segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("\n");
    if !transcript_text.trim().is_empty() {
        let summary_fut = llm::generate_summary_local(&endpoint, &model, &transcript_text, cloud_url.clone(), cloud_key.clone(), None);
        let soap_fut = soap::generate_soap_note_local(&endpoint, &model, &spec, &transcript_text, cloud_url, cloud_key);

        let (sum_res, soap_res) = tokio::join!(summary_fut, soap_fut);

        if let Ok(sum) = sum_res {
            meeting.summary = Some(sum.summary.clone());
            meeting.action_items = Some(sum.action_items.clone());
            meeting.key_decisions = Some(sum.key_decisions.clone());
            if let Ok(conn) = state.db.lock() {
                let _ = db::update_meeting_summary(&conn, &meeting_id, &sum.summary, &sum.action_items, &sum.key_decisions);
            }
        }

        if let Ok(soap_note) = soap_res {
            if let Ok(conn) = state.db.lock() {
                let _ = db::save_soap_note(&conn, &meeting_id, &soap_note);
            }
        }
    }

    meeting.transcript_count = db_segments.len();
    Ok(meeting)
}

#[tauri::command]
fn export_file(filename: String, content: String) -> Result<String, String> {
    let downloads_dir = dirs_next::download_dir()
        .or_else(|| dirs_next::home_dir().map(|h| h.join("Downloads")))
        .ok_or_else(|| "Could not locate Downloads directory".to_string())?;

    let clean_name = std::path::Path::new(&filename)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "export.txt".to_string());

    let file_path = downloads_dir.join(&clean_name);
    std::fs::write(&file_path, &content).map_err(|e| e.to_string())?;

    Ok(file_path.to_string_lossy().to_string())
}

#[tauri::command]
fn open_export_pdf(filename: String, html_content: String) -> Result<String, String> {
    let downloads_dir = dirs_next::download_dir()
        .or_else(|| dirs_next::home_dir().map(|h| h.join("Downloads")))
        .ok_or_else(|| "Could not locate Downloads directory".to_string())?;

    let clean_name = std::path::Path::new(&filename)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "export.html".to_string());

    let file_path = downloads_dir.join(&clean_name);
    std::fs::write(&file_path, &html_content).map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&file_path).spawn();
    }

    Ok(file_path.to_string_lossy().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let conn = db::get_db_conn().expect("Failed to initialize SQLite database");

    // Attempt to locate pre-bundled Whisper model file across bundle locations
    #[cfg(not(target_os = "ios"))]
    let stt_engine = {
        let search_dirs = get_all_candidate_model_dirs();
        let saved_settings = db::get_settings(&conn).ok();
        let pref_model = saved_settings.as_ref().map(|s| s.stt_model.clone());
        let lang = saved_settings.as_ref().map(|s| s.stt_language.clone()).unwrap_or_else(|| "auto".to_string());
        let initial_prompt = saved_settings.and_then(|s| s.custom_vocabulary);

        if let Some(cand) = find_best_model_in_dirs(&search_dirs, pref_model.as_deref(), Some(&lang)) {
            println!("[lib.rs] Initializing WhisperEngine with model file: '{}', language: '{}'", cand.display(), lang);
            Some(stt::WhisperEngine::new(&cand.to_string_lossy(), &lang, initial_prompt))
        } else {
            eprintln!("[lib.rs] Notice: No Whisper model file found at any candidate paths.");
            None
        }
    };

    #[cfg(target_os = "ios")]
    let stt_engine = {
        println!("[lib.rs] Initializing native iOS SFSpeechRecognizer STT engine.");
        Some(stt::WhisperEngine::new("", "auto", None))
    };

    let app_state = AppState {
        db: Arc::new(Mutex::new(conn)),
        audio_engine: Arc::new(Mutex::new(audio::AudioEngine::new())),
        stt_engine: Arc::new(Mutex::new(stt_engine)),
    };

    tauri::Builder::default()
        .setup(|_app| {
            #[cfg(not(target_os = "ios"))]
            {
                let state: tauri::State<'_, AppState> = _app.state();
                let need_init = state.stt_engine.lock().map(|g| g.is_none()).unwrap_or(false);
                if need_init {
                    let (pref_model, pref_lang) = {
                        let conn = state.db.lock().map_err(|e| e.to_string())?;
                        let s = db::get_settings(&conn).unwrap_or_default();
                        (s.stt_model, s.stt_language)
                    };
                    let mut search_dirs = get_all_candidate_model_dirs();
                    if let Ok(res_dir) = _app.path().resource_dir() {
                        search_dirs.push(res_dir.clone());
                        search_dirs.push(res_dir.join("_up_").join("models"));
                        search_dirs.push(res_dir.join("models"));
                    }
                    if let Some(target_path) = find_best_model_in_dirs(&search_dirs, Some(&pref_model), Some(&pref_lang)) {
                        if let Ok(mut engine_guard) = state.stt_engine.lock() {
                            println!("[lib.rs] Initializing WhisperEngine from bundle resource path: '{}', lang: '{}'", target_path.display(), pref_lang);
                            *engine_guard = Some(stt::WhisperEngine::new(&target_path.to_string_lossy(), &pref_lang, None));
                        }
                    }
                }
            }
            println!("[lib.rs] App setup completed.");
            Ok(())
        })
        .manage(app_state)
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_meetings,
            get_meeting,
            start_recording,
            start_audio_capture,
            get_live_transcript,
            stop_audio_capture_and_transcribe,
            transcribe_pcm_buffer,
            transcribe_file,
            save_meeting_segments,
            update_speaker_label,
            diarize_meeting,
            stop_recording,
            delete_meeting,
            generate_summary,
            generate_soap_note,
            generate_soap_note_direct,
            get_soap_note,
            toggle_always_on_top,
            get_settings,
            save_settings,
            request_microphone_permission,
            check_ollama_status,
            pull_local_model,
            get_meeting_audio_bytes,
            import_audio_pcm_and_diarize,
            export_file,
            open_export_pdf,
            check_builtin_model_status,
            download_builtin_model
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let state: tauri::State<'_, AppState> = window.state();
                let stt_arc = Arc::clone(&state.stt_engine);
                let audio_arc = Arc::clone(&state.audio_engine);
                { let mut g = stt_arc.lock().unwrap_or_else(|e| e.into_inner()); drop(g.take()); }
                { let mut a = audio_arc.lock().unwrap_or_else(|e| e.into_inner()); a.stop_capture(); }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(target_os = "ios")]
pub use crate::stt_ios::*;

#[cfg(not(target_os = "ios"))]
pub use desktop::*;

#[cfg(not(target_os = "ios"))]
mod desktop {
    use serde::{Deserialize, Serialize};
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct WhisperSegment {
        pub start_timestamp: f32,
        pub end_timestamp: f32,
        pub text: String,
        pub speaker: String,
    }

    // Whisper hallucination tokens emitted on silent/noisy audio — filtered out.
    const HALLUCINATION_TOKENS: &[&str] = &[
        "[blank_audio]", "[BLANK_AUDIO]", "[silence]", "[SILENCE]",
        "[music]", "[MUSIC]", "[noise]", "[NOISE]", "(silence)",
        "thanks for watching", "please subscribe", "thank you for watching",
    ];

    pub struct WhisperEngine {
        pub model_path: String,
        pub language: String,
        pub initial_prompt: Option<String>,
        context: Arc<Mutex<Option<WhisperContext>>>,
    }

    impl WhisperEngine {
        pub fn new(model_path: &str, language: &str, initial_prompt: Option<String>) -> Self {
            unsafe {
                whisper_rs::set_log_callback(None, std::ptr::null_mut());
            }

            let ctx_opt = if Path::new(model_path).exists() {
                let mut params = WhisperContextParameters::default();
                params.use_gpu(true);
                match WhisperContext::new_with_params(model_path, params) {
                    Ok(c) => {
                        println!("[WhisperEngine] Loaded model from '{}' (Metal GPU accelerated)", model_path);
                        Some(c)
                    }
                    Err(e) => {
                        eprintln!("[WhisperEngine] Notice initializing context from '{}': {:?}", model_path, e);
                        // Fallback to CPU if Metal encounters an issue
                        let mut cpu_params = WhisperContextParameters::default();
                        cpu_params.use_gpu(false);
                        WhisperContext::new_with_params(model_path, cpu_params).ok()
                    }
                }
            } else {
                eprintln!("[WhisperEngine] Model not found at '{}'", model_path);
                None
            };

            Self {
                model_path: model_path.to_string(),
                language: if language.is_empty() { "en".to_string() } else { language.to_string() },
                initial_prompt,
                context: Arc::new(Mutex::new(ctx_opt)),
            }
        }

        /// Short model name for UI display, e.g. "base.en" or "tiny.en".
        pub fn model_name(&self) -> String {
            std::path::Path::new(&self.model_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .trim_start_matches("ggml-")
                .to_string()
        }

        pub fn transcribe_buffer(&self, pcm_float32: &[f32]) -> Result<Vec<WhisperSegment>, String> {
            if pcm_float32.is_empty() {
                return Ok(Vec::new());
            }

            let total_samples = pcm_float32.len();
            let total_duration = total_samples as f32 / 16000.0;
            println!(
                "[WhisperEngine] Fast processing audio buffer: {} samples ({:.2}s)",
                total_samples, total_duration
            );

            let ctx_guard = self.context.lock().map_err(|e| format!("Context mutex error: {}", e))?;
            let ctx = ctx_guard.as_ref().ok_or_else(|| format!(
                "Whisper model not loaded. Ensure a valid model exists at '{}'", self.model_path
            ))?;

            // Create single state instance reused across all chunks to prevent GBs of alloc/dealloc overhead
            let mut state = ctx.create_state().map_err(|e| format!("Create state error: {:?}", e))?;

            let optimal_threads = std::thread::available_parallelism()
                .map(|p| (p.get() as i32).clamp(4, 8))
                .unwrap_or(4);

            // If audio is under 35 seconds, transcribe in 1 pass
            if total_duration <= 35.0 {
                return self.transcribe_chunk_with_state(&mut state, pcm_float32, optimal_threads);
            }

            // For long audio (e.g. multi-minute consultation files), process in 30-second sliding windows
            let chunk_samples = 30 * 16000;
            let mut all_segments = Vec::new();

            let mut offset = 0;
            while offset < total_samples {
                let end = (offset + chunk_samples).min(total_samples);
                let chunk = &pcm_float32[offset..end];
                let time_offset = offset as f32 / 16000.0;

                let chunk_rms = (chunk.iter().map(|&x| x * x).sum::<f32>() / chunk.len() as f32).sqrt();
                if chunk_rms >= 0.001 && chunk.len() >= 8000 {
                    if let Ok(segs) = self.transcribe_chunk_with_state(&mut state, chunk, optimal_threads) {
                        for mut s in segs {
                            s.start_timestamp += time_offset;
                            s.end_timestamp += time_offset;
                            all_segments.push(s);
                        }
                    }
                }

                offset += chunk_samples;
            }

            println!("[WhisperEngine] Done processing total {} segment(s).", all_segments.len());
            Ok(all_segments)
        }

        fn transcribe_chunk_with_state(
            &self,
            state: &mut WhisperState,
            pcm_float32: &[f32],
            threads: i32,
        ) -> Result<Vec<WhisperSegment>, String> {
            if pcm_float32.is_empty() {
                return Ok(Vec::new());
            }

            let rms = (pcm_float32.iter().map(|&x| x * x).sum::<f32>() / pcm_float32.len() as f32).sqrt();
            let duration = pcm_float32.len() as f32 / 16000.0;

            if rms < 0.001 || duration < 0.5 {
                return Ok(Vec::new());
            }

            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            let lang_lower = self.language.trim().to_lowercase();
            if lang_lower == "zh" || lang_lower == "chinese" || lang_lower == "mandarin" || lang_lower == "cmn" {
                params.set_language(Some("zh"));
            } else if lang_lower == "yue" || lang_lower == "cantonese" {
                params.set_language(Some("yue"));
            } else if lang_lower == "en" || lang_lower == "english" {
                params.set_language(Some("en"));
            } else if lang_lower == "ms" || lang_lower == "malay" {
                params.set_language(Some("ms"));
            } else if lang_lower == "ta" || lang_lower == "tamil" {
                params.set_language(Some("ta"));
            } else if lang_lower.is_empty() || lang_lower == "auto" || lang_lower == "auto-detect" {
                params.set_language(None);
            } else {
                params.set_language(Some(&self.language));
            }

            if let Some(ref prompt) = self.initial_prompt {
                if !prompt.trim().is_empty() {
                    params.set_initial_prompt(prompt.trim());
                }
            }

            params.set_translate(false);
            params.set_n_threads(threads);
            params.set_suppress_blank(true);
            params.set_suppress_nst(true);
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);

            state.full(params, pcm_float32).map_err(|e| format!("Whisper inference error: {:?}", e))?;
            let mut segments = Self::collect_segments(state);

            // Strictly restrict language recognition to English ('en') and Chinese ('zh') only
            if self.language == "auto" || self.language.is_empty() {
                let lang_id = state.full_lang_id_from_state();
                let detected = whisper_rs::get_lang_str(lang_id).unwrap_or("en");

                if detected != "en" && detected != "zh" {
                    let has_chinese = segments.iter().any(|s| s.text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)));
                    let target_lang = if has_chinese { "zh" } else { "en" };

                    let mut retry_params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
                    retry_params.set_language(Some(target_lang));
                    if let Some(ref prompt) = self.initial_prompt {
                        if !prompt.trim().is_empty() {
                            retry_params.set_initial_prompt(prompt.trim());
                        }
                    }
                    retry_params.set_translate(false);
                    retry_params.set_n_threads(threads);
                    retry_params.set_suppress_blank(true);
                    retry_params.set_suppress_nst(true);
                    retry_params.set_print_special(false);
                    retry_params.set_print_progress(false);
                    retry_params.set_print_realtime(false);
                    retry_params.set_print_timestamps(false);

                    if state.full(retry_params, pcm_float32).is_ok() {
                        segments = Self::collect_segments(state);
                    }
                }
            }

            Ok(segments)
        }

        fn collect_segments(state: &WhisperState) -> Vec<WhisperSegment> {
            let mut segments = Vec::new();

            for seg in state.as_iter() {
                let text = seg.to_str_lossy().unwrap_or_default().trim().to_string();
                if text.is_empty() { continue; }

                let text_lower = text.to_lowercase();
                // Filter bracketed noise tags like [BLANK_AUDIO], [MUSIC], [NOISE]
                if (text.starts_with('[') && text.ends_with(']')) && (text_lower.contains("blank") || text_lower.contains("music") || text_lower.contains("noise") || text_lower.contains("silence") || text_lower.contains("applause") || text_lower.contains("laughter")) {
                    println!("[WhisperEngine] Filtered noise token: '{}'", text);
                    continue;
                }
                if HALLUCINATION_TOKENS.iter().any(|p| text_lower.trim() == p.to_lowercase().as_str()) {
                    println!("[WhisperEngine] Filtered hallucination token: '{}'", text);
                    continue;
                }

                // Clean edge brackets if present but text has actual speech
                let clean_text = text.trim_start_matches(['(', '[', '（'])
                                     .trim_end_matches([')', ']', '）'])
                                     .trim()
                                     .to_string();

                if clean_text.is_empty() { continue; }

                segments.push(WhisperSegment {
                    start_timestamp: seg.start_timestamp() as f32 / 100.0,
                    end_timestamp: seg.end_timestamp() as f32 / 100.0,
                    text: clean_text,
                    speaker: "Speaker".to_string(),
                });
            }
            segments
        }

        pub fn transcribe_file(&self, file_path: &str) -> Result<Vec<WhisperSegment>, String> {
            let path = std::path::Path::new(file_path);
            if !path.exists() {
                return Err(format!("File does not exist: '{}'", file_path));
            }

            let pcm = crate::audio::read_wav_to_16k_pcm(path)?;
            self.transcribe_buffer(&pcm)
        }
    }
}

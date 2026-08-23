use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperSegment {
    pub start_timestamp: f32,
    pub end_timestamp: f32,
    pub text: String,
    pub speaker: String,
}

#[repr(C)]
struct STTResultRaw {
    json_result: *mut std::os::raw::c_char,
    success: bool,
}

extern "C" {
    fn ios_speech_request_authorization() -> bool;
    fn ios_speech_transcribe_file(
        file_path: *const std::os::raw::c_char,
        lang: *const std::os::raw::c_char,
    ) -> STTResultRaw;
    fn ios_speech_transcribe_buffer(
        pcm_data: *const f32,
        sample_count: usize,
        sample_rate: u32,
        lang: *const std::os::raw::c_char,
    ) -> STTResultRaw;
    fn ios_speech_free_result(res: STTResultRaw);
}

pub struct WhisperEngine {
    pub model_path: String,
    pub language: String,
    pub initial_prompt: Option<String>,
}

impl WhisperEngine {
    pub fn new(model_path: &str, language: &str, initial_prompt: Option<String>) -> Self {
        unsafe {
            let granted = ios_speech_request_authorization();
            println!("[stt_ios] Native SFSpeechRecognizer authorization granted: {}", granted);
        }
        Self {
            model_path: model_path.to_string(),
            language: if language.is_empty() { "en".to_string() } else { language.to_string() },
            initial_prompt,
        }
    }

    pub fn transcribe_buffer(&self, pcm_float32: &[f32]) -> Result<Vec<WhisperSegment>, String> {
        if pcm_float32.is_empty() {
            return Ok(Vec::new());
        }

        let rms = (pcm_float32.iter().map(|&x| x * x).sum::<f32>() / pcm_float32.len() as f32).sqrt();
        let duration = pcm_float32.len() as f32 / 16000.0;
        println!(
            "[stt_ios] Processing buffer: {} samples ({:.2}s), RMS: {:.5}",
            pcm_float32.len(),
            duration,
            rms
        );

        if rms < 0.00001 {
            println!("[stt_ios] Complete silence detected. Skipping STT.");
            return Ok(Vec::new());
        }

        let lang_c = std::ffi::CString::new(self.language.as_str()).unwrap_or_default();
        let raw_res = unsafe {
            ios_speech_transcribe_buffer(
                pcm_float32.as_ptr(),
                pcm_float32.len(),
                16000,
                lang_c.as_ptr(),
            )
        };

        parse_raw_result(raw_res)
    }

    pub fn transcribe_file(&self, file_path: &str) -> Result<Vec<WhisperSegment>, String> {
        let file_c = match std::ffi::CString::new(file_path) {
            Ok(c) => c,
            Err(e) => return Err(format!("Invalid file path string: {}", e)),
        };
        let lang_c = std::ffi::CString::new(self.language.as_str()).unwrap_or_default();

        let raw_res = unsafe {
            ios_speech_transcribe_file(file_c.as_ptr(), lang_c.as_ptr())
        };

        parse_raw_result(raw_res)
    }
}

fn parse_raw_result(raw_res: STTResultRaw) -> Result<Vec<WhisperSegment>, String> {
    if raw_res.json_result.is_null() {
        unsafe { ios_speech_free_result(raw_res); }
        return Err("Received null response from iOS speech recognizer".to_string());
    }

    let c_str = unsafe { std::ffi::CStr::from_ptr(raw_res.json_result) };
    let json_str = c_str.to_string_lossy().to_string();
    let success = raw_res.success;

    unsafe { ios_speech_free_result(raw_res); }

    if !success {
        return Err(format!("iOS Speech Recognition error: {}", json_str));
    }

    let segments: Vec<WhisperSegment> = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse speech segments JSON: {}", e))?;

    println!("[stt_ios] Transcription finished: extracted {} segment(s).", segments.len());
    Ok(segments)
}

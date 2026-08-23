use crate::db::TranscriptSegment;
use std::collections::HashMap;

/// Acoustic voiceprint feature vector extracted from speech audio
#[derive(Debug, Clone)]
pub struct Voiceprint {
    pub mean_pitch_hz: f32,
    pub pitch_std_hz: f32,
    pub spectral_centroid: f32,
    pub spectral_rolloff: f32,
    pub zero_crossing_rate: f32,
    pub energy_bands: [f32; 8],
    pub energy_rms: f32,
}

impl Voiceprint {
    /// Convert features to normalized vector for cosine distance computation
    pub fn to_vector(&self) -> Vec<f32> {
        let mut vec = Vec::with_capacity(14);
        // Salient speaker pitch (50 - 400Hz) with 2.5x discriminative weight
        vec.push(((self.mean_pitch_hz - 50.0).max(0.0) / 350.0) * 2.5);
        vec.push((self.pitch_std_hz / 100.0) * 1.5);
        // Spectral centroid (timbre / vocal tract length) with 2.0x weight
        vec.push(((self.spectral_centroid / 4000.0).min(1.0)) * 2.0);
        vec.push(((self.spectral_rolloff / 6000.0).min(1.0)) * 1.5);
        vec.push((self.zero_crossing_rate * 5.0).min(1.0));
        vec.push((self.energy_rms * 5.0).min(1.0));
        for &b in &self.energy_bands {
            vec.push(b.min(1.0) * 1.2);
        }
        vec
    }
}

/// Compute cosine distance between two feature vectors: 1.0 - (A . B) / (|A| * |B|)
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 1.0;
    }

    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-6 {
        return 1.0;
    }

    let sim = (dot / denom).clamp(-1.0, 1.0);
    1.0 - sim
}

/// Extract voiceprint features from a PCM slice (16000 Hz mono)
pub fn extract_voiceprint(samples: &[f32], sample_rate: f32) -> Voiceprint {
    if samples.len() < 320 {
        return Voiceprint {
            mean_pitch_hz: 150.0,
            pitch_std_hz: 0.0,
            spectral_centroid: 1200.0,
            spectral_rolloff: 2500.0,
            zero_crossing_rate: 0.05,
            energy_bands: [0.125; 8],
            energy_rms: 0.01,
        };
    }

    let sr = sample_rate.max(8000.0);
    let frame_size = (sr * 0.03) as usize; // 30ms frame
    let hop_size = (sr * 0.015) as usize; // 15ms hop

    // 1. RMS Energy
    let total_sum_sq: f32 = samples.iter().map(|&x| x * x).sum();
    let energy_rms = (total_sum_sq / samples.len() as f32).sqrt();

    // 2. Zero Crossing Rate
    let mut zcr_count = 0;
    for i in 1..samples.len() {
        if (samples[i] >= 0.0 && samples[i - 1] < 0.0) || (samples[i] < 0.0 && samples[i - 1] >= 0.0) {
            zcr_count += 1;
        }
    }
    let zero_crossing_rate = zcr_count as f32 / samples.len() as f32;

    // 3. Pitch (F0) estimation across frames using Autocorrelation
    let min_lag = (sr / 400.0) as usize; // Max F0 ~ 400 Hz
    let max_lag = (sr / 65.0) as usize;  // Min F0 ~ 65 Hz
    let mut pitch_estimates = Vec::new();

    let mut offset = 0;
    while offset + frame_size <= samples.len() {
        let frame = &samples[offset..offset + frame_size];
        let frame_rms = (frame.iter().map(|&x| x * x).sum::<f32>() / frame.len() as f32).sqrt();

        if frame_rms > 0.005 {
            let max_l = max_lag.min(frame_size / 2);
            let mut corrs = vec![0.0f32; max_l + 1];
            let mut global_max_corr = -1.0f32;
            let mut global_best_lag = 0;

            for lag in min_lag..=max_l {
                let mut corr = 0.0;
                let mut norm1 = 0.0;
                let mut norm2 = 0.0;
                for j in 0..(frame_size - lag) {
                    corr += frame[j] * frame[j + lag];
                    norm1 += frame[j] * frame[j];
                    norm2 += frame[j + lag] * frame[j + lag];
                }
                let norm = (norm1 * norm2).sqrt();
                if norm > 1e-6 {
                    let ncorr = corr / norm;
                    corrs[lag] = ncorr;
                    if ncorr > global_max_corr {
                        global_max_corr = ncorr;
                        global_best_lag = lag;
                    }
                }
            }

            // Find first prominent local maximum (avoids picking subharmonic lag multiples)
            let mut selected_lag = 0;
            for lag in (min_lag + 1)..max_l {
                if corrs[lag] > corrs[lag - 1] && corrs[lag] >= corrs[lag + 1]
                    && corrs[lag] >= 0.38 && (corrs[lag] >= 0.72 * global_max_corr || corrs[lag] >= 0.65) {
                    selected_lag = lag;
                    break;
                }
            }

            if selected_lag == 0 && global_max_corr > 0.38 {
                selected_lag = global_best_lag;
            }

            if selected_lag > 0 {
                let f0 = sr / selected_lag as f32;
                if (65.0..=400.0).contains(&f0) {
                    pitch_estimates.push(f0);
                }
            }
        }
        offset += hop_size;
    }

    let (mean_pitch_hz, pitch_std_hz) = if !pitch_estimates.is_empty() {
        let mean = pitch_estimates.iter().sum::<f32>() / pitch_estimates.len() as f32;
        let var = pitch_estimates.iter().map(|&p| (p - mean) * (p - mean)).sum::<f32>() / pitch_estimates.len() as f32;
        (mean, var.sqrt())
    } else {
        (150.0, 0.0)
    };

    // 4. Spectral Centroid & 8 Energy Bands via Simplified Discrete Cosine Filterbank
    let num_bands = 8;
    let mut band_energies = [0.0f32; 8];
    let mut centroid_num = 0.0f32;
    let mut centroid_den = 0.0f32;
    let fft_size = 256;

    if samples.len() >= fft_size {
        let chunk_step = samples.len() / 8;
        for c in 0..8 {
            let start = c * chunk_step;
            let end = (start + fft_size).min(samples.len());
            let sub = &samples[start..end];

            for k in 0..(fft_size / 2) {
                let freq = (k as f32 * sr) / fft_size as f32;
                let mut re = 0.0;
                let mut im = 0.0;
                for (n, &val) in sub.iter().enumerate().take(fft_size) {
                    let angle = 2.0 * std::f32::consts::PI * (k * n) as f32 / fft_size as f32;
                    re += val * angle.cos();
                    im -= val * angle.sin();
                }
                let mag = (re * re + im * im).sqrt();

                centroid_num += freq * mag;
                centroid_den += mag;

                let band_idx = ((freq / (sr / 2.0)) * num_bands as f32).floor() as usize;
                if band_idx < num_bands {
                    band_energies[band_idx] += mag;
                }
            }
        }
    }

    let spectral_centroid = if centroid_den > 1e-6 {
        centroid_num / centroid_den
    } else {
        1500.0
    };

    let spectral_rolloff = (spectral_centroid * 1.5).min(sr / 2.0);

    // Normalize band energies to sum to 1.0
    let total_band: f32 = band_energies.iter().sum();
    if total_band > 1e-6 {
        for b in &mut band_energies {
            *b /= total_band;
        }
    } else {
        band_energies = [1.0 / 8.0; 8];
    }

    Voiceprint {
        mean_pitch_hz,
        pitch_std_hz,
        spectral_centroid,
        spectral_rolloff,
        zero_crossing_rate,
        energy_bands: band_energies,
        energy_rms,
    }
}

/// Diarize a set of transcript segments given full recording PCM data (16kHz mono)
pub fn diarize_transcript_segments(
    pcm_16k: &[f32],
    segments: &mut [TranscriptSegment],
) {
    if segments.is_empty() {
        return;
    }

    if segments.len() == 1 {
        let is_pat = is_patient_utterance(&segments[0].text);
        segments[0].speaker_label = if is_pat { "Patient".to_string() } else { "Doctor".to_string() };
        return;
    }

    let sr = 16000.0;
    let total_samples = pcm_16k.len();

    // 1. Extract voiceprints for each segment
    let mut segment_vectors: Vec<Vec<f32>> = Vec::with_capacity(segments.len());

    for seg in segments.iter() {
        let start_sample = ((seg.start_time * sr as f64).round() as usize).min(total_samples);
        let end_sample = ((seg.end_time * sr as f64).round() as usize).min(total_samples);

        let slice = if end_sample > start_sample && (end_sample - start_sample) >= 320 {
            &pcm_16k[start_sample..end_sample]
        } else if total_samples > 0 {
            &pcm_16k[0..total_samples.min(16000)]
        } else {
            &[]
        };

        let vp = extract_voiceprint(slice, sr);
        segment_vectors.push(vp.to_vector());
    }

    // 2. Perform 2-cluster k-means / Agglomerative clustering with temporal continuity
    let n = segment_vectors.len();
    if n == 0 {
        return;
    }

    let mut best_i = 0;
    let mut best_j = 0;
    let mut max_dist = -1.0f32;

    for i in 0..n {
        for j in (i + 1)..n {
            let d = cosine_distance(&segment_vectors[i], &segment_vectors[j]);
            if d > max_dist {
                max_dist = d;
                best_i = i;
                best_j = j;
            }
        }
    }

    let mut cluster_assignments = vec![0usize; n];

    // If acoustic distance is strong enough, use K-means centroids
    if max_dist >= 0.025 {
        let mut centroid_0 = segment_vectors[best_i].clone();
        let mut centroid_1 = segment_vectors[best_j].clone();

        for _iter in 0..6 {
            let mut count_0 = 0;
            let mut count_1 = 0;
            let mut sum_0 = vec![0.0f32; centroid_0.len()];
            let mut sum_1 = vec![0.0f32; centroid_1.len()];

            for i in 0..n {
                let d0 = cosine_distance(&segment_vectors[i], &centroid_0);
                let d1 = cosine_distance(&segment_vectors[i], &centroid_1);

                let prev_bias_0 = if i > 0 && cluster_assignments[i - 1] == 0 { -0.03 } else { 0.0 };
                let prev_bias_1 = if i > 0 && cluster_assignments[i - 1] == 1 { -0.03 } else { 0.0 };

                let assigned = if (d0 + prev_bias_0) <= (d1 + prev_bias_1) { 0 } else { 1 };
                cluster_assignments[i] = assigned;

                if assigned == 0 {
                    count_0 += 1;
                    for k in 0..centroid_0.len() { sum_0[k] += segment_vectors[i][k]; }
                } else {
                    count_1 += 1;
                    for k in 0..centroid_1.len() { sum_1[k] += segment_vectors[i][k]; }
                }
            }

            if count_0 > 0 {
                for k in 0..centroid_0.len() { centroid_0[k] = sum_0[k] / count_0 as f32; }
            }
            if count_1 > 0 {
                for k in 0..centroid_1.len() { centroid_1[k] = sum_1[k] / count_1 as f32; }
            }
        }
    } else {
        // Subtle acoustic variance: initialize using conversational turn alternation
        let mut current_speaker = 0usize;
        for i in 0..n {
            let text = &segments[i].text;
            if i > 0 {
                let prev_text = &segments[i - 1].text;
                let pause = segments[i].start_time - segments[i - 1].end_time;
                // Switch speaker if previous was a question, or pause > 0.8s, or symptom answer pattern
                if prev_text.ends_with('?') || prev_text.contains('?') || pause > 0.8 || is_patient_utterance(text) || is_doctor_utterance(text) {
                    current_speaker = 1 - current_speaker;
                }
            }
            cluster_assignments[i] = current_speaker;
        }
    }

    for (i, seg) in segments.iter_mut().enumerate() {
        seg.speaker_label = if cluster_assignments[i] == 0 {
            "Speaker 1".to_string()
        } else {
            "Speaker 2".to_string()
        };
    }
}

pub fn is_doctor_utterance(text: &str) -> bool {
    let lower = text.to_lowercase();
    let cues = [
        "what brought you in", "how are you feeling", "how long", "any pain", "fever",
        "blood pressure", "prescribe", "examine", "recommend", "take a look", "history",
        "symptoms", "dosage", "mg", "follow up", "test", "scan", "tell me about",
        "is it sharp", "where does it hurt", "are you taking", "let me check",
        "你好", "哪里不舒服", "多长时间", "痛吗", "发烧", "量一下", "开药"
    ];
    cues.iter().any(|&c| lower.contains(c)) || (text.contains('?') && !lower.contains("i have") && !lower.contains("i feel"))
}

pub fn is_patient_utterance(text: &str) -> bool {
    let lower = text.to_lowercase();
    let cues = [
        "i'm just having", "having a lot of", "chest pain", "i feel", "it hurts",
        "my head", "my chest", "since yesterday", "started", "i have", "painful",
        "cannot sleep", "dizzy", "tired", "feel weak", "ache", "- sure", "sure.",
        "yeah", "yes doctor", "no doctor", "i was", "my stomach", "hurts a lot",
        "我感觉", "疼", "头痛", "难受", "好几天", "睡不着", "发晕", "胸痛"
    ];
    cues.iter().any(|&c| lower.contains(c))
}

/// Classify and map Speaker 1 and Speaker 2 to clinical roles (Doctor vs Patient) using dialogue analysis
pub async fn resolve_clinical_roles(
    segments: &mut [TranscriptSegment],
    endpoint: &str,
    model: &str,
    cloud_fallback_url: Option<String>,
    cloud_api_key: Option<String>,
) {
    if segments.is_empty() {
        return;
    }

    let dialogue_snippet: String = segments
        .iter()
        .take(15)
        .enumerate()
        .map(|(idx, s)| format!("[Line {}] {}: {}", idx, s.speaker_label, s.text))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "You are an expert clinical conversation analyzer.\n\
        Analyze the following dialogue turns from a medical consultation.\n\
        Determine for each line whether the speaker is 'Doctor' (taking history, asking questions, advising treatment) or 'Patient' (describing symptoms, answering questions).\n\n\
        DIALOGUE:\n{}\n\n\
        Respond ONLY with a JSON object mapping line numbers to roles, e.g.:\n\
        {{\"0\": \"Doctor\", \"1\": \"Patient\", \"2\": \"Patient\"}}",
        dialogue_snippet
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build();

    let mut line_mapping: HashMap<usize, String> = HashMap::new();

    if let Ok(c) = client {
        // 1. Try local Ollama
        let local_url = format!("{}/api/generate", endpoint.trim_end_matches('/'));
        let local_body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
            "format": "json"
        });

        if let Ok(resp) = c.post(&local_url).json(&local_body).send().await {
            if resp.status().is_success() {
                if let Ok(val) = resp.json::<serde_json::Value>().await {
                    if let Some(res_str) = val["response"].as_str() {
                        if let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(res_str.trim()) {
                            for (k, v) in parsed {
                                if let Ok(idx) = k.parse::<usize>() {
                                    if v.eq_ignore_ascii_case("Doctor") || v.eq_ignore_ascii_case("Patient") {
                                        let normalized = if v.eq_ignore_ascii_case("Doctor") { "Doctor".to_string() } else { "Patient".to_string() };
                                        line_mapping.insert(idx, normalized);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. Try cloud fallback if local did not resolve line-by-line
        if line_mapping.is_empty() {
            if let (Some(cloud_url), Some(api_key)) = (cloud_fallback_url, cloud_api_key) {
                if !cloud_url.trim().is_empty() && !api_key.trim().is_empty() {
                    let cloud_body = serde_json::json!({
                        "model": "gpt-4o-mini",
                        "messages": [
                            { "role": "system", "content": "You are a clinical conversation analyzer." },
                            { "role": "user", "content": prompt }
                        ],
                        "response_format": { "type": "json_object" }
                    });

                    if let Ok(resp) = c.post(&cloud_url)
                        .header("Authorization", format!("Bearer {}", api_key))
                        .header("Content-Type", "application/json")
                        .json(&cloud_body)
                        .send()
                        .await
                    {
                        if resp.status().is_success() {
                            if let Ok(val) = resp.json::<serde_json::Value>().await {
                                if let Some(content) = val["choices"][0]["message"]["content"].as_str() {
                                    if let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(content.trim()) {
                                        for (k, v) in parsed {
                                            if let Ok(idx) = k.parse::<usize>() {
                                                let normalized = if v.eq_ignore_ascii_case("Doctor") { "Doctor".to_string() } else { "Patient".to_string() };
                                                line_mapping.insert(idx, normalized);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Apply line-level LLM classifications if available
    for (idx, label) in &line_mapping {
        if *idx < segments.len() {
            segments[*idx].speaker_label = label.clone();
        }
    }

    // 4. Fallback Rule-Based Turn-Flow Classifier for any unmapped segments
    let mut current_role = "Doctor".to_string();

    for i in 0..segments.len() {
        if line_mapping.contains_key(&i) {
            current_role = segments[i].speaker_label.clone();
            continue;
        }

        let text = &segments[i].text;
        let is_doc = is_doctor_utterance(text);
        let is_pat = is_patient_utterance(text);

        if is_doc && !is_pat {
            current_role = "Doctor".to_string();
        } else if is_pat && !is_doc {
            current_role = "Patient".to_string();
        } else if i > 0 {
            let prev_text = &segments[i - 1].text;
            let prev_role = &segments[i - 1].speaker_label;
            // If previous was a Doctor question, this response is the Patient
            if prev_role == "Doctor" && (prev_text.ends_with('?') || prev_text.contains('?') || is_doctor_utterance(prev_text)) {
                current_role = "Patient".to_string();
            }
        }

        segments[i].speaker_label = current_role.clone();
    }
}

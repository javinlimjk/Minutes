use hound::WavReader;
use minutes_lib::stt::WhisperEngine;
use std::path::PathBuf;

#[test]
fn test_real_audio_transcription() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wav_path = manifest_dir.join("test_assets").join("test_speech.wav");
    let model_path = manifest_dir.join("models").join("ggml-tiny.en.bin");

    println!("[Test] Checking test assets...");
    println!("[Test] WAV Path: {}", wav_path.display());
    println!("[Test] Model Path: {}", model_path.display());

    assert!(wav_path.exists(), "Test audio WAV file does not exist at {}", wav_path.display());
    assert!(model_path.exists(), "Whisper model file does not exist at {}", model_path.display());

    // Read WAV file
    let mut reader = WavReader::open(&wav_path).expect("Failed to open test WAV file");
    let spec = reader.spec();
    println!(
        "[Test] WAV Spec: {} channels, {} Hz, {} bits per sample, format {:?}",
        spec.channels, spec.sample_rate, spec.bits_per_sample, spec.sample_format
    );

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.expect("Failed to read sample") as f32 / max_val)
                .collect()
        }
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.expect("Failed to read float sample"))
            .collect(),
    };

    let mono_samples: Vec<f32> = if spec.channels > 1 {
        samples
            .chunks(spec.channels as usize)
            .map(|chunk| chunk.iter().sum::<f32>() / spec.channels as f32)
            .collect()
    } else {
        samples
    };

    let resampled_pcm = minutes_lib::audio::resample_to_16k(&mono_samples, spec.sample_rate as f32);
    println!("[Test] Resampled PCM length: {} samples", resampled_pcm.len());

    let rms = (resampled_pcm.iter().map(|&x| x * x).sum::<f32>() / resampled_pcm.len() as f32).sqrt();
    println!("[Test] PCM Peak RMS Energy: {:.5}", rms);
    assert!(rms > 0.001, "Audio signal RMS too low / silent: {:.5}", rms);

    let engine = WhisperEngine::new(&model_path.to_string_lossy(), "en", None);
    let segments = engine.transcribe_buffer(&resampled_pcm).expect("Transcription failed");

    println!("[Test] Transcribed Segments Count: {}", segments.len());
    for (i, seg) in segments.iter().enumerate() {
        println!(
            "[Test] Segment {}: [{:.2}s - {:.2}s] \"{}\"",
            i, seg.start_timestamp, seg.end_timestamp, seg.text
        );
    }

    assert!(!segments.is_empty(), "Whisper transcription returned 0 segments");
    let full_transcript: String = segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    println!("\n=== REAL AUDIO TRANSCRIPT OUTPUT ===");
    println!("{}", full_transcript);
    println!("====================================\n");

    assert!(!full_transcript.trim().is_empty(), "Transcribed text is empty");
}

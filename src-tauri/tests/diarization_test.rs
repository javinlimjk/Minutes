use minutes_lib::db::TranscriptSegment;
use minutes_lib::diarization::{cosine_distance, extract_voiceprint, diarize_transcript_segments, resolve_clinical_roles};

#[tokio::test]
async fn test_automated_speaker_diarization_and_roles() {
    let sample_rate = 16000.0f32;
    let duration_secs = 6.0f32;
    let total_samples = (sample_rate * duration_secs) as usize;

    // Generate synthetic 2-speaker audio signal:
    // 0.0s - 3.0s: Speaker 1 (Low pitch tone ~130 Hz with harmonics)
    // 3.0s - 6.0s: Speaker 2 (High pitch tone ~240 Hz with harmonics)
    let mut pcm = vec![0.0f32; total_samples];

    for i in 0..(total_samples / 2) {
        let t = i as f32 / sample_rate;
        // Speaker 1 (Low male pitch voice proxy)
        pcm[i] = 0.5 * (2.0 * std::f32::consts::PI * 130.0 * t).sin()
               + 0.25 * (2.0 * std::f32::consts::PI * 260.0 * t).sin();
    }

    for i in (total_samples / 2)..total_samples {
        let t = i as f32 / sample_rate;
        // Speaker 2 (Higher female pitch voice proxy)
        pcm[i] = 0.5 * (2.0 * std::f32::consts::PI * 240.0 * t).sin()
               + 0.25 * (2.0 * std::f32::consts::PI * 480.0 * t).sin();
    }

    // 1. Test feature extraction & cosine distance
    let vp1 = extract_voiceprint(&pcm[0..(total_samples / 2)], sample_rate);
    let vp2 = extract_voiceprint(&pcm[(total_samples / 2)..total_samples], sample_rate);

    println!("[Diarization Test] Speaker 1 Pitch: {:.1} Hz", vp1.mean_pitch_hz);
    println!("[Diarization Test] Speaker 2 Pitch: {:.1} Hz", vp2.mean_pitch_hz);

    assert!((vp1.mean_pitch_hz - 130.0).abs() < 15.0, "Speaker 1 pitch detection deviation: {:.1}", vp1.mean_pitch_hz);
    assert!((vp2.mean_pitch_hz - 240.0).abs() < 20.0, "Speaker 2 pitch detection deviation: {:.1}", vp2.mean_pitch_hz);

    let dist = cosine_distance(&vp1.to_vector(), &vp2.to_vector());
    println!("[Diarization Test] Inter-speaker Voiceprint Distance: {:.4}", dist);
    assert!(dist > 0.05, "Voiceprint distance should be distinctly non-zero between speakers");

    // 2. Test multi-segment clustering
    let mut segments = vec![
        TranscriptSegment {
            id: "seg_1".to_string(),
            meeting_id: "m_test".to_string(),
            speaker_label: "Speaker".to_string(),
            start_time: 0.0,
            end_time: 2.8,
            text: "Good morning, where are you experiencing pain and how long have you had this fever?".to_string(),
            confidence: Some(0.95),
        },
        TranscriptSegment {
            id: "seg_2".to_string(),
            meeting_id: "m_test".to_string(),
            speaker_label: "Speaker".to_string(),
            start_time: 3.1,
            end_time: 5.8,
            text: "I feel terrible doctor, my throat hurts and I have had a severe headache since yesterday.".to_string(),
            confidence: Some(0.95),
        },
    ];

    diarize_transcript_segments(&pcm, &mut segments);

    println!("[Diarization Test] Post-Acoustic Clustering:");
    for s in &segments {
        println!(" - [{}]: \"{}\"", s.speaker_label, s.text);
    }

    assert_ne!(segments[0].speaker_label, segments[1].speaker_label, "Segments from different audio sections must have different speaker labels");

    // 3. Test Clinical Role Resolution (Doctor vs Patient)
    resolve_clinical_roles(
        &mut segments,
        "http://localhost:11434",
        "qwen2.5:7b",
        None,
        None,
    ).await;

    println!("[Diarization Test] Post-Clinical Role Resolution:");
    for s in &segments {
        println!(" - [{}]: \"{}\"", s.speaker_label, s.text);
    }

    assert_eq!(segments[0].speaker_label, "Doctor", "Segment 1 (clinician asking about pain/fever) should be classified as Doctor");
    assert_eq!(segments[1].speaker_label, "Patient", "Segment 2 (patient describing headache/throat hurt) should be classified as Patient");

    // 4. Test User Consultation Sequence (Question -> Answer -> Complaint)
    let mut consultation_segments = vec![
        TranscriptSegment {
            id: "u_1".to_string(),
            meeting_id: "m_u".to_string(),
            speaker_label: "Speaker".to_string(),
            start_time: 0.0,
            end_time: 2.0,
            text: "what brought you in today?".to_string(),
            confidence: Some(0.95),
        },
        TranscriptSegment {
            id: "u_2".to_string(),
            meeting_id: "m_u".to_string(),
            speaker_label: "Speaker".to_string(),
            start_time: 2.0,
            end_time: 3.0,
            text: "- Sure.".to_string(),
            confidence: Some(0.95),
        },
        TranscriptSegment {
            id: "u_3".to_string(),
            meeting_id: "m_u".to_string(),
            speaker_label: "Speaker".to_string(),
            start_time: 3.0,
            end_time: 7.0,
            text: "I'm just having a lot of chest pain,".to_string(),
            confidence: Some(0.95),
        },
    ];

    diarize_transcript_segments(&pcm[0..total_samples.min((16000.0 * 7.0) as usize)], &mut consultation_segments);
    resolve_clinical_roles(
        &mut consultation_segments,
        "http://localhost:11434",
        "qwen2.5:7b",
        None,
        None,
    ).await;

    println!("[Diarization Test] Consultation Sequence Roles:");
    for s in &consultation_segments {
        println!(" - [{}]: \"{}\"", s.speaker_label, s.text);
    }

    assert_eq!(consultation_segments[0].speaker_label, "Doctor", "Line 0 ('what brought you in today?') must be Doctor");
    assert_eq!(consultation_segments[1].speaker_label, "Patient", "Line 1 ('- Sure.') must be Patient");
    assert_eq!(consultation_segments[2].speaker_label, "Patient", "Line 2 ('I\\'m just having a lot of chest pain,') must be Patient");
}

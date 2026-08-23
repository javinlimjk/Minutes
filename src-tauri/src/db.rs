use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use crate::crypto::{decrypt_text, encrypt_text};
use crate::soap::SOAPNote;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub duration_seconds: i64,
    pub status: String,
    pub audio_path: Option<String>,
    pub summary: Option<String>,
    pub action_items: Option<Vec<String>>,
    pub key_decisions: Option<Vec<String>>,
    pub transcript_count: usize,
    pub specialty: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: String,
    pub meeting_id: String,
    pub speaker_label: String,
    pub start_time: f64,
    pub end_time: f64,
    pub text: String,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSettings {
    pub stt_model: String,
    pub stt_language: String,
    pub llm_provider: String,
    pub ollama_endpoint: String,
    pub ollama_model: String,
    pub auto_summarize: bool,
    pub vad_sensitivity: f32,
    pub cloud_fallback_url: Option<String>,
    pub cloud_api_key: Option<String>,
    pub default_specialty: String,
    pub custom_summary_prompt: Option<String>,
    pub enable_speaker_diarization: bool,
    pub custom_vocabulary: Option<String>,
}

pub fn get_app_data_dir() -> std::path::PathBuf {
    if let Some(mut path) = dirs_next::data_dir() {
        path.push("com.minutes.scribe");
        path
    } else {
        std::path::PathBuf::from(".minutes_data")
    }
}

pub fn get_recordings_dir() -> std::path::PathBuf {
    let rec_dir = get_app_data_dir().join("recordings");
    let _ = std::fs::create_dir_all(&rec_dir);
    rec_dir
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            stt_model: "small".to_string(),
            stt_language: "auto".to_string(),
            llm_provider: "ollama".to_string(),
            ollama_endpoint: "http://localhost:11434".to_string(),
            ollama_model: "qwen2.5:7b".to_string(),
            auto_summarize: true,
            vad_sensitivity: 0.5,
            cloud_fallback_url: None,
            cloud_api_key: None,
            default_specialty: "General Practice".to_string(),
            custom_summary_prompt: None,
            enable_speaker_diarization: false,
            custom_vocabulary: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: String,
    pub timestamp: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub details: Option<String>,
}

pub fn get_db_conn() -> Result<Connection> {
    let app_dir = get_app_data_dir();
    let _ = std::fs::create_dir_all(&app_dir);
    let db_path = app_dir.join("minutes_scribe.db");

    // Migration helper: If persistent db does not exist but legacy temp db exists, copy it
    let legacy_temp_db = std::env::temp_dir().join("minutes_scribe.db");
    if !db_path.exists() && legacy_temp_db.exists() {
        let _ = std::fs::copy(&legacy_temp_db, &db_path);
    }

    let conn = Connection::open(&db_path)?;
    init_db(&conn)?;
    Ok(conn)
}

pub fn init_db(conn: &Connection) -> Result<()> {
    let _ = conn.execute_batch("
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
    ");

    conn.execute(
        "CREATE TABLE IF NOT EXISTS meetings (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created_at TEXT NOT NULL,
            duration_seconds INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'completed',
            audio_path TEXT,
            summary TEXT,
            action_items TEXT,
            key_decisions TEXT,
            specialty TEXT DEFAULT 'General Practice'
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS transcript_segments (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL,
            speaker_label TEXT NOT NULL,
            start_time REAL NOT NULL,
            end_time REAL NOT NULL,
            text TEXT NOT NULL,
            confidence REAL,
            FOREIGN KEY(meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS soap_notes (
            meeting_id TEXT PRIMARY KEY,
            specialty TEXT NOT NULL,
            subjective_enc TEXT NOT NULL,
            objective_enc TEXT NOT NULL,
            assessment_enc TEXT NOT NULL,
            plan_enc TEXT NOT NULL,
            icd10_json TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            stt_model TEXT NOT NULL,
            stt_language TEXT NOT NULL,
            llm_provider TEXT NOT NULL,
            ollama_endpoint TEXT NOT NULL,
            ollama_model TEXT NOT NULL,
            auto_summarize INTEGER NOT NULL,
            vad_sensitivity REAL NOT NULL,
            cloud_fallback_url TEXT,
            cloud_api_key TEXT,
            default_specialty TEXT DEFAULT 'General Practice',
            custom_summary_prompt TEXT
        )",
        [],
    )?;

    let _ = conn.execute("ALTER TABLE settings ADD COLUMN custom_summary_prompt TEXT", []);
    let _ = conn.execute("ALTER TABLE settings ADD COLUMN enable_speaker_diarization INTEGER DEFAULT 0", []);

    // Performance index for rapid segment queries
    let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_segments_meeting ON transcript_segments(meeting_id, start_time)", []);

    conn.execute(
        "CREATE TABLE IF NOT EXISTS audit_logs (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            action TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            entity_id TEXT,
            details TEXT
        )",
        [],
    )?;

    conn.execute(
        "INSERT OR IGNORE INTO settings (id, stt_model, stt_language, llm_provider, ollama_endpoint, ollama_model, auto_summarize, vad_sensitivity, default_specialty)
         VALUES (1, 'small', 'auto', 'ollama', 'http://localhost:11434', 'qwen2.5:7b', 1, 0.5, 'General Practice')",
        [],
    )?;

    // Migrate settings: Ensure high quality Multilingual small model and qwen2.5:7b are used
    let _ = conn.execute("UPDATE settings SET stt_language = 'auto' WHERE stt_language = 'en'", []);
    let _ = conn.execute("UPDATE settings SET ollama_model = 'qwen2.5:7b' WHERE ollama_model = 'qwen2.5:1.5b'", []);
    let _ = conn.execute("UPDATE settings SET stt_model = 'small' WHERE stt_model = 'small.en' OR stt_model = 'base'", []);
    let _ = conn.execute("ALTER TABLE settings ADD COLUMN custom_vocabulary TEXT", []);

    Ok(())
}

pub fn log_audit_event(
    conn: &Connection,
    action: &str,
    entity_type: &str,
    entity_id: Option<&str>,
    details: Option<&str>,
) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let timestamp = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO audit_logs (id, timestamp, action, entity_type, entity_id, details)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, timestamp, action, entity_type, entity_id, details],
    )?;
    Ok(())
}

pub fn insert_meeting(conn: &Connection, meeting: &Meeting) -> Result<()> {
    let encrypted_title = encrypt_text(&meeting.title);
    let action_json = meeting.action_items.as_ref().map(|items| serde_json::to_string(items).unwrap_or_default());
    let decision_json = meeting.key_decisions.as_ref().map(|items| serde_json::to_string(items).unwrap_or_default());

    conn.execute(
        "INSERT OR REPLACE INTO meetings (id, title, created_at, duration_seconds, status, audio_path, summary, action_items, key_decisions, specialty)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            meeting.id,
            encrypted_title,
            meeting.created_at,
            meeting.duration_seconds,
            meeting.status,
            meeting.audio_path,
            meeting.summary.as_ref().map(|s| encrypt_text(s)),
            action_json,
            decision_json,
            meeting.specialty.as_deref().unwrap_or("General Practice")
        ],
    )?;

    let _ = log_audit_event(conn, "UPSERT", "MEETING", Some(&meeting.id), Some("Meeting record saved or updated"));
    Ok(())
}

pub fn insert_transcript_segment(conn: &Connection, segment: &TranscriptSegment) -> Result<()> {
    let encrypted_text = encrypt_text(&segment.text);
    conn.execute(
        "INSERT OR REPLACE INTO transcript_segments (id, meeting_id, speaker_label, start_time, end_time, text, confidence)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            segment.id,
            segment.meeting_id,
            segment.speaker_label,
            segment.start_time,
            segment.end_time,
            encrypted_text,
            segment.confidence
        ],
    )?;
    Ok(())
}

pub fn insert_transcript_segments(conn: &Connection, meeting_id: &str, segments: &[TranscriptSegment]) -> Result<()> {
    for segment in segments {
        let encrypted_text = encrypt_text(&segment.text);
        conn.execute(
            "INSERT OR REPLACE INTO transcript_segments (id, meeting_id, speaker_label, start_time, end_time, text, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                segment.id,
                meeting_id,
                segment.speaker_label,
                segment.start_time,
                segment.end_time,
                encrypted_text,
                segment.confidence
            ],
        )?;
    }
    Ok(())
}

pub fn update_speaker_label(conn: &Connection, segment_id: &str, new_label: &str) -> Result<()> {
    conn.execute(
        "UPDATE transcript_segments SET speaker_label = ?1 WHERE id = ?2",
        params![new_label, segment_id],
    )?;
    let _ = log_audit_event(conn, "UPDATE", "TRANSCRIPT_SEGMENT", Some(segment_id), Some("Speaker label updated"));
    Ok(())
}

pub fn update_meeting_summary(
    conn: &Connection,
    meeting_id: &str,
    summary: &str,
    action_items: &[String],
    key_decisions: &[String],
) -> Result<()> {
    let encrypted_summary = encrypt_text(summary);
    let action_json = serde_json::to_string(action_items).unwrap_or_default();
    let decision_json = serde_json::to_string(key_decisions).unwrap_or_default();

    conn.execute(
        "UPDATE meetings SET summary = ?1, action_items = ?2, key_decisions = ?3 WHERE id = ?4",
        params![encrypted_summary, action_json, decision_json, meeting_id],
    )?;
    let _ = log_audit_event(conn, "UPDATE", "MEETING", Some(meeting_id), Some("Executive summary updated"));
    Ok(())
}

pub fn save_soap_note(conn: &Connection, meeting_id: &str, note: &SOAPNote) -> Result<()> {
    let subj_enc = encrypt_text(&note.subjective);
    let obj_enc = encrypt_text(&note.objective);
    let ass_enc = encrypt_text(&note.assessment);
    let plan_enc = encrypt_text(&note.plan);
    let icd10_json = serde_json::to_string(&note.icd10_suggestions).unwrap_or_default();

    conn.execute(
        "INSERT OR REPLACE INTO soap_notes (meeting_id, specialty, subjective_enc, objective_enc, assessment_enc, plan_enc, icd10_json, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            meeting_id,
            note.specialty,
            subj_enc,
            obj_enc,
            ass_enc,
            plan_enc,
            icd10_json,
            note.timestamp
        ],
    )?;

    let _ = log_audit_event(conn, "SAVE", "SOAP_NOTE", Some(meeting_id), Some("Clinical SOAP note generated/updated"));
    Ok(())
}

pub fn get_soap_note(conn: &Connection, meeting_id: &str) -> Result<Option<SOAPNote>> {
    let mut stmt = conn.prepare(
        "SELECT specialty, subjective_enc, objective_enc, assessment_enc, plan_enc, icd10_json, updated_at
         FROM soap_notes WHERE meeting_id = ?1",
    )?;

    let mut rows = stmt.query(params![meeting_id])?;
    if let Some(row) = rows.next()? {
        let specialty: String = row.get(0)?;
        let subj_enc: String = row.get(1)?;
        let obj_enc: String = row.get(2)?;
        let ass_enc: String = row.get(3)?;
        let plan_enc: String = row.get(4)?;
        let icd10_json: String = row.get(5)?;
        let updated_at: String = row.get(6)?;

        let icd10_suggestions = serde_json::from_str(&icd10_json).unwrap_or_default();

        Ok(Some(SOAPNote {
            specialty,
            subjective: decrypt_text(&subj_enc),
            objective: decrypt_text(&obj_enc),
            assessment: decrypt_text(&ass_enc),
            plan: decrypt_text(&plan_enc),
            icd10_suggestions,
            timestamp: updated_at,
        }))
    } else {
        Ok(None)
    }
}

pub fn get_all_meetings(conn: &Connection) -> Result<Vec<Meeting>> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.title, m.created_at, m.duration_seconds, m.status, m.audio_path, m.summary, m.action_items, m.key_decisions,
                (SELECT COUNT(*) FROM transcript_segments t WHERE t.meeting_id = m.id) as transcript_count,
                m.specialty
         FROM meetings m
         ORDER BY m.created_at DESC",
    )?;

    let meeting_iter = stmt.query_map([], |row| {
        let title_enc: String = row.get(1)?;
        let summary_enc: Option<String> = row.get(6)?;
        let action_items_raw: Option<String> = row.get(7)?;
        let key_decisions_raw: Option<String> = row.get(8)?;

        let action_items = action_items_raw.and_then(|s| serde_json::from_str(&s).ok());
        let key_decisions = key_decisions_raw.and_then(|s| serde_json::from_str(&s).ok());

        Ok(Meeting {
            id: row.get(0)?,
            title: decrypt_text(&title_enc),
            created_at: row.get(2)?,
            duration_seconds: row.get(3)?,
            status: row.get(4)?,
            audio_path: row.get(5)?,
            summary: summary_enc.map(|s| decrypt_text(&s)),
            action_items,
            key_decisions,
            transcript_count: row.get(9)?,
            specialty: row.get(10)?,
        })
    })?;

    let mut meetings = Vec::new();
    for meeting in meeting_iter {
        meetings.push(meeting?);
    }

    Ok(meetings)
}

pub fn get_meeting_segments(conn: &Connection, meeting_id: &str) -> Result<Vec<TranscriptSegment>> {
    let mut stmt = conn.prepare(
        "SELECT id, meeting_id, speaker_label, start_time, end_time, text, confidence
         FROM transcript_segments
         WHERE meeting_id = ?1
         ORDER BY start_time ASC",
    )?;

    let seg_iter = stmt.query_map(params![meeting_id], |row| {
        let text_enc: String = row.get(5)?;
        Ok(TranscriptSegment {
            id: row.get(0)?,
            meeting_id: row.get(1)?,
            speaker_label: row.get(2)?,
            start_time: row.get(3)?,
            end_time: row.get(4)?,
            text: decrypt_text(&text_enc),
            confidence: row.get(6)?,
        })
    })?;

    let mut segments = Vec::new();
    for seg in seg_iter {
        segments.push(seg?);
    }

    Ok(segments)
}

pub fn get_settings(conn: &Connection) -> Result<ModelSettings> {
    let mut stmt = conn.prepare(
        "SELECT stt_model, stt_language, llm_provider, ollama_endpoint, ollama_model, auto_summarize, vad_sensitivity, cloud_fallback_url, cloud_api_key, default_specialty, custom_summary_prompt, enable_speaker_diarization, custom_vocabulary
         FROM settings WHERE id = 1",
    )?;

    stmt.query_row([], |row| {
        let auto_sum_int: i32 = row.get(5)?;
        let diarize_int: Option<i32> = row.get(11).ok();
        let cloud_key_enc: Option<String> = row.get(8)?;
        let cloud_api_key = cloud_key_enc.map(|k| if k.is_empty() { k } else { decrypt_text(&k) });

        Ok(ModelSettings {
            stt_model: row.get(0)?,
            stt_language: row.get(1)?,
            llm_provider: row.get(2)?,
            ollama_endpoint: row.get(3)?,
            ollama_model: row.get(4)?,
            auto_summarize: auto_sum_int != 0,
            vad_sensitivity: row.get(6)?,
            cloud_fallback_url: row.get(7)?,
            cloud_api_key,
            default_specialty: row.get::<_, Option<String>>(9)?.unwrap_or_else(|| "General Practice".to_string()),
            custom_summary_prompt: row.get(10)?,
            enable_speaker_diarization: diarize_int.unwrap_or(0) != 0,
            custom_vocabulary: row.get(12).ok(),
        })
    })
}

pub fn update_settings(conn: &Connection, settings: &ModelSettings) -> Result<()> {
    let encrypted_cloud_key = settings.cloud_api_key.as_ref().map(|k| if k.is_empty() { k.clone() } else { encrypt_text(k) });

    conn.execute(
        "UPDATE settings SET stt_model=?1, stt_language=?2, llm_provider=?3, ollama_endpoint=?4, ollama_model=?5, auto_summarize=?6, vad_sensitivity=?7, cloud_fallback_url=?8, cloud_api_key=?9, default_specialty=?10, custom_summary_prompt=?11, enable_speaker_diarization=?12, custom_vocabulary=?13 WHERE id=1",
        params![
            settings.stt_model,
            settings.stt_language,
            settings.llm_provider,
            settings.ollama_endpoint,
            settings.ollama_model,
            if settings.auto_summarize { 1 } else { 0 },
            settings.vad_sensitivity,
            settings.cloud_fallback_url,
            encrypted_cloud_key,
            settings.default_specialty,
            settings.custom_summary_prompt,
            if settings.enable_speaker_diarization { 1 } else { 0 },
            settings.custom_vocabulary,
        ],
    )?;
    Ok(())
}

pub fn delete_meeting(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM soap_notes WHERE meeting_id = ?1", params![id])?;
    conn.execute("DELETE FROM transcript_segments WHERE meeting_id = ?1", params![id])?;
    conn.execute("DELETE FROM meetings WHERE id = ?1", params![id])?;

    // Purge corresponding physical audio recording on disk to prevent disk leaks
    let audio_file = get_recordings_dir().join(format!("audio_{}.wav", id));
    if audio_file.exists() {
        let _ = std::fs::remove_file(audio_file);
    }

    let _ = log_audit_event(conn, "DELETE", "MEETING", Some(id), Some("Meeting record and associated PHI purged"));
    Ok(())
}

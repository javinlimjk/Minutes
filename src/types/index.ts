export interface Meeting {
  id: string;
  title: string;
  created_at: string;
  duration_seconds: number;
  status: 'recording' | 'processing' | 'summarizing' | 'completed' | 'failed';
  audio_path?: string;
  summary?: string;
  action_items?: string[];
  key_decisions?: string[];
  transcript_count?: number;
  specialty?: string;
}

export interface TranscriptSegment {
  id: string;
  meeting_id: string;
  speaker_label: string;
  start_time: number;
  end_time: number;
  text: string;
  confidence?: number;
}

export interface Speaker {
  id: string;
  name: string;
  color: string;
}

export interface ICD10Code {
  code: string;
  description: string;
}

export interface SOAPNote {
  specialty: string;
  subjective: string;
  objective: string;
  assessment: string;
  plan: string;
  icd10_suggestions: ICD10Code[];
  timestamp: string;
}

export type SpecialtyType = 'General Practice' | 'Psychiatry' | 'Pediatrics' | 'Orthopedics';

export interface ModelSettings {
  stt_model: string;
  stt_language: string;
  llm_provider: 'ollama' | 'lmstudio' | 'cloud_fallback';
  ollama_endpoint: string;
  ollama_model: string;
  auto_summarize: boolean;
  vad_sensitivity: number;
  cloud_fallback_url?: string;
  cloud_api_key?: string;
  default_specialty?: SpecialtyType;
  custom_summary_prompt?: string;
  enable_speaker_diarization?: boolean;
  custom_vocabulary?: string;
}

export interface OllamaModelInfo {
  name: string;
  size: number;
  digest: string;
  modified_at: string;
}

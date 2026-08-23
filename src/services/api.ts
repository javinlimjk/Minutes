import { invoke } from '@tauri-apps/api/core';
import { Meeting, TranscriptSegment, ModelSettings, OllamaModelInfo, SOAPNote } from '../types';

export const isTauri = (): boolean => {
  return typeof window !== 'undefined' && ('__TAURI_INTERNALS__' in window || '__TAURI__' in window || (window as any).__TAURI_IPC__ !== undefined);
};

async function invokeTauri<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (isTauri()) {
    return await invoke<T>(command, args);
  }
  throw new Error(`Tauri IPC not available for ${command}`);
}

export const DEFAULT_SETTINGS: ModelSettings = {
  stt_model: 'small',
  stt_language: 'auto',
  llm_provider: 'ollama',
  ollama_endpoint: 'http://localhost:11434',
  ollama_model: 'qwen2.5:7b',
  auto_summarize: true,
  vad_sensitivity: 0.5,
  default_specialty: 'General Practice',
  enable_speaker_diarization: false,
  custom_vocabulary: '',
};

export const api = {
  async getLiveTranscript(): Promise<any[]> {
    try {
      return await invokeTauri<any[]>('get_live_transcript');
    } catch {
      return [];
    }
  },

  async getMeetings(): Promise<Meeting[]> {
    try {
      return await invokeTauri<Meeting[]>('get_meetings');
    } catch (err) {
      if (isTauri()) {
        console.error('[Enterprise Security] IPC error in getMeetings:', err);
        return [];
      }
      const stored = localStorage.getItem('minutes_meetings');
      return stored ? JSON.parse(stored) : [];
    }
  },

  async getMeeting(id: string): Promise<{ meeting: Meeting; segments: TranscriptSegment[]; soap_note?: SOAPNote }> {
    try {
      return await invokeTauri<{ meeting: Meeting; segments: TranscriptSegment[]; soap_note?: SOAPNote }>('get_meeting', { id });
    } catch (err) {
      if (isTauri()) {
        console.error('[Enterprise Security] IPC error in getMeeting:', err);
        throw err;
      }
      const meetings = await this.getMeetings();
      const meeting = meetings.find(m => m.id === id) || {
        id,
        title: 'Consultation Session',
        created_at: new Date().toISOString(),
        duration_seconds: 0,
        status: 'completed',
        transcript_count: 0
      };
      const storedSegs = localStorage.getItem(`minutes_segments_${id}`);
      const segments: TranscriptSegment[] = storedSegs ? JSON.parse(storedSegs) : [];
      const storedSoap = localStorage.getItem(`minutes_soap_${id}`);
      const soap_note = storedSoap ? JSON.parse(storedSoap) : undefined;
      return { meeting, segments, soap_note };
    }
  },

  async startRecording(title: string, specialty?: string): Promise<Meeting> {
    try {
      return await invokeTauri<Meeting>('start_recording', { title, specialty });
    } catch (err) {
      if (isTauri()) {
        console.error('[Enterprise Security] IPC error in startRecording:', err);
        throw err;
      }
      const newMeeting: Meeting = {
        id: 'm_' + Date.now(),
        title: title || `Consultation ${new Date().toLocaleDateString()} ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`,
        created_at: new Date().toISOString(),
        duration_seconds: 0,
        status: 'recording',
        transcript_count: 0,
        specialty: specialty || 'General Practice',
      };
      const meetings = await this.getMeetings();
      const updated = [newMeeting, ...meetings];
      localStorage.setItem('minutes_meetings', JSON.stringify(updated));
      return newMeeting;
    }
  },

  async saveMeetingSegments(meetingId: string, durationSeconds: number, segments: TranscriptSegment[]): Promise<void> {
    try {
      await invokeTauri('save_meeting_segments', { meetingId, durationSeconds, segments });
    } catch (err) {
      if (isTauri()) {
        console.error('[Enterprise Security] IPC error in saveMeetingSegments:', err);
        throw err;
      }
      localStorage.setItem(`minutes_segments_${meetingId}`, JSON.stringify(segments));
      const meetings = await this.getMeetings();
      const idx = meetings.findIndex(m => m.id === meetingId);
      if (idx !== -1) {
        meetings[idx].duration_seconds = durationSeconds;
        meetings[idx].transcript_count = segments.length;
        localStorage.setItem('minutes_meetings', JSON.stringify(meetings));
      }
    }
  },

  async stopRecording(meetingId: string): Promise<Meeting> {
    try {
      return await invokeTauri<Meeting>('stop_recording', { meetingId });
    } catch (err) {
      if (isTauri()) {
        console.error('[Enterprise Security] IPC error in stopRecording:', err);
        throw err;
      }
      const meetings = await this.getMeetings();
      const index = meetings.findIndex(m => m.id === meetingId);
      if (index !== -1) {
        meetings[index].status = 'completed';
        localStorage.setItem('minutes_meetings', JSON.stringify(meetings));
        return meetings[index];
      }
      throw new Error('Meeting not found');
    }
  },

  async deleteMeeting(id: string): Promise<void> {
    try {
      await invokeTauri('delete_meeting', { id });
    } catch (err) {
      if (isTauri()) {
        console.error('[Enterprise Security] IPC error in deleteMeeting:', err);
        throw err;
      }
      const meetings = await this.getMeetings();
      const filtered = meetings.filter(m => m.id !== id);
      localStorage.setItem('minutes_meetings', JSON.stringify(filtered));
      localStorage.removeItem(`minutes_segments_${id}`);
      localStorage.removeItem(`minutes_soap_${id}`);
    }
  },

  async updateSpeakerLabel(meetingId: string, segmentId: string, newLabel: string): Promise<void> {
    try {
      await invokeTauri('update_speaker_label', { segmentId, newLabel });
    } catch {
      const { segments } = await this.getMeeting(meetingId);
      const updated = segments.map(s => s.id === segmentId ? { ...s, speaker_label: newLabel } : s);
      localStorage.setItem(`minutes_segments_${meetingId}`, JSON.stringify(updated));
    }
  },

  async diarizeMeeting(meetingId: string): Promise<TranscriptSegment[]> {
    try {
      return await invokeTauri<TranscriptSegment[]>('diarize_meeting', { meetingId });
    } catch {
      const { segments } = await this.getMeeting(meetingId);
      const updated = segments.map((seg, idx) => ({
        ...seg,
        speaker_label: idx % 2 === 0 ? 'Doctor' : 'Patient',
      }));
      localStorage.setItem(`minutes_segments_${meetingId}`, JSON.stringify(updated));
      return updated;
    }
  },

  async generateSOAPNote(meetingId: string, specialty?: string): Promise<SOAPNote> {
    return await invokeTauri<SOAPNote>('generate_soap_note', { meetingId, specialty });
  },

  async generateSOAPNoteDirect(transcript: string, specialty?: string): Promise<SOAPNote> {
    return await invokeTauri<SOAPNote>('generate_soap_note_direct', { transcript, specialty });
  },

  async getSOAPNote(meetingId: string): Promise<SOAPNote | null> {
    try {
      return await invokeTauri<SOAPNote | null>('get_soap_note', { meetingId });
    } catch {
      const stored = localStorage.getItem(`minutes_soap_${meetingId}`);
      return stored ? JSON.parse(stored) : null;
    }
  },

  async toggleAlwaysOnTop(alwaysOnTop: boolean): Promise<void> {
    try {
      await invokeTauri('toggle_always_on_top', { alwaysOnTop });
    } catch (e) {
      console.warn('Always on top notice:', e);
    }
  },

  async getMeetingAudioBlobUrl(meetingId: string): Promise<string | null> {
    if (isTauri()) {
      try {
        const bytes = await invokeTauri<number[]>('get_meeting_audio_bytes', { meetingId });
        if (bytes && bytes.length > 0) {
          const u8Array = new Uint8Array(bytes);
          const blob = new Blob([u8Array], { type: 'audio/wav' });
          return URL.createObjectURL(blob);
        }
      } catch (e) {
        console.warn('getMeetingAudioBlobUrl notice:', e);
      }
    }
    const storedBlob = localStorage.getItem(`minutes_audio_blob_${meetingId}`);
    return storedBlob || null;
  },

  async importAudioFile(file: File, specialty?: string, enableDiarization?: boolean): Promise<Meeting> {
    const meetingTitle = file.name.replace(/\.[^/.]+$/, '');
    const meetingId = 'm_' + Date.now();
    const spec = specialty || 'General Practice';

    try {
      const arrayBuffer = await file.arrayBuffer();
      const AudioContextClass = window.AudioContext || (window as any).webkitAudioContext;
      const audioCtx = new AudioContextClass();
      const decodedBuffer = await audioCtx.decodeAudioData(arrayBuffer.slice(0));
      const durationSeconds = Math.round(decodedBuffer.duration);

      // Ultra-fast hardware resampling directly to 16kHz mono via OfflineAudioContext
      const offlineCtx = new OfflineAudioContext(1, Math.max(1, Math.ceil(decodedBuffer.duration * 16000)), 16000);
      const source = offlineCtx.createBufferSource();
      source.buffer = decodedBuffer;
      source.connect(offlineCtx.destination);
      source.start(0);
      const rendered16kBuffer = await offlineCtx.startRendering();
      const pcm16kArray = Array.from(rendered16kBuffer.getChannelData(0));

      try { audioCtx.close(); } catch {}

      if (isTauri()) {
        try {
          const importedMeeting = await invokeTauri<Meeting>('import_audio_pcm_and_diarize', {
            meetingId,
            title: `Consultation: ${meetingTitle}`,
            durationSeconds,
            pcmData: pcm16kArray,
            sampleRate: 16000,
            specialty: spec,
            enableDiarization: enableDiarization ?? undefined,
          });

          if (importedMeeting) {
            return importedMeeting;
          }
        } catch (tauriErr) {
          console.warn('[importAudioFile] Native import notice:', tauriErr);
        }
      }

      // Browser Fallback with Local Blob URL for real audio playback
      const blobUrl = URL.createObjectURL(file);
      try {
        localStorage.setItem(`minutes_audio_blob_${meetingId}`, blobUrl);
      } catch {}

      const newMeeting: Meeting = {
        id: meetingId,
        title: `Consultation: ${meetingTitle}`,
        created_at: new Date().toISOString(),
        duration_seconds: durationSeconds,
        status: 'completed',
        audio_path: blobUrl,
        transcript_count: 0,
        specialty: spec,
      };

      const meetings = await this.getMeetings();
      localStorage.setItem('minutes_meetings', JSON.stringify([newMeeting, ...meetings]));
      return newMeeting;
    } catch (err: any) {
      console.error('[importAudioFile] Error processing audio file:', err);
      throw err;
    }
  },

  async generateSummary(meetingId: string): Promise<{ summary: string; action_items: string[]; key_decisions: string[] }> {
    return await invokeTauri('generate_summary', { meetingId });
  },

  async getSettings(): Promise<ModelSettings> {
    try {
      return await invokeTauri<ModelSettings>('get_settings');
    } catch {
      const stored = localStorage.getItem('minutes_settings');
      return stored ? JSON.parse(stored) : DEFAULT_SETTINGS;
    }
  },

  async saveSettings(settings: ModelSettings): Promise<void> {
    try {
      await invokeTauri('save_settings', { settings });
    } catch {
      localStorage.setItem('minutes_settings', JSON.stringify(settings));
    }
  },

  async checkOllama(endpoint: string): Promise<{ online: boolean; models: string[] }> {
    try {
      const res = await fetch(`${endpoint}/api/tags`, { method: 'GET' });
      if (res.ok) {
        const data = await res.json();
        const models = (data.models || []).map((m: OllamaModelInfo) => m.name);
        return { online: true, models };
      }
      return { online: false, models: [] };
    } catch {
      return { online: false, models: [] };
    }
  },

  async checkOllamaStatus(): Promise<{ online: boolean; installed_models: string[] }> {
    try {
      return await invokeTauri<{ online: boolean; installed_models: string[] }>('check_ollama_status');
    } catch {
      return { online: false, installed_models: [] };
    }
  },

  async checkBuiltinModelStatus(): Promise<{ is_downloaded: boolean; model_path?: string; file_size_mb: number; recommended_model: string }> {
    try {
      return await invokeTauri('check_builtin_model_status');
    } catch {
      return { is_downloaded: false, file_size_mb: 0, recommended_model: 'qwen2.5-1.5b-instruct-q4_k_m.gguf' };
    }
  },

  async downloadBuiltinModel(): Promise<string> {
    try {
      return await invokeTauri('download_builtin_model');
    } catch (err) {
      console.warn('Failed to download builtin model:', err);
      throw err;
    }
  },

  async pullLocalModel(modelName: string): Promise<void> {
    try {
      await invokeTauri('pull_local_model', { modelName });
    } catch (err) {
      console.warn('Failed to pull local model:', err);
      throw err;
    }
  },

  async exportFile(filename: string, content: string): Promise<string> {
    try {
      return await invokeTauri<string>('export_file', { filename, content });
    } catch {
      const blob = new Blob([content], { type: 'text/plain;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      return filename;
    }
  },

  async openExportPDF(filename: string, htmlContent: string): Promise<string> {
    try {
      return await invokeTauri<string>('open_export_pdf', { filename, htmlContent });
    } catch {
      const blob = new Blob([htmlContent], { type: 'text/html;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      window.open(url, '_blank');
      return filename;
    }
  }
};

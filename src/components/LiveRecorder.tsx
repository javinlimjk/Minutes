import React, { useState, useEffect, useRef } from 'react';
import { Mic, Square, Pause, Play, Clock, CheckCircle2, Sparkles, AlertCircle, Upload, Pin, Layout, Copy, Check, RefreshCw, ChevronDown } from 'lucide-react';
import { TranscriptSegment, Meeting, ModelSettings, SOAPNote, SpecialtyType } from '../types';
import { AudioVisualizer } from './AudioVisualizer';
import { SOAPNoteView } from './SOAPNoteView';
import { api, isTauri } from '../services/api';

interface LiveRecorderProps {
  onMeetingCreated: (meeting: Meeting, navigateToMeeting?: boolean) => void;
  onMeetingUpdated?: (meeting: Meeting) => void;
  settings: ModelSettings;
  setIsRecordingGlobal?: (recording: boolean) => void;
  isAlwaysOnTop?: boolean;
  onToggleAlwaysOnTop?: (enabled: boolean) => void;
  currentSpecialty?: SpecialtyType;
  onSpecialtyChange?: (specialty: SpecialtyType) => void;
}

export const LiveRecorder: React.FC<LiveRecorderProps> = ({
  onMeetingCreated,
  onMeetingUpdated,
  settings,
  setIsRecordingGlobal,
  isAlwaysOnTop = false,
  onToggleAlwaysOnTop,
  currentSpecialty = 'General Practice',
  onSpecialtyChange,
}) => {
  const [viewMode, setViewMode] = useState<'standard' | 'sidecar'>('standard');
  const [isRecording, setIsRecording] = useState(false);
  const [isPaused, setIsPaused] = useState(false);
  const [title, setTitle] = useState('');
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const [vadState, setVadState] = useState<'speech' | 'silence'>('silence');
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [noticeMsg, setNoticeMsg] = useState<string | null>(null);

  // Live SOAP note state for Sidecar / Live Consultation
  const [liveSOAP, setLiveSOAP] = useState<SOAPNote | undefined>(undefined);
  const [isGeneratingSOAP, setIsGeneratingSOAP] = useState(false);
  const [copiedQuick, setCopiedQuick] = useState(false);

  // Smart Auto-Scroll State
  const transcriptContainerRef = useRef<HTMLDivElement | null>(null);
  const sidecarContainerRef = useRef<HTMLDivElement | null>(null);
  const userScrolledUpRef = useRef(false);
  const [isScrolledUp, setIsScrolledUp] = useState(false);

  const elapsedSecondsRef = useRef(0);
  const lastProcessedTextRef = useRef('');
  const lastSegmentTimeRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const recognitionRef = useRef<any>(null);

  const [audioData, setAudioData] = useState<Uint8Array | null>(null);

  const audioCtxRef = useRef<AudioContext | null>(null);
  const analyserRef = useRef<AnalyserNode | null>(null);
  const mediaStreamRef = useRef<MediaStream | null>(null);
  const animIdRef = useRef<number | null>(null);

  const jsPcmBufferRef = useRef<number[]>([]);
  const scriptNodeRef = useRef<ScriptProcessorNode | null>(null);

  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
    const target = e.currentTarget;
    const distanceFromBottom = target.scrollHeight - target.scrollTop - target.clientHeight;
    const scrolledUp = distanceFromBottom > 65;
    userScrolledUpRef.current = scrolledUp;
    setIsScrolledUp(scrolledUp);
  };

  const scrollToBottom = (smooth = true) => {
    userScrolledUpRef.current = false;
    setIsScrolledUp(false);
    if (viewMode === 'sidecar' && sidecarContainerRef.current) {
      sidecarContainerRef.current.scrollTo({
        top: sidecarContainerRef.current.scrollHeight,
        behavior: smooth ? 'smooth' : 'auto',
      });
    } else if (transcriptContainerRef.current) {
      transcriptContainerRef.current.scrollTo({
        top: transcriptContainerRef.current.scrollHeight,
        behavior: smooth ? 'smooth' : 'auto',
      });
    }
  };

  // Smart auto-scroll: auto-scroll when new segments arrive IF user hasn't scrolled up
  useEffect(() => {
    if (!userScrolledUpRef.current) {
      scrollToBottom(true);
    }
  }, [segments, viewMode]);

  useEffect(() => {
    elapsedSecondsRef.current = elapsedSeconds;
  }, [elapsedSeconds]);

  const triggerHaptic = () => {
    if (typeof window !== 'undefined' && 'vibrate' in navigator) {
      try { navigator.vibrate(40); } catch {}
    }
  };

  // Audio visualizer: use getUserMedia only in browser; on Tauri/desktop use a
  // simulated visualizer to avoid triggering CoreAudio's TCC speech-recognition
  // preflight check (which crashes if NSSpeechRecognitionUsageDescription is
  // not readable from the binary — a known macOS 15 + WKWebView issue).
  useEffect(() => {
    if (!isRecording || isPaused) {
      if (animIdRef.current) cancelAnimationFrame(animIdRef.current);
      if (scriptNodeRef.current) {
        try { scriptNodeRef.current.disconnect(); } catch {}
        scriptNodeRef.current = null;
      }
      if (mediaStreamRef.current) {
        mediaStreamRef.current.getTracks().forEach((t) => t.stop());
        mediaStreamRef.current = null;
      }
      if (audioCtxRef.current && audioCtxRef.current.state !== 'closed') {
        try { audioCtxRef.current.close(); } catch {}
        audioCtxRef.current = null;
      }
      setAudioData(null);
      setVadState('silence');
      return;
    }

    let isCancelled = false;

    const isTauriEnv = isTauri();

    if (isTauriEnv) {
      const dataArray = new Uint8Array(64);
      const updateSimulatedAudio = () => {
        if (isCancelled) return;
        for (let i = 0; i < dataArray.length; i++) {
          dataArray[i] = Math.floor(Math.random() * 140 + 20);
        }
        setAudioData(new Uint8Array(dataArray));
        setVadState('speech');
        animIdRef.current = requestAnimationFrame(updateSimulatedAudio);
      };
      updateSimulatedAudio();
      return () => { isCancelled = true; if (animIdRef.current) cancelAnimationFrame(animIdRef.current); };
    }

    async function initMicAudio() {
      try {
        const stream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
        if (isCancelled) {
          stream.getTracks().forEach((t) => t.stop());
          return;
        }

        mediaStreamRef.current = stream;
        const AudioContextClass = window.AudioContext || (window as any).webkitAudioContext;
        const audioCtx = new AudioContextClass();
        audioCtxRef.current = audioCtx;

        const source = audioCtx.createMediaStreamSource(stream);
        const analyser = audioCtx.createAnalyser();
        analyser.fftSize = 128;
        source.connect(analyser);
        analyserRef.current = analyser;

        jsPcmBufferRef.current = [];
        const scriptNode = audioCtx.createScriptProcessor(4096, 1, 1);
        scriptNodeRef.current = scriptNode;

        scriptNode.onaudioprocess = (e) => {
          if (isCancelled) return;
          const input = e.inputBuffer.getChannelData(0);
          for (let i = 0; i < input.length; i++) {
            jsPcmBufferRef.current.push(input[i]);
          }
        };

        source.connect(scriptNode);
        scriptNode.connect(audioCtx.destination);

        const bufferLength = analyser.frequencyBinCount;
        const dataArray = new Uint8Array(bufferLength);

        const updateAudio = () => {
          if (!analyserRef.current || isCancelled) return;
          analyserRef.current.getByteFrequencyData(dataArray);
          setAudioData(new Uint8Array(dataArray));

          let sum = 0;
          for (let i = 0; i < dataArray.length; i++) {
            sum += dataArray[i] * dataArray[i];
          }
          const rms = Math.sqrt(sum / dataArray.length);
          setVadState(rms > 15 ? 'speech' : 'silence');

          animIdRef.current = requestAnimationFrame(updateAudio);
        };

        updateAudio();
      } catch (err: any) {
        console.warn('Microphone access notice:', err?.message);
        setErrorMsg('Microphone permission is denied. Please open System Settings -> Privacy & Security -> Microphone and turn ON access for Minutes.');
        const dataArray = new Uint8Array(64);
        const updateSimulatedAudio = () => {
          if (isCancelled) return;
          for (let i = 0; i < dataArray.length; i++) {
            dataArray[i] = Math.floor(Math.random() * 120 + 20);
          }
          setAudioData(new Uint8Array(dataArray));
          setVadState('speech');
          animIdRef.current = requestAnimationFrame(updateSimulatedAudio);
        };
        updateSimulatedAudio();
      }
    }

    initMicAudio();

    return () => {
      isCancelled = true;
      if (animIdRef.current) cancelAnimationFrame(animIdRef.current);
      if (scriptNodeRef.current) {
        try { scriptNodeRef.current.disconnect(); } catch {}
        scriptNodeRef.current = null;
      }
      if (mediaStreamRef.current) {
        mediaStreamRef.current.getTracks().forEach((t) => t.stop());
        mediaStreamRef.current = null;
      }
      if (audioCtxRef.current && audioCtxRef.current.state !== 'closed') {
        try { audioCtxRef.current.close(); } catch {}
        audioCtxRef.current = null;
      }
    };
  }, [isRecording, isPaused]);

  // Recording Duration Timer Tick
  useEffect(() => {
    if (isRecording && !isPaused) {
      timerRef.current = setInterval(() => {
        setElapsedSeconds((prev) => prev + 1);
      }, 1000);
    } else if (timerRef.current) {
      clearInterval(timerRef.current);
    }
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [isRecording, isPaused]);

  // iOS-Compatible Web Speech Recognition fallback.
  // On Tauri/macOS desktop, skip this: webkitSpeechRecognition also triggers
  // CoreAudio's TCC preflight for NSSpeechRecognitionUsageDescription and
  // crashes the app. The native Whisper engine (get_live_transcript) is used
  // on desktop instead.
  useEffect(() => {
    if (!isRecording || isPaused) {
      if (recognitionRef.current) {
        try { recognitionRef.current.stop(); } catch {}
        recognitionRef.current = null;
      }
      return;
    }

    // Skip Web Speech API on Tauri — native Whisper handles transcription.
    if (isTauri()) return;

    const SpeechRecognition = (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;

    if (SpeechRecognition) {
      try {
        const recognition = new SpeechRecognition();
        // iOS WebKit requires continuous = false to prevent silent failure
        recognition.continuous = false;
        recognition.interimResults = true;
        recognition.lang = settings.stt_language === 'auto' ? 'en-US' : (settings.stt_language || 'en-US');

        recognition.onstart = () => {
          setErrorMsg(null);
        };

        recognition.onresult = (event: any) => {
          const now = Date.now();

          for (let i = event.resultIndex; i < event.results.length; ++i) {
            const transcriptSegment = event.results[i][0].transcript.trim();
            const confidence = event.results[i][0].confidence || 0.95;

            const isMockTest = /^(test|testing|test test|audio test|sample|dictation)$/i.test(transcriptSegment);

            // Only append non-empty real user speech (excluding WebKit simulator dummy 'test' dictation)
            if (
              transcriptSegment.length > 1 &&
              !isMockTest &&
              transcriptSegment.toLowerCase() !== lastProcessedTextRef.current.toLowerCase()
            ) {
              lastProcessedTextRef.current = transcriptSegment;
              lastSegmentTimeRef.current = now;

              const currentSecs = elapsedSecondsRef.current;
              const startTime = Math.max(0, currentSecs - 2);

              const newSeg: TranscriptSegment = {
                id: `seg_${now}_${Math.random().toString(36).substring(2, 6)}`,
                meeting_id: 'live',
                speaker_label: 'Speaker',
                start_time: startTime,
                end_time: Math.max(startTime + 1, currentSecs),
                text: transcriptSegment,
                confidence
              };

              // Replace last interim segment or append if final
              setSegments((prev) => {
                const isFinal = event.results[i].isFinal;
                if (!isFinal && prev.length > 0 && prev[prev.length - 1].id.startsWith('seg_interim_')) {
                  const updated = [...prev];
                  updated[updated.length - 1] = { ...newSeg, id: `seg_interim_${now}` };
                  return updated;
                }
                return [...prev, { ...newSeg, id: isFinal ? `seg_final_${now}` : `seg_interim_${now}` }];
              });
            }
          }
        };

        recognition.onerror = (event: any) => {
          if (event.error !== 'no-speech' && event.error !== 'aborted') {
            console.warn('Speech recognition status:', event.error);
          }
        };

        recognition.onend = () => {
          // Restart immediately on iOS Safari / WKWebView after phrase completion
          if (isRecording && !isPaused) {
            setTimeout(() => {
              if (isRecording && !isPaused && recognitionRef.current === recognition) {
                try { recognition.start(); } catch {}
              }
            }, 150);
          }
        };

        recognition.start();
        recognitionRef.current = recognition;
      } catch (err: any) {
        console.warn('Speech recognition initialization warning:', err);
      }
    }

    return () => {
      if (recognitionRef.current) {
        try { recognitionRef.current.stop(); } catch {}
        recognitionRef.current = null;
      }
    };
  }, [isRecording, isPaused, settings.stt_language]);

  // Real-time Native Whisper STT Live Polling
  useEffect(() => {
    if (!isRecording || isPaused) return;

    const interval = setInterval(async () => {
      try {
        let liveWhisperSegs: any[] = [];
        try {
          liveWhisperSegs = await api.getLiveTranscript();
        } catch {}

        if ((!liveWhisperSegs || liveWhisperSegs.length === 0) && jsPcmBufferRef.current.length > 0 && audioCtxRef.current) {
          if (isTauri()) {
            try {
              const { invoke } = await import('@tauri-apps/api/core');
              const sr = audioCtxRef.current.sampleRate || 44100;
              const maxSamples = Math.floor(sr * 12);
              const pcmChunk = jsPcmBufferRef.current.slice(-maxSamples);
              const rawSegs = await invoke<any[]>('transcribe_pcm_buffer', {
                pcmData: pcmChunk,
                sampleRate: sr,
              });
              if (rawSegs && rawSegs.length > 0) {
                liveWhisperSegs = rawSegs;
              }
            } catch (e) {
              console.warn('JS mic live stream STT notice:', e);
            }
          }
        }

        if (liveWhisperSegs && liveWhisperSegs.length > 0) {
          setSegments((prev) => {
            const updated = [...prev];
            liveWhisperSegs.forEach((s: any) => {
              const text = (s.text || '').trim();
              if (!text) return;

              // Filter out bracketed hallucination tokens like [Motor], [Music], (Silence)
              if ((text.startsWith('[') && text.endsWith(']')) || (text.startsWith('(') && text.endsWith(')'))) {
                return;
              }

              const cleanNew = text.toLowerCase().replace(/[^\w\s\u4e00-\u9fff]/g, '');
              if (cleanNew.length < 2) return;

              const startSecs = Math.round(s.start_timestamp);
              const endSecs = Math.round(s.end_timestamp);

              // Check if this text or timestamp range overlaps with any existing segment in the list
              const existingIdx = updated.findIndex((seg) => {
                const segClean = seg.text.toLowerCase().replace(/[^\w\s\u4e00-\u9fff]/g, '');
                if (!segClean) return false;

                // Exact or substring match
                if (segClean === cleanNew || segClean.includes(cleanNew) || cleanNew.includes(segClean)) {
                  return true;
                }

                // Check for overlapping timestamp range (within 5 seconds) + prefix/phrase similarity
                const startDiff = Math.abs(seg.start_time - startSecs);
                const endDiff = Math.abs(seg.end_time - endSecs);
                if (startDiff <= 5 || endDiff <= 5) {
                  const prefix1 = segClean.slice(0, 10);
                  const prefix2 = cleanNew.slice(0, 10);
                  if (prefix1 === prefix2 || segClean.startsWith(prefix2) || cleanNew.startsWith(prefix1)) {
                    return true;
                  }
                }
                return false;
              });

              if (existingIdx !== -1) {
                // Update existing segment in-place with latest/longest phrase
                if (text.length >= updated[existingIdx].text.length) {
                  updated[existingIdx] = {
                    ...updated[existingIdx],
                    start_time: Math.min(updated[existingIdx].start_time, startSecs),
                    end_time: Math.max(updated[existingIdx].end_time, endSecs),
                    text: text,
                  };
                }
              } else {
                // Push new unique segment
                updated.push({
                  id: `seg_live_${Date.now()}_${Math.random().toString(36).substring(2, 6)}`,
                  meeting_id: 'live',
                  speaker_label: s.speaker || 'Speaker',
                  start_time: startSecs,
                  end_time: endSecs,
                  text: text,
                  confidence: 0.95,
                });
              }
            });

            // Always keep segments sorted chronologically by start_time
            updated.sort((a, b) => a.start_time - b.start_time);

            // Consolidate adjacent continuous segments into unified paragraph cards (if gap <= 5s and same speaker)
            const consolidated: any[] = [];
            updated.forEach((seg) => {
              if (consolidated.length === 0) {
                consolidated.push({ ...seg });
                return;
              }
              const last = consolidated[consolidated.length - 1];
              const gap = seg.start_time - last.end_time;
              const isSameSpeaker = (last.speaker_label || 'Speaker') === (seg.speaker_label || 'Speaker');

              if (isSameSpeaker && gap <= 5 && (last.text.length + seg.text.length) < 600) {
                const cleanLast = last.text.trim();
                const cleanSeg = seg.text.trim();
                if (cleanLast.toLowerCase() !== cleanSeg.toLowerCase() && !cleanLast.toLowerCase().includes(cleanSeg.toLowerCase())) {
                  last.text = `${cleanLast} ${cleanSeg}`;
                  last.end_time = Math.max(last.end_time, seg.end_time);
                }
              } else {
                consolidated.push({ ...seg });
              }
            });

            return consolidated;
          });
        }
      } catch (e) {
        console.warn('Live transcript polling notice:', e);
      }
    }, 1500);

    return () => clearInterval(interval);
  }, [isRecording, isPaused]);

  useEffect(() => {
    if (isTauri()) {
      import('@tauri-apps/api/core').then(({ invoke }) => {
        invoke('request_microphone_permission').catch((e) => console.warn('Mic perm request notice:', e));
      });
    }
  }, []);

  const handleStartRecording = async () => {
    triggerHaptic();
    try {
      const defaultTitle = title.trim() || `Meeting ${new Date().toLocaleDateString()} ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`;
      setTitle(defaultTitle);
      setSegments([]);
      setElapsedSeconds(0);
      elapsedSecondsRef.current = 0;
      lastProcessedTextRef.current = '';
      lastSegmentTimeRef.current = 0;
      setIsRecording(true);
      if (setIsRecordingGlobal) setIsRecordingGlobal(true);
      setIsPaused(false);
      setErrorMsg(null);

      if (isTauri()) {
        try {
          const { invoke } = await import('@tauri-apps/api/core');
          await invoke('request_microphone_permission');
          await invoke('start_audio_capture');
        } catch (e: any) {
          console.warn('Native start_audio_capture notice:', e);
        }
      }
    } catch (err: any) {
      setErrorMsg(err?.message || 'Failed to initialize microphone capture');
    }
  };

  const handlePauseRecording = () => {
    triggerHaptic();
    setIsPaused((prev) => !prev);
  };

  const handleStopRecording = async () => {
    triggerHaptic();
    if (recognitionRef.current) {
      try { recognitionRef.current.stop(); } catch {}
    }
    setIsRecording(false);
    if (setIsRecordingGlobal) setIsRecordingGlobal(false);
    setIsPaused(false);

    try {
      const currentElapsed = elapsedSecondsRef.current;
      const meetingTitle = title.trim() || `Consultation ${new Date().toLocaleDateString()} ${new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`;
      const meeting = await api.startRecording(meetingTitle, currentSpecialty);

      let finalSegments = [...segments];
      if (isTauri()) {
        try {
          const { invoke } = await import('@tauri-apps/api/core');
          const nativeSegs = await invoke<TranscriptSegment[]>('stop_audio_capture_and_transcribe', { meetingId: meeting.id });
          if (nativeSegs && nativeSegs.length > 0) {
            finalSegments = nativeSegs;
          }
        } catch (e) {
          console.warn('Native audio capture stop notice:', e);
        }

        if (finalSegments.length === 0 && jsPcmBufferRef.current.length > 0 && audioCtxRef.current) {
          try {
            const { invoke } = await import('@tauri-apps/api/core');
            const sr = audioCtxRef.current.sampleRate || 44100;
            const rawSegs = await invoke<any[]>('transcribe_pcm_buffer', {
              pcmData: jsPcmBufferRef.current,
              sampleRate: sr,
            });
            if (rawSegs && rawSegs.length > 0) {
              finalSegments = rawSegs.map((s, idx) => ({
                id: `seg_${Date.now()}_${idx}`,
                meeting_id: meeting.id,
                speaker_label: s.speaker || 'Speaker 1',
                start_time: Math.round(s.start_timestamp),
                end_time: Math.round(s.end_timestamp),
                text: s.text,
                confidence: 0.95,
              }));
            }
          } catch (e) {
            console.warn('Final JS mic transcription notice:', e);
          }
        }
      }

      await api.saveMeetingSegments(meeting.id, currentElapsed, finalSegments);

      meeting.duration_seconds = currentElapsed;
      meeting.status = 'summarizing';
      meeting.transcript_count = finalSegments.length;

      // Save meeting immediately to list without switching screens
      onMeetingCreated(meeting, false);

      // Show instant confirmation notice
      setNoticeMsg(`Saved "${meeting.title}". AI summary is generating in the background.`);
      setTimeout(() => setNoticeMsg(null), 6000);

      // Immediately reset recorder so doctor is ready for the next patient
      setTitle('');
      setSegments([]);
      setElapsedSeconds(0);
      elapsedSecondsRef.current = 0;
      lastProcessedTextRef.current = '';
      lastSegmentTimeRef.current = 0;
      setLiveSOAP(undefined);

      // Asynchronous background summarization
      api.generateSummary(meeting.id).then(async (summaryData) => {
        const updated: Meeting = {
          ...meeting,
          status: 'completed',
          summary: summaryData.summary,
          action_items: summaryData.action_items,
          key_decisions: summaryData.key_decisions,
        };
        onMeetingUpdated?.(updated);
      }).catch((err) => {
        console.warn('Background summary notice:', err);
        onMeetingUpdated?.({ ...meeting, status: 'completed' });
      });

    } catch (err: any) {
      setErrorMsg('Error finalizing transcript: ' + err.message);
    }
  };

  const handleFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    setNoticeMsg(`Importing and transcribing "${file.name}"...`);
    try {
      const importedMeeting = await api.importAudioFile(file, settings.default_specialty, settings.enable_speaker_diarization);
      onMeetingCreated(importedMeeting, true);
    } catch (err: any) {
      setErrorMsg('Failed to process audio file: ' + err.message);
    } finally {
      setNoticeMsg(null);
    }
  };

  const handleGenerateSOAP = async (spec?: SpecialtyType) => {
    const selectedSpec = spec || currentSpecialty;
    if (onSpecialtyChange && spec) {
      onSpecialtyChange(spec);
    }
    setIsGeneratingSOAP(true);
    try {
      const fullTranscript = segments.map((s) => `${s.speaker_label}: ${s.text}`).join('\n');
      const note = await api.generateSOAPNoteDirect(fullTranscript, selectedSpec);
      setLiveSOAP(note);
    } catch (e: any) {
      console.warn('SOAP generation notice:', e);
    } finally {
      setIsGeneratingSOAP(false);
    }
  };

  const copyQuickSummary = () => {
    if (!liveSOAP) return;
    const text = `S: ${liveSOAP.subjective}\nO: ${liveSOAP.objective}\nA: ${liveSOAP.assessment}\nP: ${liveSOAP.plan}`;
    navigator.clipboard.writeText(text);
    setCopiedQuick(true);
    setTimeout(() => setCopiedQuick(false), 2000);
  };

  const formatTime = (secs: number) => {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  };

  return (
    <div className={`h-full flex flex-col p-3 md:p-5 mx-auto w-full space-y-3 md:space-y-4 overflow-y-auto scroll-touch pb-20 md:pb-12 ${viewMode === 'sidecar' ? 'max-w-7xl' : 'max-w-5xl'}`}>
      <input
        type="file"
        ref={fileInputRef}
        onChange={handleFileChange}
        accept="audio/*,.wav,.mp3,.m4a,.aac"
        className="hidden"
      />

      {/* Background Task Notice */}
      {noticeMsg && (
        <div className="p-3.5 rounded-xl bg-indigo-950/70 border border-indigo-800/80 text-indigo-200 text-xs flex items-center justify-between shadow-lg">
          <div className="flex items-center gap-2.5">
            <RefreshCw className="w-4 h-4 animate-spin text-indigo-400 flex-shrink-0" />
            <span className="font-medium">{noticeMsg}</span>
          </div>
          <button
            onClick={() => setNoticeMsg(null)}
            className="text-indigo-400 hover:text-white text-xs px-2 py-0.5 rounded transition-colors"
          >
            Dismiss
          </button>
        </div>
      )}

      {/* Control Header Card */}
      <div className="p-3.5 md:p-4 rounded-xl bg-slate-900/60 border border-slate-800/80 flex flex-col md:flex-row md:items-center justify-between gap-3 shadow-lg">
        <div className="flex-1 space-y-1.5">
          <div className="flex items-center gap-2">
            <input
              type="text"
              placeholder="Meeting title (optional)..."
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              disabled={isRecording}
              className="w-full bg-slate-950/80 border border-slate-800 rounded-lg px-3 py-1.5 text-slate-100 font-medium text-sm focus:outline-none focus:border-indigo-500/60 focus:ring-1 focus:ring-indigo-500/60 placeholder-slate-500 min-h-[38px]"
            />
            {/* View Switcher Pill */}
            <div className="flex items-center bg-slate-950 border border-slate-800 rounded-lg p-0.5 flex-shrink-0">
              <button
                onClick={() => setViewMode('standard')}
                title="Standard View"
                className={`flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-semibold transition-all ${
                  viewMode === 'standard'
                    ? 'bg-indigo-600/20 text-indigo-300 border border-indigo-500/40 shadow-sm'
                    : 'text-slate-400 hover:text-slate-200'
                }`}
              >
                <Sparkles className="w-3.5 h-3.5" />
                <span className="hidden sm:inline">Standard</span>
              </button>
              <button
                onClick={() => setViewMode('sidecar')}
                title="Sidecar View"
                className={`flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-semibold transition-all ${
                  viewMode === 'sidecar'
                    ? 'bg-indigo-600/20 text-indigo-300 border border-indigo-500/40 shadow-sm'
                    : 'text-slate-400 hover:text-slate-200'
                }`}
              >
                <Layout className="w-3.5 h-3.5 text-indigo-400" />
                <span>Sidecar</span>
              </button>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-3 text-xs text-slate-400 font-medium px-1">
            <span className="flex items-center gap-1.5">
              <Clock className="w-3.5 h-3.5 text-indigo-400" />
              Duration: <strong className="text-slate-200 font-mono">{formatTime(elapsedSeconds)}</strong>
            </span>
            <span className="flex items-center gap-1.5">
              VAD:
              <span className={`px-2 py-0.5 rounded text-[11px] font-medium ${
                vadState === 'speech' ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20' : 'bg-slate-800/80 text-slate-400'
              }`}>
                {vadState === 'speech' ? 'Speech Active' : 'Listening'}
              </span>
            </span>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {/* Always on Top Pin Button (Desktop) */}
          {onToggleAlwaysOnTop && (
            <button
              onClick={() => onToggleAlwaysOnTop(!isAlwaysOnTop)}
              title={isAlwaysOnTop ? 'Unpin Window' : 'Keep Window Always on Top'}
              className={`p-2 rounded-lg border transition-all ${
                isAlwaysOnTop
                  ? 'bg-indigo-600 text-white border-indigo-400 shadow-md shadow-indigo-950/50'
                  : 'bg-slate-800 text-slate-400 border-slate-700 hover:text-white'
              }`}
            >
              <Pin className={`w-4 h-4 ${isAlwaysOnTop ? 'fill-current' : ''}`} />
            </button>
          )}

          {/* Quick Copy Button */}
          {liveSOAP && (
            <button
              onClick={copyQuickSummary}
              title="Copy Summary"
              className="p-2 rounded-lg bg-slate-800 text-slate-300 border border-slate-700 hover:text-white transition-all flex items-center gap-1"
            >
              {copiedQuick ? <Check className="w-4 h-4 text-emerald-400" /> : <Copy className="w-4 h-4" />}
            </button>
          )}

          {!isRecording ? (
            <>
              <button
                onClick={handleStartRecording}
                className="flex items-center justify-center gap-2 bg-red-600 hover:bg-red-500 text-white font-medium px-4 py-2 rounded-lg transition-colors active:scale-[0.98] text-xs md:text-sm min-h-[38px]"
              >
                <Mic className="w-4 h-4" />
                <span>Start Recording</span>
              </button>

              <button
                onClick={() => fileInputRef.current?.click()}
                title="Import Audio File (.wav, .mp3, .m4a)"
                className="p-2 bg-slate-800/80 hover:bg-slate-800 text-slate-300 rounded-lg border border-slate-700/80 transition-colors active:scale-[0.98] min-h-[38px] min-w-[38px] flex items-center justify-center"
              >
                <Upload className="w-4 h-4" />
              </button>
            </>
          ) : (
            <>
              <button
                onClick={handlePauseRecording}
                className="flex items-center gap-1.5 bg-slate-800/80 hover:bg-slate-800 text-slate-200 font-medium px-3.5 py-2 rounded-lg border border-slate-700/80 transition-colors active:scale-[0.98] text-xs md:text-sm min-h-[38px]"
              >
                {isPaused ? <Play className="w-4 h-4 text-emerald-400" /> : <Pause className="w-4 h-4 text-amber-400" />}
                <span>{isPaused ? 'Resume' : 'Pause'}</span>
              </button>

              <button
                onClick={handleStopRecording}
                className="flex items-center gap-1.5 bg-slate-800/80 hover:bg-red-950/40 text-red-300 font-medium px-3.5 py-2 rounded-lg border border-red-900/40 transition-colors active:scale-[0.98] text-xs md:text-sm min-h-[38px]"
              >
                <Square className="w-4 h-4 text-red-400" />
                <span>Finish & Summarize</span>
              </button>
            </>
          )}
        </div>
      </div>

      {errorMsg && (
        <div className="p-3 rounded-lg bg-red-950/40 border border-red-900/60 text-red-300 text-xs flex items-center gap-2.5">
          <AlertCircle className="w-4 h-4 text-red-400 flex-shrink-0" />
          <span>{errorMsg}</span>
        </div>
      )}

      {/* RENDER VIEW MODE: EHR SIDECAR DUAL-PANE vs STANDARD VIEW */}
      {viewMode === 'sidecar' ? (
        <div className="flex-1 grid grid-cols-1 lg:grid-cols-2 gap-4 overflow-hidden min-h-[480px]">
          {/* Left Column: Live Speech Stream */}
          <div className="flex flex-col h-full bg-slate-900/60 rounded-xl border border-slate-800/80 p-3.5 overflow-hidden shadow-lg relative">
            <div className="flex items-center justify-between pb-2 mb-2 border-b border-slate-800/60 text-xs">
              <span className="font-bold text-slate-200 uppercase tracking-wider flex items-center gap-1.5 text-xs">
                <Mic className="w-3.5 h-3.5 text-emerald-400" /> Live Speech Feed
              </span>
              <span className="text-[11px] text-slate-400 font-mono">{segments.length} Segments</span>
            </div>

            <div
              ref={sidecarContainerRef}
              onScroll={handleScroll}
              className="flex-1 overflow-y-auto space-y-2.5 p-2.5 bg-slate-950/80 rounded-lg border border-slate-900 custom-scrollbar scroll-touch"
            >
              {segments.length === 0 ? (
                <div className="h-full flex flex-col items-center justify-center text-center p-6 text-slate-500 italic text-xs space-y-2">
                  <Mic className="w-6 h-6 text-slate-600 opacity-60" />
                  <p>{isRecording ? 'Listening for microphone speech input...' : 'Click "Start Recording" or import an audio file to start consultation transcript feed.'}</p>
                </div>
              ) : (
                segments.map((seg) => (
                  <div key={seg.id} className="p-3 rounded-lg bg-slate-900/90 border border-slate-800/80 space-y-1">
                    <div className="flex items-center justify-between text-[10px] text-slate-400 border-b border-slate-800/40 pb-1">
                      <span className="font-semibold text-emerald-400 uppercase">{seg.speaker_label || 'Speaker'}</span>
                      <span className="font-mono">{formatTime(seg.start_time)} – {formatTime(seg.end_time)}</span>
                    </div>
                    <p className="text-slate-200 text-xs leading-relaxed font-sans pt-0.5">{seg.text}</p>
                  </div>
                ))
              )}
            </div>

            {/* Smart Scroll User Pause Indicator */}
            {isScrolledUp && isRecording && (
              <button
                type="button"
                onClick={() => scrollToBottom(true)}
                className="absolute bottom-5 left-1/2 -translate-x-1/2 bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold px-3.5 py-1.5 rounded-full shadow-xl shadow-indigo-950/80 border border-indigo-400/40 flex items-center gap-1.5 transition-all active:scale-95 z-20 animate-fade-in"
              >
                <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
                <span>Jump to Live</span>
                <ChevronDown className="w-3.5 h-3.5" />
              </button>
            )}
          </div>

          {/* Right Column: Live SOAP Note Preview */}
          <div className="flex flex-col h-full overflow-hidden bg-slate-900/40 rounded-xl border border-slate-800/80 p-1 shadow-lg">
            <SOAPNoteView
              soapNote={liveSOAP}
              isGenerating={isGeneratingSOAP}
              onGenerateSOAP={handleGenerateSOAP}
              currentSpecialty={currentSpecialty}
              onSpecialtyChange={(s) => {
                if (onSpecialtyChange) onSpecialtyChange(s);
              }}
            />
          </div>
        </div>
      ) : (
        /* STANDARD VIEW */
        <>
          {/* Audio Waveform Canvas */}
          <AudioVisualizer isRecording={isRecording} isPaused={isPaused} audioData={audioData} />

          {/* Live Transcript Stream Container */}
          <div className="flex-1 rounded-xl p-4 md:p-5 bg-slate-900/50 border border-slate-800/80 flex flex-col min-h-[280px] relative overflow-hidden shadow-lg">
            <div className="flex items-center justify-between pb-3 mb-3 border-b border-slate-800/80 text-xs">
              <span className="font-medium text-slate-300 flex items-center gap-2">
                <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                Live Transcript Stream
              </span>
              <span className="text-slate-400 font-mono text-[11px]">{segments.length} Segments</span>
            </div>

            <div
              ref={transcriptContainerRef}
              onScroll={handleScroll}
              className="flex-1 overflow-y-auto space-y-3 pr-1 scroll-touch"
            >
              {segments.length === 0 ? (
                <div className="h-full flex flex-col items-center justify-center text-center p-6 space-y-3 my-auto text-slate-400">
                  <div className="w-12 h-12 rounded-xl bg-slate-900 border border-slate-800 flex items-center justify-center">
                    <Mic className="w-5 h-5 text-emerald-400 opacity-80" />
                  </div>
                  <div className="space-y-1 max-w-sm">
                    <p className="text-xs font-medium text-slate-300">Ready to record consultation</p>
                    <p className="text-[11px] text-slate-400 leading-relaxed">
                      Speech will transcribe in real-time directly on device using local Whisper GPU acceleration.
                    </p>
                  </div>
                </div>
              ) : (
                segments.map((seg) => (
                  <div
                    key={seg.id}
                    className="p-4 rounded-xl bg-slate-900/80 border border-slate-800/90 shadow-sm space-y-2 hover:border-slate-700/80 transition-all"
                  >
                    <div className="flex items-center justify-between border-b border-slate-800/60 pb-2">
                      <div className="flex items-center gap-2">
                        <span className="w-2 h-2 rounded-full bg-emerald-400"></span>
                        <span className="text-[11px] font-semibold text-slate-300 uppercase tracking-wider">
                          {seg.speaker_label || 'Speaker'}
                        </span>
                      </div>
                      <span className="text-emerald-400 font-mono text-[11px] bg-slate-950/80 px-2 py-0.5 rounded border border-slate-800 flex items-center gap-1.5">
                        <Clock className="w-3 h-3 text-slate-400" />
                        {formatTime(seg.start_time)} – {formatTime(seg.end_time)}
                      </span>
                    </div>
                    <p className="text-slate-100 text-sm md:text-[15px] leading-relaxed font-sans pt-0.5 tracking-normal">
                      {seg.text}
                    </p>
                  </div>
                ))
              )}
            </div>

            {/* Smart Scroll User Pause Indicator */}
            {isScrolledUp && isRecording && (
              <button
                type="button"
                onClick={() => scrollToBottom(true)}
                className="absolute bottom-6 left-1/2 -translate-x-1/2 bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold px-4 py-2 rounded-full shadow-xl shadow-indigo-950/80 border border-indigo-400/40 flex items-center gap-2 transition-all active:scale-95 z-20 animate-fade-in"
              >
                <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
                <span>Jump to Latest Segment</span>
                <ChevronDown className="w-3.5 h-3.5" />
              </button>
            )}
          </div>
        </>
      )}
    </div>
  );
};


import React, { useState, useEffect } from 'react';
import { ArrowLeft, Calendar, Clock, Download, CheckSquare, Lightbulb, FileText, Trash2, Sparkles, Share2, Play, Pause, RotateCcw, Stethoscope, Users, User, RefreshCw, ArrowLeftRight } from 'lucide-react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { Meeting, TranscriptSegment, SOAPNote, SpecialtyType } from '../types';
import { api, isTauri } from '../services/api';
import { SOAPNoteView } from './SOAPNoteView';
import { ExportModal } from './ExportModal';

interface MeetingDetailProps {
  meetingId: string;
  onBack: () => void;
  onDelete: (id: string) => void;
}

export const MeetingDetail: React.FC<MeetingDetailProps> = ({ meetingId, onBack, onDelete }) => {
  const [meeting, setMeeting] = useState<Meeting | null>(null);
  const [segments, setSegments] = useState<TranscriptSegment[]>([]);
  const [soapNote, setSoapNote] = useState<SOAPNote | undefined>(undefined);
  const [isGeneratingSOAP, setIsGeneratingSOAP] = useState(false);
  const [isGeneratingSummary, setIsGeneratingSummary] = useState(false);
  const [isDiarizing, setIsDiarizing] = useState(false);
  const [currentSpecialty, setCurrentSpecialty] = useState<SpecialtyType>('General Practice');
  const [activeTab, setActiveTab] = useState<'soap' | 'summary' | 'transcript'>('soap');
  const [actionItems, setActionItems] = useState<{ text: string; done: boolean }[]>([]);
  const [isExportModalOpen, setIsExportModalOpen] = useState(false);

  // Audio Playback State & HTML5 Audio Element Ref
  const [isPlaying, setIsPlaying] = useState(false);
  const [playbackTime, setPlaybackTime] = useState(0);
  const [playbackSpeed, setPlaybackSpeed] = useState<1 | 1.25 | 1.5 | 2>(1);
  const audioRef = React.useRef<HTMLAudioElement | null>(null);

  useEffect(() => {
    let isMounted = true;
    let pollInterval: ReturnType<typeof setInterval> | null = null;

    const fetchDetails = async () => {
      try {
        const { meeting: m, segments: s, soap_note } = await api.getMeeting(meetingId);
        if (isMounted && m) {
          setMeeting(m);
          setSegments(s);
          if (soap_note) setSoapNote(soap_note);
          if (m.specialty) setCurrentSpecialty(m.specialty as SpecialtyType);
          if (m.action_items) {
            setActionItems(m.action_items.map(item => ({ text: item, done: false })));
          }
          if (m.status !== 'summarizing' && m.summary) {
            if (pollInterval) {
              clearInterval(pollInterval);
              pollInterval = null;
            }
          }
        }
      } catch (err) {
        console.warn('Meeting load notice:', err);
      }
    };

    fetchDetails().then(() => {
      if (isMounted) {
        pollInterval = setInterval(fetchDetails, 2500);
      }
    });

    return () => {
      isMounted = false;
      if (pollInterval) clearInterval(pollInterval);
    };
  }, [meetingId]);

  const handleAutoDiarize = async () => {
    setIsDiarizing(true);
    try {
      const updated = await api.diarizeMeeting(meetingId);
      if (updated && updated.length > 0) {
        setSegments(updated);
      }
    } catch (e) {
      console.warn('Auto-diarize notice:', e);
    } finally {
      setIsDiarizing(false);
    }
  };

  const handleToggleSpeakerRole = async (segId: string, currentLabel: string) => {
    const nextLabel = currentLabel.toLowerCase().includes('doc') ? 'Patient' : 'Doctor';
    await api.updateSpeakerLabel(meetingId, segId, nextLabel);
    setSegments(prev => prev.map(s => s.id === segId ? { ...s, speaker_label: nextLabel } : s));
  };

  const handleSwapAllRoles = async () => {
    const updated = segments.map(s => {
      const isDoc = s.speaker_label.toLowerCase().includes('doc');
      const isPat = s.speaker_label.toLowerCase().includes('pat');
      let newLbl = s.speaker_label;
      if (isDoc) newLbl = 'Patient';
      else if (isPat) newLbl = 'Doctor';
      else if (s.speaker_label === 'Speaker 1') newLbl = 'Speaker 2';
      else if (s.speaker_label === 'Speaker 2') newLbl = 'Speaker 1';
      return { ...s, speaker_label: newLbl };
    });
    setSegments(updated);
    for (const seg of updated) {
      await api.updateSpeakerLabel(meetingId, seg.id, seg.speaker_label);
    }
  };

  const handleGenerateSummary = async () => {
    setIsGeneratingSummary(true);
    try {
      const sum = await api.generateSummary(meetingId);
      if (sum) {
        setMeeting(prev => prev ? {
          ...prev,
          summary: sum.summary,
          action_items: sum.action_items,
          key_decisions: sum.key_decisions,
        } : null);
        if (sum.action_items) {
          setActionItems(sum.action_items.map(item => ({ text: item, done: false })));
        }
      }
    } catch (e: any) {
      console.warn('Generate summary error:', e);
      alert(`AI Generation Notice: ${e?.message || e || 'Failed to connect to local AI model'}`);
    } finally {
      setIsGeneratingSummary(false);
    }
  };

  const handleGenerateSOAP = async (spec?: SpecialtyType) => {
    setIsGeneratingSOAP(true);
    try {
      const note = await api.generateSOAPNote(meetingId, spec || currentSpecialty);
      setSoapNote(note);
    } catch (e: any) {
      console.warn('SOAP generation notice:', e);
      alert(`SOAP Note Notice: ${e?.message || e || 'Failed to connect to local AI model'}`);
    } finally {
      setIsGeneratingSOAP(false);
    }
  };

  // Initialize Real HTML5 Audio Player for Recorded/Imported WAV files
  useEffect(() => {
    if (!meeting) return;

    let isMounted = true;
    let objectUrlToRevoke: string | null = null;

    async function initAudio() {
      let src = '';

      // 1. Try loading via native binary IPC directly from disk
      const blobUrl = await api.getMeetingAudioBlobUrl(meetingId);
      if (blobUrl) {
        src = blobUrl;
        objectUrlToRevoke = blobUrl;
      } else if (meeting?.audio_path) {
        if (meeting.audio_path.startsWith('blob:') || meeting.audio_path.startsWith('http:') || meeting.audio_path.startsWith('https:')) {
          src = meeting.audio_path;
        } else if (isTauri()) {
          try {
            src = convertFileSrc(meeting.audio_path);
          } catch (e) {
            console.warn('convertFileSrc notice:', e);
          }
        }
      }

      if (!src) {
        const storedBlob = localStorage.getItem(`minutes_audio_blob_${meeting?.id}`);
        if (storedBlob) src = storedBlob;
      }

      if (!isMounted || !src) return;

      const audio = new Audio(src);
      audio.preload = 'auto';
      audioRef.current = audio;

      const handleTimeUpdate = () => {
        setPlaybackTime(Math.floor(audio.currentTime));
      };
      const handleEnded = () => {
        setIsPlaying(false);
        setPlaybackTime(0);
      };
      const handleError = (e: any) => {
        console.warn('Audio element playback notice:', e);
      };

      audio.addEventListener('timeupdate', handleTimeUpdate);
      audio.addEventListener('ended', handleEnded);
      audio.addEventListener('error', handleError);
    }

    initAudio();

    return () => {
      isMounted = false;
      if (audioRef.current) {
        audioRef.current.pause();
        audioRef.current = null;
      }
      if (objectUrlToRevoke) {
        try {
          URL.revokeObjectURL(objectUrlToRevoke);
        } catch {}
      }
    };
  }, [meeting?.audio_path, meeting?.id, meetingId]);

  // Sync Play/Pause Controls & Playback Speed
  useEffect(() => {
    if (audioRef.current) {
      audioRef.current.playbackRate = playbackSpeed;
      if (isPlaying) {
        audioRef.current.play().catch((err) => {
          console.warn('Audio playback notice:', err);
          setIsPlaying(false);
        });
      } else {
        audioRef.current.pause();
      }
    } else if (isPlaying) {
      setIsPlaying(false);
    }
  }, [isPlaying, playbackSpeed]);

  const seekAudio = (timeSecs: number) => {
    setPlaybackTime(timeSecs);
    if (audioRef.current) {
      audioRef.current.currentTime = timeSecs;
    }
  };

  const handlePlaySegment = (startTime: number) => {
    setPlaybackTime(startTime);
    if (audioRef.current) {
      audioRef.current.currentTime = startTime;
      audioRef.current.play().catch((err) => {
        console.warn('Audio playback notice:', err);
      });
    }
    setIsPlaying(true);
  };

  if (!meeting) {
    return (
      <div className="h-full flex items-center justify-center">
        <div className="w-8 h-8 border-2 border-emerald-500 border-t-transparent rounded-full animate-spin"></div>
      </div>
    );
  }

  const toggleActionItem = (index: number) => {
    setActionItems(prev => prev.map((item, i) => i === index ? { ...item, done: !item.done } : item));
  };

  const getMarkdownContent = () => {
    let text = `# ${meeting.title}\n\n**Date:** ${new Date(meeting.created_at).toLocaleString()}\n**Duration:** ${Math.floor(meeting.duration_seconds / 60)}m ${meeting.duration_seconds % 60}s\n\n`;
    if (soapNote) {
      text += `## CLINICAL SOAP NOTE (${soapNote.specialty})\n\n### SUBJECTIVE\n${soapNote.subjective}\n\n### OBJECTIVE\n${soapNote.objective}\n\n### ASSESSMENT\n${soapNote.assessment}\n\n### PLAN\n${soapNote.plan}\n\n### SUGGESTED ICD-10 CODES\n${soapNote.icd10_suggestions.map(c => `- ${c.code}: ${c.description}`).join('\n')}\n\n`;
    }
    text += `## Full Transcript\n${segments.map(s => `**${s.speaker_label}** (${Math.floor(s.start_time / 60)}:${(s.start_time % 60).toString().padStart(2, '0')}):\n${s.text}`).join('\n\n')}`;
    return text;
  };

  const handleShareNative = async () => {
    const textContent = getMarkdownContent();
    if (typeof navigator !== 'undefined' && 'share' in navigator) {
      try {
        await navigator.share({
          title: meeting.title,
          text: textContent,
        });
        return;
      } catch {}
    }
    handleExportMarkdown();
  };

  const handleExportMarkdown = () => {
    const mdContent = getMarkdownContent();
    const blob = new Blob([mdContent], { type: 'text/markdown' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${meeting.title.toLowerCase().replace(/[^a-z0-9]/g, '_')}_soap_note.md`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const formatDuration = (secs: number) => {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  };

  const cycleSpeed = () => {
    const speeds: (1 | 1.25 | 1.5 | 2)[] = [1, 1.25, 1.5, 2];
    const nextIdx = (speeds.indexOf(playbackSpeed) + 1) % speeds.length;
    setPlaybackSpeed(speeds[nextIdx]);
  };

  return (
    <div className="h-full flex flex-col p-4 pt-[max(1rem,env(safe-area-inset-top))] md:p-6 max-w-5xl mx-auto w-full space-y-4 md:space-y-6 overflow-y-auto scroll-touch pb-24 md:pb-16">
      {/* Back Button & Action Controls Header */}
      <div className="flex items-center justify-between gap-2 border-b border-slate-800/80 pb-3">
        <button
          onClick={onBack}
          className="flex items-center gap-2 text-slate-300 hover:text-white font-medium text-xs md:text-sm px-3.5 py-2 rounded-lg bg-slate-900/60 border border-slate-800/80 transition-colors active:scale-[0.98] min-h-[44px]"
        >
          <ArrowLeft className="w-4 h-4" />
          <span>Back</span>
        </button>

        <div className="flex items-center gap-2">
          <button
            onClick={handleShareNative}
            className="flex items-center gap-1.5 bg-emerald-600/10 hover:bg-emerald-600/20 text-emerald-300 font-medium px-3.5 py-2 rounded-lg border border-emerald-500/20 transition-colors text-xs active:scale-[0.98] min-h-[44px]"
          >
            <Share2 className="w-4 h-4" />
            <span>Share</span>
          </button>

          <button
            onClick={() => setIsExportModalOpen(true)}
            className="flex items-center gap-1.5 bg-slate-900/80 hover:bg-slate-800 text-slate-200 font-medium px-3.5 py-2 rounded-lg border border-slate-800 transition-colors text-xs active:scale-[0.98] min-h-[44px]"
          >
            <Download className="w-4 h-4 text-emerald-400" />
            <span className="hidden sm:inline">Export Note</span>
          </button>

          <button
            onClick={() => onDelete(meeting.id)}
            className="flex items-center gap-1.5 bg-red-950/30 hover:bg-red-950/50 text-red-300 font-medium px-3 py-2 rounded-lg border border-red-900/40 transition-colors text-xs active:scale-[0.98] min-h-[44px]"
            title="Delete Session"
          >
            <Trash2 className="w-4 h-4 text-red-400" />
          </button>
        </div>
      </div>

      {/* Meeting Header Banner */}
      <div className="p-4 md:p-5 rounded-xl bg-slate-900/50 border border-slate-800/80 space-y-2">
        <h1 className="text-lg md:text-xl font-semibold text-slate-100 tracking-tight">{meeting.title}</h1>
        <div className="flex flex-wrap items-center gap-4 text-xs text-slate-400 font-medium">
          <span className="flex items-center gap-1.5">
            <Calendar className="w-3.5 h-3.5 text-emerald-400" />
            {new Date(meeting.created_at).toLocaleString()}
          </span>
          <span className="flex items-center gap-1.5">
            <Clock className="w-3.5 h-3.5 text-emerald-400" />
            {formatDuration(meeting.duration_seconds)}
          </span>
          <span className="flex items-center gap-1.5">
            <FileText className="w-3.5 h-3.5 text-emerald-400" />
            {segments.length} Dialogue Segments
          </span>
        </div>
      </div>

      {/* Audio Player Bar */}
      <div className="p-3.5 rounded-xl bg-slate-900/80 border border-slate-800/90 flex flex-col sm:flex-row sm:items-center justify-between gap-3 shadow-sm">
        <div className="flex items-center gap-3">
          <button
            onClick={() => setIsPlaying(!isPlaying)}
            className="w-10 h-10 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white flex items-center justify-center transition-colors flex-shrink-0 min-h-[44px] min-w-[44px]"
          >
            {isPlaying ? <Pause className="w-5 h-5" /> : <Play className="w-5 h-5 ml-0.5" />}
          </button>

          <button
            onClick={() => seekAudio(0)}
            className="p-2.5 rounded-lg text-slate-400 hover:text-slate-200 hover:bg-slate-800/60 transition-colors min-h-[44px] min-w-[44px] flex items-center justify-center"
            title="Reset Playback"
          >
            <RotateCcw className="w-4 h-4" />
          </button>

          <div className="text-xs font-mono text-slate-300">
            <span>{formatDuration(playbackTime)}</span>
            <span className="text-slate-400"> / {formatDuration(meeting.duration_seconds)}</span>
          </div>
        </div>

        <div className="flex-1 flex items-center gap-3">
          <input
            type="range"
            min="0"
            max={meeting.duration_seconds || 1}
            value={playbackTime}
            onChange={(e) => seekAudio(Number(e.target.value))}
            className="w-full h-1.5 bg-slate-800 rounded-lg appearance-none cursor-pointer accent-emerald-500"
          />

          <button
            onClick={cycleSpeed}
            className="px-2.5 py-1.5 rounded-lg bg-slate-800/80 text-slate-300 border border-slate-700/80 font-mono text-xs hover:text-white transition-colors min-h-[36px] flex-shrink-0"
          >
            {playbackSpeed}x
          </button>
        </div>
      </div>

      {/* View Switcher Tabs */}
      <div className="flex items-center gap-2 border-b border-slate-800/80 pb-2 text-xs md:text-sm">
        <button
          onClick={() => setActiveTab('soap')}
          className={`flex items-center gap-2 px-4 py-2.5 rounded-lg font-medium transition-all min-h-[44px] ${
            activeTab === 'soap'
              ? 'bg-emerald-500/10 text-emerald-300 border border-emerald-500/20'
              : 'text-slate-400 hover:text-slate-200 border border-transparent'
          }`}
        >
          <Stethoscope className="w-4 h-4 text-emerald-400" />
          <span>Clinical SOAP Note</span>
        </button>

        <button
          onClick={() => setActiveTab('summary')}
          className={`flex items-center gap-2 px-4 py-2.5 rounded-lg font-medium transition-all min-h-[44px] ${
            activeTab === 'summary'
              ? 'bg-emerald-500/10 text-emerald-300 border border-emerald-500/20'
              : 'text-slate-400 hover:text-slate-200 border border-transparent'
          }`}
        >
          <Sparkles className="w-4 h-4 text-emerald-400" />
          <span>Executive Summary</span>
        </button>

        <button
          onClick={() => setActiveTab('transcript')}
          className={`flex items-center gap-2 px-4 py-2.5 rounded-lg font-medium transition-all min-h-[44px] ${
            activeTab === 'transcript'
              ? 'bg-emerald-500/10 text-emerald-300 border border-emerald-500/20'
              : 'text-slate-400 hover:text-slate-200 border border-transparent'
          }`}
        >
          <FileText className="w-4 h-4 text-emerald-400" />
          <span>Transcript Stream</span>
        </button>
      </div>

      {/* Tab Content */}
      <div className="flex-1 overflow-y-auto space-y-5 pr-1 scroll-touch">
        {activeTab === 'soap' && (
          <SOAPNoteView
            soapNote={soapNote}
            isGenerating={isGeneratingSOAP}
            onGenerateSOAP={handleGenerateSOAP}
            currentSpecialty={currentSpecialty}
            onSpecialtyChange={setCurrentSpecialty}
            onOpenExport={() => setIsExportModalOpen(true)}
          />
        )}

        {activeTab === 'summary' && (
          <div className="space-y-4">
            {/* Summary Action Toolbar */}
            <div className="flex flex-wrap items-center justify-between gap-2 p-3 bg-slate-900/60 rounded-xl border border-slate-800/80">
              <div className="flex items-center gap-2 text-xs font-medium text-slate-300">
                <Sparkles className="w-4 h-4 text-emerald-400" />
                <span>Executive Summary & Action Extraction</span>
                <span className="text-[10px] bg-emerald-500/10 text-emerald-400 px-2 py-0.5 rounded border border-emerald-500/20 font-medium">
                  Ollama / Cloud LLM
                </span>
              </div>
              <button
                onClick={handleGenerateSummary}
                disabled={isGeneratingSummary}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-emerald-600/20 hover:bg-emerald-600/30 text-emerald-300 text-xs font-semibold border border-emerald-500/30 transition-all active:scale-95 disabled:opacity-50"
                title="Generate or refresh high-level meeting summary and action items"
              >
                <RefreshCw className={`w-3.5 h-3.5 ${isGeneratingSummary ? 'animate-spin text-emerald-400' : ''}`} />
                <span>{isGeneratingSummary ? 'Generating Summary...' : 'Generate / Refresh Summary'}</span>
              </button>
            </div>

            {meeting.status === 'summarizing' && !meeting.summary ? (
              <div className="p-8 rounded-2xl bg-slate-900/60 border border-slate-800/80 flex flex-col items-center justify-center text-center space-y-3 shadow-lg">
                <RefreshCw className="w-6 h-6 animate-spin text-indigo-400" />
                <div>
                  <h3 className="text-sm font-semibold text-slate-100">Generating AI Summary in Background</h3>
                  <p className="text-xs text-slate-400 mt-1 max-w-sm">
                    The local AI model is analyzing this consultation. This view will automatically update when finished.
                  </p>
                </div>
              </div>
            ) : (
              <>
                <div className="p-5 rounded-xl bg-slate-900/50 border border-slate-800/80 space-y-3">
                  <div className="flex items-center justify-between">
                    <h2 className="text-xs font-medium uppercase tracking-wider text-slate-400 flex items-center gap-2">
                      <Sparkles className="w-4 h-4 text-emerald-400" />
                      Executive Summary
                    </h2>
                  </div>
                  <p className="text-slate-200 text-xs md:text-sm leading-relaxed whitespace-pre-wrap font-sans">
                    {meeting.summary || 'No executive summary generated yet. Click "Generate / Refresh Summary" above to generate.'}
                  </p>
                </div>

                <div className="p-5 rounded-xl bg-slate-900/50 border border-slate-800/80 space-y-3">
                  <h2 className="text-xs font-medium uppercase tracking-wider text-slate-400 flex items-center gap-2">
                    <CheckSquare className="w-4 h-4 text-emerald-400" />
                    Action Items & Follow-Ups
                  </h2>
                  <div className="space-y-2">
                    {actionItems.length === 0 ? (
                      <p className="text-xs text-slate-400 font-medium">No action items recorded for this session.</p>
                    ) : (
                      actionItems.map((item, idx) => (
                        <div
                          key={idx}
                          onClick={() => toggleActionItem(idx)}
                          className="flex items-center gap-3 p-3 rounded-lg bg-slate-900/80 border border-slate-800/80 hover:border-slate-700/80 cursor-pointer transition-colors min-h-[44px]"
                        >
                          <input
                            type="checkbox"
                            checked={item.done}
                            onChange={() => {}}
                            className="rounded border-slate-700 text-emerald-600 focus:ring-emerald-500 bg-slate-800 w-4 h-4"
                          />
                          <span className={`text-xs md:text-sm font-medium ${item.done ? 'line-through text-slate-400' : 'text-slate-200'}`}>
                            {item.text}
                          </span>
                        </div>
                      ))
                    )}
                  </div>
                </div>

                <div className="p-5 rounded-xl bg-slate-900/50 border border-slate-800/80 space-y-3">
                  <h2 className="text-xs font-medium uppercase tracking-wider text-slate-400 flex items-center gap-2">
                    <Lightbulb className="w-4 h-4 text-amber-400" />
                    Key Decisions
                  </h2>
                  <ul className="space-y-2">
                    {(meeting.key_decisions || []).length === 0 ? (
                      <li className="text-xs text-slate-400 font-medium">No key decisions captured.</li>
                    ) : (
                      (meeting.key_decisions || []).map((dec, idx) => (
                        <li key={idx} className="flex items-start gap-2.5 text-slate-200 text-xs md:text-sm leading-relaxed">
                          <span className="text-emerald-400 font-bold">•</span>
                          <span>{dec}</span>
                        </li>
                      ))
                    )}
                  </ul>
                </div>
              </>
            )}
          </div>
        )}

        {activeTab === 'transcript' && (
          <div className="space-y-3">
            {/* Diarization Action Toolbar */}
            <div className="flex flex-wrap items-center justify-between gap-2 p-3 bg-slate-900/60 rounded-xl border border-slate-800/80">
              <div className="flex items-center gap-2 text-xs font-medium text-slate-300">
                <Users className="w-4 h-4 text-emerald-400" />
                <span>Automated Speaker Diarization</span>
                <span className="text-[10px] bg-emerald-500/10 text-emerald-400 px-2 py-0.5 rounded border border-emerald-500/20 font-medium">
                  Acoustic + Clinical Role AI
                </span>
              </div>

              <div className="flex items-center gap-2">
                <button
                  onClick={handleSwapAllRoles}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-800/90 hover:bg-slate-750 text-slate-300 text-xs font-medium border border-slate-700/80 transition-all active:scale-95"
                  title="Swap Doctor and Patient roles across all segments"
                >
                  <ArrowLeftRight className="w-3.5 h-3.5 text-indigo-400" />
                  <span>Swap Doctor / Patient</span>
                </button>

                <button
                  onClick={handleAutoDiarize}
                  disabled={isDiarizing}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-emerald-600/20 hover:bg-emerald-600/30 text-emerald-300 text-xs font-semibold border border-emerald-500/30 transition-all active:scale-95 disabled:opacity-50"
                  title="Cluster acoustic voiceprints and re-classify Doctor vs Patient roles"
                >
                  <RefreshCw className={`w-3.5 h-3.5 ${isDiarizing ? 'animate-spin text-emerald-400' : ''}`} />
                  <span>{isDiarizing ? 'Diarizing Audio...' : 'Auto-Diarize Dialogue'}</span>
                </button>
              </div>
            </div>

            {segments.length === 0 ? (
              <div className="p-8 text-center bg-slate-900/40 rounded-xl border border-slate-800/80 space-y-3">
                <FileText className="w-8 h-8 text-slate-500 mx-auto" />
                <p className="text-sm font-medium text-slate-300">No transcript segments generated yet.</p>
                <p className="text-xs text-slate-400 max-w-md mx-auto">
                  Click below to transcribe the full audio recording with on-device Whisper and separate Doctor vs. Patient dialogue turns.
                </p>
                <button
                  onClick={handleAutoDiarize}
                  disabled={isDiarizing}
                  className="px-4 py-2 bg-emerald-600 hover:bg-emerald-500 text-white rounded-lg text-xs font-semibold shadow-lg shadow-emerald-950/50 transition-all active:scale-95 disabled:opacity-50 inline-flex items-center gap-2"
                >
                  <RefreshCw className={`w-3.5 h-3.5 ${isDiarizing ? 'animate-spin' : ''}`} />
                  <span>{isDiarizing ? 'Transcribing & Diarizing Audio...' : 'Transcribe & Diarize Audio'}</span>
                </button>
              </div>
            ) : (
              segments.map((seg) => {
                const isActive = playbackTime >= seg.start_time && (seg.end_time ? playbackTime <= seg.end_time : playbackTime <= seg.start_time + 5);
                const isDoctor = seg.speaker_label.toLowerCase().includes('doc') || seg.speaker_label.toLowerCase().includes('clinician');
                const isPatient = seg.speaker_label.toLowerCase().includes('pat');

                return (
                <div
                  key={seg.id}
                  id={`seg_${seg.id}`}
                  className={`p-4 rounded-xl border shadow-sm space-y-2 transition-all ${
                    isActive
                      ? 'bg-emerald-950/40 border-emerald-500/60 ring-2 ring-emerald-500/30'
                      : isDoctor
                      ? 'bg-slate-900/85 border-emerald-900/30 hover:border-emerald-700/50'
                      : isPatient
                      ? 'bg-slate-900/85 border-indigo-900/30 hover:border-indigo-700/50'
                      : 'bg-slate-900/80 border-slate-800/90 hover:border-slate-700/80'
                  }`}
                >
                  <div className="flex items-center justify-between border-b border-slate-800/60 pb-2">
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => handleToggleSpeakerRole(seg.id, seg.speaker_label)}
                        className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-semibold uppercase tracking-wider border transition-all cursor-pointer hover:opacity-90 active:scale-95 ${
                          isDoctor
                            ? 'bg-emerald-500/15 text-emerald-300 border-emerald-500/30'
                            : isPatient
                            ? 'bg-indigo-500/15 text-indigo-300 border-indigo-500/30'
                            : 'bg-slate-800 text-slate-300 border-slate-700'
                        }`}
                        title="Click to toggle speaker between Doctor and Patient"
                      >
                        {isDoctor ? (
                          <Stethoscope className="w-3.5 h-3.5 text-emerald-400" />
                        ) : isPatient ? (
                          <User className="w-3.5 h-3.5 text-indigo-400" />
                        ) : (
                          <Users className="w-3.5 h-3.5 text-slate-400" />
                        )}
                        <span>{seg.speaker_label || 'Speaker'}</span>
                      </button>
                    </div>

                    <button
                      onClick={() => handlePlaySegment(seg.start_time)}
                      className="text-emerald-400 hover:text-emerald-300 font-mono text-[11px] bg-slate-950/80 hover:bg-emerald-950/60 px-2.5 py-1 rounded border border-emerald-500/30 hover:border-emerald-500/60 flex items-center gap-1.5 transition-all active:scale-95 cursor-pointer"
                      title="Click to jump audio player to this segment"
                    >
                      <Play className="w-3 h-3 fill-emerald-400" />
                      <span>
                        {Math.floor(seg.start_time / 60)}:{Math.floor(seg.start_time % 60).toString().padStart(2, '0')} – {Math.floor(seg.end_time / 60)}:{Math.floor(seg.end_time % 60).toString().padStart(2, '0')}
                      </span>
                    </button>
                  </div>
                  <p className="text-slate-100 text-sm md:text-[15px] leading-relaxed tracking-normal font-sans pt-1">
                    {seg.text}
                  </p>
                </div>
              );
            }))}
          </div>
        )}
      </div>

      <ExportModal
        isOpen={isExportModalOpen}
        onClose={() => setIsExportModalOpen(false)}
        meeting={meeting}
        segments={segments}
        soapNote={soapNote}
      />
    </div>
  );
};

import React, { useState, useEffect } from 'react';
import { Settings, Cpu, HardDrive, CheckCircle2, RefreshCw, Save, Download, Users } from 'lucide-react';
import { ModelSettings } from '../types';
import { api, isTauri } from '../services/api';

const DEFAULT_SUMMARY_PROMPT = `You are an executive scribe that writes precise, factually grounded meeting summaries.
Extract a clear, professional executive summary, key action items, and major decisions strictly from the speech transcript below.

STRICT FACTUAL ACCURACY DIRECTIVES:
- ZERO FABRICATION: Include ONLY facts, names, numbers, symptoms, and decisions that are EXPLICITLY STATED in the transcript.
- Under NO circumstances should you guess, assume, or extrapolate unmentioned facts, people, or details.
- IMPORTANT: Always write the summary, action items, and key decisions in clear, professional ENGLISH regardless of the input transcript language.

TRANSCRIPT:
{transcript}

Respond in this EXACT format with no extra text:
## Summary
<3-4 concise, professional executive sentences in English summarizing the key topics strictly from the spoken text>

## Action Items
<list each concrete action item in English, one per line starting with '-'. If none, write '- None identified.'>

## Key Decisions
<list each decision or agreement in English, one per line starting with '-'. If none, write '- None identified.'>`;

interface SettingsModalProps {
  settings: ModelSettings;
  onSave: (newSettings: ModelSettings) => void;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({ settings, onSave }) => {
  const [form, setForm] = useState<ModelSettings>(settings);
  const [testingEndpoint, setTestingEndpoint] = useState(false);
  const [connectionStatus, setConnectionStatus] = useState<{ tested: boolean; online: boolean; models: string[] }>({
    tested: false,
    online: false,
    models: []
  });
  const [savedSuccess, setSavedSuccess] = useState(false);
  const [downloadingModel, setDownloadingModel] = useState<string | null>(null);
  const [builtinStatus, setBuiltinStatus] = useState<{ is_downloaded: boolean; file_size_mb: number }>({
    is_downloaded: false,
    file_size_mb: 0,
  });
  const [downloadProgress, setDownloadProgress] = useState<{ status: string; percentage: number; completed: number; total: number }>({
    status: '',
    percentage: 0,
    completed: 0,
    total: 0,
  });

  useEffect(() => {
    setForm(settings);
    handleTestConnection();
    api.checkBuiltinModelStatus().then(st => {
      setBuiltinStatus({ is_downloaded: st.is_downloaded, file_size_mb: st.file_size_mb });
    }).catch(() => {});
  }, [settings]);

  useEffect(() => {
    let unlistenOllama: (() => void) | undefined;
    let unlistenBuiltin: (() => void) | undefined;
    if (isTauri()) {
      import('@tauri-apps/api/event').then(({ listen }) => {
        listen<any>('ollama-download-progress', (event) => {
          const payload = event.payload;
          setDownloadProgress({
            status: payload.status,
            percentage: payload.percentage,
            completed: payload.completed,
            total: payload.total,
          });
          if (payload.percentage >= 100 || payload.status.includes('success')) {
            setDownloadingModel(null);
            handleTestConnection();
          }
        }).then(fn => { unlistenOllama = fn; });

        listen<any>('builtin-download-progress', (event) => {
          const payload = event.payload;
          setDownloadProgress({
            status: payload.status,
            percentage: payload.percentage,
            completed: payload.completed,
            total: payload.total,
          });
          if (payload.percentage >= 100 || payload.status.includes('ready')) {
            setDownloadingModel(null);
            setBuiltinStatus(prev => ({ ...prev, is_downloaded: true }));
          }
        }).then(fn => { unlistenBuiltin = fn; });
      });
    }
    return () => {
      if (unlistenOllama) unlistenOllama();
      if (unlistenBuiltin) unlistenBuiltin();
    };
  }, []);

  const handleTestConnection = async () => {
    setTestingEndpoint(true);
    const res = await api.checkOllama(form.ollama_endpoint);
    setConnectionStatus({ tested: true, online: res.online, models: res.models });
    setTestingEndpoint(false);
  };

  const handlePullModel = async (modelName: string) => {
    setDownloadingModel(modelName);
    setDownloadProgress({ status: 'Connecting to model repository...', percentage: 0, completed: 0, total: 0 });
    try {
      if (connectionStatus.online) {
        await api.pullLocalModel(modelName);
      } else {
        await api.downloadBuiltinModel();
        setBuiltinStatus({ is_downloaded: true, file_size_mb: 980 });
      }
      setForm(prev => ({ ...prev, ollama_model: modelName }));
      await handleTestConnection();
    } catch (err) {
      alert(`Model download notice: ${err}`);
    } finally {
      setDownloadingModel(null);
    }
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onSave(form);
    setSavedSuccess(true);
    setTimeout(() => setSavedSuccess(false), 2000);
  };

  return (
    <div className="h-full flex flex-col p-4 pt-[max(1rem,env(safe-area-inset-top))] md:p-6 max-w-4xl mx-auto w-full space-y-4 md:space-y-6 overflow-y-auto scroll-touch pb-24 md:pb-12">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-slate-800/80 pb-4">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-xl bg-indigo-500/10 border border-indigo-500/20 flex items-center justify-center text-indigo-400">
            <Settings className="w-4 h-4" />
          </div>
          <div>
            <h2 className="text-base font-bold text-slate-100">Settings</h2>
            <p className="text-xs text-slate-400">Configure speech recognition and local AI summarization models</p>
          </div>
        </div>

        <button
          type="button"
          onClick={handleSubmit}
          className="flex items-center gap-2 bg-indigo-600 hover:bg-indigo-500 text-white px-4 py-2 rounded-xl text-xs font-semibold shadow-lg shadow-indigo-950/50 transition-all min-h-[40px]"
        >
          <Save className="w-3.5 h-3.5" />
          <span>{savedSuccess ? 'Saved!' : 'Save Settings'}</span>
        </button>
      </div>

      <form onSubmit={handleSubmit} className="space-y-5">
        {/* Speech Recognition Section */}
        <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 space-y-4">
          <div className="flex items-center gap-2 text-indigo-400 font-semibold text-xs uppercase tracking-wider">
            <Cpu className="w-4 h-4" />
            <span>Speech Recognition</span>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div>
              <label className="block text-xs font-medium text-slate-300 mb-1.5">Whisper Model</label>
              <select
                value={form.stt_model}
                onChange={(e) => {
                  const newModel = e.target.value;
                  setForm({
                    ...form,
                    stt_model: newModel,
                    stt_language: newModel.includes('.en') && (form.stt_language === 'zh' || form.stt_language === 'yue') ? 'en' : form.stt_language,
                  });
                }}
                className="w-full bg-slate-900 border border-slate-800 rounded-lg px-3 py-2.5 text-xs text-slate-100 focus:outline-none focus:border-indigo-500/60 min-h-[44px]"
              >
                <option value="small">Small Multilingual (488 MB - Recommended for English & Chinese)</option>
                <option value="small.en">Small English-Only (487 MB)</option>
                <option value="base">Base Multilingual (140 MB)</option>
                <option value="medium">Medium Multilingual (1.5 GB)</option>
              </select>
            </div>

            <div>
              <label className="block text-xs font-medium text-slate-300 mb-1.5">Spoken Language</label>
              <select
                value={form.stt_language || 'auto'}
                onChange={(e) => {
                  const newLang = e.target.value;
                  setForm({
                    ...form,
                    stt_language: newLang,
                    stt_model: (newLang === 'zh' || newLang === 'yue') && form.stt_model.includes('.en') ? 'small' : form.stt_model,
                  });
                }}
                className="w-full bg-slate-900 border border-slate-800 rounded-lg px-3 py-2.5 text-xs text-slate-100 focus:outline-none focus:border-indigo-500/60 min-h-[44px]"
              >
                <option value="auto">Auto-Detect (English, Chinese & Code-Switching)</option>
                <option value="zh">Chinese / 中文 (Mandarin / 普通话)</option>
                <option value="yue">Cantonese / 粤语</option>
                <option value="en">English</option>
                <option value="ms">Malay / Bahasa Melayu</option>
                <option value="ta">Tamil / தமிழ்</option>
              </select>
            </div>

            <div>
              <label className="block text-xs font-medium text-slate-300 mb-1.5">Silence Detection (VAD)</label>
              <div className="flex items-center gap-3 pt-2">
                <input
                  type="range"
                  min="0.1"
                  max="0.9"
                  step="0.1"
                  value={form.vad_sensitivity}
                  onChange={(e) => setForm({ ...form, vad_sensitivity: parseFloat(e.target.value) })}
                  className="w-full accent-indigo-500 h-1.5 bg-slate-800 rounded-lg cursor-pointer"
                />
                <span className="text-xs font-mono text-slate-300 min-w-[28px]">{form.vad_sensitivity.toFixed(1)}</span>
              </div>
            </div>
          </div>

          {/* Custom Vocabulary & Medical Term Injection */}
          <div className="pt-3 border-t border-slate-800/80">
            <label className="block text-xs font-medium text-slate-200 mb-1">
              Custom Terms & Names (Vocabulary Seeding)
            </label>
            <p className="text-[11px] text-slate-400 mb-2 leading-relaxed">
              Whisper's decoder will prioritize these terms. Enter comma-separated medical terms, drug names, acronyms, or clinician names (e.g. <span className="text-slate-300 font-mono text-[10.5px]">Atorvastatin, Metformin, Dr. Sarah Lee, COPD, GERD, HbA1c</span>).
            </p>
            <input
              type="text"
              placeholder="e.g. Lisinopril, Omeprazole, Dr. Tan, Singlish, clinical terminology..."
              value={form.custom_vocabulary || ''}
              onChange={(e) => setForm({ ...form, custom_vocabulary: e.target.value })}
              className="w-full bg-slate-900 border border-slate-800 rounded-lg px-3 py-2.5 text-xs text-slate-100 placeholder-slate-500 focus:outline-none focus:border-indigo-500/60 focus:ring-1 focus:ring-indigo-500/60 min-h-[42px]"
            />
          </div>
        </div>

        {/* Diarization Multi-Speaker Section */}
        <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 space-y-2">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2 text-indigo-400 font-semibold text-xs uppercase tracking-wider">
              <Users className="w-4 h-4" />
              <span>Speaker Diarization</span>
            </div>
            <label className="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                checked={form.enable_speaker_diarization ?? false}
                onChange={(e) => setForm({ ...form, enable_speaker_diarization: e.target.checked })}
                className="sr-only peer"
              />
              <div className="w-9 h-5 bg-slate-800 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-indigo-600"></div>
            </label>
          </div>
          <p className="text-xs text-slate-400">
            Automatically distinguish and label different speakers in transcripts.
          </p>
        </div>

        {/* Local Generative AI Section */}
        <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 space-y-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2 text-indigo-400 font-semibold text-xs uppercase tracking-wider">
              <HardDrive className="w-4 h-4" />
              <span>AI Summarization Engine</span>
            </div>
            {connectionStatus.online || builtinStatus.is_downloaded ? (
              <div className="flex items-center gap-1.5 text-xs font-semibold px-2.5 py-1 rounded-full bg-emerald-950/40 text-emerald-300 border border-emerald-800/50">
                <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
                <span>100% On-Device GPU</span>
              </div>
            ) : (
              <div className="flex items-center gap-1.5 text-xs font-semibold px-2.5 py-1 rounded-full bg-amber-950/40 text-amber-300 border border-amber-800/50">
                <span className="w-2 h-2 rounded-full bg-amber-400"></span>
                <span>Download Model Below</span>
              </div>
            )}
          </div>

          <div className="space-y-4">
            <div>
              <label className="block text-xs font-medium text-slate-300 mb-1.5">Active Summarization Model</label>
              {connectionStatus.models.length > 0 ? (
                <select
                  value={form.ollama_model}
                  onChange={(e) => setForm({ ...form, ollama_model: e.target.value })}
                  className="w-full bg-slate-900 border border-slate-800 rounded-lg px-3 py-2.5 text-xs text-slate-100 focus:outline-none focus:border-indigo-500/60 min-h-[44px]"
                >
                  {connectionStatus.models.map((m) => (
                    <option key={m} value={m}>{m}</option>
                  ))}
                  {!connectionStatus.models.includes(form.ollama_model) && (
                    <option value={form.ollama_model}>{form.ollama_model}</option>
                  )}
                </select>
              ) : (
                <select
                  value={form.ollama_model}
                  onChange={(e) => setForm({ ...form, ollama_model: e.target.value })}
                  className="w-full bg-slate-900 border border-slate-800 rounded-lg px-3 py-2.5 text-xs text-slate-100 focus:outline-none focus:border-indigo-500/60 min-h-[44px]"
                >
                  <option value="qwen2.5:7b">Qwen 2.5 (7B) - High Accuracy (Recommended)</option>
                  <option value="qwen2.5:1.5b">Qwen 2.5 (1.5B) - Fast & Lightweight</option>
                </select>
              )}
            </div>

            {/* 1-Click Local Model Downloader */}
            <div className="space-y-2 pt-2">
              <label className="block text-xs font-semibold text-slate-200">Available Scribe Models</label>

              <div className="grid grid-cols-1 md:grid-cols-2 gap-3 pt-1">
                {[
                  { name: 'qwen2.5:7b', label: 'Qwen 2.5 (7B)', size: '~4.5 GB', desc: 'Detailed, high-accuracy clinical summaries (Recommended)' },
                  { name: 'qwen2.5:1.5b', label: 'Qwen 2.5 (1.5B)', size: '~1.1 GB', desc: 'Fast, lightweight summaries for smaller machines' },
                ].map((m) => {
                  const isInstalled = connectionStatus.models.some((installed) => installed.startsWith(m.name.split(':')[0])) || (m.name === 'qwen2.5:1.5b' && builtinStatus.is_downloaded);
                  const isDownloading = downloadingModel === m.name;

                  return (
                    <div key={m.name} className="p-3.5 rounded-xl bg-slate-900/90 border border-slate-800 flex flex-col justify-between space-y-3">
                      <div>
                        <div className="flex items-center justify-between">
                          <span className="font-bold text-slate-200 text-xs">{m.label}</span>
                          <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-slate-800 text-slate-400 border border-slate-700">{m.size}</span>
                        </div>
                        <p className="text-[11px] text-slate-400 mt-1">{m.desc}</p>
                      </div>

                      {isDownloading ? (
                        <div className="space-y-1.5">
                          <div className="flex items-center justify-between text-[11px] font-mono text-emerald-400">
                            <span className="truncate max-w-[140px]">{downloadProgress.status || 'Downloading...'}</span>
                            <span>{downloadProgress.percentage.toFixed(1)}%</span>
                          </div>
                          <div className="w-full h-1.5 bg-slate-800 rounded-full overflow-hidden">
                            <div className="h-full bg-emerald-500 transition-all duration-300" style={{ width: `${downloadProgress.percentage}%` }} />
                          </div>
                        </div>
                      ) : (
                        <button
                          type="button"
                          onClick={() => handlePullModel(m.name)}
                          disabled={downloadingModel !== null}
                          className={`w-full py-2 px-3 rounded-lg text-xs font-semibold flex items-center justify-center gap-1.5 transition-all min-h-[36px] ${
                            isInstalled
                              ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 hover:bg-emerald-500/20'
                              : 'bg-indigo-600 hover:bg-indigo-500 text-white shadow-lg shadow-indigo-950/50'
                          }`}
                        >
                          {isInstalled ? <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" /> : <Download className="w-3.5 h-3.5" />}
                          <span>{isInstalled ? 'Installed & Ready' : `Download ${m.label}`}</span>
                        </button>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>

            {/* Advanced Server Configuration Collapsible */}
            <details className="group pt-3 border-t border-slate-800/80">
              <summary className="flex items-center justify-between cursor-pointer text-xs font-medium text-slate-400 hover:text-slate-200">
                <span>Advanced: Custom Engine Port / Fallback</span>
                <span className="text-[11px] text-indigo-400 group-open:rotate-180 transition-transform">▼</span>
              </summary>
              <div className="pt-3 space-y-3">
                <div>
                  <label className="block text-xs font-medium text-slate-300 mb-1">Local Host / Socket Endpoint</label>
                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={form.ollama_endpoint}
                      onChange={(e) => setForm({ ...form, ollama_endpoint: e.target.value })}
                      className="flex-1 bg-slate-900 border border-slate-800 rounded-lg px-3 py-2 text-xs text-slate-100 focus:outline-none focus:border-indigo-500/60 min-h-[40px]"
                      placeholder="http://localhost:11434"
                    />
                    <button
                      type="button"
                      onClick={handleTestConnection}
                      disabled={testingEndpoint}
                      className="flex items-center gap-2 bg-slate-800/80 hover:bg-slate-800 text-slate-200 px-3 py-2 rounded-lg text-xs font-medium border border-slate-700/80 transition-colors disabled:opacity-50 min-h-[40px]"
                    >
                      <RefreshCw className={`w-3.5 h-3.5 ${testingEndpoint ? 'animate-spin' : ''}`} />
                      <span>Test Socket</span>
                    </button>
                  </div>
                </div>
              </div>
            </details>

            {/* Custom Summarization Prompt Collapsible */}
            <details className="group pt-3 border-t border-slate-800/80">
              <summary className="flex items-center justify-between cursor-pointer text-xs font-medium text-slate-400 hover:text-slate-200">
                <span>Advanced: Custom Prompt Template</span>
                <span className="text-[11px] text-indigo-400 group-open:rotate-180 transition-transform">▼</span>
              </summary>
              <div className="pt-3 space-y-2">
                <div className="flex justify-end">
                  <button
                    type="button"
                    onClick={() => setForm({ ...form, custom_summary_prompt: DEFAULT_SUMMARY_PROMPT })}
                    className="text-[11px] text-indigo-400 hover:text-indigo-300 font-medium"
                  >
                    Reset to Default
                  </button>
                </div>
                <textarea
                  rows={6}
                  value={form.custom_summary_prompt ?? DEFAULT_SUMMARY_PROMPT}
                  onChange={(e) => setForm({ ...form, custom_summary_prompt: e.target.value })}
                  className="w-full bg-slate-900 border border-slate-800 rounded-lg p-3 text-xs text-slate-200 font-mono focus:outline-none focus:border-indigo-500/60 leading-relaxed"
                />
              </div>
            </details>
          </div>
        </div>
      </form>
    </div>
  );
};


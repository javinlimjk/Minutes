import React, { useState } from 'react';
import { SOAPNote, SpecialtyType } from '../types';
import { SpecialtySelector } from './SpecialtySelector';
import { Copy, Check, RefreshCw, FileText, Activity, Stethoscope, ClipboardList, Tag, Download } from 'lucide-react';

interface SOAPNoteViewProps {
  soapNote?: SOAPNote;
  isGenerating?: boolean;
  onGenerateSOAP: (specialty?: SpecialtyType) => void;
  currentSpecialty: SpecialtyType;
  onSpecialtyChange: (specialty: SpecialtyType) => void;
  onOpenExport?: () => void;
}

export const SOAPNoteView: React.FC<SOAPNoteViewProps> = ({
  soapNote,
  isGenerating = false,
  onGenerateSOAP,
  currentSpecialty,
  onSpecialtyChange,
  onOpenExport,
}) => {
  const [copiedSection, setCopiedSection] = useState<string | null>(null);

  const [noteState, setNoteState] = useState<SOAPNote | undefined>(soapNote);

  React.useEffect(() => {
    setNoteState(soapNote);
  }, [soapNote]);

  const updateField = (field: 'subjective' | 'objective' | 'assessment' | 'plan', val: string) => {
    if (!noteState) return;
    setNoteState({ ...noteState, [field]: val });
  };

  const copyToClipboard = (text: string, sectionName: string) => {
    navigator.clipboard.writeText(text);
    setCopiedSection(sectionName);
    setTimeout(() => setCopiedSection(null), 2000);
  };

  const copyFullSOAP = () => {
    const active = noteState || soapNote;
    if (!active) return;
    const fullText = `SPECIALTY: ${active.specialty}
TIMESTAMP: ${new Date(active.timestamp).toLocaleString()}

SUBJECTIVE:
${active.subjective}

OBJECTIVE:
${active.objective}

ASSESSMENT:
${active.assessment}

PLAN:
${active.plan}

SUGGESTED ICD-10 CODES:
${active.icd10_suggestions.map(c => `- ${c.code}: ${c.description}`).join('\n')}`;

    copyToClipboard(fullText, 'full');
  };

  const active = noteState || soapNote;

  return (
    <div className="flex flex-col bg-slate-950/80 rounded-2xl border border-slate-800/80 shadow-2xl backdrop-blur-xl">
      {/* Top Controls Bar */}
      <div className="flex flex-wrap items-center justify-between gap-3 p-4 bg-slate-900/90 border-b border-slate-800/80 rounded-t-2xl">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-xl bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
            <FileText className="w-5 h-5" />
          </div>
          <div>
            <h2 className="text-base font-bold text-white">Clinical SOAP Note</h2>
            <p className="text-xs text-slate-400">Clinical SOAP note and suggested ICD-10 codes</p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <SpecialtySelector
            currentSpecialty={currentSpecialty}
            onSelectSpecialty={(s) => {
              onSpecialtyChange(s);
              if (soapNote) onGenerateSOAP(s);
            }}
            disabled={isGenerating}
          />

          <button
            onClick={() => onGenerateSOAP(currentSpecialty)}
            disabled={isGenerating}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold border border-slate-700 transition-all disabled:opacity-50 min-h-[36px]"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${isGenerating ? 'animate-spin text-emerald-400' : ''}`} />
            <span>{isGenerating ? 'Extracting...' : 'Regenerate'}</span>
          </button>

          {active && (
            <>
              <button
                onClick={copyFullSOAP}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-gradient-to-r from-emerald-500 to-teal-600 hover:from-emerald-400 hover:to-teal-500 text-white text-xs font-semibold shadow-lg shadow-emerald-950/50 transition-all min-h-[36px]"
              >
                {copiedSection === 'full' ? <Check className="w-3.5 h-3.5 text-white" /> : <Copy className="w-3.5 h-3.5" />}
                <span>{copiedSection === 'full' ? 'Copied Full Note!' : 'Copy Note'}</span>
              </button>

              {onOpenExport && (
                <button
                  onClick={onOpenExport}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-indigo-600/20 hover:bg-indigo-600/30 text-indigo-300 text-xs font-semibold border border-indigo-500/30 transition-all min-h-[36px]"
                >
                  <Download className="w-3.5 h-3.5" />
                  <span>Export...</span>
                </button>
              )}
            </>
          )}
        </div>
      </div>

      {/* Main SOAP Sections Content */}
      <div className="p-4 space-y-4 text-slate-200 text-xs leading-relaxed">
        {!active && !isGenerating && (
          <div className="flex flex-col items-center justify-center h-64 text-center text-slate-500 p-6">
            <Stethoscope className="w-12 h-12 text-slate-700 mb-3" />
            <p className="text-sm font-semibold text-slate-400">No SOAP Note Generated Yet</p>
            <p className="text-xs max-w-sm mt-1">Start recording live audio or click Regenerate to extract clinical documentation from transcript.</p>
          </div>
        )}

        {isGenerating && (
          <div className="flex flex-col items-center justify-center h-64 text-center space-y-3">
            <RefreshCw className="w-8 h-8 text-emerald-400 animate-spin" />
            <p className="text-xs font-medium text-emerald-400">Extracting Structured Clinical SOAP Data...</p>
            <p className="text-[11px] text-slate-500">Processing Subjective, Objective, Assessment, Plan & ICD-10 codes</p>
          </div>
        )}

        {active && !isGenerating && (
          <>
            {/* Subjective */}
            <div className="group relative bg-slate-900/70 rounded-xl p-4 border border-slate-800/90 hover:border-slate-700/80 transition-all space-y-2">
              <div className="flex items-center justify-between">
                <span className="font-bold text-emerald-400 uppercase tracking-wider text-[11px] flex items-center gap-1.5">
                  <FileText className="w-3.5 h-3.5" /> Subjective (S)
                </span>
                <button
                  onClick={() => copyToClipboard(active.subjective, 'Subjective')}
                  className="opacity-0 group-hover:opacity-100 flex items-center gap-1 text-[11px] text-slate-400 hover:text-white bg-slate-800 px-2 py-1 rounded border border-slate-700 transition-all"
                >
                  {copiedSection === 'Subjective' ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
                  <span>{copiedSection === 'Subjective' ? 'Copied' : 'Copy Section'}</span>
                </button>
              </div>
              <textarea
                value={active.subjective}
                onChange={(e) => updateField('subjective', e.target.value)}
                rows={4}
                className="w-full bg-slate-950/60 border border-slate-800/80 rounded-lg p-2.5 text-slate-200 text-xs focus:outline-none focus:border-emerald-500/60 resize-y leading-relaxed font-sans"
              />
            </div>

            {/* Objective */}
            <div className="group relative bg-slate-900/70 rounded-xl p-4 border border-slate-800/90 hover:border-slate-700/80 transition-all space-y-2">
              <div className="flex items-center justify-between">
                <span className="font-bold text-teal-400 uppercase tracking-wider text-[11px] flex items-center gap-1.5">
                  <Activity className="w-3.5 h-3.5" /> Objective (O)
                </span>
                <button
                  onClick={() => copyToClipboard(active.objective, 'Objective')}
                  className="opacity-0 group-hover:opacity-100 flex items-center gap-1 text-[11px] text-slate-400 hover:text-white bg-slate-800 px-2 py-1 rounded border border-slate-700 transition-all"
                >
                  {copiedSection === 'Objective' ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
                  <span>{copiedSection === 'Objective' ? 'Copied' : 'Copy Section'}</span>
                </button>
              </div>
              <textarea
                value={active.objective}
                onChange={(e) => updateField('objective', e.target.value)}
                rows={4}
                className="w-full bg-slate-950/60 border border-slate-800/80 rounded-lg p-2.5 text-slate-200 text-xs focus:outline-none focus:border-teal-500/60 resize-y leading-relaxed font-sans"
              />
            </div>

            {/* Assessment */}
            <div className="group relative bg-slate-900/70 rounded-xl p-4 border border-slate-800/90 hover:border-slate-700/80 transition-all space-y-2">
              <div className="flex items-center justify-between">
                <span className="font-bold text-cyan-400 uppercase tracking-wider text-[11px] flex items-center gap-1.5">
                  <Stethoscope className="w-3.5 h-3.5" /> Assessment (A)
                </span>
                <button
                  onClick={() => copyToClipboard(active.assessment, 'Assessment')}
                  className="opacity-0 group-hover:opacity-100 flex items-center gap-1 text-[11px] text-slate-400 hover:text-white bg-slate-800 px-2 py-1 rounded border border-slate-700 transition-all"
                >
                  {copiedSection === 'Assessment' ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
                  <span>{copiedSection === 'Assessment' ? 'Copied' : 'Copy Section'}</span>
                </button>
              </div>
              <textarea
                value={active.assessment}
                onChange={(e) => updateField('assessment', e.target.value)}
                rows={4}
                className="w-full bg-slate-950/60 border border-slate-800/80 rounded-lg p-2.5 text-slate-200 text-xs focus:outline-none focus:border-cyan-500/60 resize-y leading-relaxed font-sans"
              />
            </div>

            {/* Plan */}
            <div className="group relative bg-slate-900/70 rounded-xl p-4 border border-slate-800/90 hover:border-slate-700/80 transition-all space-y-2">
              <div className="flex items-center justify-between">
                <span className="font-bold text-sky-400 uppercase tracking-wider text-[11px] flex items-center gap-1.5">
                  <ClipboardList className="w-3.5 h-3.5" /> Plan (P)
                </span>
                <button
                  onClick={() => copyToClipboard(active.plan, 'Plan')}
                  className="opacity-0 group-hover:opacity-100 flex items-center gap-1 text-[11px] text-slate-400 hover:text-white bg-slate-800 px-2 py-1 rounded border border-slate-700 transition-all"
                >
                  {copiedSection === 'Plan' ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
                  <span>{copiedSection === 'Plan' ? 'Copied' : 'Copy Section'}</span>
                </button>
              </div>
              <textarea
                value={active.plan}
                onChange={(e) => updateField('plan', e.target.value)}
                rows={4}
                className="w-full bg-slate-950/60 border border-slate-800/80 rounded-lg p-2.5 text-slate-200 text-xs focus:outline-none focus:border-sky-500/60 resize-y leading-relaxed font-sans"
              />
            </div>

            {/* ICD-10 Suggestions Badges */}
            <div className="bg-slate-900/90 rounded-xl p-4 border border-slate-800">
              <div className="flex items-center justify-between mb-3">
                <span className="font-bold text-amber-400 uppercase tracking-wider text-[11px] flex items-center gap-1.5">
                  <Tag className="w-3.5 h-3.5" /> Suggested ICD-10 Codes
                </span>
                <span className="text-[10px] text-slate-500">Extracted from Assessment</span>
              </div>
              <div className="flex flex-wrap gap-2">
                {active.icd10_suggestions.map((icd, idx) => (
                  <button
                    key={idx}
                    onClick={() => copyToClipboard(`${icd.code}: ${icd.description}`, `icd_${idx}`)}
                    className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 border border-slate-700/80 transition-all text-left group"
                  >
                    <span className="font-mono font-bold text-emerald-400 text-xs">{icd.code}</span>
                    <span className="text-slate-300 text-xs">{icd.description}</span>
                    <Copy className="w-3 h-3 text-slate-500 group-hover:text-white transition-colors" />
                  </button>
                ))}
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
};

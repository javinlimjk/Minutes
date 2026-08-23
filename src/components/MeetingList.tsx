import React, { useState } from 'react';
import { Search, Calendar, Clock, ChevronRight, FileText, Trash2, Mic, Plus, RefreshCw } from 'lucide-react';
import { Meeting } from '../types';

interface MeetingListProps {
  meetings: Meeting[];
  onSelectMeeting: (id: string) => void;
  onDeleteMeeting: (id: string) => void;
  onNewRecording: () => void;
}

export const MeetingList: React.FC<MeetingListProps> = ({
  meetings,
  onSelectMeeting,
  onDeleteMeeting,
  onNewRecording,
}) => {
  const [searchTerm, setSearchTerm] = useState('');

  const filteredMeetings = meetings.filter((m) =>
    m.title.toLowerCase().includes(searchTerm.toLowerCase()) ||
    (m.summary && m.summary.toLowerCase().includes(searchTerm.toLowerCase()))
  );

  const formatDuration = (secs: number) => {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}m ${s}s`;
  };

  return (
    <div className="h-full flex flex-col p-4 pt-[max(1rem,env(safe-area-inset-top))] md:p-6 max-w-5xl mx-auto w-full space-y-4 md:space-y-6 select-none">
      {/* Top Header & Search Bar */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-slate-800/80 pb-4">
        <div>
          <h1 className="text-lg md:text-xl font-semibold text-slate-100 tracking-tight">Meetings</h1>
          <p className="text-xs text-slate-400 font-medium">All recorded transcripts and generated summaries.</p>
        </div>

        <div className="flex items-center gap-3">
          <div className="relative flex-1 sm:flex-initial">
            <Search className="w-4 h-4 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              placeholder="Search transcripts & summaries..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="w-full sm:w-64 bg-slate-900/80 border border-slate-800/80 rounded-lg pl-9 pr-4 py-2 text-xs text-slate-100 placeholder-slate-500 focus:outline-none focus:border-indigo-500/60 focus:ring-1 focus:ring-indigo-500/60 min-h-[44px]"
            />
          </div>

          <button
            onClick={onNewRecording}
            className="flex items-center justify-center gap-2 bg-indigo-600 hover:bg-indigo-500 text-white font-medium px-4 py-2 rounded-lg text-xs transition-colors active:scale-[0.98] min-h-[44px]"
          >
            <Plus className="w-4 h-4" />
            <span className="hidden sm:inline">New Recording</span>
          </button>
        </div>
      </div>

      {/* Meetings List */}
      <div className="flex-1 overflow-y-auto space-y-2.5 pr-1 scroll-touch">
        {filteredMeetings.length === 0 ? (
          /* Humanized Empty State */
          <div className="h-full flex flex-col items-center justify-center text-center p-8 space-y-4 my-auto">
            <div className="w-14 h-14 rounded-2xl bg-slate-900/80 border border-slate-800 flex items-center justify-center text-slate-400">
              <Mic className="w-7 h-7 text-indigo-400" />
            </div>
            <div className="space-y-1.5 max-w-sm">
              <h3 className="text-sm font-semibold text-slate-200">No meetings recorded yet</h3>
              <p className="text-xs text-slate-400 leading-relaxed">
                Tap <strong className="text-slate-300">New Recording</strong> to start dictating and capturing real-time transcriptions offline.
              </p>
            </div>
            <button
              onClick={onNewRecording}
              className="flex items-center gap-2 px-4 py-2.5 rounded-lg bg-indigo-600/10 hover:bg-indigo-600/20 text-indigo-300 border border-indigo-500/20 font-medium text-xs transition-colors min-h-[44px]"
            >
              <Mic className="w-4 h-4 text-indigo-400" />
              <span>Record First Session</span>
            </button>
          </div>
        ) : (
          filteredMeetings.map((m) => (
            <div
              key={m.id}
              onClick={() => onSelectMeeting(m.id)}
              className="p-3.5 md:p-4 rounded-xl bg-slate-900/50 border border-slate-800/80 hover:border-slate-700/80 hover:bg-slate-900/80 flex items-center justify-between gap-4 cursor-pointer transition-all group min-h-[64px]"
            >
              <div className="flex items-center gap-3.5 min-w-0">
                <div className="w-10 h-10 rounded-lg bg-slate-900 border border-slate-800 flex items-center justify-center flex-shrink-0 group-hover:border-indigo-500/30 transition-colors">
                  <FileText className="w-4 h-4 text-indigo-400" />
                </div>

                <div className="space-y-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <h3 className="font-medium text-xs md:text-sm text-slate-100 truncate group-hover:text-indigo-300 transition-colors">
                      {m.title}
                    </h3>
                    {m.status === 'summarizing' && (
                      <span className="flex items-center gap-1 text-[10px] font-medium px-2 py-0.5 rounded-full bg-indigo-500/15 text-indigo-300 border border-indigo-500/30 animate-pulse">
                        <RefreshCw className="w-2.5 h-2.5 animate-spin text-indigo-400" />
                        <span>Summarizing...</span>
                      </span>
                    )}
                  </div>
                  <p className="text-[11px] md:text-xs text-slate-400 line-clamp-1 font-normal">
                    {m.status === 'summarizing'
                      ? 'AI summary is generating in the background...'
                      : (m.summary || 'Tap to view transcript stream and executive notes.')}
                  </p>
                  <div className="flex items-center gap-3 text-[11px] text-slate-400 font-medium pt-0.5">
                    <span className="flex items-center gap-1">
                      <Calendar className="w-3 h-3 text-slate-400" />
                      {new Date(m.created_at).toLocaleDateString()}
                    </span>
                    <span className="flex items-center gap-1">
                      <Clock className="w-3 h-3 text-slate-400" />
                      {formatDuration(m.duration_seconds)}
                    </span>
                  </div>
                </div>
              </div>

              <div className="flex items-center gap-1 flex-shrink-0">
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onDeleteMeeting(m.id);
                  }}
                  className="p-2.5 rounded-lg text-slate-400 hover:text-red-400 hover:bg-red-950/30 transition-colors min-h-[44px] min-w-[44px] flex items-center justify-center"
                  title="Delete meeting"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
                <ChevronRight className="w-4 h-4 text-slate-400 group-hover:text-indigo-400 transition-colors" />
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};


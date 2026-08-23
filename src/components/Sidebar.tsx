import React from 'react';
import { Mic, FileText, Settings } from 'lucide-react';

interface SidebarProps {
  activeTab: 'recorder' | 'meetings' | 'settings';
  setActiveTab: (tab: 'recorder' | 'meetings' | 'settings') => void;
  isRecording: boolean;
}

export const Sidebar: React.FC<SidebarProps> = ({ activeTab, setActiveTab, isRecording }) => {
  return (
    <aside className="hidden md:flex w-56 bg-slate-900/60 border-r border-slate-800/80 flex-col justify-between p-4 pt-[max(1rem,env(safe-area-inset-top))] pb-[max(1rem,env(safe-area-inset-bottom))] z-20 select-none">
      <div>
        {/* Brand Header */}
        <div className="flex items-center gap-2.5 px-2 py-3 mb-6 border-b border-slate-800/80">
          <div className="w-8 h-8 rounded-lg bg-indigo-500/10 border border-indigo-500/20 flex items-center justify-center text-indigo-400 flex-shrink-0">
            <Mic className="w-4 h-4" />
          </div>
          <div>
            <h1 className="font-bold text-sm text-slate-100 tracking-tight">Minutes</h1>
            <p className="text-[11px] text-slate-400">Local AI Scribe</p>
          </div>
        </div>

        {/* Navigation Menu */}
        <nav className="space-y-1">
          <button
            onClick={() => setActiveTab('recorder')}
            className={`w-full flex items-center justify-between px-3 py-2.5 rounded-lg font-medium text-xs transition-all ${
              activeTab === 'recorder'
                ? 'bg-indigo-600/15 text-indigo-300 border border-indigo-500/30'
                : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/50 border border-transparent'
            }`}
          >
            <div className="flex items-center gap-2.5">
              <Mic className={`w-4 h-4 ${isRecording ? 'text-red-400 animate-pulse' : ''}`} />
              <span>Record</span>
            </div>
            {isRecording && (
              <span className="flex h-2 w-2 relative">
                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-red-400 opacity-75"></span>
                <span className="relative inline-flex rounded-full h-2 w-2 bg-red-500"></span>
              </span>
            )}
          </button>

          <button
            onClick={() => setActiveTab('meetings')}
            className={`w-full flex items-center gap-2.5 px-3 py-2.5 rounded-lg font-medium text-xs transition-all ${
              activeTab === 'meetings'
                ? 'bg-indigo-600/15 text-indigo-300 border border-indigo-500/30'
                : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/50 border border-transparent'
            }`}
          >
            <FileText className="w-4 h-4" />
            <span>Meetings</span>
          </button>

          <button
            onClick={() => setActiveTab('settings')}
            className={`w-full flex items-center gap-2.5 px-3 py-2.5 rounded-lg font-medium text-xs transition-all ${
              activeTab === 'settings'
                ? 'bg-indigo-600/15 text-indigo-300 border border-indigo-500/30'
                : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/50 border border-transparent'
            }`}
          >
            <Settings className="w-4 h-4" />
            <span>Settings</span>
          </button>
        </nav>
      </div>

      <div className="pt-4 border-t border-slate-800/80 px-2 text-[11px] text-slate-400">
        <span>v0.1.0 • 100% On-Device</span>
      </div>
    </aside>
  );
};

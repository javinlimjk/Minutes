import React from 'react';
import { Mic, FileText, Settings } from 'lucide-react';

interface MobileTabBarProps {
  activeTab: 'recorder' | 'meetings' | 'settings';
  setActiveTab: (tab: 'recorder' | 'meetings' | 'settings') => void;
  isRecording: boolean;
}

export const MobileTabBar: React.FC<MobileTabBarProps> = ({ activeTab, setActiveTab, isRecording }) => {
  const triggerHaptic = () => {
    if (typeof window !== 'undefined' && 'vibrate' in navigator) {
      try { navigator.vibrate(20); } catch {}
    }
  };

  const handleTabClick = (tab: 'recorder' | 'meetings' | 'settings') => {
    triggerHaptic();
    setActiveTab(tab);
  };

  return (
    <nav className="fixed bottom-0 left-0 right-0 bg-slate-950/95 border-t border-slate-800/80 px-3 pt-2 pb-[max(0.5rem,env(safe-area-inset-bottom))] flex items-center justify-around z-50 md:hidden backdrop-blur-md select-none">
      <button
        onClick={() => handleTabClick('recorder')}
        className={`flex-1 flex flex-col items-center justify-center gap-1 py-1 px-2 min-h-[44px] rounded-lg transition-all duration-150 ${
          activeTab === 'recorder'
            ? 'text-emerald-400 bg-emerald-500/10 border border-emerald-500/20 font-medium'
            : 'text-slate-400 hover:text-slate-200 border border-transparent'
        }`}
      >
        <div className="relative">
          <Mic className={`w-5 h-5 ${isRecording ? 'text-red-400 animate-pulse' : ''}`} />
          {isRecording && (
            <span className="absolute -top-1 -right-1 flex h-2 w-2">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-red-400 opacity-75"></span>
              <span className="relative inline-flex rounded-full h-2 w-2 bg-red-500"></span>
            </span>
          )}
        </div>
        <span className="text-[11px] tracking-tight">Record</span>
      </button>

      <button
        onClick={() => handleTabClick('meetings')}
        className={`flex-1 flex flex-col items-center justify-center gap-1 py-1 px-2 min-h-[44px] rounded-lg transition-all duration-150 ${
          activeTab === 'meetings'
            ? 'text-emerald-400 bg-emerald-500/10 border border-emerald-500/20 font-medium'
            : 'text-slate-400 hover:text-slate-200 border border-transparent'
        }`}
      >
        <FileText className="w-5 h-5" />
        <span className="text-[11px] tracking-tight">Meetings</span>
      </button>

      <button
        onClick={() => handleTabClick('settings')}
        className={`flex-1 flex flex-col items-center justify-center gap-1 py-1 px-2 min-h-[44px] rounded-lg transition-all duration-150 ${
          activeTab === 'settings'
            ? 'text-emerald-400 bg-emerald-500/10 border border-emerald-500/20 font-medium'
            : 'text-slate-400 hover:text-slate-200 border border-transparent'
        }`}
      >
        <Settings className="w-5 h-5" />
        <span className="text-[11px] tracking-tight">Settings</span>
      </button>
    </nav>
  );
};

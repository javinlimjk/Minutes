import { useState, useEffect } from 'react';
import { Sidebar } from './components/Sidebar';
import { MobileTabBar } from './components/MobileTabBar';
import { LiveRecorder } from './components/LiveRecorder';
import { MeetingList } from './components/MeetingList';
import { MeetingDetail } from './components/MeetingDetail';
import { SettingsModal } from './components/SettingsModal';
import { Meeting, ModelSettings, SpecialtyType } from './types';
import { api, DEFAULT_SETTINGS } from './services/api';

export default function App() {
  const [activeTab, setActiveTab] = useState<'recorder' | 'meetings' | 'settings'>('recorder');
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [selectedMeetingId, setSelectedMeetingId] = useState<string | null>(null);
  const [settings, setSettings] = useState<ModelSettings>(DEFAULT_SETTINGS);
  const [isRecording, setIsRecording] = useState(false);
  const [isAlwaysOnTop, setIsAlwaysOnTop] = useState(false);
  const [currentSpecialty, setCurrentSpecialty] = useState<SpecialtyType>('General Practice');

  // Load meetings and settings on initial mount
  useEffect(() => {
    let isMounted = true;
    api.getMeetings().then((m) => {
      if (isMounted) setMeetings(m);
    });
    api.getSettings().then((s) => {
      if (isMounted) {
        setSettings(s);
        if (s.default_specialty) setCurrentSpecialty(s.default_specialty);
      }
    });
    return () => { isMounted = false; };
  }, []);

  const handleToggleAlwaysOnTop = async (enabled: boolean) => {
    setIsAlwaysOnTop(enabled);
    await api.toggleAlwaysOnTop(enabled);
  };

  const handleMeetingCreated = (newMeeting: Meeting, navigateToMeeting = false) => {
    setMeetings((prev) => {
      const exists = prev.some(m => m.id === newMeeting.id);
      if (exists) {
        return prev.map(m => m.id === newMeeting.id ? newMeeting : m);
      }
      return [newMeeting, ...prev];
    });
    if (navigateToMeeting) {
      setSelectedMeetingId(newMeeting.id);
      setActiveTab('meetings');
    }
  };

  const handleMeetingUpdated = (updatedMeeting: Meeting) => {
    setMeetings((prev) => prev.map(m => m.id === updatedMeeting.id ? { ...m, ...updatedMeeting } : m));
  };

  const handleDeleteMeeting = async (id: string) => {
    await api.deleteMeeting(id);
    setMeetings((prev) => prev.filter((m) => m.id !== id));
    if (selectedMeetingId === id) {
      setSelectedMeetingId(null);
    }
  };

  const handleSaveSettings = async (newSettings: ModelSettings) => {
    setSettings(newSettings);
    await api.saveSettings(newSettings);
  };

  return (
    <div className="flex h-full w-full bg-slate-950 text-slate-100 overflow-hidden font-sans pl-[env(safe-area-inset-left)] pr-[env(safe-area-inset-right)]">
      {/* Desktop Sidebar Navigation */}
      <Sidebar
        activeTab={activeTab}
        setActiveTab={(tab) => {
          setSelectedMeetingId(null);
          setActiveTab(tab);
        }}
        isRecording={isRecording}
      />

      {/* Main Content Workspace */}
      <main className="flex-1 h-full overflow-hidden bg-slate-950 pb-[calc(4.25rem+env(safe-area-inset-bottom))] md:pb-0 relative">
        <div className={`h-full w-full ${activeTab === 'recorder' ? 'block' : 'hidden'}`}>
          <LiveRecorder
            onMeetingCreated={handleMeetingCreated}
            onMeetingUpdated={handleMeetingUpdated}
            settings={settings}
            setIsRecordingGlobal={setIsRecording}
            isAlwaysOnTop={isAlwaysOnTop}
            onToggleAlwaysOnTop={handleToggleAlwaysOnTop}
            currentSpecialty={currentSpecialty}
            onSpecialtyChange={setCurrentSpecialty}
          />
        </div>

        <div className={`h-full w-full ${activeTab === 'meetings' ? 'block' : 'hidden'}`}>
          {selectedMeetingId ? (
            <MeetingDetail
              meetingId={selectedMeetingId}
              onBack={() => setSelectedMeetingId(null)}
              onDelete={handleDeleteMeeting}
            />
          ) : (
            <MeetingList
              meetings={meetings}
              onSelectMeeting={(id) => setSelectedMeetingId(id)}
              onDeleteMeeting={handleDeleteMeeting}
              onNewRecording={() => setActiveTab('recorder')}
            />
          )}
        </div>

        <div className={`h-full w-full ${activeTab === 'settings' ? 'block' : 'hidden'}`}>
          <SettingsModal
            settings={settings}
            onSave={handleSaveSettings}
          />
        </div>
      </main>

      {/* Mobile Bottom Navigation Bar */}
      <MobileTabBar
        activeTab={activeTab}
        setActiveTab={(tab) => {
          setSelectedMeetingId(null);
          setActiveTab(tab);
        }}
        isRecording={isRecording}
      />
    </div>
  );
}

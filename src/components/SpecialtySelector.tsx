import React from 'react';
import { SpecialtyType } from '../types';
import { Stethoscope, Brain, Baby, Bone } from 'lucide-react';

interface SpecialtySelectorProps {
  currentSpecialty: SpecialtyType;
  onSelectSpecialty: (specialty: SpecialtyType) => void;
  disabled?: boolean;
}

const specialties: { id: SpecialtyType; label: string; icon: React.FC<{ className?: string }> }[] = [
  { id: 'General Practice', label: 'General Practice', icon: Stethoscope },
  { id: 'Psychiatry', label: 'Psychiatry', icon: Brain },
  { id: 'Pediatrics', label: 'Pediatrics', icon: Baby },
  { id: 'Orthopedics', label: 'Orthopedics', icon: Bone },
];

export const SpecialtySelector: React.FC<SpecialtySelectorProps> = ({
  currentSpecialty,
  onSelectSpecialty,
  disabled = false,
}) => {
  return (
    <div className="flex flex-wrap gap-2 items-center bg-slate-900/60 p-1.5 rounded-xl border border-slate-800/80 backdrop-blur-md">
      <span className="text-xs font-semibold text-slate-400 px-2 uppercase tracking-wider">Specialty:</span>
      {specialties.map((s) => {
        const Icon = s.icon;
        const isActive = currentSpecialty === s.id;
        return (
          <button
            key={s.id}
            onClick={() => onSelectSpecialty(s.id)}
            disabled={disabled}
            className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-200 ${
              isActive
                ? 'bg-gradient-to-r from-emerald-500 to-teal-600 text-white shadow-lg shadow-emerald-950/40 font-semibold ring-1 ring-emerald-400/30'
                : 'text-slate-300 hover:text-white hover:bg-slate-800/80 border border-transparent'
            } ${disabled ? 'opacity-50 cursor-not-allowed' : ''}`}
          >
            <Icon className={`w-3.5 h-3.5 ${isActive ? 'text-white' : 'text-emerald-400'}`} />
            <span>{s.label}</span>
          </button>
        );
      })}
    </div>
  );
};

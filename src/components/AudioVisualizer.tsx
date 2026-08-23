import React, { useEffect, useRef } from 'react';

interface AudioVisualizerProps {
  isRecording: boolean;
  isPaused?: boolean;
  audioData?: Uint8Array | null;
}

export const AudioVisualizer: React.FC<AudioVisualizerProps> = ({ isRecording, isPaused = false, audioData }) => {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    let animationFrameId: number;
    let phase = 0;

    const render = () => {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      const width = canvas.width;
      const height = canvas.height;
      const barCount = 36;
      const barWidth = 4;
      const gap = (width - barCount * barWidth) / (barCount - 1);

      phase += 0.08;

      for (let i = 0; i < barCount; i++) {
        let barHeight = 6;
        if (isRecording && !isPaused) {
          if (audioData && audioData.length > 0) {
            // Map real microphone FFT frequency data to bar heights
            const index = Math.floor((i / barCount) * (audioData.length * 0.6));
            const val = audioData[index] || 0;
            barHeight = Math.max(6, (val / 255) * height * 0.85);
          } else {
            // Dynamic height fallback using combination of sine waves
            const sinVal = Math.sin(phase + i * 0.3) * Math.cos(phase * 0.5 + i * 0.2);
            const randAmp = (Math.sin(i * 99) + 1) * 0.5;
            barHeight = Math.max(6, (sinVal * 0.5 + 0.5) * height * 0.75 * randAmp + 8);
          }
        }

        const x = i * (barWidth + gap);
        const y = (height - barHeight) / 2;

        const gradient = ctx.createLinearGradient(0, y, 0, y + barHeight);
        if (isRecording && !isPaused) {
          gradient.addColorStop(0, '#818cf8'); // Indigo 400
          gradient.addColorStop(0.5, '#6366f1'); // Indigo 500
          gradient.addColorStop(1, '#4f46e5'); // Indigo 600
        } else {
          gradient.addColorStop(0, '#334155');
          gradient.addColorStop(1, '#1e293b');
        }

        ctx.fillStyle = gradient;
        ctx.beginPath();
        ctx.roundRect(x, y, barWidth, barHeight, 2);
        ctx.fill();
      }

      animationFrameId = requestAnimationFrame(render);
    };

    render();

    return () => {
      cancelAnimationFrame(animationFrameId);
    };
  }, [isRecording, isPaused, audioData]);

  return (
    <div className="w-full bg-slate-900/40 border border-slate-800/80 rounded-lg p-2.5 flex items-center justify-center">
      <canvas
        ref={canvasRef}
        width={480}
        height={54}
        className="w-full h-12 max-w-lg"
      />
    </div>
  );
};


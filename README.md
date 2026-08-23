# Minutes 🎙️

> **100% Offline AI Medical & Meeting Transcriber + SOAP Note Generator**
> Built with Tauri v2, Rust, React, TypeScript, and Whisper.

Minutes is a privacy-first, fully local AI application designed for transcribing consultations and meetings, performing speaker diarization, and generating structured clinical SOAP notes (or meeting summaries) without sending any audio or text data to external cloud servers.

---

## ✨ Features

- **🔒 100% Offline & Private**: All speech-to-text processing and LLM reasoning run locally on your device. Zero external data leakage.
- **🎙️ Real-Time & Batch Transcription**: Powered by `whisper.cpp` (Metal GPU accelerated on Apple Silicon / CoreML / CPU).
- **👥 Speaker Diarization**: Separates speakers (e.g. Doctor / Patient or Host / Participant) locally.
- **📋 Clinical SOAP Note Generation**: Automatically creates structured clinical documentation (Subjective, Objective, Assessment, Plan) tailored to clinical specialties (General Practice, Cardiology, Pediatrics, Psychiatry, etc.).
- **⚡ Local LLM Support**: Integrates with local Ollama instances or embedded models for zero-cloud summarization.
- **🛡️ Encrypted Local Database**: SQLite database with local encryption for storing transcripts and consultations securely.
- **📱 Cross-Platform**: macOS (Desktop) and iOS support with native audio capture engines.

---

## 🛠️ Tech Stack

- **Frontend**: [React 19](https://react.dev/), [TypeScript](https://www.typescriptlang.org/), [Tailwind CSS](https://tailwindcss.com/), [Vite](https://vitejs.dev/), [Lucide React](https://lucide.dev/)
- **Backend / Core**: [Tauri v2](https://v2.tauri.app/), [Rust](https://www.rust-lang.org/)
- **Audio & AI Engine**: `whisper-rs` (`whisper.cpp`), CPAL audio capture, local Ollama API
- **Storage**: SQLite (`rusqlite`)

---

## 🚀 Getting Started

### Prerequisites

1. **Node.js**: >= 18 (`npm` or `pnpm` / `yarn`)
2. **Rust**: Latest stable Rust toolchain (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
3. **C/C++ Compiler**: Xcode Command Line Tools on macOS (`xcode-select --install`)
4. *(Optional)* **Ollama**: For local LLM summary and SOAP note generation ([ollama.ai](https://ollama.ai))

### Installation & Development

1. **Clone the repository**:
   ```bash
   git clone https://github.com/<your-username>/Minutes.git
   cd Minutes
   ```

2. **Install dependencies**:
   ```bash
   npm install
   ```

3. **Run development mode**:
   ```bash
   npm run tauri dev
   ```

4. **Build production app**:
   ```bash
   npm run tauri build
   ```

---

## 📦 Model Management

Whisper models (`ggml-small.en.bin`, `ggml-base.en.bin`, etc.) can be downloaded directly from the in-app Settings modal or placed in the `models/` directory.

---

## 📄 License

MIT License. See [LICENSE](LICENSE) for details.


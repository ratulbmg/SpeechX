# SpeechX

Offline, menu-bar dictation for macOS. Hold a key, speak, release — the transcript is
typed at your cursor. Runs fully offline (Whisper + AI4Bharat IndicConformer via
`sherpa-onnx`), with English, Hindi, and Bengali at launch.

Full spec: [`PROMPT.md`](./PROMPT.md).

## Status

Building milestone by milestone per `PROMPT.md` §13. Currently: **M1 — hotkey foundation.**

## Development

```bash
npm install
npm run tauri dev
```

Requires macOS, Rust (stable), and Node 18+. First run will prompt for Accessibility,
Input Monitoring, and Microphone permissions — see `PROMPT.md` §11.

## Layout

- `src-tauri/` — Rust backend: hotkeys, audio, ASR, injection, session state machine.
- `src/` — React/TS frontend: the recording pill and settings window only.
- `docs/` — architecture notes and the language-extension guide.

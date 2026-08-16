# SpeechX — Build Specification

> **How to use this file:** save it as `PROMPT.md` in the root of your `speech_x/` folder and point Claude Code (or your VS Code agent) at it. Work through the milestones in order — do not let the agent build everything at once. Say *"Read PROMPT.md and implement Milestone 1 only"*, verify it works, then move to Milestone 2.

---

## 1. What we are building

**SpeechX** is a macOS menu-bar dictation app. The user holds a key, speaks, releases, and the transcribed text appears at their cursor in whatever application is focused. All speech recognition runs **fully offline** — no network calls, no API keys, no per-user cost.

Launch languages: **English, Hindi, Bengali.** The architecture must scale to 20+ Indian languages without code changes — adding a language is a data-file edit plus a model download entry.

### Hard requirements

| # | Requirement |
|---|---|
| R1 | Hold **Right Command** → record → release → text is typed at the cursor |
| R2 | Hold **Right Command + Right Option** → speak a language name → active language switches |
| R3 | Press **Esc** while the hotkey is held → recording is discarded, nothing is typed |
| R4 | A floating pill appears at top-center of screen while recording, showing a live waveform driven by real microphone amplitude, with the active language name below it |
| R5 | The pill must **never** take keyboard focus — the target app's cursor must survive |
| R6 | 100% offline inference. No network at runtime except explicit model downloads |
| R7 | Devanagari and Bengali text must inject correctly into native apps, Electron apps, and browsers |

### Explicit non-goals for v1

Do not build these. They are listed so the agent does not invent them:

- Live/streaming word-by-word typing into the target app (v2 — see §9)
- Automatic language detection (the user chose manual selection deliberately)
- Windows or Linux support (architecture must not *block* it, but do not implement it)
- Cloud ASR fallback
- Any telemetry that leaves the machine

---

## 2. Naming

Use folder `speech_x/`, product name **SpeechX**, bundle identifier `com.speechx.desktop`.

*Optional consideration before you commit:* **Bolo** (বলো / बोलो — the imperative "speak", identical in Bengali and Hindi) is shorter, unambiguous to type, and speaks directly to the Indian-language audience this is built for. If you want it, change it now — renaming a bundle identifier after users have granted macOS permissions forces every user to re-grant them.

---

## 3. Locked technical decisions

Do not re-litigate these. They were chosen deliberately.

| Layer | Choice | Reason |
|---|---|---|
| Shell | **Tauri 2.x** | Uses macOS's native WebView. ~45 MB installer vs ~150 MB for Electron. Direct Rust access to native APIs. |
| Backend language | **Rust** | ~75% of the code. Hotkeys, audio, ASR, injection. |
| Frontend | **React + TypeScript + Vite** | Only the pill and settings window. |
| Global hotkey | `rdev` | Distinguishes left/right modifiers, which `global-hotkey` does not. |
| Audio capture | `cpal` | |
| ASR runtime | `sherpa-onnx` (Rust bindings) | Runs both NeMo Conformer and Whisper through one API. CPU-only, small footprint. |
| Text injection | `enigo` + native `CGEvent` / `AXUIElement` | |
| Overlay window | `tauri-nspanel` | **Required.** A normal Tauri window steals focus and breaks everything. |
| VAD | Silero VAD via sherpa-onnx | ~2 MB |
| Fuzzy matching | `strsim` (Jaro-Winkler) | Language-name matching |
| Settings storage | `serde` + TOML in `~/Library/Application Support/SpeechX/` | Human-editable, debuggable |
| Logging | `tracing` + `tracing-subscriber`, rolling file appender | Local only |
| Errors | `thiserror` in library modules, `anyhow` at boundaries | |

### Models

| Language | Model | Approx. size (int8) |
|---|---|---|
| English | Whisper `base` (multilingual build) | ~60 MB |
| Hindi | AI4Bharat IndicConformer Hindi (CTC/RNNT hybrid, 120M) | ~130 MB |
| Bengali | AI4Bharat IndicConformer Bengali (CTC/RNNT hybrid, 120M) | ~130 MB |
| Command mode | Whisper `base` — reused, always resident | (shared) |

IndicConformer is MIT-licensed and free for commercial use. All models expect **16 kHz mono** audio.

---

## 4. Repository structure

Create exactly this. Every module listed has a single clear responsibility — that is what makes the codebase extensible later.

```
speech_x/
├── PROMPT.md                        # this file
├── README.md
├── .gitignore                       # must ignore /models and /target
├── package.json
├── vite.config.ts
├── tsconfig.json
│
├── src-tauri/
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── Entitlements.plist           # mic + accessibility entitlements
│   ├── icons/
│   └── src/
│       ├── main.rs                  # entry, tray, window setup, wiring only
│       ├── lib.rs
│       ├── error.rs                 # AppError enum, thiserror
│       ├── state.rs                 # AppState: Arc<RwLock<…>>, shared handles
│       │
│       ├── config/
│       │   ├── mod.rs
│       │   ├── settings.rs          # Settings struct, load/save/defaults, migration
│       │   └── paths.rs             # app support dir, model dir, log dir
│       │
│       ├── hotkey/
│       │   ├── mod.rs
│       │   ├── listener.rs          # rdev event loop on its own thread
│       │   ├── chord.rs             # 150 ms mode-resolution logic (§6)
│       │   ├── keymap.rs            # macOS keycodes, rebindable
│       │   └── secure_input.rs      # IsSecureEventInputEnabled() polling
│       │
│       ├── audio/
│       │   ├── mod.rs
│       │   ├── capture.rs           # cpal stream lifecycle
│       │   ├── ring_buffer.rs       # lock-free producer/consumer
│       │   ├── level.rs             # RMS → smoothed 0..1 level, 30 fps emit
│       │   ├── resample.rs          # device rate → 16 kHz mono f32
│       │   └── vad.rs               # Silero, silence auto-stop
│       │
│       ├── asr/
│       │   ├── mod.rs
│       │   ├── engine.rs            # trait AsrEngine  ← the key abstraction
│       │   ├── whisper.rs           # impl for Whisper via sherpa-onnx
│       │   ├── conformer.rs         # impl for IndicConformer via sherpa-onnx
│       │   ├── manager.rs           # lazy load, LRU unload, idle eviction, warmup
│       │   └── postprocess.rs       # trim, punctuation, custom vocabulary
│       │
│       ├── lang/
│       │   ├── mod.rs
│       │   ├── registry.rs          # loads languages.toml at startup
│       │   ├── matcher.rs           # Jaro-Winkler alias matching (§8)
│       │   └── languages.toml       # ← adding language #4..#20 happens HERE
│       │
│       ├── models/
│       │   ├── mod.rs
│       │   ├── manifest.rs          # parse manifest.json
│       │   ├── manifest.json        # url + sha256 + size per model
│       │   ├── downloader.rs        # resumable download, checksum verify
│       │   └── store.rs             # what's installed, disk usage, delete
│       │
│       ├── inject/
│       │   ├── mod.rs               # trait TextInjector + strategy selection
│       │   ├── macos_cgevent.rs     # CGEventKeyboardSetUnicodeString (primary)
│       │   ├── macos_ax.rs          # AXUIElement insertion (preferred for native apps)
│       │   ├── clipboard.rs         # fallback: set clipboard, Cmd+V, restore
│       │   └── undo.rs              # tracks last insert length for Right Cmd+Z
│       │
│       ├── session/
│       │   ├── mod.rs
│       │   ├── machine.rs           # the state machine (§5) — single source of truth
│       │   ├── dictate.rs           # dictation session flow
│       │   └── command.rs           # language-switch session flow
│       │
│       ├── overlay/
│       │   ├── mod.rs
│       │   ├── panel.rs             # tauri-nspanel conversion, show/hide
│       │   └── position.rs          # top-center, notch-aware, multi-monitor
│       │
│       ├── permissions/
│       │   ├── mod.rs
│       │   └── macos.rs             # check + request accessibility/input/mic
│       │
│       ├── history/
│       │   └── mod.rs               # last 20 transcripts, in-memory + optional disk
│       │
│       └── ipc/
│           ├── mod.rs
│           ├── commands.rs          # #[tauri::command] handlers
│           └── events.rs            # typed event names, payload structs
│
├── src/
│   ├── main.tsx
│   ├── pill.html
│   ├── settings.html
│   │
│   ├── windows/
│   │   ├── pill/
│   │   │   ├── PillApp.tsx
│   │   │   └── pill.css
│   │   └── settings/
│   │       ├── SettingsApp.tsx
│   │       ├── panes/
│   │       │   ├── GeneralPane.tsx
│   │       │   ├── LanguagesPane.tsx     # download/delete models
│   │       │   ├── HotkeysPane.tsx
│   │       │   ├── VocabularyPane.tsx
│   │       │   └── HistoryPane.tsx
│   │       └── onboarding/
│   │           └── PermissionsWizard.tsx
│   │
│   ├── components/
│   │   ├── SonicWaveform.tsx        # §10 — the animation
│   │   ├── LanguageBadge.tsx
│   │   └── StatusLine.tsx
│   │
│   ├── hooks/
│   │   ├── useAudioLevel.ts         # subscribes to audio-level events
│   │   └── useSessionState.ts
│   │
│   └── lib/
│       ├── ipc.ts                   # typed invoke/listen wrappers
│       └── types.ts                 # MUST mirror ipc/events.rs exactly
│
├── models/                          # gitignored, runtime downloads
├── scripts/
│   ├── export_indicconformer.py     # HF → ONNX int8 conversion
│   ├── sign.sh
│   └── notarize.sh
└── docs/
    ├── ARCHITECTURE.md
    └── ADDING_A_LANGUAGE.md
```

---

## 5. The session state machine

`session/machine.rs` owns all state. Nothing else mutates session state. This is what keeps the app debuggable as it grows.

```
        ┌──────┐
        │ Idle │◄──────────────────────────────────────┐
        └──┬───┘                                       │
           │ hotkey down                               │
           ▼                                           │
      ┌─────────┐  150 ms elapsed, resolve mode        │
      │ Arming  │──────────┬─────────────┐             │
      └─────────┘          ▼             ▼             │
                    ┌───────────┐  ┌───────────┐       │
                    │ Recording │  │ Recording │       │
                    │ (Dictate) │  │ (Command) │       │
                    └─────┬─────┘  └─────┬─────┘       │
                          │              │             │
          ┌───────────────┼──────────────┤             │
          │ Esc           │ release      │ release     │
          ▼               ▼              ▼             │
    ┌───────────┐   ┌────────────┐  ┌────────────┐     │
    │ Cancelled │   │Transcribing│  │Transcribing│     │
    └─────┬─────┘   └─────┬──────┘  └─────┬──────┘     │
          │               ▼               ▼            │
          │         ┌──────────┐   ┌────────────┐      │
          │         │Injecting │   │MatchingLang│      │
          │         └────┬─────┘   └─────┬──────┘      │
          │              │               │             │
          └──────────────┴───────────────┴─────────────┘
```

Rules the agent must enforce:

1. **`Cancelled` is terminal for that session.** Audio buffer is dropped, no ASR runs, no text is injected.
2. **Capture the target application's focused element reference on entering `Recording`, not on entering `Injecting`.** The user may click elsewhere while transcription runs.
3. Every transition emits a `session-state` event to the pill window.
4. If `Recording` exceeds 120 seconds, force-stop and transcribe. Do not record unbounded.
5. If VAD reports 2.5 s of continuous silence during `Recording`, auto-stop as if released.

---

## 6. Hotkey handling

The chord problem: Right Command is *contained inside* Right Command + Right Option, so on key-down you cannot yet know which mode the user wants.

```rust
// hotkey/chord.rs — required behaviour

// On RightCommand key-down:
//   1. enter Arming, start a 150 ms timer
//   2. begin buffering audio IMMEDIATELY (do not wait — you would clip
//      the first syllable)
//   3. when the timer fires, read the current modifier state:
//        RightOption also held  → Mode::Command
//        RightOption not held   → Mode::Dictate
//   4. show the pill with the resolved mode's appearance
//
// On RightCommand key-up:
//   Arming    → discard, treat as an accidental tap
//   Recording → stop capture, advance state machine
//   Cancelled → discard silently, return to Idle
//
// On Esc while Recording:
//   → Cancelled. Do not wait for key-up to decide this.
```

macOS keycodes for `rdev`: `RightCommand = 54`, `LeftCommand = 55`, `RightOption = 61`, `LeftOption = 58`, `Escape = 53`. Never bind plain Left Command — it collides with every standard shortcut.

Run the `rdev` listener on a **dedicated OS thread**, not a Tokio task. `rdev::listen` blocks and takes a `'static` callback; send events into the async world over a `tokio::sync::mpsc` channel.

**Secure input:** poll `IsSecureEventInputEnabled()` when a session starts. If true, the OS blocks synthetic keystrokes — show `Can't type here` in the pill and skip injection rather than silently failing.

---

## 7. The `AsrEngine` abstraction

This trait is the single most important design decision in the codebase. It is what lets you add languages, swap models, or bolt on an optional cloud engine later without touching the session layer.

```rust
// asr/engine.rs

#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    pub confidence: Option<f32>,
    pub language: LanguageCode,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct AsrOptions {
    /// Bias decoding toward these terms (Whisper initial_prompt /
    /// sherpa contextual biasing). Used for custom vocabulary AND
    /// for the language-name list in Command mode.
    pub hotwords: Vec<String>,
    pub language_hint: Option<LanguageCode>,
}

pub trait AsrEngine: Send + Sync {
    fn id(&self) -> &str;
    fn supports(&self, lang: LanguageCode) -> bool;

    /// samples: 16 kHz, mono, f32 normalised to [-1.0, 1.0]
    fn transcribe(&self, samples: &[f32], opts: &AsrOptions)
        -> Result<Transcript, AsrError>;

    /// Approximate resident memory, for the manager's eviction policy.
    fn memory_estimate_mb(&self) -> usize;

    /// Run one throwaway inference so the first real one isn't 3–5× slower.
    fn warmup(&self) -> Result<(), AsrError>;
}
```

`asr/manager.rs` requirements:

- **Lazy loading.** Do not load a model until its language is first used.
- **At most one dictation model resident at a time.** Loading all three costs ~1.2 GB and will get the app uninstalled. Unload the previous model before loading a new one.
- **Exception: the Whisper `base` command model stays resident permanently** (~120 MB). Language switching must feel instant.
- **Idle eviction.** Unload the dictation model after 5 minutes of no use; drop back to ~80 MB resident.
- **Warm up on load,** not on first keypress.
- **Memory-map weights** where sherpa-onnx allows it — cuts reported RSS by 30–40%.

Target footprint: **~80 MB idle, ~450 MB while actively dictating.**

---

## 8. Language system

### `lang/languages.toml`

Adding languages 4 through 20 must require **no Rust changes**. This file is the extension point.

```toml
[[language]]
code = "en"
display = "English"
native = "English"
engine = "whisper"
model = "whisper-base"
aliases = ["english", "inglish", "angrezi", "ingreji", "ইংরেজি", "अंग्रेज़ी"]

[[language]]
code = "hi"
display = "Hindi"
native = "हिंदी"
engine = "conformer"
model = "indicconformer-hi"
aliases = ["hindi", "hindee", "hindhi", "हिंदी", "हिन्दी", "hindustani"]

[[language]]
code = "bn"
display = "Bengali"
native = "বাংলা"
engine = "conformer"
model = "indicconformer-bn"
aliases = ["bengali", "bangla", "bangali", "bengoli", "benagli", "বাংলা", "বাঙলা"]
```

### `lang/matcher.rs`

```rust
pub fn match_language(spoken: &str, enabled: &[Language])
    -> MatchResult
{
    // 1. Lowercase, trim, strip trailing punctuation.
    // 2. Strip filler words: switch, to, change, set, please,
    //    language, karo, kar, do, koro, cholo, mode.
    // 3. Score EVERY remaining word against EVERY alias of EVERY
    //    ENABLED language using Jaro-Winkler. Scoring per-word (not
    //    on the whole string) means "switch to Bengali please" works
    //    without special-casing sentence forms.
    // 4. Take the best score.
    //      < 0.82                      → NoMatch
    //      best and runner-up within 0.05 → Ambiguous(top_two)
    //      otherwise                   → Matched(lang, score)
}
```

Three outcomes, three distinct pill responses:

| Result | Pill shows | Duration |
|---|---|---|
| `Matched` | `✓ বাংলা` in teal | 1.2 s then fade |
| `NoMatch` | `Didn't catch that — still English` | 2 s |
| `Ambiguous` | both options, arrow keys to pick | until chosen or Esc |
| model missing | `தமிழ் isn't installed — Download?` | until dismissed |

Only match against **enabled** languages, never all 22 — this alone removes most confusions (Hindi/Sindhi, Marathi/Maithili).

Pass the full list of enabled language names as `hotwords` in Command mode. Biasing Whisper toward the exact words it might hear is free accuracy.

**Fallback path:** if the user presses Right Cmd + Right Option and releases **without speaking** (VAD detects no voice), open a small searchable language palette instead. Voice switching must never hard-fail.

---

## 9. Text injection

Strategy selection, in order of preference:

1. **`AXUIElement`** (`macos_ax.rs`) — insert directly into the focused accessibility element. Most reliable in native apps like Mail, Notes, Xcode.
2. **`CGEventKeyboardSetUnicodeString`** (`macos_cgevent.rs`) — posts arbitrary Unicode directly. Handles Devanagari and Bengali natively on macOS; no clipboard needed. Use for Electron apps and browsers where AX insertion is unreliable.
3. **Clipboard + Cmd+V** (`clipboard.rs`) — last resort. Must save and restore the user's previous clipboard contents.

**Undo:** record the exact character count of the last insertion. `Right Cmd + Z` sends that many backspaces. Invalidate the undo buffer after 5 seconds, or immediately if any other keystroke is observed — otherwise you will delete text the user typed themselves.

**Injection is not live in v1.** Text streams into the *pill* as a preview while transcribing, then commits to the target app in one shot on completion. Live typing requires backspace-based correction of revised tokens, which can destroy user content if the cursor moved. Leave a `TextInjector::supports_streaming()` hook in the trait for v2.

---

## 10. The pill overlay

### Window setup — get this right first

This is the highest-risk part of the build. A normal Tauri window activates the app and steals focus; when focus moves, the target app's cursor is lost and there is nowhere to insert text.

```rust
// overlay/panel.rs
let pill = WebviewWindowBuilder::new(app, "pill", WebviewUrl::App("pill.html".into()))
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .focused(false)
    .skip_taskbar(true)
    .resizable(false)
    .shadow(false)
    .inner_size(240.0, 84.0)
    .build()?;

// REQUIRED on macOS — a plain NSWindow still activates the app.
// Convert to a non-activating NSPanel:
//   style mask:            NSWindowStyleMaskNonactivatingPanel
//   level:                 NSScreenSaverWindowLevel (above fullscreen apps)
//   collectionBehavior:    canJoinAllSpaces | stationary | fullScreenAuxiliary
// Use the `tauri-nspanel` crate.
```

**Verify before building anything else:** open TextEdit, place the cursor, trigger the pill, and confirm the TextEdit cursor still blinks. If it stops blinking, the panel is stealing focus and nothing downstream will work.

### Position

Top-center, just below the notch:

```rust
let mon = window.primary_monitor()?.ok_or(AppError::NoMonitor)?;
let scale = mon.scale_factor();
let logical_w = mon.size().width as f64 / scale;
let x = (logical_w - 240.0) / 2.0;
window.set_position(LogicalPosition::new(x, 44.0))?;
```

Recompute on `monitor-changed` and when the pill is shown — the user may be on an external display.

### Layout

```
        ╭────────────────────────────╮
        │   ∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿∿   │  ← canvas, 240×56
        │           বাংলা             │  ← language, 11px
        ╰────────────────────────────╯
                  240 × 84
```

Visual direction is **fixed by the supplied component**: teal `rgb(0, 255, 192)` on black, with the trailing motion-blur. Do not substitute a different palette.

One consequence worth understanding: the trail effect works by painting `rgba(0,0,0,0.1)` over the previous frame instead of clearing it. That requires an **opaque black** canvas — it is incompatible with a translucent blurred pill. Keep the pill solid black with a 20 px radius. It reads as deliberate against the menu bar.

Font stack must cover both scripts or you will get tofu boxes instead of বাংলা:

```css
font-family: -apple-system, "SF Pro Text", "Noto Sans Bengali",
             "Noto Sans Devanagari", sans-serif;
```

### The waveform component

Adapt the supplied `SonicWaveform` as follows. Four changes are mandatory:

**(a) Retina scaling.** The original sets `canvas.width = canvas.clientWidth`, which ignores device pixel ratio and renders visibly blurry on a MacBook display.

**(b) Line count 60 → 14.** The original draws 60 × 80 = 4,800 segments per frame. That is fine full-screen, but this pill is always-on-top and running while the user works. 14 lines keeps the identical look at a fraction of the cost.

**(c) Amplitude reactivity.** The original is purely time-driven ambient motion. It must respond to the microphone: quiet speech → a narrow band, loud speech → tall spikes.

**(d) Lifecycle.** Stop `requestAnimationFrame` entirely when not recording. An always-running rAF in a floating window is a real battery drain.

```tsx
// src/components/SonicWaveform.tsx
import React, { useEffect, useRef } from "react";

interface Props {
  /** 0..1 smoothed microphone level, from useAudioLevel() */
  level: number;
  /** false → freeze and stop the rAF loop */
  active: boolean;
}

const SonicWaveform: React.FC<Props> = ({ level, active }) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const levelRef = useRef(0);      // smoothed, read inside rAF
  const rafRef = useRef<number>();

  // Keep the latest level in a ref so the rAF closure isn't re-created
  // on every render.
  useEffect(() => {
    levelRef.current = level;
  }, [level]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) return;

    let time = 0;
    let smoothed = 0;

    const resize = () => {
      const dpr = window.devicePixelRatio || 1;
      canvas.width = canvas.clientWidth * dpr;
      canvas.height = canvas.clientHeight * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);   // (a) retina
    };

    const draw = () => {
      const w = canvas.clientWidth;
      const h = canvas.clientHeight;
      const mid = h / 2;

      // (c) Asymmetric smoothing: snap up fast on speech onset,
      // decay slowly. Symmetric smoothing looks sluggish and dead.
      const target = levelRef.current;
      const k = target > smoothed ? 0.5 : 0.12;
      smoothed += (target - smoothed) * k;

      // Floor at 0.05 so the line still breathes in silence
      // rather than going flat and looking broken.
      const amp = Math.max(0.05, smoothed);

      const noiseAmp = h * (0.05 + amp * 0.20);
      const spikeAmp = h * (0.08 + amp * 0.55);

      // Trailing motion blur — requires an opaque canvas.
      ctx.fillStyle = "rgba(0, 0, 0, 0.1)";
      ctx.fillRect(0, 0, w, h);

      const lineCount = 14;          // (b) was 60
      const segmentCount = 60;

      for (let i = 0; i < lineCount; i++) {
        ctx.beginPath();
        const progress = i / lineCount;
        const intensity = Math.sin(progress * Math.PI);

        // Brightness also tracks amplitude, so louder reads as hotter.
        ctx.strokeStyle =
          `rgba(0, 255, 192, ${intensity * (0.25 + amp * 0.45)})`;
        ctx.lineWidth = 1.2;

        for (let j = 0; j <= segmentCount; j++) {
          const x = (j / segmentCount) * w;
          const noise = Math.sin(j * 0.1 + time + i * 0.2) * noiseAmp;
          const spike =
            Math.cos(j * 0.2 + time + i * 0.1) *
            Math.sin(j * 0.05 + time) * spikeAmp;
          const y = mid + noise + spike;
          j === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
        }
        ctx.stroke();
      }

      // Speed up slightly when speaking — subtle but it reads as alive.
      time += 0.02 + amp * 0.02;
      rafRef.current = requestAnimationFrame(draw);
    };

    window.addEventListener("resize", resize);
    resize();

    if (active) draw();                                    // (d)

    return () => {
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
      window.removeEventListener("resize", resize);
    };
  }, [active]);

  return <canvas ref={canvasRef} className="waveform" />;
};

export default SonicWaveform;
```

Respect `prefers-reduced-motion`: fall back to a static centre line with an opacity pulse.

### Audio level pipeline

```rust
// audio/level.rs — inside the cpal input callback
let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
let level = (rms * 8.0).clamp(0.0, 1.0);   // tune the multiplier per device
// Throttle to ~30 Hz. Emitting per-buffer floods the IPC channel and
// gains nothing visible.
app.emit_to("pill", "audio-level", level)?;
```

### Pill states

| State | Waveform | Label |
|---|---|---|
| Dictate / recording | live, amplitude-driven | active language, e.g. `বাংলা` |
| Command / recording | live, amplitude-driven | `Say a language…` |
| Transcribing | frozen, 40% opacity | `…` |
| Cancelled | instant fade out | — |
| Language matched | frozen | `✓ বাংলা` in teal, 1.2 s |
| Secure input | frozen, dim | `Can't type here` |

---

## 11. macOS permissions

Three separate TCC prompts. Firing them all at launch with no context gets them denied, and a denied app fails **silently** — this is the number-one cause of bad reviews for this category.

Build `PermissionsWizard.tsx` as a first-run flow, one permission per screen, each explaining what breaks without it:

| Permission | Needed for | Check |
|---|---|---|
| Microphone | recording | `AVCaptureDevice.authorizationStatus` |
| Accessibility | injecting text | `AXIsProcessTrustedWithOptions` |
| Input Monitoring | seeing global keypresses | `IOHIDCheckAccess(kIOHIDRequestTypeListenEvent)` |

Re-check on every launch and show a persistent banner if any was revoked.

**Development note:** macOS ties TCC grants to the code signature, so every unsigned rebuild is treated as a new app and permissions reset constantly. Set up an ad-hoc signing identity in `scripts/sign.sh` from day one. For distribution you need a paid Apple Developer account for signing and notarization — unsigned apps are Gatekeeper-blocked and Accessibility behaves unreliably.

---

## 12. Settings schema

```toml
# ~/Library/Application Support/SpeechX/settings.toml
schema_version = 1

[general]
launch_at_login = true
play_sounds = true
active_language = "bn"

[hotkeys]
dictate = "RightCommand"
command_mode = "RightCommand+RightOption"
cancel = "Escape"
undo = "RightCommand+KeyZ"

[audio]
input_device = "default"      # "default" or a device name
vad_enabled = true
vad_silence_ms = 2500
max_recording_secs = 120

[overlay]
position = "top-center"       # top-center | bottom-center
offset_y = 44

[languages]
enabled = ["en", "hi", "bn"]

[vocabulary]
terms = []                    # names, jargon — passed as ASR hotwords
```

Include a `schema_version` and a migration path from the start. Retrofitting one onto shipped user configs is painful.

---

## 13. Build milestones

Implement **one milestone per session.** Verify each before moving on.

**M1 — Hotkey foundation**
`rdev` listener on its own thread. Detect Right Command down/up, Right Option, Esc. Implement the 150 ms chord resolution. Log resolved mode to console. *Done when:* holding Right Cmd logs `Dictate`, holding Right Cmd + Right Option logs `Command`, Esc mid-hold logs `Cancelled`, and normal Cmd+C still works everywhere.

**M2 — Audio capture**
`cpal` capture into a ring buffer while the key is held. Resample to 16 kHz mono f32. Write to `/tmp/speechx_test.wav` on release. *Done when:* the WAV plays back correctly with no clipped first syllable.

**M3 — ASR**
Wire up sherpa-onnx. Implement the `AsrEngine` trait plus the Whisper impl. Transcribe M2's WAV, print to console. Then add the Conformer impl and confirm Hindi and Bengali output in the correct scripts. *Done when:* all three languages transcribe from file.

**M4 — Text injection**
Implement the three injection strategies with fallback. *Done when:* Bengali text lands correctly in TextEdit, Chrome, and VS Code — all three, they behave differently.

*At this point you have a working dictation tool with no UI. Everything after this is making it pleasant.*

**M5 — The pill**
NSPanel setup (verify focus is not stolen **first**), positioning, the waveform component, the audio-level event pipeline.

**M6 — Command mode**
Language registry, fuzzy matcher, always-resident Whisper base, all four pill result states, the palette fallback.

**M7 — Model management**
Manifest, resumable downloads with checksum verification, the Languages settings pane, lazy loading and idle eviction.

**M8 — Settings, history, undo, custom vocabulary, VAD auto-stop, sounds**

**M9 — Permissions wizard, signing, notarization, DMG packaging**

---

## 14. Quality bar

- `#![deny(unsafe_op_in_unsafe_fn)]`; every `unsafe` block carries a comment justifying it
- No `unwrap()` or `expect()` outside tests and `main()`
- All fallible IPC commands return `Result<T, String>` with messages fit for display
- `lib/types.ts` and `ipc/events.rs` must not drift — generate the TS types from Rust with `ts-rs` if practical
- Unit tests required for: `lang/matcher.rs` (including near-collisions), `hotkey/chord.rs` timing, `audio/resample.rs`, settings migration
- `tracing` spans around every session; logs roll daily and stay local
- The app must never leave a session stuck — every path returns to `Idle`, including panics inside the ASR thread

---

## 15. Known hazards

Flag these to the agent explicitly; each has cost people a full day:

1. **NSPanel focus theft.** Test with a blinking cursor in TextEdit before building anything on top of the pill.
2. **Recording must start on key-down, not after the 150 ms chord timer.** Otherwise the first syllable is clipped and it sounds broken.
3. **Canvas without DPR scaling** looks blurry on every Mac made in the last decade.
4. **Missing Bengali/Devanagari fonts** render as tofu boxes, which looks like data corruption.
5. **Loading all models at once** costs ~1.2 GB and will get the app uninstalled on 8 GB machines.
6. **Undo after the user has moved the cursor** deletes their content. Invalidate aggressively.
7. **Secure input mode** silently swallows synthetic keystrokes in password fields. Detect and report it.
8. **IndicConformer outputs unpunctuated text.** Whisper punctuates natively; CTC/RNNT models do not. Either add a punctuation-restoration model in `postprocess.rs` or accept it in v1 — but decide deliberately rather than shipping it as a surprise.

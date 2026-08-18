import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import SonicWaveform from "./components/SonicWaveform";

type PillState = "hidden" | "recording" | "transcribing" | "matched";

interface ShowPayload {
  mode: "dictate" | "command";
  language: string;
}

// The fully rounded stadium shape (pill.css's `.pill` border-radius) glitches
// in WebView2's transparent-window compositing on Windows — corners outside
// the radius don't clear properly. Square corners sidestep it. macOS (NSPanel
// compositing, not WebView2) doesn't have this problem, so only Windows gets
// the square variant.
const IS_WINDOWS = navigator.userAgent.includes("Windows");

function PillApp() {
  const [state, setState] = useState<PillState>("hidden");
  const [label, setLabel] = useState("");
  const [level, setLevel] = useState(0);
  // requestAnimationFrame keeps the waveform smooth between the ~30 Hz
  // level events rather than snapping once per event.
  const levelRef = useRef(0);

  useEffect(() => {
    const unlisten = Promise.all([
      listen<ShowPayload>("pill-show", (event) => {
        setLabel(event.payload.language);
        setState("recording");
      }),
      listen("pill-transcribing", () => {
        setState("transcribing");
      }),
      listen("pill-hide", () => {
        setState("hidden");
      }),
      listen("pill-cancelled", () => {
        setState("hidden");
      }),
      listen<string>("pill-matched", (event) => {
        setLabel(event.payload);
        setState("matched");
      }),
      listen<number>("audio-level", (event) => {
        levelRef.current = event.payload;
        setLevel(event.payload);
      }),
    ]);

    return () => {
      unlisten.then((fns) => fns.forEach((fn) => fn()));
    };
  }, []);

  const shapeClass = IS_WINDOWS ? "pill-square" : "";

  if (state === "hidden") {
    return <div className={`pill pill-hidden ${shapeClass}`} />;
  }

  return (
    <div className={`pill pill-${state} ${shapeClass}`}>
      <SonicWaveform level={state === "recording" ? level : 0} active={state === "recording"} />
      <div className="pill-label">
        {state === "matched" ? `✓ ${label}` : state === "transcribing" ? "…" : label}
      </div>
    </div>
  );
}

export default PillApp;

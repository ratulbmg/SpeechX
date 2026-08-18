import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

// Windows uses Right Alt; macOS uses Right ⌘. Detected at runtime so the
// same binary works on both platforms without a rebuild.
const TRIGGER_KEY = navigator.userAgent.includes("Windows") ? "Right Alt" : "Right ⌘";

type PermissionState = "checking" | "granted" | "denied";

const POLL_INTERVAL_MS = 2000;

interface PermissionRowProps {
  label: string;
  state: PermissionState;
  onRequest: () => void;
  locked?: boolean;
  lockedLabel?: string;
}

function PermissionRow({ label, state, onRequest, locked, lockedLabel }: PermissionRowProps) {
  const granted = state === "granted";
  return (
    <div className="permission-row">
      <span className="permission-label">{label}</span>
      <button
        className={granted ? "permission-button granted" : "permission-button"}
        disabled={granted || state === "checking" || locked}
        onClick={onRequest}
      >
        {granted
          ? "✓ Granted"
          : locked
            ? lockedLabel
            : state === "checking"
              ? "Checking…"
              : "Grant Access"}
      </button>
    </div>
  );
}

interface ListeningToggleProps {
  enabled: boolean | null;
  onChange: (enabled: boolean) => void;
}

function ListeningToggle({ enabled, onChange }: ListeningToggleProps) {
  const checked = enabled ?? true;
  return (
    <div className="toggle-row">
      <div className="toggle-text">
        <span className="toggle-label">Listening</span>
        <span className="toggle-description">
          {checked
            ? `${TRIGGER_KEY} arms dictation.`
            : `${TRIGGER_KEY} is ignored until re-enabled.`}
        </span>
      </div>
      <button
        role="switch"
        aria-checked={checked}
        aria-label="Listening"
        disabled={enabled === null}
        className={checked ? "switch on" : "switch"}
        onClick={() => onChange(!checked)}
      >
        <span className="switch-knob" />
      </button>
    </div>
  );
}

type Tab = "controls" | "permissions";

function App() {
  const [tab, setTab] = useState<Tab>("controls");
  const [mic, setMic] = useState<PermissionState>("checking");
  const [accessibility, setAccessibility] = useState<PermissionState>("checking");
  const [listening, setListening] = useState<boolean | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;

    const checkAll = async () => {
      const [micGranted, axGranted, listeningEnabled] = await Promise.all([
        invoke<boolean>("check_microphone_permission"),
        invoke<boolean>("check_accessibility_permission"),
        invoke<boolean>("get_listening_enabled"),
      ]);
      if (!mounted.current) return;
      setMic(micGranted ? "granted" : "denied");
      setAccessibility(axGranted ? "granted" : "denied");
      setListening(listeningEnabled);
    };

    checkAll();
    const interval = setInterval(checkAll, POLL_INTERVAL_MS);
    return () => {
      mounted.current = false;
      clearInterval(interval);
    };
  }, []);

  const toggleListening = (next: boolean) => {
    setListening(next);
    invoke("set_listening_enabled", { enabled: next });
  };

  return (
    <main className="dashboard">
      <h1>SpeechX</h1>
      <p className="subtitle">
        Hold <kbd>{TRIGGER_KEY}</kbd> to dictate.
      </p>

      <div className="tab-bar" role="tablist">
        <button
          role="tab"
          aria-selected={tab === "controls"}
          className={tab === "controls" ? "tab active" : "tab"}
          onClick={() => setTab("controls")}
        >
          Controls
        </button>
        <button
          role="tab"
          aria-selected={tab === "permissions"}
          className={tab === "permissions" ? "tab active" : "tab"}
          onClick={() => setTab("permissions")}
        >
          Permissions
        </button>
      </div>

      <div className="tab-panel">
        {tab === "controls" && <ListeningToggle enabled={listening} onChange={toggleListening} />}

        {tab === "permissions" && (
          <>
            <div className="permission-list">
              <PermissionRow
                label="Accessibility"
                state={accessibility}
                onRequest={() => invoke("request_accessibility_permission")}
              />
              <PermissionRow
                label="Microphone"
                state={mic}
                locked={accessibility !== "granted"}
                lockedLabel="Grant Accessibility first"
                onRequest={() => invoke("request_microphone_permission")}
              />
            </div>

            <p className="hint">
              Granting a permission may require quitting and reopening SpeechX once before it
              takes effect.
            </p>
          </>
        )}
      </div>

      <button className="quit-button" onClick={() => invoke("quit_app")}>
        Quit SpeechX
      </button>
    </main>
  );
}

export default App;

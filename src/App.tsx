import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import "./App.css";

// Windows uses Right Alt; macOS uses Right ⌘. Detected at runtime so the
// same binary works on both platforms without a rebuild.
const IS_WINDOWS = navigator.userAgent.includes("Windows");
const TRIGGER_KEY = IS_WINDOWS ? "Right Alt" : "Right ⌘";

// Manual fallback steps for when a permission's automatic "Grant Access"
// flow doesn't do anything visible (see MANUAL_PERMISSIONS.md — Input
// Monitoring especially has a known macOS quirk where its system prompt
// can silently fail to appear). macOS-only: Windows has no equivalent
// permission system to walk through manually (see check_accessibility_
// permission's Windows arm in commands.rs) — nothing to link to there.
const MANUAL_PERMISSIONS_URL =
  "https://github.com/ratulbmg/SpeechX/blob/main/MANUAL_PERMISSIONS.md";

type PermissionState = "checking" | "granted" | "denied";

const POLL_INTERVAL_MS = 2000;

// macOS's Accessibility trust check (AXIsProcessTrustedWithOptions) can
// briefly report an already-granted permission as false right after the
// process launches — it self-corrects within a poll cycle or two, but at
// the normal 2s interval that can take the better part of a minute to
// resolve, which reads as "it's not detecting my permission" even though
// nothing is actually wrong. Polling fast for the first few seconds after
// the dashboard mounts (a fresh install, or reopening after granting
// something, are exactly when this matters most) makes an already-granted
// state show correctly almost immediately instead of leaving it to chance.
const FAST_POLL_INTERVAL_MS = 400;
const FAST_POLL_DURATION_MS = 8000;

interface PermissionRowProps {
  label: string;
  state: PermissionState;
  onRequest: () => void;
  locked?: boolean;
  lockedLabel?: string;
  manualLinkAnchor?: string;
}

function PermissionRow({
  label,
  state,
  onRequest,
  locked,
  lockedLabel,
  manualLinkAnchor,
}: PermissionRowProps) {
  const granted = state === "granted";
  return (
    <div className="permission-row">
      <div className="permission-text">
        <span className="permission-label">{label}</span>
        {!IS_WINDOWS && (
          <button
            className="permission-manual-link"
            onClick={() => openUrl(`${MANUAL_PERMISSIONS_URL}#${manualLinkAnchor}`)}
          >
            Manual steps
          </button>
        )}
      </div>
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
  const [inputMonitoring, setInputMonitoring] = useState<PermissionState>("checking");
  const [listening, setListening] = useState<boolean | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    const mountedAt = Date.now();
    let timer: ReturnType<typeof setTimeout>;

    const checkAll = async () => {
      const [micGranted, axGranted, inputMonitoringGranted, listeningEnabled] = await Promise.all([
        invoke<boolean>("check_microphone_permission"),
        invoke<boolean>("check_accessibility_permission"),
        invoke<boolean>("check_input_monitoring_permission"),
        invoke<boolean>("get_listening_enabled"),
      ]);
      if (!mounted.current) return;
      setMic(micGranted ? "granted" : "denied");
      setAccessibility(axGranted ? "granted" : "denied");
      setInputMonitoring(inputMonitoringGranted ? "granted" : "denied");
      setListening(listeningEnabled);

      const stillWarmingUp = Date.now() - mountedAt < FAST_POLL_DURATION_MS;
      timer = setTimeout(checkAll, stillWarmingUp ? FAST_POLL_INTERVAL_MS : POLL_INTERVAL_MS);
    };

    checkAll();
    return () => {
      mounted.current = false;
      clearTimeout(timer);
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
              {!IS_WINDOWS && (
                <PermissionRow
                  label="Accessibility"
                  state={accessibility}
                  onRequest={() => invoke("request_accessibility_permission")}
                  manualLinkAnchor="accessibility"
                />
              )}
              {!IS_WINDOWS && (
                <PermissionRow
                  label="Input Monitoring"
                  state={inputMonitoring}
                  locked={accessibility !== "granted"}
                  lockedLabel="Grant Accessibility first"
                  onRequest={() => invoke("request_input_monitoring_permission")}
                  manualLinkAnchor="input-monitoring"
                />
              )}
              <PermissionRow
                label="Microphone"
                state={mic}
                locked={accessibility !== "granted" || inputMonitoring !== "granted"}
                lockedLabel="Grant steps above first"
                onRequest={() => invoke("request_microphone_permission")}
                manualLinkAnchor="microphone"
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

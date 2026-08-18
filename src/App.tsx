import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import "./App.css";

// Windows uses Right Alt; macOS uses Right ⌘. Detected at runtime so the
// same binary works on both platforms without a rebuild.
const IS_WINDOWS = navigator.userAgent.includes("Windows");
const TRIGGER_KEY = IS_WINDOWS ? "Right Alt" : "Right ⌘";

// Manual fallback steps for when a permission's automatic prompt (fired
// once at launch — see `onboard_permissions_sequentially` in lib.rs)
// doesn't do anything visible (see MANUAL_PERMISSIONS.md — Input
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
  manualLinkAnchor?: string;
}

// Pure status display — no button, nothing to click. Permissions are now
// requested automatically, one at a time, at launch (see
// `onboard_permissions_sequentially` in lib.rs); this row exists only to
// show whether each one has actually been granted.
function PermissionRow({ label, state, manualLinkAnchor }: PermissionRowProps) {
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
      <span className={granted ? "permission-status granted" : "permission-status"}>
        {granted ? "Granted" : state === "checking" ? "Checking…" : "Not granted"}
      </span>
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

interface MicrophoneSelectorProps {
  devices: string[];
  selected: string | null;
  onChange: (device: string | null) => void;
}

// `null` means "System Default" — whatever the OS currently treats as
// the default input device, tracked live rather than pinned to whichever
// one happened to be default when the user last opened the dashboard.
const SYSTEM_DEFAULT_VALUE = "";

function MicrophoneSelector({ devices, selected, onChange }: MicrophoneSelectorProps) {
  return (
    <div className="mic-row">
      <div className="toggle-text">
        <span className="toggle-label">Microphone</span>
        <span className="toggle-description">Input device dictation records from.</span>
      </div>
      <select
        className="mic-select"
        value={selected ?? SYSTEM_DEFAULT_VALUE}
        onChange={(e) => onChange(e.target.value === SYSTEM_DEFAULT_VALUE ? null : e.target.value)}
      >
        <option value={SYSTEM_DEFAULT_VALUE}>System Default</option>
        {devices.map((device) => (
          <option key={device} value={device}>
            {device}
          </option>
        ))}
      </select>
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
  const [microphones, setMicrophones] = useState<string[]>([]);
  const [selectedMic, setSelectedMic] = useState<string | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    const mountedAt = Date.now();
    let timer: ReturnType<typeof setTimeout>;

    const checkAll = async () => {
      const [micGranted, axGranted, inputMonitoringGranted, listeningEnabled, deviceList, selectedDevice] =
        await Promise.all([
          invoke<boolean>("check_microphone_permission"),
          invoke<boolean>("check_accessibility_permission"),
          invoke<boolean>("check_input_monitoring_permission"),
          invoke<boolean>("get_listening_enabled"),
          invoke<string[]>("list_microphones"),
          invoke<string | null>("get_selected_microphone"),
        ]);
      if (!mounted.current) return;
      setMic(micGranted ? "granted" : "denied");
      setAccessibility(axGranted ? "granted" : "denied");
      setInputMonitoring(inputMonitoringGranted ? "granted" : "denied");
      setListening(listeningEnabled);
      setMicrophones(deviceList);
      setSelectedMic(selectedDevice);

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

  const changeMicrophone = (device: string | null) => {
    setSelectedMic(device);
    invoke("set_selected_microphone", { name: device });
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
        {tab === "controls" && (
          <>
            <ListeningToggle enabled={listening} onChange={toggleListening} />
            <MicrophoneSelector devices={microphones} selected={selectedMic} onChange={changeMicrophone} />
          </>
        )}

        {tab === "permissions" && (
          <>
            <div className="permission-list">
              {!IS_WINDOWS && (
                <PermissionRow label="Accessibility" state={accessibility} manualLinkAnchor="accessibility" />
              )}
              {!IS_WINDOWS && (
                <PermissionRow
                  label="Input Monitoring"
                  state={inputMonitoring}
                  manualLinkAnchor="input-monitoring"
                />
              )}
              <PermissionRow label="Microphone" state={mic} manualLinkAnchor="microphone" />
            </div>

            <p className="hint">
              SpeechX asks for these automatically, one at a time, when it first launches. If one
              shows "Not granted", use the manual steps link above or quit and reopen SpeechX
              once after granting it in System Settings.
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

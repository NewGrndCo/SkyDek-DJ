import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
export type Preferences = {
  sampler_modes: ("restart" | "toggle" | "overlap" | "hold")[];
  cue_seconds: number; crossfade_seconds: number; audio_device: string | null; sampler_volume: number;
  auto_dj_seconds: number;
  profiles: Record<string, import("./MapperPanel").Mappings>; active_profile: string;
};
export type GeneralSettingsProps = {
  preferences: Preferences; sensitivity: number; device: string; rate: number;
  command: (action: string, deck?: number, value?: number) => Promise<void>;
  resetLayout: () => void;
  section?: "general" | "audio" | "controller";
};
export function GeneralSettings({ preferences, sensitivity, device, rate, command, resetLayout, section }: GeneralSettingsProps) {
  const [devices, setDevices] = useState<string[]>([]), [selected, setSelected] = useState(preferences.audio_device || ""),
    [message, setMessage] = useState(""), [applying, setApplying] = useState(false);
  async function refresh() {
    try { setDevices(await invoke<string[]>("audio_devices")); }
    catch(e) { setMessage(String(e)); }
  }
  useEffect(() => { if (!section || section === "audio") void refresh(); }, [section]);
  useEffect(() => { setSelected(preferences.audio_device || ""); }, [preferences.audio_device]);
  const runCommand: typeof command = async (...args) => {
    try { await command(...args); } catch (error) { setMessage(String(error)); }
  };
  return <div className="general-settings">
    {(!section || section === "audio") && <>
    <h3>Audio output</h3>
    <p>Active: {device || "No output available"} · {rate} Hz · WASAPI shared</p>
    <label>Output device <select aria-label="Audio output device" value={selected} onChange={e => setSelected(e.target.value)}>
      <option value="">Windows default output</option>
      {preferences.audio_device && !devices.includes(preferences.audio_device) && <option value={preferences.audio_device}>{preferences.audio_device} (unavailable)</option>}
      {devices.map(name => <option key={name} value={name}>{name}</option>)}
    </select></label>
    <div className="settings-actions"><button onClick={refresh}>Refresh devices</button><button disabled={applying} onClick={async () => {
      if (!window.confirm("Apply this audio device and restart SkyDek DJ? Playback will stop. Library and mappings will be saved.")) return;
      setApplying(true);
      try { await invoke("select_audio_device", { device: selected || null }); }
      catch(e) { setMessage(String(e)); setApplying(false); }
    }}>Apply output & restart</button></div>
    <small>Connect your audio interface, refresh, then choose its output. Changing output currently requires an application restart.</small>
    </>}
    {(!section || section === "general") && <>
    <h3>Library analysis</h3>
    <button onClick={()=>invoke("analyze_library",{force:true,cancel:false}).then(()=>setMessage("Library rescan started; progress appears in the main toolbar.")).catch(e=>setMessage(String(e)))}>Rescan all tracks for duration & BPM</button>
    <h3>Cue hold time</h3>
    <label>Hold before release keeps playing (seconds) <input aria-label="Cue hold seconds" type="number" min="0.1" max="10" step="0.1" defaultValue={preferences.cue_seconds} onBlur={e => runCommand("cue_seconds", undefined, Math.max(0.1, Math.min(10, Number(e.target.value) || 0.1)))} /></label>
    <p>Tap to return to the cue. Hold for this duration, then release to continue playing.</p>
    </>}
    {(!section || section === "controller") && <>
    <h3>Crossfader</h3>
    <label>Stick sensitivity <input aria-label="Crossfader sensitivity" type="range" min="0.1" max="4" step="0.1" defaultValue={sensitivity} onChange={e => runCommand("sensitivity", undefined, Number(e.target.value))} /></label>
    <label>Automatic crossfade duration (seconds) <input aria-label="Automatic crossfade seconds" type="number" min="0.1" max="10" step="0.1" defaultValue={preferences.crossfade_seconds} onBlur={e => runCommand("crossfade_seconds", undefined, Math.max(0.1, Math.min(10, Number(e.target.value) || 0.1)))}/></label>
    <p>L3 starts a smooth crossfade to the opposite side. A shorter duration moves faster. Moving the stick or slider takes over immediately.</p>
    </>}
    {(!section || section === "general") && <>
    <h3>Auto DJ</h3>
    <label>
      Start fading before track ends (seconds)
      <input
        type="number"
        min="1"
        max="60"
        step="1"
        defaultValue={preferences.auto_dj_seconds}
        onBlur={event => {
          const seconds = event.currentTarget.valueAsNumber;
          if (Number.isFinite(seconds)) {
            const bounded = Math.max(1, Math.min(60, seconds));
            event.currentTarget.value = String(bounded);
            void runCommand("auto_dj_seconds", undefined, bounded);
          } else {
            event.currentTarget.value = String(preferences.auto_dj_seconds);
          }
        }}
      />
    </label>
    <p>Turn on Auto DJ and play a track to mix through the imported songs in its folder. Short tracks use a shorter fade.</p>
    </>}
    {!section && <>
    <h3>Display</h3>
    <button onClick={resetLayout}>Reset panel sizes</button>{" "}
    <button onClick={() => invoke("toggle_fullscreen").catch(e => setMessage(String(e)))}>Toggle fullscreen (F11)</button>
    </>}
    {(!section || section === "controller") && <>
    <h3>Controller navigation</h3>
    <p>D-pad left/right selects Folders → Library → Sampler. Within the sampler, all four directions navigate the 4×4 pad grid. Left from its first column returns to the library; Shift+left also changes panels. Up/down browses folders and songs in their panels. R3 triggers the highlighted pad using its playback mode. L3 starts the automatic crossfade by default. Clicked sticks must return to center before their axes respond again. Deck effects stop on release. View toggles Normal / Shift; Share remains reserved by Windows.</p>
    </>}
    {(!section || section === "general") && <>
    <h3>About</h3><p>SkyDek DJ 0.10.0 · New Ground Enterprise<br/>Standalone audio and Xbox control. Stems separation and phase sync remain unavailable.</p>
    <details><summary>Third-party notices</summary><p>MP3 recording uses a separate bundled encoding utility built with mp3lame-encoder 0.3.0, mp3lame-sys 0.2.0 and LAME, covered by the LGPL and upstream notices. The utility’s complete source is supplied with the download. Full license texts are supplied in the installation’s resources/licenses folder. Other components include React, Tauri, CPAL and Symphonia.</p></details>
    </>}
    <p role="status">{message}</p>
  </div>;
}

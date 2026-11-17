import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
export type Preferences = {
  sampler_modes: ("restart" | "toggle" | "overlap" | "hold")[];
  cue_seconds: number; crossfade_seconds: number; audio_device: string | null; mic_device: string | null; sampler_volume: number; mic_volume: number; mic_mute: boolean; mic_ducking: boolean; linear_curve: boolean;
  auto_dj_seconds: number; auto_dj_source: string; library_monitor:boolean; library_scan_minutes:number; auto_dj_crossfade_seconds:number; auto_dj_sample_count:number; auto_dj_sample_spacing:number; auto_dj_random_samples: boolean; normalize_tracks: boolean; effect_toggle: boolean;
  profiles: Record<string, import("./MapperPanel").Mappings>; active_profile: string;
};
export type GeneralSettingsProps = {
  preferences: Preferences; sensitivity: number; device: string; micStatus: string; rate: number;
  command: (action: string, deck?: number, value?: number) => Promise<void>;
  resetLayout: () => void;
  section?: "general" | "audio" | "controller" | "autodj";
};
export function GeneralSettings({ preferences, sensitivity, device, micStatus, rate, command, resetLayout, section }: GeneralSettingsProps) {
  const [devices, setDevices] = useState<string[]>([]), [selected, setSelected] = useState(preferences.audio_device || ""),
    [micDevices, setMicDevices] = useState<string[]>([]), [selectedMic, setSelectedMic] = useState(preferences.mic_device || ""),
    [message, setMessage] = useState(""), [applying, setApplying] = useState(false);
  async function refresh() {
    try { const [outputs, inputs] = await Promise.all([invoke<string[]>("audio_devices"), invoke<string[]>("mic_devices")]); setDevices(outputs); setMicDevices(inputs); }
    catch(e) { setMessage(String(e)); }
  }
  useEffect(() => { if (!section || section === "audio") void refresh(); }, [section]);
  useEffect(() => { setSelected(preferences.audio_device || ""); }, [preferences.audio_device]);
  useEffect(() => { setSelectedMic(preferences.mic_device || ""); }, [preferences.mic_device]);
  const runCommand: typeof command = async (...args) => {
    try { await command(...args); } catch (error) { setMessage(String(error)); }
  };
  return <div className="general-settings">
    {(!section || section === "audio") && <>
    <h3>Audio output</h3>
    <p>Active: {device || "No output available"} · {rate} Hz · {navigator.platform.includes("Win") ? "WASAPI shared" : "System output"}</p>
    <label>Output device <select aria-label="Audio output device" value={selected} onChange={e => setSelected(e.target.value)}>
      <option value="">System default output</option>
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
    <h3>Microphone</h3>
    <p role="status">{micStatus || "Microphone status unavailable"}</p>
    <label>Input device <select aria-label="Microphone input device" value={selectedMic} onChange={e => setSelectedMic(e.target.value)}>
      <option value="">System default input</option>
      {preferences.mic_device && !micDevices.includes(preferences.mic_device) && <option value={preferences.mic_device}>{preferences.mic_device} (unavailable)</option>}
      {micDevices.map(name => <option key={name} value={name}>{name}</option>)}
    </select></label>
    <button disabled={applying} onClick={async () => {
      if (!window.confirm("Apply this microphone and restart SkyDek DJ? Playback will stop. Library and mappings will be saved.")) return;
      setApplying(true);
      try { await invoke("select_mic_device", { device: selectedMic || null }); }
      catch(e) { setMessage(String(e)); setApplying(false); }
    }}>Apply input &amp; restart</button>
    <label>Microphone level<input aria-label="Microphone level" type="range" min="0" max="1" step="0.01" value={preferences.mic_volume ?? 0.8} onChange={e=>void runCommand("mic_volume",undefined,+e.target.value)} /></label>
    <label className="settings-check"><input type="checkbox" checked={preferences.mic_mute ?? false} onChange={e=>void runCommand("mic_mute",undefined,e.target.checked?1:0)} />Mute microphone</label>
    <label className="settings-check"><input type="checkbox" checked={preferences.mic_ducking ?? false} onChange={e=>void runCommand("mic_ducking",undefined,e.target.checked?1:0)} />Duck music while speaking</label>
    <p>Ducking smoothly reduces deck and sampler audio while voice level is detected.</p>
    <h3>Playback levels</h3>
    <label className="settings-check"><input type="checkbox" checked={preferences.normalize_tracks} onChange={event => void runCommand("normalize_tracks", undefined, event.target.checked ? 1 : 0)} />Match perceived track levels</label>
    <p>Applies a bounded gain estimate to deck tracks when loaded. Volume, crossfader, and master controls remain available. Peak safety limits boosts; strongly compressed tracks may still sound different.</p>
    <h3>Audio effects</h3><label className="settings-check"><input type="checkbox" checked={preferences.effect_toggle} onChange={event=>void runCommand("effect_toggle",undefined,event.target.checked?1:0)}/>Tap effects to toggle on/off</label><p>Off: effects play while held. On: tap once to turn on, then tap the same effect again to turn off. Backspin and brake remain time-limited effects.</p>
    </>}
    {(!section || section === "general") && <>
    <h3>Library analysis</h3>
    <h4>Library monitoring</h4>
    <label className="settings-check"><input type="checkbox" checked={preferences.library_monitor} onChange={e=>void runCommand("library_monitor",undefined,e.target.checked?1:0)}/>Monitor imported folders automatically</label>
    <label>Periodic sync scan (minutes)<input type="number" min="1" max="1440" value={preferences.library_scan_minutes} onChange={e=>void runCommand("library_scan_minutes",undefined,Math.max(1,Math.min(1440,Number(e.target.value)||15)))}/></label>
    <p>Scans locally synced Google Drive, OneDrive, and Dropbox folders. New audio is analyzed only after its file is fully readable.</p>
    <button onClick={()=>invoke("analyze_library",{force:true,cancel:false}).then(()=>setMessage("Library rescan started; progress appears in the main toolbar.")).catch(e=>setMessage(String(e)))}>Rescan all tracks for duration & BPM</button>
    <h3>Cue hold time</h3>
    <label>Hold before release keeps playing (seconds) <input aria-label="Cue hold seconds" type="number" min="0.1" max="10" step="0.1" defaultValue={preferences.cue_seconds} onBlur={e => runCommand("cue_seconds", undefined, Math.max(0.1, Math.min(10, Number(e.target.value) || 0.1)))} /></label>
    <p>Tap to return to the cue. Hold for this duration, then release to continue playing.</p>
    <h3>Mix Editor</h3><p>While recording, SkyDek captures deck, sample, effect, and mixer changes. When you stop, a .skydek.json log is saved beside the MP3 or WAV. Open the log and its matching recording in Mix Editor.</p><button onClick={()=>window.dispatchEvent(new Event("skydek-open-replay"))}>Open Mix Editor</button>
    </>}
    {(!section || section === "controller") && <>
    <h3>Crossfader</h3>
    <label>Stick sensitivity <input aria-label="Crossfader sensitivity" type="range" min="0.1" max="4" step="0.1" defaultValue={sensitivity} onChange={e => runCommand("sensitivity", undefined, Number(e.target.value))} /></label>
    <label>Automatic crossfade duration (seconds) <input aria-label="Automatic crossfade seconds" type="number" min="0.1" max="10" step="0.1" defaultValue={preferences.crossfade_seconds} onBlur={e => runCommand("crossfade_seconds", undefined, Math.max(0.1, Math.min(10, Number(e.target.value) || 0.1)))}/></label>
    <label>Crossfader curve<select aria-label="Crossfader curve" value={preferences.linear_curve ? "1" : "0"} onChange={e=>void runCommand("curve",undefined,+e.target.value)}><option value="0">Equal power</option><option value="1">Linear</option></select></label>
    <p>L3 starts a smooth crossfade to the opposite side. A shorter duration moves faster. Moving the stick or slider takes over immediately.</p>
    </>}
    {(!section || section === "autodj") && <>
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
    <label>Crossfade duration (seconds)<input type="number" min="0.1" max="30" step="0.1" defaultValue={preferences.auto_dj_crossfade_seconds} onBlur={e=>void runCommand("auto_dj_crossfade_seconds",undefined,Math.max(.1,Math.min(30,Number(e.target.value)||8)))}/></label>
    <label className="settings-check"><input type="checkbox" checked={preferences.auto_dj_random_samples} onChange={event => void runCommand("auto_dj_random_samples", undefined, event.target.checked ? 1 : 0)} />Play assigned samples during transitions</label>
    {preferences.auto_dj_random_samples && <><label>Samples per transition<input type="number" min="0" max="8" step="1" defaultValue={preferences.auto_dj_sample_count} onBlur={e=>void runCommand("auto_dj_sample_count",undefined,Math.max(0,Math.min(8,Number(e.target.value)||0)))}/></label><label>Seconds between samples<input type="number" min="0.1" max="30" step="0.1" defaultValue={preferences.auto_dj_sample_spacing} onBlur={e=>void runCommand("auto_dj_sample_spacing",undefined,Math.max(.1,Math.min(30,Number(e.target.value)||2)))}/></label></>}
    </>}
    {!section && <>
    <h3>Display</h3>
    <button onClick={resetLayout}>Reset panel sizes</button>{" "}
    <button onClick={() => invoke("toggle_fullscreen").catch(e => setMessage(String(e)))}>Toggle fullscreen (F11)</button>
    </>}
    {(!section || section === "controller") && <>
    <h3>Controller navigation</h3>
    <p>D-pad left/right selects Folders → Library → Sampler. Up/down browses folders and songs only in the focused panel. In the sampler, move through the 4×4 pad grid; press Up from the top row to focus the bank tabs, then Left/Right selects Bank A, B, or C. Press Down to return to the pads. Left from the first column returns to the library; Shift+Left also changes panels. R3 triggers the highlighted pad. L3 starts the automatic crossfade by default. Deck effects follow the hold or toggle option in Audio settings. View toggles Normal / Shift; Share remains reserved by Windows.</p>
    </>}
    {(!section || section === "general") && <>
    <h3>About</h3><p>SkyDek DJ · New Ground Media<br/>Standalone audio, microphone input, Xbox control, and non-destructive Mix Editor. Stems separation and phase sync remain unavailable.</p>
    <details><summary>Third-party notices</summary><p>MP3 recording uses a separate bundled encoding utility built with mp3lame-encoder 0.3.0, mp3lame-sys 0.2.0 and LAME, covered by the LGPL and upstream notices. The utility’s complete source is supplied with the download. Full license texts are supplied in the installation’s resources/licenses folder. Other components include React, Tauri, CPAL and Symphonia.</p></details>
    </>}
    <p role="status">{message}</p>
  </div>;
}

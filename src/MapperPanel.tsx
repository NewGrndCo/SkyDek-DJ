import { useState, useEffect } from "react";
import { ControllerMonitor } from "./ControllerMonitor";
import { invoke } from "@tauri-apps/api/core";
export type Mappings = {
  normal: Record<string, string>;
  shift: Record<string, string>;
};
const buttons = [
  "a",
  "b",
  "x",
  "y",
  "lb",
  "rb",
  "lt",
  "rt",
  "l3",
  "r3",
  "menu",
  "view",
  "dpad-up",
  "dpad-down",
  "dpad-left",
  "dpad-right",
];
const axes = ["left-x", "left-y", "right-x", "right-y"];
const actions: Record<string, string> = {
  auto_cross: "Automatic crossfade to the other deck",
  none: "Unassigned",
  cue_hold: "Cue • hold configured duration to keep playing",
  play: "Play / pause",
  set_cue: "Set cue mark at current position",
  load_selected: "Load highlighted → preparation deck",
  load_next: "Load next Sidelist → preparation deck",
  load_1: "Load highlighted → Deck 1 (protected)",
  load_2: "Load highlighted → Deck 2 (protected)",
  queue_add: "Add highlighted to Sidelist",
  browse_up: "Browse previous track",
  browse_down: "Browse next track",
  panel: "Switch browser panel",
  backspin: "Backspin",
  brake: "Brake stop",
  start: "Brake start",
  echo: "Echo",
  reverb: "Reverb",
  sample: "Trigger selected sampler pad",
  next_sample: "Select next sampler pad",
  stop_samples: "Stop all samples",
  target_next: "Switch target deck",
  auto_target: "Automatic target deck",
  shift: "Toggle Normal / Shift",
  tempo_match: "Sync tempo (analyze both songs first)",
};
const analog: Record<string, string> = {
  none: "Unassigned",
  cross: "Crossfader",
  filter: "Target deck filter",
  wet: "Target deck effect mix",
  volume: "Target deck volume",
  master: "Master volume",
};
export function MapperPanel({ initial, profiles, activeProfile }: { initial: Mappings; profiles: Record<string, Mappings>; activeProfile: string }) {
  const [draft, setDraft] = useState<Mappings>(() => structuredClone(initial)),
    [layer, setLayer] = useState<"normal" | "shift">("normal"),
    [message, setMessage] = useState(""),
    [saving, setSaving] = useState(false);
  const [profileName, setProfileName] = useState("");
  useEffect(() => { setDraft(structuredClone(initial)); setMessage(""); }, [activeProfile]);
  async function runProfile(operation: string, name: string) {
    if (operation === "save" && profiles[name] && !window.confirm("Replace profile " + name + "?")) return;
    setSaving(true);
    try {
      await invoke("mapping_profile", { operation, name, mappings: operation === "save" ? draft : null });
      setProfileName("");
      setMessage(operation === "save" ? "Profile saved and active." : "Profile updated.");
    } catch(e) { setMessage(String(e)); }
    finally { setSaving(false); }
  }
  return (
    <section className="mapper-editor">
      <h3>SkyDek Mapper</h3>
      <ControllerMonitor onSelect={control => {
        const select = document.getElementById("map-" + control);
        select?.scrollIntoView({ block: "nearest" });
        select?.focus();
      }} />
      <div className="profile-tools">
        <label>Map profile <select aria-label="Mapping profile" value={activeProfile} disabled={saving} onChange={e => runProfile("select", e.target.value)}>{Object.keys(profiles).map(name => <option key={name}>{name}</option>)}</select></label>
        <input aria-label="New profile name" placeholder="New profile name" value={profileName} onChange={e => setProfileName(e.target.value)} maxLength={80}/>
        <button disabled={saving || !profileName.trim()} onClick={() => runProfile("save", profileName.trim())}>Save as new profile</button>
        <button disabled={saving || Object.keys(profiles).length < 2} onClick={() => { if(window.confirm("Delete profile " + activeProfile + "?")) void runProfile("delete", activeProfile); }}>Delete profile</button>
      </div>
      <p>
        These assignments control SkyDek DJ directly. Changes take effect after
        Save mappings. B defaults to cue preview; hold it for the time configured in General settings and
        release to continue playing. The View button uses the
        overlapping-squares icon; Xbox Share is not exposed by XInput.
      </p>
      <div className="tabs">
        {(["normal", "shift"] as const).map((l) => (
          <button
            key={l}
            className={layer === l ? "active" : ""}
            onClick={() => setLayer(l)}
          >
            {l.toUpperCase()}
          </button>
        ))}
      </div>
      <div className="mapping-scroll">
        {[...buttons, ...axes].map((control) => {
          const choices = axes.includes(control) ? analog : actions;
          return (
            <label className="mapping-row" key={control}>
              <b>{control.toUpperCase()}</b>
              <select
                id={"map-" + control}
                aria-label={`${layer} ${control} assignment`}
                value={draft[layer][control] || "none"}
                onChange={(e) => {
                  const value = e.target.value;
                  setDraft((old) => ({
                    ...old,
                    [layer]: { ...old[layer], [control]: value },
                  }));
                  setMessage("Unsaved changes");
                }}
              >
                {Object.entries(choices).map(([value, label]) => (
                  <option value={value} key={value}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
          );
        })}
      </div>
      <div className="mapper-actions">
        <button
          disabled={saving}
          className="active"
          onClick={async () => {
            setSaving(true);
            try {
              await invoke("save_mappings", { mappings: draft });
              setMessage("Mappings saved. Controller assignments are active.");
            } catch (e) {
              setMessage(String(e));
            } finally {
              setSaving(false);
            }
          }}
        >
          Save mappings
        </button>
        <button
          disabled={saving}
          onClick={async () => {
            if (
              !window.confirm(
                "Replace both layers with the default assignments? Save mappings to apply.",
              )
            )
              return;
            try {
              setDraft(await invoke<Mappings>("default_mappings"));
              setMessage("Defaults loaded. Save mappings to apply.");
            } catch (e) {
              setMessage(String(e));
            }
          }}
        >
          Restore defaults
        </button>
      </div>
      <small role="status">{message}</small>
    </section>
  );
}

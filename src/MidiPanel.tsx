import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./midi.css";

type Device = { id: string; name: string };
type Input = { timestamp: number; kind: "note" | "cc"; channel: number; number: number; value: number };
type Assignment = { id: string; kind: "note" | "cc"; mode: "button" | "absolute"; channel: number; number: number; action: string; deck: number; index: number | null };
type Status = {
  connected: boolean;
  device: Device | null;
  latest_input: Input | null;
  learned_event: Input | null;
  learning: boolean;
  mappings: Assignment[];
  error: string | null;
};
const absoluteActions = new Set(["volume", "cross", "filter", "wet", "master", "rate"]);
const globalActions = new Set(["cross", "master", "sample", "next_sample", "stop_samples", "browse_up", "browse_down"]);
const actions = [
  ["play", "Play / pause"],
  ["cue_hold", "Cue"],
  ["load_deck", "Load selected song"],
  ["set_cue", "Set cue point"],
  ["tempo_match", "Sync tempo"],
  ["volume", "Channel level"],
  ["cross", "Crossfader"],
  ["filter", "Filter"],
  ["wet", "Effect mix"],
  ["master", "Master level"],
  ["rate", "Tempo slider"],
  ["backspin", "Backspin"],
  ["brake", "Brake"],
  ["echo", "Echo"],
  ["reverb", "Reverb"],
  ["sample", "Sampler pad"],
  ["next_sample", "Select next sampler pad"],
  ["stop_samples", "Stop all samples"],
  ["browse_up", "Previous song"],
  ["browse_down", "Next song"],
] as const;
const quickSetup = [
  { action: "play", deck: 0, label: "Deck 1 Play" },
  { action: "cue_hold", deck: 0, label: "Deck 1 Cue" },
  { action: "volume", deck: 0, label: "Deck 1 level" },
  { action: "play", deck: 1, label: "Deck 2 Play" },
  { action: "cue_hold", deck: 1, label: "Deck 2 Cue" },
  { action: "volume", deck: 1, label: "Deck 2 level" },
  { action: "cross", deck: 0, label: "Crossfader" },
];

function describeMapping(mapping: Assignment) {
  const label = actions.find(([id]) => id === mapping.action)?.[1] ?? mapping.action;
  if (mapping.action === "sample") return mapping.index === null ? "Selected sampler pad" : `Sampler pad ${mapping.index + 1}`;
  return globalActions.has(mapping.action) ? label : `${label} · Deck ${mapping.deck + 1}`;
}

export function MidiPanel() {
  const [devices, setDevices] = useState<Device[]>([]);
  const [selected, setSelected] = useState("");
  const [status, setStatus] = useState<Status | null>(null);
  const [mappings, setMappings] = useState<Assignment[]>([]);
  const [action, setAction] = useState("play");
  const [deck, setDeck] = useState(0);
  const [pad, setPad] = useState(-1);
  const [learning, setLearning] = useState(false);
  const [setup, setSetup] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const learningRef = useRef(false);

  async function refresh() {
    try {
      const ports = await invoke<Device[]>("midi_devices");
      setDevices(ports);
      setSelected(old => ports.some(port => port.id === old) ? old : ports[0]?.id ?? "");
    } catch (error) { setMessage(String(error)); }
  }
  useEffect(() => { void refresh(); }, []);
  useEffect(() => {
    let mounted = true;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const next = await invoke<Status>("midi_status");
        if (mounted) {
          setStatus(next);
          setMappings(next.mappings);
          if (!next.connected) {
            learningRef.current = false;
            setLearning(false);
            setSetup(null);
          }
        }
      } catch (error) { if (mounted) setMessage(String(error)); }
      if (mounted) timer = setTimeout(poll, 150);
    }
    void poll();
    return () => {
      mounted = false;
      clearTimeout(timer);
      learningRef.current = false;
      void invoke("midi_learn", { armed: false }).catch(() => {});
    };
  }, []);

  async function connect() {
    setBusy(true);
    try {
      const next = await invoke<Status>("midi_connect", { deviceId: selected });
      setStatus(next);
      setMappings(next.mappings);
      setSetup(null);
      setMessage("Connected. Choose Auto Map to restore or create a mapping.");
    } catch (error) { setMessage(String(error)); }
    finally { setBusy(false); }
  }
  async function arm() {
    setBusy(true);
    try {
      const next = await invoke<Status>("midi_learn", { armed: true });
      setStatus(next);
      learningRef.current = true;
      setLearning(true);
      const requested = setup === null ? action : quickSetup[setup].action;
      setMessage(absoluteActions.has(requested) ? "Move the corresponding knob or fader." : "Press the corresponding controller button.");
    } catch (error) { setMessage(String(error)); }
    finally { setBusy(false); }
  }
  async function cancelLearning() {
    learningRef.current = false;
    setLearning(false);
    setSetup(null);
    await invoke("midi_learn", { armed: false }).catch(() => {});
  }
  useEffect(() => {
    const event = status?.learned_event;
    if (!event || !learningRef.current) return;
    learningRef.current = false;
    const step = setup === null ? null : quickSetup[setup];
    const existing = mappings.find(mapping => mapping.kind === event.kind && mapping.channel === event.channel && mapping.number === event.number);
    const assignment: Assignment = {
      id: existing?.id ?? `${event.kind}:${event.channel}:${event.number}`,
      kind: event.kind, channel: event.channel, number: event.number,
      action: step?.action ?? action, deck: step?.deck ?? deck,
      mode: absoluteActions.has(step?.action ?? action) ? "absolute" : "button",
      index: (step?.action ?? action) === "sample" && pad >= 0 ? pad : null,
    };
    setBusy(true);
    void invoke<Status>("midi_save_mapping", { mapping: assignment })
      .then(async next => {
        setMappings(next.mappings);
        await invoke("midi_learn", { armed: false });
        setMessage("Mapping saved.");
        setLearning(false);
        if (setup !== null && setup + 1 < quickSetup.length) setSetup(setup + 1);
        else setSetup(null);
      })
      .catch(error => {
        setMessage(String(error));
        setLearning(false);
        void invoke("midi_learn", { armed: false }).catch(() => {});
      })
      .finally(() => setBusy(false));
  }, [status?.learned_event, learning]);

  async function autoMap() {
    if (mappings.length > 0) {
      setMessage("Saved mapping is active. You can test the controls now.");
      return;
    }
    setSetup(0);
    setMessage("Map the core controls once. Your device mapping is saved automatically.");
  }
  const input = status?.latest_input;
  const liveMapping = input && mappings.find(mapping =>
    mapping.kind === input.kind && mapping.channel === input.channel && mapping.number === input.number
  );
  return (
    <div className="midi-panel">
      <div className="midi-connect">
        <label>Controller
          <select value={selected} onChange={event => setSelected(event.target.value)} disabled={busy || learning}>
            {devices.length === 0 && <option value="">No MIDI input detected</option>}
            {devices.map(device => <option key={device.id} value={device.id}>{device.name}</option>)}
          </select>
        </label>
        <button disabled={busy || learning} onClick={refresh}>Refresh</button>
        <button disabled={busy || learning || !selected} onClick={connect}>Connect</button>
      </div>
      <div className="midi-connection-status">
        <span>{status?.connected ? `Connected · ${status.device?.name ?? "MIDI controller"}` : "Disconnected"}</span>
        {status?.connected && <button disabled={busy} onClick={() => {
          void cancelLearning();
          void invoke("midi_disconnect").catch(error => setMessage(String(error)));
        }}>Disconnect</button>}
      </div>
      <p>Connect a USB MIDI controller, then restore a saved mapping or learn its controls.</p>
      <button disabled={!status?.connected || busy || learning} onClick={autoMap}>Auto Map</button>
      {status?.connected && <section className="midi-learn">
        <h4>{setup === null ? "MIDI Learn" : `Quick setup · ${setup + 1} of ${quickSetup.length}`}</h4>
        {setup === null ? <div className="midi-learn-controls">
          <label>Action<select value={action} disabled={learning || busy} onChange={event => setAction(event.target.value)}>
            {actions.map(([id, label]) => <option key={id} value={id}>{label}</option>)}
          </select></label>
          {!globalActions.has(action) && <label>Deck<select value={deck} disabled={learning || busy} onChange={event => setDeck(+event.target.value)}>
            <option value={0}>Deck 1</option><option value={1}>Deck 2</option>
          </select></label>}
          {action === "sample" && <label>Pad<select value={pad} disabled={learning || busy} onChange={event => setPad(+event.target.value)}>
            <option value={-1}>Selected pad</option>
            {Array.from({length:16}, (_, index) => <option key={index} value={index}>Pad {index + 1}</option>)}
          </select></label>}
        </div> : <p>{quickSetup[setup].label}</p>}
        <div className="settings-actions">
          <button disabled={busy || learning} onClick={arm}>{learning ? "Listening…" : "Learn control"}</button>
          {(learning || setup !== null) && <button onClick={() => void cancelLearning()}>Cancel</button>}
          {setup !== null && !learning && <button disabled={busy} onClick={() => setSetup(setup + 1 < quickSetup.length ? setup + 1 : null)}>Skip</button>}
        </div>
        <p className="midi-input">
          {input ? `${input.kind.toUpperCase()} · Channel ${input.channel + 1} · Control ${input.number} · Value ${input.value}` : "Waiting for controller input"}
          {liveMapping && <> · {describeMapping(liveMapping)}</>}
        </p>
      </section>}
      {mappings.length > 0 && <section className="midi-assignments">
        <h4>Saved controls</h4>
        {mappings.map(mapping => <div className="midi-assignment" key={`${mapping.kind}-${mapping.channel}-${mapping.number}`}>
          <span>{mapping.kind.toUpperCase()} {mapping.number} · Ch {mapping.channel + 1}</span>
          <span>{describeMapping(mapping)}</span>
          <button disabled={busy || learning} aria-label={`Remove ${mapping.action} mapping`} onClick={() => {
            setBusy(true);
            void invoke("midi_delete_mapping", { id: mapping.id })
              .catch(error => setMessage(String(error))).finally(() => setBusy(false));
          }}>Remove</button>
        </div>)}
      </section>}
      <p role="status">{status?.error || message}</p>
      <details><summary>Controller compatibility</summary>
        <p>MIDI controls need a mapping for their device. Saved mappings restore when you reconnect. HID-only controls, proprietary jog wheels, displays and controller lights require model-specific support.</p>
      </details>
    </div>
  );
}

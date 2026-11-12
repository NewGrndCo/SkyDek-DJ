import React, { useState, useEffect, useMemo, useRef } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import {
  Settings,
  Folder,
  Music,
  Search,
  Headphones,
  Gamepad2,
  Plus,
  Play,
  Pause,
  Square,
  Radio,
  Disc3,
  Volume2,
} from "lucide-react";
import "./style.css";
import "./premium.css";
import "./workspace.css";
import "./theme.css";
import "./feedback.css";
import { DeckGlow } from "./DeckGlow";
import { MidiPanel } from "./MidiPanel";
import { Wave } from "./Waveform";
import { MasterVisualizer } from "./MasterVisualizer";
import { Dialog } from "./Dialog";
import { usePanelLayout } from "./usePanelLayout";
import { HoldButton } from "./HoldButton";
import { type Preferences } from "./GeneralSettings";
import { FolderTree, inFolder } from "./FolderTree";
import { type Mappings } from "./MapperPanel";
import { SettingsPanel, type SettingsSkin } from "./SettingsPanel";
import { BrandMark } from "./BrandMark";
import { SoundBrowser, type SoundFile } from "./SoundBrowser";
type Track = {
  path: string;
  title: string;
  duration: number;
  waveform: number[];
  bpm: number | null;
  bpm_confidence: number;
  analysis_error: string | null;
  cues: (number | null)[];
};
type Deck = {
  end_events: number;
  previewing: boolean;
  position: number;
  duration: number;
  playing: boolean;
  looping: boolean;
  volume: number;
  filter: number;
  rate: number;
  cue: number;
  cue_held: boolean;
  cue_elapsed: number;
  effect: number;
  wet: number;
  hot: (number | null)[];
  peak: number;
};
type Snapshot = {
  auto_dj: boolean;
  auto_dj_status: string;
  analysis: {running:boolean; completed:number; total:number; failed:number; current:string; message:string};
  preferences: Preferences;
  sensitivity: number;
  library: Track[];
  queue: string[];
  loaded: (string | null)[];
  selected: number;
  target: number;
  shift: boolean;
  sampler: number;
  samples: (string | null)[];
  last: string;
  connected: boolean;
  battery: number | null;
  audio: {
    sampler_voices: number[];
    decks: Deck[];
    crossfader: number;
    master: number;
    sampler_volume: number;
    peak: number;
    clock: number;
  };
  error: string;
  device: string;
  sample_rate: number;
  tab: string;
  recording: boolean;
  recorded_seconds: number;
  recording_path: string;
  mappings: Mappings;
  sound_folder: string | null;
  sound_revision: number;
  music_roots: string[];
  library_folder: string;
};
const time = (s: number) =>
  `${Math.floor(s / 60)}:${Math.floor(s % 60)
    .toString()
    .padStart(2, "0")}`;
function App() {
  const panelLayout=usePanelLayout();
  const [folderFilter, setFolderFilter] = useState("");
  const [s, setS] = useState<Snapshot | null>(null),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [query, setQuery] = useState(""),
    [settings, setSettings] = useState(false),
    [bottom, setBottom] = useState("sampler"),
    [recordPending, setRecordPending] = useState(false);
  const [waveSpan,setWaveSpan] = useState(16);
  const [skin, setSkin] = useState<SettingsSkin>(() => {
    const saved = localStorage.getItem("skydek-theme");
    return saved === "light" || saved === "xbox" || saved === "matrix" ? saved : "dark";
  });
  const [reducedMotion, setReducedMotion] = useState(() => {
    const saved = localStorage.getItem("skydek-reduced-motion");
    return saved === null ? window.matchMedia("(prefers-reduced-motion: reduce)").matches : saved === "true";
  });
  useEffect(() => {
    localStorage.setItem("skydek-reduced-motion", String(reducedMotion));
  }, [reducedMotion]);
  const [recordFormat,setRecordFormat] = useState<"wav"|"mp3">("mp3");
  const [recordBusy,setRecordBusy] = useState(false);
  useEffect(() => {
    if (!recordPending || s?.recording) return;
    if ((s?.audio.peak ?? 0) < 0.005) return;
    setRecordPending(false);
    setRecordBusy(true);
    void invoke("record", { start: true, format: recordFormat })
      .catch(e => setError(String(e)))
      .finally(() => setRecordBusy(false));
  }, [recordPending, s?.audio.peak, s?.recording, recordFormat]);
  useEffect(() => {
    localStorage.setItem("skydek-theme", skin);
    document.documentElement.style.colorScheme = skin === "dark" ? "dark" : "light";
  }, [skin]);
  const [samplerEdit, setSamplerEdit] = useState(false);
  const [samplerBank, setSamplerBank] = useState(0);
  const heldPads = useRef<boolean[]>(Array(48).fill(false));
  async function changeSamplerMode(mode: Preferences["sampler_modes"][number], all = false) {
    try { await invoke("sampler_mode", { index: s?.sampler || 0, mode, all }); setError(""); }
    catch(e) { setError(String(e)); }
  }
  useEffect(() => {
    if (!s) return;
    const timer = setTimeout(() => {
      void invoke("library_view", { folder: folderFilter, query }).catch((e) =>
        setError(String(e)),
      );
    }, 150);
    return () => clearTimeout(timer);
  }, [folderFilter, query]);
  useEffect(() => {
    for (const [selector, parent] of [
      ['[data-selected-track="true"]', ".track-table"],
      ['[data-selected-pad="true"]', ".sideview"],
      [".folders .selected-folder", ".folder-tree"],
    ]) {
      const el = document.querySelector(selector),
        container = el?.closest(parent);
      if (el && container) {
        const a = el.getBoundingClientRect(),
          b = container.getBoundingClientRect();
        if (a.top < b.top) container.scrollTop += a.top - b.top;
        else if (a.bottom > b.bottom)
          container.scrollTop += a.bottom - b.bottom;
      }
    }
  }, [s?.selected, s?.sampler, s?.library_folder]);
  const [assigning, setAssigning] = useState<number | null>(null);
  useEffect(() => {
    if (s) setFolderFilter(s.library_folder);
  }, [s?.library_folder]);
  const [sounds, setSounds] = useState<SoundFile[]>([]);
  useEffect(() => {
    let active = true;
    invoke<SoundFile[]>("sound_files")
      .then((files) => {
        if (active) setSounds(files);
      })
      .catch((e) => {
        if (active) setError(String(e));
      });
    return () => {
      active = false;
    };
  }, [s?.sound_revision]);
  useEffect(() => {
    if (s?.tab) setBottom(s.tab === "queue" ? "queue" : "sampler");
  }, [s?.tab]);
  const targetRef = useRef(0),
    keyboardCue = useRef<number | null>(null);
  const lastScrubbedDeck = useRef<number | null>(null);
  targetRef.current = s?.target || 0;
  async function assign(index: number, path?: string) {
    if (
      s?.samples[index] &&
      s.samples[index] !== path &&
      !window.confirm(`Replace the sound on Pad ${index + 1}?`)
    )
      return false;
    setAssigning(index);
    try {
      await invoke("assign_sample", { index, path });
      setError("");
      return true;
    } catch (e) {
      setError(String(e));
      throw e;
    } finally {
      setAssigning(null);
    }
  }
  const cmd = async (
    action: string,
    deck?: number,
    value?: number,
    index?: number,
  ) => {
    try {
      await invoke("command", { action, deck, value, index });
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };
  useEffect(() => {
    let active = true,
      inflight = false;
    let libraryRevision: number | null = null;
    let library: Snapshot["library"] = [];
    let musicRoots: string[] = [];
    const poll = async () => {
      if (inflight) return;
      inflight = true;
      try {
        const v = await invoke<Omit<Snapshot, "library"> & {
          library: Snapshot["library"] | null;
          library_revision: number;
        }>("snapshot", { libraryRevision });
        if (Array.isArray(v.library)) library = v.library;
        if (v.music_roots.length !== musicRoots.length ||
            v.music_roots.some((root, index) => root !== musicRoots[index])) {
          musicRoots = v.music_roots;
        }
        libraryRevision = v.library_revision;
        if (active) setS({ ...v, library, music_roots: musicRoots });
      } catch (e) {
        if (active) setError(String(e));
      } finally {
        inflight = false;
      }
    };
    void poll();
    const t = setInterval(poll, 75);
    return () => {
      active = false;
      clearInterval(t);
    };
  }, []);
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if(e.code === "F11") { e.preventDefault(); void invoke("toggle_fullscreen").catch(e => setError(String(e))); return; }
      if (
        e.defaultPrevented ||
        (e.target as HTMLElement).closest("button,[role=dialog]") ||
        (e.target as HTMLElement).matches("input,select,textarea") ||
        e.repeat
      )
        return;
      if (e.code === "Space") {
        e.preventDefault();
        void cmd("play");
      }
      if (e.code === "Digit1") void cmd("target", 0);
      if (e.code === "Digit2") void cmd("target", 1);
      if ((e.code === "KeyC" || e.code === "KeyQ")) {
        keyboardCue.current = lastScrubbedDeck.current ?? targetRef.current;
        lastScrubbedDeck.current = null;
        void cmd("cue_hold_down", keyboardCue.current);
      }
    };
    const up = (e: KeyboardEvent) => {
      if ((e.code === "KeyC" || e.code === "KeyQ") && keyboardCue.current !== null) {
        void cmd("cue_hold_up", keyboardCue.current);
        keyboardCue.current = null;
      }
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    const cancelCue = () => {
      if (keyboardCue.current !== null) {
        void cmd("cue_hold_cancel", keyboardCue.current);
        keyboardCue.current = null;
      }
    };
    window.addEventListener("blur", cancelCue);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", cancelCue);
    };
  }, []);
  const load = async (path: string, deck: number) => {
    let force = false;
    if (s?.audio.decks[deck].playing) {
      force = window.confirm(`Deck ${deck + 1} is playing. Replace its track?`);
      if (!force) return;
    }
    setBusy(true);
    try {
      await invoke("load_track", { path, deck, force });
      setError("");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const imp = async (folder = false) => {
    setBusy(true);
    try {
      await invoke("import_audio", { folder });
      setError("");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const library = s?.library;
  const libraryPaths = useMemo(() => library?.map(track => track.path) ?? [], [library]);
  const visibleTracks = useMemo(() => {
    const search = query.toLowerCase();
    return (library ?? []).flatMap((track, index) =>
      inFolder(track.path, folderFilter) && track.title.toLowerCase().includes(search)
        ? [{ track, index }]
        : [],
    );
  }, [library, folderFilter, query]);
  const seekWave = (deck: number, seconds: number, dragged?: boolean) => {
    lastScrubbedDeck.current = dragged ? deck : null;
    return cmd(dragged ? "scrub_end" : "seek", deck, seconds);
  };
  const trackFor = (d: number) =>
    s?.library.find((t) => t.path === s.loaded[d]);
  const renderDeck = (d: number) => {
    const a = s?.audio.decks[d],
      t = trackFor(d);
    return (
      <section className={`deck deck${d + 1} panel`} key={d}>
        <div className="deck-title">
          <button className="deck-tag" onClick={() => cmd("target", d)}>
            DEK {d + 1}
          </button>
          <div className="track-name">
            <strong>{t?.title || "Load a track"}</strong>
            <small>
              {t
                ? "Local audio • " + time(a?.duration || 0)
                : "WAV / MP3 • import your music"}
            </small>
          </div>
          <div className="tempo">
            <b>{t?.bpm ? (t.bpm * (a?.rate || 1)).toFixed(2) : "—"}</b>
            <small>{t?.bpm ? (t.bpm_confidence > 0 ? "BPM · estimated" : "BPM") : "Awaiting analysis"}</small>
          </div>
        </div>
        <div className="overview">
          <Wave skin={skin} track={t} deck={a} overview onSeek={(v, dragged) => seekWave(d, v, dragged)} />
        </div>
        <div className="deck-body">
          <div className="fx">
            <span className="caption">AUDIO FX</span>
            {["backspin", "brake", "start", "echo", "reverb"].map((f, effect) => (
              <HoldButton disabled={!t} key={f} className={a?.effect === effect + 1 ? "active" : ""} aria-pressed={a?.effect === effect + 1} onPress={() => void cmd("fx_down", d, undefined, effect + 1)} onRelease={() => void cmd("fx_up", d, undefined, effect + 1)}>
                {f === "start" ? "BRAKE START" : f.toUpperCase()}
              </HoldButton>
            ))}
            <label className="filter-control">
              FILTER
              <input
                aria-label={`Deck ${d + 1} filter`}
                type="range"
                min="-1"
                max="1"
                step="0.01"
                value={a?.filter || 0}
                onChange={(e) => cmd("filter", d, +e.target.value)}
              />
              <button aria-label={`Reset Deck ${d+1} filter`} title="Reset filter" onClick={() => cmd("filter", d, 0)}>↺</button>
            </label>
            <label className="effect-mix" title="Reverb/echo wet mix and backspin intensity. Also controlled by the mapped effect axis.">FX MIX
              <input aria-label={`Deck ${d+1} effect mix`} type="range" min="0" max="1" step="0.01" value={a?.wet ?? .4} onChange={e=>cmd("wet",d,+e.target.value)} />
            </label>
          </div>
          <div className="platter-area">
            <div className={"platter " + (a?.playing ? "spinning" : "")}>
              <div
                className="platter-ring"
                style={{ transform: `rotate(${(a?.position || 0) * 120}deg)` }}
              />
              <div className="platter-center"><BrandMark /></div>
            </div>
            <div className="times">
              <span>{time(a?.position || 0)}</span>
              <small>
                REMAIN −
                {time(Math.max(0, (a?.duration || 0) - (a?.position || 0)))}
              </small>
            </div>
          </div>
          <div className="pitch">
            <label>
              TEMPO
              <input
                className="tempo-slider"
                aria-label={`Deck ${d + 1} tempo`}
                aria-orientation="vertical"
                type="range"
                min="0.5"
                max="1.5"
                step="0.001"
                value={a?.rate || 1}
                onChange={(e) => cmd("rate", d, +e.target.value)}
              />
            </label>
            <small>{(((a?.rate || 1) - 1) * 100).toFixed(1)}%</small>
            <button onClick={() => cmd("rate", d, 1)}>RESET</button>
            <label>
              LEVEL
              <input
                aria-label={`Deck ${d + 1} volume`}
                className="level-slider"
                aria-orientation="vertical"
                type="range"
                min="0"
                max="1"
                step="0.01"
                value={a?.volume ?? 0.8}
                onChange={(e) => cmd("volume", d, +e.target.value)}
              />
            </label>
          </div>
        </div>
        <div className="transport">
          <button
            disabled={!t}
            onPointerDown={(e) => {
              e.currentTarget.setPointerCapture(e.pointerId);
              void cmd("cue_hold_down", d);
            }}
            onPointerUp={() => cmd("cue_hold_up", d)}
            onPointerCancel={() => cmd("cue_hold_cancel", d)}
            title={`Drag the waveform, then press Cue to save a point (up to three). Tap to return to the last cue. Hold ${s?.preferences.cue_seconds ?? 3} seconds, then release to keep playing.`}
          >
            CUE
          </button>
          <button
            disabled={!t}
            className={"play " + (a?.playing ? "active" : "")}
            aria-label={`${a?.playing ? "Pause" : "Play"} Deck ${d + 1}`}
            onClick={() => cmd("play", d)}
          >
            {a?.playing ? <Pause size={18} /> : <Play size={18} />}
          </button>
          <button
            disabled={!t?.bpm || !trackFor(1-d)?.bpm}
            onClick={() => cmd("tempo_match", d)}
            title="Sync this deck’s tempo to the other deck’s current BPM. Analyze both songs first. Tempo matching changes pitch; beat-phase alignment is not included."
          >
            SYNC
          </button>

        </div>
        <div className="hotcues">
          <button
            disabled={!t?.bpm}
            aria-pressed={a?.looping ?? false}
            className={a?.looping ? "active" : undefined}
            onClick={() => cmd("loop", d, 4)}
            title="Toggle a four-beat loop from the current position"
          >
            {a?.looping ? "EXIT LOOP" : "LOOP 4"}
          </button>
          <span className="caption">HOT CUES</span>
          {Array.from({ length: 3 }, (_, i) => (
            <button
              disabled={!t}
              className={a?.hot[i] != null ? "set" : ""}
              title="Click: set/trigger • Shift-click: clear"
              key={i}
              onClick={(e) => {
                if (e.shiftKey && !window.confirm("Clear this hot cue?"))
                  return;
                void cmd(e.shiftKey ? "clear_hot" : "hot", d, undefined, i);
              }}
            >
              {i + 1}
            </button>
          ))}
        </div>
        <div className="cue-hold-status" role="status" aria-live="polite">
          {a?.cue_held ? (a.cue_elapsed >= (s?.preferences.cue_seconds ?? 3) ? "Release to keep playing" : `Cue preview · ${((s?.preferences.cue_seconds ?? 3)-a.cue_elapsed).toFixed(1)}s to play on release`) : "\u00a0"}
        </div>
        <DeckGlow deck={a} index={d} path={t?.path} crossfader={s?.audio.crossfader ?? 0} reducedMotion={reducedMotion} />
      </section>
    );
  };
  return (
    <main style={panelLayout.style} data-theme={skin === "dark" ? "dark" : "light"} data-skin={skin} data-motion={reducedMotion ? "reduced" : "full"}>
      <header>
        <img className="brand-logo" src="/brand.png" alt="SkyDek DJ" />
        <span className="divider" />
        <span className={"connection " + (s?.connected ? "online" : "")}>
          <span className="connection-pulse" aria-hidden="true" />
          {s?.connected ? "Xbox controller" : "Controller disconnected"}
        </span>
        <div className="header-right">
          <button
            aria-pressed={s?.auto_dj ?? false}
            className={s?.auto_dj ? "active" : undefined}
            title={s?.auto_dj_status || "Mix tracks from the playing track's folder"}
            onClick={() => cmd("auto_dj", undefined, s?.auto_dj ? 0 : 1)}
          >
            Auto DJ
          </button>
          <button className="update-header-button" title="Check for application updates" onClick={() => { setSettings(true); setTimeout(() => window.dispatchEvent(new Event("skydek-check-updates")), 0); }}>Check for updates</button>
          <button className="analysis-progress" title={s?.analysis.running ? s.analysis.current : (s?.analysis.message || "Analyze duration and estimate BPM for your library")} onClick={() => invoke("analyze_library", {force:false, cancel:s?.analysis.running || false}).catch(e=>setError(String(e)))}>{s?.analysis.running ? `Analyzing ${s.analysis.completed}/${s.analysis.total} · Pause` : "Analyze library"}</button>
          <label className="skin-picker">
            Skin
            <select value={skin} onChange={event => setSkin(event.currentTarget.value as SettingsSkin)}>
              <option value="dark">Dark</option>
              <option value="light">Light</option>
              <option value="xbox">Xbox</option>
              <option value="matrix">Matrix</option>
            </select>
          </label>
          <button aria-label="Toggle fullscreen" title="Fullscreen (F11)" onClick={() => invoke("toggle_fullscreen").catch(e => setError(String(e)))}>⛶</button>
          <button onClick={() => cmd("shift")}>
            {s?.shift ? "SHIFT" : "NORMAL"}
          </button>
          <button
            onClick={() => setSettings(true)}
            aria-label="Application settings"
          >
            <Settings size={18} />
          </button>
        </div>
      </header>
      <div className="stacked">
        <div>
          <Wave skin={skin} track={trackFor(0)} deck={s?.audio.decks[0]} span={waveSpan} onSpan={setWaveSpan} onSeek={(v, dragged) => seekWave(0, v, dragged)} />
        </div>
        <div className="redwave">
          <Wave skin={skin} track={trackFor(1)} deck={s?.audio.decks[1]} span={waveSpan} onSpan={setWaveSpan} onSeek={(v, dragged) => seekWave(1, v, dragged)} />
        </div>
        <div className="centerline" />
        {!s?.loaded.some(Boolean) && (
          <span>Import audio to display real waveforms</span>
        )}
      </div>
      <div className="performance">
        {renderDeck(0)}
        <section className="mixer panel">
          <h2>MASTER</h2>
          <Volume2 />
          <label>
            MASTER LEVEL
            <input
              type="range"
              min="0"
              max="1"
              step="0.01"
              value={s?.audio.master ?? 0.8}
              onChange={(e) => cmd("master", undefined, +e.target.value)}
            />
          </label>
          <label>
            SAMPLER LEVEL
            <input type="range" min="0" max="1" step="0.01" value={s?.preferences.sampler_volume ?? 0.8} onChange={(e) => cmd("sampler_volume", undefined, +e.target.value)} />
          </label>
          <MasterVisualizer peak={s?.audio.peak ?? 0} />
          <small>
            {(s?.audio.peak || 0) > 1 ? "CLIPPING" : "MASTER OUTPUT"}
          </small>
          <div className="targets">
            <span className="caption">TARGET DECK</span>
            <div>
              {[0, 1].map((i) => (
                <button
                  className={s?.target === i ? "active" : ""}
                  onClick={() => cmd("target", i)}
                  key={i}
                >
                  {i + 1}
                </button>
              ))}
            </div>
            <button onClick={() => cmd("auto_target")}>AUTO TARGET</button>
          </div>
          <label className="record-format">Recording format
            <select aria-label="Recording format" value={recordFormat} disabled={s?.recording || recordBusy} onChange={e=>setRecordFormat(e.target.value as "wav"|"mp3")}><option value="mp3">MP3 · 320 kbps</option><option value="wav">WAV · lossless</option></select>
          </label>
          <button
            className={s?.recording ? "active" : ""}
            disabled={recordBusy}
            onClick={async () => {
              setRecordBusy(true);
              try {
                if (!s?.recording && (s?.audio.peak ?? 0) < 0.005) {
                  setRecordPending(true);
                  setError("Waiting for audio input…");
                } else {
                  await invoke("record", { start: !s?.recording, format:recordFormat });
                }
                setError("");
              } catch (e) {
                setError(String(e));
              } finally { setRecordBusy(false); }
            }}
          >
            <Radio size={13} /> {recordBusy ? "Please wait…" : recordPending ? "WAITING FOR AUDIO" : s?.recording ? "STOP REC" : `RECORD ${recordFormat.toUpperCase()}`}
          </button>
          <small>
            {s?.recording ? time(s.recorded_seconds) : "Post-mixer master"}
          </small>
          <span className="caption">CROSSFADER</span>
          <input
            aria-label="Crossfader"
            type="range"
            min="-1"
            max="1"
            step="0.01"
            value={s?.audio.crossfader || 0}
            onChange={(e) => cmd("cross", undefined, +e.target.value)}
          />
          <div className="cross-label">
            <span>1</span>
            <span>2</span>
          </div>
          <select
            aria-label="Crossfader curve"
            onChange={(e) => cmd("curve", undefined, +e.target.value)}
          >
            <option value="0">Equal power</option>
            <option value="1">Linear</option>
          </select>
        </section>
        {renderDeck(1)}
      </div>
      <div className="row-splitter" {...panelLayout.separator("height")} />
      <div className="browser">
        <aside className={`folders panel ${s?.tab === "folders" ? "focused-panel" : ""}`}>
          <h2>
            <Folder size={15} /> BROWSER
          </h2>
          <button className="active" onClick={() => setBottom("sampler")}>
            <Music size={15} /> Local library
          </button>
          <button disabled={busy} onClick={() => imp(false)}>
            <Plus size={15} /> Import tracks
          </button>
          <button disabled={busy} onClick={() => imp(true)}>
            <Folder size={15} /> Import folder
          </button>
          <button onClick={() => setBottom("queue")}>
            Sidelist ({s?.queue.length || 0})
          </button>
          <FolderTree
            paths={libraryPaths}
            roots={s?.music_roots || []}
            selected={folderFilter}
            onSelect={p => { setFolderFilter(p); void cmd("focus_folders"); }}
          />
          <details className="guide">
            <summary>Controller quick guide</summary>
            <p>
              D-pad: browse · sampler uses 4×4 grid
              <br />
              A: load preparation deck
              <br />
              B: cue • hold {s?.preferences.cue_seconds ?? 3}s to play on release
              <br />
              Y: add to Sidelist
              <br />
              X: load next Sidelist
              <br />
              LB / RB: spin / brake
              <br />
              View: Shift layer
              <br />
              Menu: target deck
            </p>
            <small>
              Share is reserved by Windows.
              <br />
              Space: play/pause • Q: cue
            </small>
          </details>
        </aside>
        <div className="column-splitter folder-splitter" {...panelLayout.separator("left")} />
        <section
          onPointerDown={() => { if(s?.tab !== "library") void cmd("focus_library"); }}
          className={`library panel ${s?.tab === "library" ? "focused-panel" : ""}`}
        >
          <div className="library-location" title={folderFilter}>
            {folderFilter ? "Library / " + folderFilter.replace(/\\/g, "/").split("/").pop() : "Library / All tracks"}
          </div>
          <div className="search">
            <Search size={17} />
            <input
              placeholder="Search your library…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <span>{s?.library.length || 0} tracks</span>
          </div>
          <div className="track-table">
            <table>
              <thead>
                <tr>
                  <th>Title</th>
                  <th>BPM</th>
                  <th>Time</th>
                  <th>Load</th>
                  <th>Queue</th>
                </tr>
              </thead>
              <tbody>
                {s && visibleTracks.map(({ track: t, index: i }) => {
                    return (
                      <tr
                        key={t.path}
                        className={s.selected === i ? "selected" : ""}
                        data-selected-track={s.selected === i}
                        onClick={() => cmd("select", undefined, undefined, i)}
                        draggable
                        onDragStart={(e) =>
                          e.dataTransfer.setData("text/plain", t.path)
                        }
                      >
                        <td>
                          <Music size={13} /> {t.title}
                        </td>
                        <td title={t.analysis_error || (t.bpm_confidence > 0 ? "Estimated tempo; half/double BPM is possible" : "No reliable BPM detected yet")}>{t.bpm ? (t.bpm_confidence > 0 ? "~" : "") + t.bpm.toFixed(1) : "—"}</td>
                        <td>{t.duration ? time(t.duration) : "—"}</td>
                        <td>
                          <button
                            disabled={busy}
                            onClick={(e) => {
                              e.stopPropagation();
                              void load(t.path, 0);
                            }}
                          >
                            1
                          </button>
                          <button
                            disabled={busy}
                            onClick={(e) => {
                              e.stopPropagation();
                              void load(t.path, 1);
                            }}
                          >
                            2
                          </button>
                        </td>
                        <td>
                          <button
                            onClick={async (e) => {
                              e.stopPropagation();
                              await cmd("select", undefined, undefined, i);
                              await cmd("queue_add");
                            }}
                          >
                            +
                          </button>
                        </td>
                      </tr>
                    );
                  })}
              </tbody>
            </table>
            {Boolean(s?.library.length) &&
              !s?.library.some(
                (t) =>
                  inFolder(t.path, folderFilter) &&
                  t.title.toLowerCase().includes(query.toLowerCase()),
              ) && (
                <div className="empty">
                  No tracks directly in this folder. Select a subfolder on the
                  left or clear your search.
                </div>
              )}
            {!s?.library.length && (
              <div className="empty">
                <Disc3 size={36} />
                <b>Your music starts here</b>
                <p>
                  Import WAV or MP3 files to begin.
                  <br />
                  Select a track, then load Deck 1 or Deck 2.
                </p>
                <button onClick={() => imp(false)}>Import audio</button>
              </div>
            )}
          </div>
        </section>
        <div className="column-splitter sampler-splitter" {...panelLayout.separator("right")} />
        <section
          className={`sideview panel ${s?.tab === "sampler" ? "focused-panel" : ""}`}
          onPointerDown={() => {
            if (s?.tab !== "sampler") void cmd("focus_sampler");
          }}
        >
          <div className="tabs">
            <button
              className={bottom === "sampler" ? "active" : ""}
              onClick={() => setBottom("sampler")}
            >
              SAMPLER
            </button>
            <button
              className={bottom === "queue" ? "active" : ""}
              onClick={() => setBottom("queue")}
            >
              SIDELIST
            </button>
          </div>
          {bottom === "sampler" ? (
            <>
              <div className="sampler-banks" role="tablist" aria-label="Sampler banks">
                {["A", "B", "C"].map((name, i) => <button key={name} className={samplerBank === i ? "active" : ""} onClick={() => setSamplerBank(i)}>{`BANK ${name}`}</button>)}
              </div>
              {<div className="pads">
                {Array.from({ length: 16 }, (_, i) => { const slot = samplerBank * 16 + i; return (
                  <div
                    className={s?.sampler === slot ? "pad selected" : "pad"}
                    key={i}
                    data-selected-pad={s?.sampler === slot}
                    onPointerDown={() => void cmd("sample_select", undefined, undefined, slot)}
                    title={s?.samples[slot]?.replace(/\\/g, "/").split("/").pop() || `Pad ${i + 1} — select then open Sampler settings`}
                    draggable={Boolean(s?.samples[slot])}
                    onDragStart={(e) => {
                      e.dataTransfer.setData(
                        "application/x-skydek-pad",
                        String(slot),
                      );
                      e.dataTransfer.effectAllowed = "move";
                    }}
                    onDragOver={(e) => {
                      e.preventDefault();
                      e.dataTransfer.dropEffect = e.dataTransfer.types.includes(
                        "application/x-skydek-pad",
                      )
                        ? "move"
                        : "copy";
                    }}
                    onDrop={(e) => {
                      e.preventDefault();
                      const source = e.dataTransfer.getData(
                        "application/x-skydek-pad",
                      );
                      if (source !== "") {
                        void invoke("swap_samples", {
                          from: Number(source),
                          to: slot,
                        }).catch((e) => setError(String(e)));
                        return;
                      }
                      const path =
                        e.dataTransfer.getData("application/x-skydek-sound") ||
                        e.dataTransfer.getData("text/plain");
                      if (path) void assign(slot, path).catch(() => {});
                    }}
                  >
                    <HoldButton
                      onFocus={() =>
                        cmd("sample_select", undefined, undefined, slot)
                      }
                      className={(s?.audio.sampler_voices[slot] || 0) > 0 ? "active" : ""}
                      aria-pressed={(s?.audio.sampler_voices[slot] || 0) > 0}
                      onPress={() => { heldPads.current[slot] = s?.preferences.sampler_modes[slot] === "hold"; void cmd("sample", undefined, undefined, slot); }}
                      onRelease={() => { if (heldPads.current[slot]) { heldPads.current[slot] = false; void cmd("sample_stop", undefined, undefined, slot); } }}
                      disabled={!s?.samples[slot]}
                    >
                      <Music size={17} />
                      <span>
                        {s?.samples[slot]?.replace(/\\/g, "/").split("/").pop() || `Pad ${i + 1}`}
                      </span>
                    </HoldButton>
                  </div>
                ); })}
              </div>}
              <div className="sampler-tools"><button className={samplerEdit ? "active" : ""} onClick={() => setSamplerEdit(v => !v)}>Sampler settings</button></div>
              <button onClick={() => cmd("stop_samples")}>
                <Square size={13} /> Stop all samples
              </button>
            </>
          ) : (
            <div className="queue">
              {s?.queue.map((p, i) => (
                <div key={i}>
                  <span>{s.library.find((t) => t.path === p)?.title || p}</span>
                  <button
                    onClick={() => {
                      if (window.confirm("Remove this Sidelist entry?"))
                        void cmd("queue_remove", undefined, undefined, i);
                    }}
                  >
                    ×
                  </button>
                </div>
              ))}
              {!s?.queue.length && (
                <p>Highlight a track and press Y or click +.</p>
              )}
              <button onClick={() => cmd("load_next")}>
                Load next → preparation deck
              </button>
            </div>
          )}
        </section>
      </div>
      <footer>
        <span className="dot" />
        {busy
          ? "Loading audio…"
          : error || s?.error || (s?.auto_dj && s.auto_dj_status) || s?.last || "Connecting to native engine…"}
        <span>
          {s?.device || "No audio device"} • {s?.sample_rate || 0} Hz
        </span>
      </footer>
      {samplerEdit && (
        <Dialog title="Sampler settings" onClose={() => setSamplerEdit(false)}>
          <div className="sampler-editor-content">
            <label>
              Pad
              <select value={s?.sampler ?? 0} onChange={e => cmd("sample_select", undefined, undefined, +e.target.value)}>
                {Array.from({ length: 48 }, (_, i) => {
                  const file = s?.samples[i]?.replace(/\\/g, "/").split("/").pop();
                  return <option key={i} value={i}>Pad {i + 1} · {file || "Empty"}</option>;
                })}
              </select>
            </label>
            <label>
              Playback
              <select
                value={s?.preferences.sampler_modes[s?.sampler ?? 0] ?? "restart"}
                onChange={e => void changeSamplerMode(e.target.value as Preferences["sampler_modes"][number])}
              >
                <option value="restart">Restart on press</option>
                <option value="toggle">Toggle play / stop</option>
                <option value="overlap">Layer hits</option>
                <option value="hold">Play while held</option>
              </select>
            </label>
            <div className="sampler-assignment-actions">
              <button disabled={assigning !== null} onClick={() => assign(s?.sampler ?? 0).catch(() => {})}>
                Choose sound…
              </button>
              <button disabled={assigning !== null || !s?.samples[s.sampler]} onClick={() => invoke("clear_sample", { index: s?.sampler ?? 0 }).catch(e => setError(String(e)))}>
                Clear pad
              </button>
            </div>
            <details className="sampler-more">
              <summary>Browse folder and more options</summary>
              <SoundBrowser
                folder={s?.sound_folder ?? null}
                sounds={sounds}
                onAssign={async path => {
                  if (!(await assign(s?.sampler ?? 0, path))) throw new Error("Assignment cancelled");
                }}
                onAdd={path => invoke<number>("add_sample", { path })}
              />
              <button onClick={() => void changeSamplerMode(s?.preferences.sampler_modes[s?.sampler ?? 0] ?? "restart", true)}>
                Apply playback mode to all pads
              </button>
            </details>
          </div>
        </Dialog>
      )}
      {settings && s && (
        <SettingsPanel
          preferences={s.preferences}
          sensitivity={s.sensitivity}
          device={s.device}
          rate={s.sample_rate}
          command={cmd}
          resetLayout={panelLayout.reset}
          mappings={s.mappings}
          skin={skin}
          onSkinChange={setSkin}
          reducedMotion={reducedMotion}
          onReducedMotionChange={setReducedMotion}
          onClose={() => setSettings(false)}
          midiContent={<MidiPanel />}
        />
      )}

    </main>
  );
}
createRoot(document.getElementById("root")!).render(<App />);

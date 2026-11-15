import { useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import {
  duration,
  importLegacy,
  parseSession,
  reconstructed,
  timelineSeconds,
  visibleTimelineItems,
  type Edit,
  type MixSession,
  type Trigger,
} from "./mixEditorModel";
import "./mix-replay.css";

const fmt = (seconds: number) =>
  `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;

type Lane = {
  key: Edit["lane"];
  label: string;
  group: "DEK 1" | "DEK 2" | "MIXER";
  min: number;
  max: number;
  initial: number;
  step?: number;
};
const lanes: Lane[] = [
  { key: "volume_0", label: "Level", group: "DEK 1", min: 0, max: 1, initial: 0.8 },
  { key: "rate_0", label: "Tempo", group: "DEK 1", min: 0.5, max: 2, initial: 1 },
  { key: "filter_0", label: "Filter", group: "DEK 1", min: -1, max: 1, initial: 0 },
  { key: "wet_0", label: "FX mix", group: "DEK 1", min: 0, max: 1, initial: 0.35 },
  { key: "effect_0", label: "FX select", group: "DEK 1", min: 0, max: 5, initial: 1, step: 1 },
  { key: "volume_1", label: "Level", group: "DEK 2", min: 0, max: 1, initial: 0.8 },
  { key: "rate_1", label: "Tempo", group: "DEK 2", min: 0.5, max: 2, initial: 1 },
  { key: "filter_1", label: "Filter", group: "DEK 2", min: -1, max: 1, initial: 0 },
  { key: "wet_1", label: "FX mix", group: "DEK 2", min: 0, max: 1, initial: 0.35 },
  { key: "effect_1", label: "FX select", group: "DEK 2", min: 0, max: 5, initial: 1, step: 1 },
  { key: "crossfader", label: "Crossfader", group: "MIXER", min: -1, max: 1, initial: 0 },
  { key: "master", label: "Master", group: "MIXER", min: 0, max: 1, initial: 0.8 },
  { key: "sampler_volume", label: "Sampler", group: "MIXER", min: 0, max: 1, initial: 0.8 },
];

export type OpenedMixSession = { path: string; text: string };
type Source = { original: string; resolved: string | null; kind: "deck" | "sample" | "recording" };
type Clip = { deck: number; path: string; start: number; end: number };

function deckSegments(session: MixSession, deck: number): Clip[] {
  const frames = session.source.log.frames;
  const end = duration(session);
  const clips: Clip[] = [];
  for (let index = 0; index < frames.length; index += 1) {
    const path = frames[index].decks[deck].path;
    if (!path) continue;
    const next = index + 1 < frames.length ? frames[index + 1].seconds : end;
    const previous = clips.at(-1);
    if (previous?.path === path) previous.end = next;
    else clips.push({ deck, path, start: frames[index].seconds, end: next });
  }
  return clips;
}

export function MixEditor({ onClose, initial }: { onClose: () => void; initial?: OpenedMixSession | null }) {
  const [session, setSession] = useState<MixSession | null>(null);
  const [history, setHistory] = useState<MixSession[]>([]);
  const [future, setFuture] = useState<MixSession[]>([]);
  const [position, setPosition] = useState(0);
  const [zoom, setZoom] = useState(2);
  const [viewport, setViewport] = useState({ start: 0, end: 0 });
  const [selectedEdit, setSelectedEdit] = useState<string | null>(null);
  const [selectedTrigger, setSelectedTrigger] = useState<string | null>(null);
  const [selectedClip, setSelectedClip] = useState<Clip | null>(null);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [recordingUrl, setRecordingUrl] = useState<string | null>(null);
  const [sessionPath, setSessionPath] = useState<string | null>(null);
  const [sourcePath, setSourcePath] = useState<string | null>(null);
  const [saved, setSaved] = useState(true);
  const [busy, setBusy] = useState(false);
  const [sources, setSources] = useState<Source[]>([]);
  const [progress, setProgress] = useState({ running: false, done: 0, total: 0 });
  const [autoplayPreview, setAutoplayPreview] = useState(false);
  const audio = useRef<HTMLAudioElement>(null);
  const editorDialog = useRef<HTMLElement>(null);
  const timelineScroll = useRef<HTMLDivElement>(null);
  const openedInitial = useRef("");
  const total = session ? duration(session) : 0;
  const frame = useMemo(() => (session ? reconstructed(session, position) : null), [session, position]);
  const clips = useMemo(() => (session ? [deckSegments(session, 0), deckSegments(session, 1)] : [[], []]), [session]);
  const edit = session?.edits.find((item) => item.id === selectedEdit);
  const trigger = session?.triggers.find((item) => item.id === selectedTrigger);
  const updateViewport = () => {
    const element = timelineScroll.current;
    if (!element || total <= 0) { setViewport({ start: 0, end: total }); return; }
    const width = Math.max(1, element.scrollWidth);
    const buffer = Math.max(0.25, total * 0.01);
    setViewport({
      start: Math.max(0, element.scrollLeft / width * total - buffer),
      end: Math.min(total, (element.scrollLeft + element.clientWidth) / width * total + buffer),
    });
  };

  const invalidatePreview = () => {
    audio.current?.pause();
    setPreviewUrl(null);
    setAutoplayPreview(false);
  };
  const commit = (next: MixSession) => {
    if (session) setHistory((items) => [...items.slice(-99), session]);
    setFuture([]);
    setSession({ ...next, modified_at: new Date().toISOString() });
    setSaved(false);
    invalidatePreview();
  };
  const loadText = (text: string, path: string) => {
    const native = path.toLowerCase().endsWith(".dek");
    const next = native
      ? parseSession(text)
      : importLegacy(text, path.split(/[\\/]/).pop()?.replace(/\.(skydek\.)?(json|md)$/i, "") || "SkyDek Mix");
    audio.current?.pause();
    setSession(next);
    setHistory([]);
    setFuture([]);
    setPosition(0);
    setViewport({ start: 0, end: 0 });
    setSelectedEdit(null);
    setSelectedTrigger(null);
    setSelectedClip(null);
    setSessionPath(native ? path : null);
    setSourcePath(path);
    setSaved(native);
    setPreviewUrl(null);
    setError("");
    setStatus(native ? "Session opened." : "Legacy mix imported. Save As creates a non-destructive .dek session.");
  };
  const open = async () => {
    try {
      const result = await invoke<OpenedMixSession | null>("mix_editor_open");
      if (result) loadText(result.text, result.path);
    } catch (cause) { setError(String(cause)); }
  };
  const importOldMix = async () => {
    try {
      const result = await invoke<OpenedMixSession | null>("mix_editor_import_legacy");
      if (result) loadText(result.text, result.path);
    } catch (cause) { setError(`Could not import the older mix log. ${String(cause)}`); }
  };
  const save = async (saveAs = false) => {
    if (!session) return;
    setBusy(true);
    try {
      const path = await invoke<string | null>("mix_editor_save", {
        text: JSON.stringify(session, null, 2),
        path: saveAs ? null : sessionPath,
      });
      if (path) {
        setSessionPath(path);
        setSourcePath(path);
        setSaved(true);
        setStatus(`Saved ${path.split(/[\\/]/).pop()}`);
      }
    } catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  };
  const preparePreview = async (playAfter = false) => {
    if (!session) return;
    setBusy(true);
    setAutoplayPreview(playAfter);
    setStatus("Reconstructing edited session through the SkyDek audio engine…");
    try {
      const path = await invoke<string>("mix_editor_preview", {
        text: JSON.stringify(session),
        sessionPath: sessionPath || sourcePath,
      });
      setPreviewUrl(`${convertFileSrc(path)}?v=${Date.now()}`);
      setStatus("Edited session ready for playback.");
    } catch (cause) {
      setAutoplayPreview(false);
      setError(String(cause));
    } finally { setBusy(false); }
  };
  const play = () => {
    if (previewUrl) void audio.current?.play().catch((cause) => setError(String(cause)));
    else void preparePreview(true);
  };
  const exportMix = async (format: "wav" | "mp3") => {
    if (!session) return;
    setBusy(true);
    setStatus(`Rendering ${format.toUpperCase()}…`);
    try {
      const path = await invoke<string | null>("mix_editor_render", {
        text: JSON.stringify(session), sessionPath: sessionPath || sourcePath, format,
      });
      if (path) setStatus(`Exported ${path}`);
    } catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  };
  const seek = (seconds: number) => {
    const next = Math.max(0, Math.min(total, seconds));
    setPosition(next);
    if (audio.current) audio.current.currentTime = next;
  };
  const beginPlayheadDrag = (event: React.PointerEvent<HTMLElement>) => {
    if (!session || busy) return;
    event.preventDefault();
    const ruler = event.currentTarget;
    const move = (pointer: PointerEvent) => {
      const box = ruler.getBoundingClientRect();
      seek(timelineSeconds(pointer.clientX, box.left, box.width, total, pointer.altKey ? 0 : 0.01));
    };
    move(event.nativeEvent);
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up, { once: true });
  };
  const undo = () => {
    if (!session || !history.length) return;
    setFuture((items) => [session, ...items]);
    setSession(history.at(-1)!);
    setHistory((items) => items.slice(0, -1));
    setSaved(false);
    invalidatePreview();
  };
  const redo = () => {
    if (!session || !future.length) return;
    setHistory((items) => [...items, session]);
    setSession(future[0]);
    setFuture((items) => items.slice(1));
    setSaved(false);
    invalidatePreview();
  };
  const addEdit = (lane: Lane, seconds = position) => {
    if (!session) return;
    const id = crypto.randomUUID();
    commit({ ...session, edits: [...session.edits, { id, seconds, lane: lane.key, value: lane.initial }] });
    setSelectedEdit(id);
    setSelectedTrigger(null);
    setSelectedClip(null);
  };
  const beginDrag = (kind: "edit" | "trigger", id: string, event: React.PointerEvent<HTMLButtonElement>) => {
    if (!session || busy) return;
    event.preventDefault();
    event.stopPropagation();
    const track = event.currentTarget.parentElement;
    if (!track) return;
    const before = session;
    let moved = false;
    if (kind === "edit") { setSelectedEdit(id); setSelectedTrigger(null); }
    else { setSelectedTrigger(id); setSelectedEdit(null); }
    setSelectedClip(null);
    const move = (pointer: PointerEvent) => {
      moved = true;
      const box = track.getBoundingClientRect();
      const seconds = timelineSeconds(pointer.clientX, box.left, box.width, total, pointer.altKey ? 0 : 0.05);
      setPosition(seconds);
      setSession((current) => {
        if (!current) return current;
        return kind === "edit"
          ? { ...current, modified_at: new Date().toISOString(), edits: current.edits.map((item) => item.id === id ? { ...item, seconds } : item) }
          : { ...current, modified_at: new Date().toISOString(), triggers: current.triggers.map((item) => item.id === id ? { ...item, seconds } : item) };
      });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      if (!moved) return;
      setHistory((items) => [...items.slice(-99), before]);
      setFuture([]);
      setSaved(false);
      invalidatePreview();
      setStatus("Keyframe moved. Rebuild preview to hear the edit.");
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up, { once: true });
  };
  const chooseMedia = async (title: string) => invoke<string | null>("mix_editor_relink", { original: title });
  const replaceTrigger = async (event: Trigger) => {
    if (!session) return;
    try {
      const path = await chooseMedia(event.source);
      if (path) commit({ ...session, triggers: session.triggers.map((item) => item.id === event.id ? { ...item, source: path } : item) });
    } catch (cause) { setError(String(cause)); }
  };
  const replaceClip = async (clip: Clip) => {
    if (!session) return;
    try {
      const path = await chooseMedia(clip.path);
      if (path) commit({ ...session, relinks: { ...session.relinks, [clip.path]: path } });
    } catch (cause) { setError(String(cause)); }
  };
  const addSample = async () => {
    if (!session) return;
    try {
      const path = await chooseMedia("Choose sample audio");
      if (!path) return;
      const id = crypto.randomUUID();
      commit({ ...session, triggers: [...session.triggers, { id, seconds: position, kind: "sample", source: path, gain: 0.65 }] });
      setSelectedTrigger(id);
    } catch (cause) { setError(String(cause)); }
  };

  useEffect(() => {
    if (initial && openedInitial.current !== initial.path) {
      openedInitial.current = initial.path;
      try { loadText(initial.text, initial.path); } catch (cause) { setError(String(cause)); }
    }
  }, [initial]);
  useEffect(() => { editorDialog.current?.focus(); }, []);
  useEffect(() => {
    const frame = requestAnimationFrame(updateViewport);
    const element = timelineScroll.current;
    const observer = element && "ResizeObserver" in window ? new ResizeObserver(updateViewport) : null;
    if (element && observer) observer.observe(element);
    return () => { cancelAnimationFrame(frame); observer?.disconnect(); };
  }, [total, zoom]);
  useEffect(() => {
    if (!session) { setSources([]); return; }
    let active = true;
    invoke<Source[]>("mix_editor_sources", { text: JSON.stringify(session), sessionPath: sessionPath || sourcePath })
      .then((items) => { if (active) setSources(items); })
      .catch((cause) => { if (active) setError(String(cause)); });
    return () => { active = false; };
  }, [session, sessionPath, sourcePath]);
  useEffect(() => {
    if (!busy) { setProgress({ running: false, done: 0, total: 0 }); return; }
    const poll = () => invoke<{ running: boolean; done: number; total: number }>("mix_editor_render_status").then(setProgress).catch(() => {});
    void poll();
    const timer = setInterval(poll, 200);
    return () => clearInterval(timer);
  }, [busy]);
  useEffect(() => () => { if (recordingUrl) URL.revokeObjectURL(recordingUrl); }, [recordingUrl]);

  const onKey = (event: React.KeyboardEvent) => {
    const target = event.target as HTMLElement;
    const formControl = /^(INPUT|SELECT|TEXTAREA|BUTTON)$/.test(target.tagName);
    if (event.key === "Escape") {
      if (saved || confirm("Close without saving Mix Editor changes?")) onClose();
    } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s" && session) {
      event.preventDefault();
      void save(event.shiftKey);
    } else if (formControl) {
      return;
    } else if (event.code === "Space" && session) {
      event.preventDefault();
      if (audio.current && !audio.current.paused) audio.current.pause(); else play();
    } else if (event.key === "ArrowLeft") seek(position - (event.shiftKey ? 5 : 0.25));
    else if (event.key === "ArrowRight") seek(position + (event.shiftKey ? 5 : 0.25));
    else if (event.key === "Home") seek(0);
    else if (event.key === "End") seek(total);
    else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") { event.preventDefault(); event.shiftKey ? redo() : undo(); }
    else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "y") { event.preventDefault(); redo(); }
    else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "d" && selectedEdit && session) {
      event.preventDefault();
      const current = session.edits.find((item) => item.id === selectedEdit);
      if (current) {
        const copy = { ...current, id: crypto.randomUUID(), seconds: Math.min(total, current.seconds + 0.25) };
        commit({ ...session, edits: [...session.edits, copy] });
        setSelectedEdit(copy.id);
      }
    } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "d" && selectedTrigger && session) {
      event.preventDefault();
      const current = session.triggers.find((item) => item.id === selectedTrigger);
      if (current) {
        const copy = { ...current, id: crypto.randomUUID(), seconds: Math.min(total, current.seconds + 0.25) };
        commit({ ...session, triggers: [...session.triggers, copy] });
        setSelectedTrigger(copy.id);
      }
    }
    else if ((event.key === "Delete" || event.key === "Backspace") && selectedEdit && session) {
      event.preventDefault();
      commit({ ...session, edits: session.edits.filter((item) => item.id !== selectedEdit) });
      setSelectedEdit(null);
    } else if ((event.key === "Delete" || event.key === "Backspace") && selectedTrigger && session) {
      event.preventDefault();
      commit({ ...session, triggers: session.triggers.filter((item) => item.id !== selectedTrigger) });
      setSelectedTrigger(null);
    }
  };

  const missing = sources.some((source) => !source.resolved);
  const missingRequired = sources.some((source) => source.kind !== "recording" && !source.resolved);
  const close = () => { if (saved || confirm("Close without saving Mix Editor changes?")) onClose(); };

  return <div className="mix-replay-backdrop">
    <section ref={editorDialog} className="mix-replay mix-editor mix-editor-v2" role="dialog" aria-modal="true" aria-label="Mix Editor" tabIndex={-1} onKeyDown={onKey}>
      <header className="mix-editor-header">
        <div><small>SKYDEK DJ · BUILD 02</small><h2>Mix Editor</h2></div>
        <div className="mix-editor-session-title">{session ? <><strong>{session.name}</strong><span>{saved ? "Saved" : "Unsaved changes"}</span></> : <span>No session open</span>}</div>
        <button className="mix-editor-close" onClick={close} aria-label="Close Mix Editor">×</button>
      </header>

      <nav className="mix-editor-toolbar" aria-label="Mix Editor commands">
        <button onClick={() => void open()}>Open .dek</button>
        <button title="Convert an older SkyDek JSON or Markdown recording log into an editable session" onClick={() => void importOldMix()}>Import old mix log</button>
        <span className="toolbar-divider" />
        <button disabled={!session || busy} onClick={() => void save(false)}>Save</button>
        <button disabled={!session || busy} onClick={() => void save(true)}>Save As</button>
        <button disabled={!history.length || busy} onClick={undo}>Undo</button>
        <button disabled={!future.length || busy} onClick={redo}>Redo</button>
        <span className="toolbar-spacer" />
        <button disabled={!session || busy || missingRequired} onClick={() => void exportMix("wav")}>Export WAV</button>
        <button disabled={!session || busy || missingRequired} onClick={() => void exportMix("mp3")}>Export MP3</button>
      </nav>

      {!session ? <div className="mix-editor-empty"><div className="empty-mark">◇</div><h3>Open a recorded performance</h3><p>Open a native .dek session or convert an older SkyDek recording log without changing its original files.</p><div className="empty-actions"><button onClick={() => void open()}>Open .dek session</button><button onClick={() => void importOldMix()}>Import old mix log</button></div></div> : <>
        <div className="mix-editor-transport">
          <button onClick={() => seek(0)} title="Go to beginning">|◀</button>
          <button className="transport-primary" disabled={busy || missingRequired} onClick={play}>▶ Play session</button>
          <button onClick={() => audio.current?.pause()}>❚❚</button>
          <button onClick={() => { audio.current?.pause(); seek(0); }}>■</button>
          <time>{fmt(position)}<small> / {fmt(total)}</small></time>
          <button disabled={busy || missingRequired} onClick={() => void preparePreview(false)}>Rebuild preview</button>
          {busy && <button onClick={() => void invoke("mix_editor_cancel_render")}>Cancel</button>}
          <span className="toolbar-spacer" />
          <label>Timeline zoom <input type="range" min="1" max="16" step="0.5" value={zoom} onChange={(event) => setZoom(+event.target.value)} /></label>
          <strong>{busy && progress.total ? `${Math.round(progress.done / progress.total * 100)}%` : `${session.edits.length} keyframes`}</strong>
        </div>
        <audio
          ref={audio}
          src={previewUrl || undefined}
          onCanPlay={() => { if (autoplayPreview) { setAutoplayPreview(false); void audio.current?.play().catch((cause) => setError(String(cause))); } }}
          onTimeUpdate={(event) => setPosition(event.currentTarget.currentTime)}
          onEnded={() => setPosition(total)}
          onError={() => setError("Edited session preview could not be played.")}
        />

        {missing && <div className="mix-editor-relink"><strong>{missingRequired ? "Missing required media" : "Original recording unavailable (optional)"}</strong>{sources.filter((source) => !source.resolved).map((source) => <button key={source.original} onClick={async () => { const path = await chooseMedia(source.original); if (path) commit({ ...session, relinks: { ...session.relinks, [source.original]: path } }); }}>Relink {source.kind === "recording" ? "recording" : source.original.split(/[\\/]/).pop()}</button>)}</div>}
        {session.migration.warnings.length > 0 && <details className="mix-editor-import-notes"><summary>Imported recording notes ({session.migration.warnings.length})</summary>{session.migration.warnings.map((warning) => <p key={warning}>{warning}</p>)}</details>}

        <div className="mix-editor-workspace">
          <div className="mix-editor-timeline-shell">
            <div className="timeline-corner"><span>LAYERS</span><small>{fmt(total)}</small></div>
            <div className="mix-editor-scroll" ref={timelineScroll} onScroll={updateViewport}>
              <div className="mix-editor-timeline" style={{ width: `${Math.max(100, zoom * 100)}%` }}>
                <div className="mix-editor-ruler" onPointerDown={beginPlayheadDrag}>
                  {Array.from({ length: 17 }, (_, index) => <span key={index} style={{ left: `${index * 6.25}%` }}>{fmt(total * index / 16)}</span>)}
                  <button className="timeline-playhead-handle" style={{ left: `${position / Math.max(0.01, total) * 100}%` }} aria-label="Drag playhead" title="Drag to scrub" />
                </div>

                {clips.map((deck, deckIndex) => <div className="timeline-row clip-row" key={`clips-${deckIndex}`}>
                  <div className="timeline-layer"><b className={`deck-dot deck-${deckIndex + 1}`} /> <strong>DEK {deckIndex + 1}</strong><small>Audio</small></div>
                  <div className="timeline-track" onPointerDown={(event) => { if (event.target === event.currentTarget) { const box = event.currentTarget.getBoundingClientRect(); seek(timelineSeconds(event.clientX, box.left, box.width, total, 0)); } }}>
                    {deck.map((clip, index) => <button
                      key={`${clip.path}-${index}`}
                      className={`timeline-clip deck-${deckIndex + 1} ${selectedClip?.deck === clip.deck && selectedClip.path === clip.path && selectedClip.start === clip.start ? "selected" : ""}`}
                      style={{ left: `${clip.start / Math.max(0.01, total) * 100}%`, width: `${Math.max(0.6, (clip.end - clip.start) / Math.max(0.01, total) * 100)}%` }}
                      title={`${clip.path}\nDouble-click to replace source audio`}
                      onClick={() => { setSelectedClip(clip); setSelectedEdit(null); setSelectedTrigger(null); }}
                      onDoubleClick={() => void replaceClip(clip)}
                    >{clip.path.split(/[\\/]/).pop()}</button>)}
                    <i className="row-playhead" style={{ left: `${position / Math.max(0.01, total) * 100}%` }} />
                  </div>
                </div>)}

                <div className="timeline-row event-row">
                  <div className="timeline-layer"><b className="event-dot" /><strong>EVENTS</strong><small>Samples</small></div>
                  <div className="timeline-track">
                    {visibleTimelineItems(session.triggers, viewport.start, viewport.end, selectedTrigger).map((item) => <button
                      key={item.id}
                      className={`timeline-keyframe sample-keyframe ${selectedTrigger === item.id ? "selected" : ""}`}
                      style={{ left: `${item.seconds / Math.max(0.01, total) * 100}%` }}
                      title={`${fmt(item.seconds)} · ${item.source.split(/[\\/]/).pop()} · drag to move`}
                      onClick={() => { setSelectedTrigger(item.id); setSelectedEdit(null); setSelectedClip(null); seek(item.seconds); }}
                      onPointerDown={(event) => beginDrag("trigger", item.id, event)}
                    />)}
                    <i className="row-playhead" style={{ left: `${position / Math.max(0.01, total) * 100}%` }} />
                  </div>
                </div>

                <div className="timeline-row marker-row">
                  <div className="timeline-layer"><b className="marker-dot" /><strong>LOG</strong><small>Recorded events</small></div>
                  <div className="timeline-track">
                    {visibleTimelineItems(session.markers, viewport.start, viewport.end, null).map((marker) => <button
                      key={marker.id}
                      className="timeline-marker"
                      style={{ left: `${marker.seconds / Math.max(0.01, total) * 100}%` }}
                      title={`${fmt(marker.seconds)} · ${marker.label}`}
                      aria-label={`${marker.label} at ${fmt(marker.seconds)}`}
                      onClick={() => seek(marker.seconds)}
                    />)}
                    <i className="row-playhead" style={{ left: `${position / Math.max(0.01, total) * 100}%` }} />
                  </div>
                </div>

                {lanes.map((lane, index) => <div className={`timeline-row automation-row ${index === 0 || lanes[index - 1].group !== lane.group ? "group-start" : ""}`} key={lane.key}>
                  <div className="timeline-layer"><button className="add-keyframe" title={`Add ${lane.label} keyframe at playhead`} onClick={() => addEdit(lane)}>◆</button><strong>{lane.group}</strong><small>{lane.label}</small></div>
                  <div className="timeline-track" onDoubleClick={(event) => { const box = event.currentTarget.getBoundingClientRect(); addEdit(lane, timelineSeconds(event.clientX, box.left, box.width, total)); }}>
                    {visibleTimelineItems(session.edits.filter((item) => item.lane === lane.key), viewport.start, viewport.end, selectedEdit).map((item) => <button
                      key={item.id}
                      className={`timeline-keyframe ${selectedEdit === item.id ? "selected" : ""}`}
                      style={{ left: `${item.seconds / Math.max(0.01, total) * 100}%` }}
                      title={`${fmt(item.seconds)} · ${item.value.toFixed(2)} · drag to move`}
                      onClick={() => { setSelectedEdit(item.id); setSelectedTrigger(null); setSelectedClip(null); seek(item.seconds); }}
                      onPointerDown={(event) => beginDrag("edit", item.id, event)}
                    />)}
                    <i className="row-playhead" style={{ left: `${position / Math.max(0.01, total) * 100}%` }} />
                  </div>
                </div>)}
              </div>
            </div>
          </div>

          <aside className="mix-editor-inspector">
            <h3>Properties</h3>
            {edit ? <>
              <span className="property-type">AUTOMATION KEYFRAME</span>
              <h4>{lanes.find((lane) => lane.key === edit.lane)?.group} · {lanes.find((lane) => lane.key === edit.lane)?.label}</h4>
              <label>Time<input type="number" min="0" max={total} step="0.05" value={edit.seconds} onChange={(event) => commit({ ...session, edits: session.edits.map((item) => item.id === edit.id ? { ...item, seconds: Math.max(0, Math.min(total, +event.target.value)) } : item) })} /></label>
              <label>Value<input type="number" min={lanes.find((lane) => lane.key === edit.lane)?.min} max={lanes.find((lane) => lane.key === edit.lane)?.max} step={lanes.find((lane) => lane.key === edit.lane)?.step ?? 0.01} value={edit.value} onChange={(event) => { const lane = lanes.find((item) => item.key === edit.lane)!; const raw = Math.max(lane.min, Math.min(lane.max, +event.target.value)); const value = lane.step === 1 ? Math.round(raw) : raw; commit({ ...session, edits: session.edits.map((item) => item.id === edit.id ? { ...item, value } : item) }); }} /></label>
              <div className="inspector-actions"><button onClick={() => { const copy = { ...edit, id: crypto.randomUUID(), seconds: Math.min(total, edit.seconds + 0.25) }; commit({ ...session, edits: [...session.edits, copy] }); setSelectedEdit(copy.id); }}>Duplicate</button><button className="danger" onClick={() => { commit({ ...session, edits: session.edits.filter((item) => item.id !== edit.id) }); setSelectedEdit(null); }}>Delete</button></div>
            </> : trigger ? <>
              <span className="property-type">SAMPLE EVENT</span>
              <h4>{trigger.source.split(/[\\/]/).pop()}</h4>
              <label>Time<input type="number" min="0" max={total} step="0.05" value={trigger.seconds} onChange={(event) => commit({ ...session, triggers: session.triggers.map((item) => item.id === trigger.id ? { ...item, seconds: Math.max(0, Math.min(total, +event.target.value)) } : item) })} /></label>
              <label>Gain<input type="range" min="0" max="2" step="0.01" value={trigger.gain} onChange={(event) => commit({ ...session, triggers: session.triggers.map((item) => item.id === trigger.id ? { ...item, gain: +event.target.value } : item) })} /></label>
              <button onClick={() => void replaceTrigger(trigger)}>Replace sound</button>
              <div className="inspector-actions"><button onClick={() => { const copy = { ...trigger, id: crypto.randomUUID(), seconds: Math.min(total, trigger.seconds + 0.25) }; commit({ ...session, triggers: [...session.triggers, copy] }); setSelectedTrigger(copy.id); }}>Duplicate</button><button className="danger" onClick={() => { commit({ ...session, triggers: session.triggers.filter((item) => item.id !== trigger.id) }); setSelectedTrigger(null); }}>Delete</button></div>
            </> : selectedClip ? <>
              <span className="property-type">DECK AUDIO</span>
              <h4>DEK {selectedClip.deck + 1}</h4>
              <p title={selectedClip.path}>{selectedClip.path.split(/[\\/]/).pop()}</p>
              <dl><dt>In</dt><dd>{fmt(selectedClip.start)}</dd><dt>Out</dt><dd>{fmt(selectedClip.end)}</dd></dl>
              <button onClick={() => void replaceClip(selectedClip)}>Replace source audio</button>
              {session.relinks[selectedClip.path] && <small>Using: {session.relinks[selectedClip.path].split(/[\\/]/).pop()}</small>}
            </> : <>
              <span className="property-type">SESSION</span>
              <label>Name<input value={session.name} onChange={(event) => { setSession({ ...session, name: event.target.value.slice(0, 120), modified_at: new Date().toISOString() }); setSaved(false); }} /></label>
              <dl><dt>Duration</dt><dd>{fmt(total)}</dd><dt>Keyframes</dt><dd>{session.edits.length}</dd><dt>Samples</dt><dd>{session.triggers.length}</dd><dt>Log events</dt><dd>{session.markers.length}</dd><dt>Media</dt><dd>{sources.filter((source) => source.resolved).length}/{sources.length}</dd></dl>
              <button onClick={() => void addSample()}>Add sample at playhead</button>
              <label className="original-recording">Original recording<input type="file" accept=".mp3,.wav,audio/*" onChange={(event) => { const file = event.target.files?.[0]; if (file) { if (recordingUrl) URL.revokeObjectURL(recordingUrl); setRecordingUrl(URL.createObjectURL(file)); } }} /></label>
              {recordingUrl && <audio controls src={recordingUrl} />}
            </>}
          </aside>
        </div>

        <footer className="mix-editor-footer">
          <span className={missing ? "missing" : "ready"}>● {sources.filter((source) => source.resolved).length}/{sources.length} sources ready</span>
          <span>Crossfader {Math.round((frame?.crossfader ?? 0) * 100)}%</span>
          <span>Master {Math.round((frame?.master ?? 0) * 100)}%</span>
          <span>DEK 1 {frame?.decks[0].playing ? "playing" : "paused"}</span>
          <span>DEK 2 {frame?.decks[1].playing ? "playing" : "paused"}</span>
          <span>{session.migration.imported_from === "legacy-json" ? "Converted legacy log" : ".dek session"}</span>
        </footer>
      </>}

      {status && <p className="mix-editor-status" role="status">{status}</p>}
      {error && <p className="mix-editor-error" role="alert">{error}<button onClick={() => setError("")}>Dismiss</button></p>}
    </section>
  </div>;
}

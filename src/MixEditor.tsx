import { useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import {
  duration,
  importLegacy,
  parseSession,
  replaceDeckClip,
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
type Source = { original: string; resolved: string | null; kind: "deck" | "sample" | "recording" | "mic" };
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
    if (index > 0 && frames[index - 1].decks[deck].path === path && previous?.path === path) previous.end = next;
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
  const [previewRange, setPreviewRange] = useState({ start: 0, end: 0 });
  const previewFile = useRef<string | null>(null);
  const [recordingUrl, setRecordingUrl] = useState<string | null>(null);
  const [sessionPath, setSessionPath] = useState<string | null>(null);
  const [sourcePath, setSourcePath] = useState<string | null>(null);
  const [savedSession, setSavedSession] = useState<MixSession | null>(null);
  const [busy, setBusy] = useState(false);
  const [sources, setSources] = useState<Source[]>([]);
  const [progress, setProgress] = useState({ running: false, done: 0, total: 0 });
  const [autoplayPreview, setAutoplayPreview] = useState(false);
  const [showShortcuts, setShowShortcuts] = useState(false);
  const [inspectorWidth, setInspectorWidth] = useState(290);
  const [collapsedGroups, setCollapsedGroups] = useState<string[]>([]);
  const [waveforms, setWaveforms] = useState<Record<string, number[]>>({});
  const audio = useRef<HTMLAudioElement>(null);
  const editorDialog = useRef<HTMLElement>(null);
  const timelineScroll = useRef<HTMLDivElement>(null);
  const openedInitial = useRef("");
  const busyRef = useRef(false);
  const renderBusyRef = useRef(false);
  const previewGeneration = useRef(0);
  const saved = !session || session === savedSession;
  const total = session ? duration(session) : 0;
  const frame = useMemo(() => (session ? reconstructed(session, position) : null), [session, position]);
  const clips = useMemo(() => (session ? [deckSegments(session, 0), deckSegments(session, 1)] : [[], []]), [session?.source.log]);
  const editsByLane = useMemo(() => {
    const grouped = new Map<Edit["lane"], Edit[]>();
    for (const edit of session?.edits ?? []) {
      const row = grouped.get(edit.lane) ?? [];
      row.push(edit);
      grouped.set(edit.lane, row);
    }
    return grouped;
  }, [session?.edits]);
  const triggerSources = useMemo(() => session?.triggers.map(item => item.source).join("\u0000") ?? "", [session?.triggers]);
  const relinkKey = useMemo(() => JSON.stringify(session?.relinks ?? {}), [session?.relinks]);
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
    previewGeneration.current += 1;
    audio.current?.pause();
    audio.current?.removeAttribute("src");
    audio.current?.load();
    setPreviewUrl(null);
    setAutoplayPreview(false);
    if (previewFile.current) {
      void invoke("mix_editor_discard_preview", { path: previewFile.current }).catch(() => {});
      previewFile.current = null;
    }
  };
  const commit = (next: MixSession) => {
    if (session) setHistory((items) => [...items.slice(-99), session]);
    setFuture([]);
    setSession({ ...next, modified_at: new Date().toISOString() });
    invalidatePreview();
  };
  const loadText = (text: string, path: string) => {
    const native = path.toLowerCase().endsWith(".dek");
    const next = native
      ? parseSession(text)
      : importLegacy(text, path.split(/[\\/]/).pop()?.replace(/\.(skydek\.)?(json|md)$/i, "") || "SkyDek Mix");
    invalidatePreview();
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
    setSavedSession(native ? next : null);
    setError("");
    setStatus(native ? "Session opened." : "Legacy mix imported. Save As creates a non-destructive .dek session.");
  };
  const open = async () => {
    if (busy || busyRef.current || (!saved && !window.confirm("Open another session and discard unsaved Mix Editor changes?"))) return;
    try {
      const result = await invoke<OpenedMixSession | null>("mix_editor_open");
      if (result) loadText(result.text, result.path);
    } catch (cause) { setError(String(cause)); }
  };
  const importOldMix = async () => {
    if (busy || busyRef.current || (!saved && !window.confirm("Import another mix and discard unsaved Mix Editor changes?"))) return;
    try {
      const result = await invoke<OpenedMixSession | null>("mix_editor_import_legacy");
      if (result) loadText(result.text, result.path);
    } catch (cause) { setError(`Could not import the older mix log. ${String(cause)}`); }
  };
  const save = async (saveAs = false) => {
    if (!session || busy || busyRef.current) return;
    const saving = session;
    busyRef.current = true;
    setBusy(true);
    try {
      const path = await invoke<string | null>("mix_editor_save", {
        text: JSON.stringify(saving, null, 2),
        path: saveAs ? null : sessionPath,
      });
      if (path) {
        setSessionPath(path);
        setSourcePath(path);
        setSavedSession(saving);
        setStatus(`Saved ${path.split(/[\\/]/).pop()}`);
      }
    } catch (cause) { setError(String(cause)); }
    finally { busyRef.current = false; setBusy(false); }
  };
  const preparePreview = async (playAfter = false) => {
    if (!session || busy || renderBusyRef.current) return;
    invalidatePreview();
    const generation = previewGeneration.current;
    renderBusyRef.current = true;
    setBusy(true);
    setAutoplayPreview(playAfter);
    setStatus("Reconstructing edited session through the SkyDek audio engine…");
    try {
      const result = await invoke<{ path: string; start: number; end: number }>("mix_editor_preview", {
        text: JSON.stringify(session),
        sessionPath: sessionPath || sourcePath,
        position,
      });
      if (generation !== previewGeneration.current) {
        void invoke("mix_editor_discard_preview", { path: result.path }).catch(() => {});
        return;
      }
      previewFile.current = result.path;
      setPreviewRange({ start: result.start, end: result.end });
      setPreviewUrl(`${convertFileSrc(result.path)}?v=${Date.now()}`);
      setStatus("Edited session ready for playback.");
    } catch (cause) {
      setAutoplayPreview(false);
      setError(String(cause));
    } finally { renderBusyRef.current = false; setBusy(false); }
  };
  const play = () => {
    if (previewUrl && position >= previewRange.start && position < previewRange.end) void audio.current?.play().catch((cause) => setError(String(cause)));
    else void preparePreview(true);
  };
  const exportMix = async (format: "wav" | "mp3") => {
    if (!session || busy || renderBusyRef.current) return;
    renderBusyRef.current = true;
    setBusy(true);
    setStatus(`Rendering ${format.toUpperCase()}…`);
    try {
      const path = await invoke<string | null>("mix_editor_render", {
        text: JSON.stringify(session), sessionPath: sessionPath || sourcePath, format,
      });
      if (path) setStatus(`Exported ${path}`);
    } catch (cause) { setError(String(cause)); }
    finally { renderBusyRef.current = false; setBusy(false); }
  };
  const seek = (seconds: number) => {
    const next = Math.max(0, Math.min(total, seconds));
    setPosition(next);
    if (audio.current && previewUrl) {
      if (next >= previewRange.start && next <= previewRange.end) audio.current.currentTime = next - previewRange.start;
      else invalidatePreview();
    }
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
    invalidatePreview();
  };
  const redo = () => {
    if (!session || !future.length) return;
    setHistory((items) => [...items, session]);
    setSession(future[0]);
    setFuture((items) => items.slice(1));
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
      if (path) {
        commit(replaceDeckClip(session, clip.deck, clip.start, clip.end, clip.path, path));
        setSelectedClip({ ...clip, path });
        setStatus("Replaced only the selected clip. Other uses of the source remain unchanged.");
      }
    } catch (cause) { setError(String(cause)); }
  };
  const relinkClipSource = async (clip: Clip) => {
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
      if (session && !saved && !window.confirm("Open another session and discard unsaved Mix Editor changes?")) return;
      openedInitial.current = initial.path;
      try { loadText(initial.text, initial.path); } catch (cause) { setError(String(cause)); }
    }
  }, [initial]);
  useEffect(() => { editorDialog.current?.focus(); }, []);
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const originalOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    editorDialog.current?.focus();
    const onTab = (event: KeyboardEvent) => {
      if (event.key !== "Tab" || !editorDialog.current) return;
      const focusable = Array.from(editorDialog.current.querySelectorAll<HTMLElement>("button:not(:disabled),input:not(:disabled),select:not(:disabled),[tabindex]:not([tabindex='-1'])"))
        .filter(item => item.getClientRects().length > 0);
      if (!focusable.length) return;
      const first = focusable[0], last = focusable[focusable.length - 1];
      if (event.shiftKey && (document.activeElement === first || document.activeElement === editorDialog.current)) { event.preventDefault(); last.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
    };
    const keepFocus = (event: FocusEvent) => {
      if (event.target instanceof Node && editorDialog.current && !editorDialog.current.contains(event.target)) editorDialog.current.focus();
    };
    document.addEventListener("keydown", onTab, true);
    document.addEventListener("focusin", keepFocus);
    return () => {
      document.removeEventListener("keydown", onTab, true);
      document.removeEventListener("focusin", keepFocus);
      document.body.style.overflow = originalOverflow;
      previous?.isConnected && previous.focus();
    };
  }, []);
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
  }, [session?.source.log, triggerSources, relinkKey, sessionPath, sourcePath]);
  useEffect(() => {
    let active = true;
    const paths = [...new Set(clips.flat().map(clip => clip.path))];
    for (const path of paths) {
      void invoke<number[]>("mix_editor_waveform_summary", { path }).then(bars => {
        if (!active || !bars.length) return;
        setWaveforms(current => ({ ...current, [path]: bars }));
      }).catch(() => {});
    }
    return () => { active = false; };
  }, [clips]);
  useEffect(() => {
    if (!busy) { setProgress({ running: false, done: 0, total: 0 }); return; }
    const poll = () => invoke<{ running: boolean; done: number; total: number }>("mix_editor_render_status").then(setProgress).catch(() => {});
    void poll();
    const timer = setInterval(poll, 200);
    return () => clearInterval(timer);
  }, [busy]);
  useEffect(() => () => { if (recordingUrl) URL.revokeObjectURL(recordingUrl); }, [recordingUrl]);
  useEffect(() => () => {
    if (previewFile.current) void invoke("mix_editor_discard_preview", { path: previewFile.current }).catch(() => {});
  }, []);

  const onKey = (event: React.KeyboardEvent) => {
    const target = event.target as HTMLElement;
    const textControl = /^(INPUT|SELECT|TEXTAREA)$/.test(target.tagName) || target.isContentEditable;
    if (event.key === "Escape") {
      event.preventDefault(); event.stopPropagation();
      if (showShortcuts) setShowShortcuts(false);
      else if (selectedEdit || selectedTrigger || selectedClip) { setSelectedEdit(null); setSelectedTrigger(null); setSelectedClip(null); }
      else close();
    } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s" && session) {
      event.preventDefault();
      void save(event.shiftKey);
    } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "o") {
      event.preventDefault(); void open();
    } else if (textControl) {
      return;
    } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") { event.preventDefault(); event.shiftKey ? redo() : undo(); }
    else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "y") { event.preventDefault(); redo(); }
    else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "d") {
      event.preventDefault();
      if (!session) return;
      const currentEdit = session.edits.find(item => item.id === selectedEdit);
      const currentTrigger = session.triggers.find(item => item.id === selectedTrigger);
      if (currentEdit) { const copy = { ...currentEdit, id: crypto.randomUUID(), seconds: Math.min(total, currentEdit.seconds + 0.25) }; commit({ ...session, edits: [...session.edits, copy] }); setSelectedEdit(copy.id); }
      else if (currentTrigger) { const copy = { ...currentTrigger, id: crypto.randomUUID(), seconds: Math.min(total, currentTrigger.seconds + 0.25) }; commit({ ...session, triggers: [...session.triggers, copy] }); setSelectedTrigger(copy.id); }
    } else if (event.altKey && (event.key === "ArrowLeft" || event.key === "ArrowRight") && session) {
      event.preventDefault();
      const delta = (event.key === "ArrowRight" ? 1 : -1) * (event.shiftKey ? 1 : 0.05);
      if (selectedEdit) commit({ ...session, edits: session.edits.map(item => item.id === selectedEdit ? { ...item, seconds: Math.max(0, Math.min(total, item.seconds + delta)) } : item) });
      else if (selectedTrigger) commit({ ...session, triggers: session.triggers.map(item => item.id === selectedTrigger ? { ...item, seconds: Math.max(0, Math.min(total, item.seconds + delta)) } : item) });
    } else if (event.code === "Space" && session) {
      event.preventDefault();
      if (audio.current && !audio.current.paused) audio.current.pause(); else play();
    } else if (event.key === "ArrowLeft") { event.preventDefault(); seek(position - (event.shiftKey ? 5 : 0.25)); }
    else if (event.key === "ArrowRight") { event.preventDefault(); seek(position + (event.shiftKey ? 5 : 0.25)); }
    else if (event.key === "Home") { event.preventDefault(); seek(0); }
    else if (event.key === "End") { event.preventDefault(); seek(total); }
    else if (event.key === "+" || event.key === "=") { event.preventDefault(); setZoom(value => Math.min(16, value + 0.5)); }
    else if (event.key === "-" || event.key === "_") { event.preventDefault(); setZoom(value => Math.max(1, value - 0.5)); }
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
  const limitations = session ? [
    ...session.migration.warnings,
    ...(!session.source.log.mic_file ? ["This recording has no microphone stem; mic audio cannot be reconstructed."] : []),
    ...(session.source.log.capture_revision !== 1 ? ["Rapid sample presses and effect changes may have been missed by the original snapshot log."] : []),
  ].filter((message, index, all) => all.indexOf(message) === index) : [];
  const close = () => { if (busy || busyRef.current || renderBusyRef.current) return; if (saved || confirm("Close without saving Mix Editor changes?")) onClose(); };

  return <div className="mix-replay-backdrop">
    <section ref={editorDialog} className="mix-replay mix-editor mix-editor-v2" role="dialog" aria-modal="true" aria-label="Mix Editor" tabIndex={-1} onKeyDown={onKey}>
      <header className="mix-editor-header">
        <div><small>SKYDEK DJ · BUILD 02</small><h2>Mix Editor</h2></div>
        <div className="mix-editor-session-title">{session ? <><strong>{session.name}</strong><span>{saved ? "Saved" : "Unsaved changes"}</span></> : <span>No session open</span>}</div>
        <button className="mix-editor-close" disabled={busy} onClick={close} aria-label="Close Mix Editor">×</button>
      </header>

      <nav className="mix-editor-toolbar" aria-label="Mix Editor commands">
        <button disabled={busy} onClick={() => void open()}>Open .dek</button>
        <button disabled={busy} title="Convert an older SkyDek JSON or Markdown recording log into an editable session" onClick={() => void importOldMix()}>Import old mix log</button>
        <span className="toolbar-divider" />
        <button disabled={!session || busy} onClick={() => void save(false)}>Save</button>
        <button disabled={!session || busy} onClick={() => void save(true)}>Save As</button>
        <button disabled={!history.length || busy} onClick={undo}>Undo</button>
        <button disabled={!future.length || busy} onClick={redo}>Redo</button>
        <button type="button" aria-expanded={showShortcuts} onClick={() => setShowShortcuts(value => !value)}>Keyboard help</button>
        <span className="toolbar-spacer" />
        <button disabled={!session || busy || missingRequired} onClick={() => void exportMix("wav")}>Export WAV</button>
        <button disabled={!session || busy || missingRequired} onClick={() => void exportMix("mp3")}>Export MP3</button>
      </nav>
      {showShortcuts && <div className="mix-editor-shortcuts" role="region" aria-label="Mix Editor keyboard shortcuts">
        <strong>Keyboard controls</strong><span>Space play/pause · ←/→ seek 0.25s · Shift+←/→ seek 5s · Home/End start/end</span>
        <span>Alt+←/→ move selected point 0.05s · Shift+Alt+←/→ move 1s · +/− zoom</span>
        <span>Ctrl+O open · Ctrl+S save · Ctrl+Shift+S Save As · Ctrl+Z/Y undo/redo · Ctrl+D duplicate · Delete remove · Esc clear/close</span>
      </div>}

      {!session ? <div className="mix-editor-empty"><div className="empty-mark">◇</div><h3>Open a recorded performance</h3><p>Open a native .dek session or convert an older SkyDek recording log without changing its original files.</p><div className="empty-actions"><button onClick={() => void open()}>Open .dek session</button><button onClick={() => void importOldMix()}>Import old mix log</button></div></div> : <>
        <div className="mix-editor-transport">
          <button onClick={() => seek(0)} title="Go to beginning">|◀</button>
          <button className="transport-primary" disabled={busy || missingRequired} onClick={play} title="Audition a 30-second region around the playhead (Space)">▶ Preview 30s</button>
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
          onCanPlay={() => { if (autoplayPreview && audio.current) { audio.current.currentTime = Math.max(0, Math.min(audio.current.duration || 0, position - previewRange.start)); setAutoplayPreview(false); void audio.current.play().catch((cause) => setError(String(cause))); } }}
          onTimeUpdate={(event) => setPosition(previewRange.start + event.currentTarget.currentTime)}
          onEnded={() => setPosition(previewRange.end)}
          onError={() => setError("Edited session preview could not be played.")}
        />

        {missing && <div className="mix-editor-relink"><strong>{missingRequired ? "Missing required media" : "Original recording unavailable (optional)"}</strong>{sources.filter((source) => !source.resolved).map((source) => <button key={source.original} onClick={async () => { const path = await chooseMedia(source.original); if (path) commit({ ...session, relinks: { ...session.relinks, [source.original]: path } }); }}>Relink {source.kind === "recording" ? "recording" : source.kind === "mic" ? "microphone stem" : source.original.split(/[\\/]/).pop()}</button>)}</div>}
        {limitations.length > 0 && <details className="mix-editor-import-notes"><summary>Recording fidelity notes ({limitations.length})</summary>{limitations.map((warning) => <p key={warning}>{warning}</p>)}</details>}

        <div className="mix-editor-groupbar" aria-label="Automation lane groups">Automation
          {(["DEK 1", "DEK 2", "MIXER"] as const).map(group => <button key={group} aria-expanded={!collapsedGroups.includes(group)} onClick={() => setCollapsedGroups(current => current.includes(group) ? current.filter(value => value !== group) : [...current, group])}>{collapsedGroups.includes(group) ? "+" : "−"} {group}</button>)}
        </div>
        <div className="mix-editor-workspace" style={{ gridTemplateColumns: `minmax(0, 1fr) 8px ${inspectorWidth}px` }}>
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
                      title={`${clip.path}\nDouble-click to replace this clip only`}
                      onClick={() => { setSelectedClip(clip); setSelectedEdit(null); setSelectedTrigger(null); }}
                      onFocus={() => { setSelectedClip(clip); setSelectedEdit(null); setSelectedTrigger(null); }}
                      onDoubleClick={() => void replaceClip(clip)}
                    >{waveforms[clip.path] ? <span className="clip-waveform" aria-hidden="true">{waveforms[clip.path].map((height, bar) => <i key={bar} style={{ height: `${Math.round(height * 90)}%` }} />)}</span> : <span className="clip-no-waveform">Waveform unavailable</span>}<span className="clip-title">{clip.path.split(/[\\/]/).pop()}</span></button>)}
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
                      onFocus={() => { setSelectedTrigger(item.id); setSelectedEdit(null); setSelectedClip(null); }}
                      aria-label={`Sample ${item.source.split(/[\\/]/).pop()} at ${fmt(item.seconds)}`}
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

                {lanes.map((lane, index) => <div className={`timeline-row automation-row ${collapsedGroups.includes(lane.group) ? "collapsed" : ""} ${index === 0 || lanes[index - 1].group !== lane.group ? "group-start" : ""}`} key={lane.key}>
                  <div className="timeline-layer"><button className="add-keyframe" title={`Add ${lane.label} keyframe at playhead`} onClick={() => addEdit(lane)}>◆</button><strong>{lane.group}</strong><small>{lane.label}</small></div>
                  <div className="timeline-track" onDoubleClick={(event) => { const box = event.currentTarget.getBoundingClientRect(); addEdit(lane, timelineSeconds(event.clientX, box.left, box.width, total)); }}>
                    {visibleTimelineItems(editsByLane.get(lane.key) ?? [], viewport.start, viewport.end, selectedEdit).map((item) => <button
                      key={item.id}
                      className={`timeline-keyframe ${selectedEdit === item.id ? "selected" : ""}`}
                      style={{ left: `${item.seconds / Math.max(0.01, total) * 100}%` }}
                      title={`${fmt(item.seconds)} · ${item.value.toFixed(2)} · drag to move`}
                      onClick={() => { setSelectedEdit(item.id); setSelectedTrigger(null); setSelectedClip(null); seek(item.seconds); }}
                      onFocus={() => { setSelectedEdit(item.id); setSelectedTrigger(null); setSelectedClip(null); }}
                      aria-label={`${lane.group} ${lane.label} keyframe at ${fmt(item.seconds)}, value ${item.value.toFixed(2)}`}
                      onPointerDown={(event) => beginDrag("edit", item.id, event)}
                    />)}
                    <i className="row-playhead" style={{ left: `${position / Math.max(0.01, total) * 100}%` }} />
                  </div>
                </div>)}
              </div>
            </div>
          </div>

          <div className="mix-editor-splitter" role="separator" aria-label="Resize Mix Editor inspector" aria-orientation="vertical" tabIndex={0}
            onKeyDown={event => { if (event.key === "ArrowLeft" || event.key === "ArrowRight") { event.preventDefault(); event.stopPropagation(); setInspectorWidth(value => Math.max(240, Math.min(560, value + (event.key === "ArrowLeft" ? 20 : -20)))); } }}
            onPointerDown={event => { const start = event.clientX, width = inspectorWidth; const move = (next: PointerEvent) => setInspectorWidth(Math.max(240, Math.min(560, width + start - next.clientX))); const up = () => { window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", up); }; window.addEventListener("pointermove", move); window.addEventListener("pointerup", up, { once: true }); }} />
          <aside className="mix-editor-inspector">
            <h3>Properties</h3>
            <details className="mix-editor-point-list"><summary>All keyframes ({session.edits.length + session.triggers.length})</summary>
              {[...session.edits.map(item => ({ id: item.id, seconds: item.seconds, name: item.lane, type: "edit" as const })), ...session.triggers.map(item => ({ id: item.id, seconds: item.seconds, name: item.source.split(/[\\/]/).pop() || "Sample", type: "trigger" as const }))]
                .sort((a, b) => a.seconds - b.seconds).map(item => <button key={item.id} className={selectedEdit === item.id || selectedTrigger === item.id ? "selected" : ""} onClick={() => { seek(item.seconds); setSelectedEdit(item.type === "edit" ? item.id : null); setSelectedTrigger(item.type === "trigger" ? item.id : null); setSelectedClip(null); }}>
                  <time>{fmt(item.seconds)}</time><span>{item.name}</span>
                </button>)}
            </details>
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
              <p className="property-path">{trigger.source}</p>
              <button onClick={() => void replaceTrigger(trigger)}>Replace sound</button>
              <div className="inspector-actions"><button onClick={() => { const copy = { ...trigger, id: crypto.randomUUID(), seconds: Math.min(total, trigger.seconds + 0.25) }; commit({ ...session, triggers: [...session.triggers, copy] }); setSelectedTrigger(copy.id); }}>Duplicate</button><button className="danger" onClick={() => { commit({ ...session, triggers: session.triggers.filter((item) => item.id !== trigger.id) }); setSelectedTrigger(null); }}>Delete</button></div>
            </> : selectedClip ? <>
              <span className="property-type">DECK AUDIO</span>
              <h4>DEK {selectedClip.deck + 1}</h4>
              <p title={selectedClip.path}>{selectedClip.path.split(/[\\/]/).pop()}</p>
              <dl><dt>In</dt><dd>{fmt(selectedClip.start)}</dd><dt>Out</dt><dd>{fmt(selectedClip.end)}</dd></dl>
              <p className="property-path">{selectedClip.path}</p>
              <button onClick={() => void replaceClip(selectedClip)}>Replace this clip only</button>
              <button onClick={() => void relinkClipSource(selectedClip)}>Relink this source everywhere</button>
              {session.relinks[selectedClip.path] && <small>Relinked everywhere to: {session.relinks[selectedClip.path].split(/[\\/]/).pop()}</small>}
            </> : <>
              <span className="property-type">SESSION</span>
              <label>Name<input value={session.name} onChange={(event) => { setSession({ ...session, name: event.target.value.slice(0, 120), modified_at: new Date().toISOString() }); }} /></label>
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
          <span>{session.source.log.mic_file ? "Mic stem included" : "Mic unavailable in this log"}</span>
          <span>{session.migration.imported_from === "legacy-json" ? "Converted legacy log" : ".dek session"}</span>
        </footer>
      </>}

      {status && <p className="mix-editor-status" role="status">{status}</p>}
      {error && <p className="mix-editor-error" role="alert">{error}<button onClick={() => setError("")}>Dismiss</button></p>}
    </section>
  </div>;
}

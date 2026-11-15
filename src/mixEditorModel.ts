import { frameAt, parseMixLog, type Frame, type MixLog } from "./mixReplayModel.ts";

export const MIX_SESSION_FORMAT = "skydek.mix" as const;
export const MIX_SESSION_VERSION = 1 as const;

export type EditLane =
  | "crossfader" | "master" | "sampler_volume"
  | "volume_0" | "volume_1" | "rate_0" | "rate_1"
  | "filter_0" | "filter_1" | "wet_0" | "wet_1"
  | "effect_0" | "effect_1";
export type Edit = { id: string; seconds: number; lane: EditLane; value: number };
export type Trigger = { id: string; seconds: number; kind: "sample"; source: string; gain: number };
export type Marker = { id: string; seconds: number; label: string; kind: "recorded" | "editor" };
export type SourceReference = {
  id: string; original_path: string; relative_path: string | null; display_name: string;
  media_type: "wav" | "mp3" | "unknown"; role: "deck" | "sample" | "recording";
  duration: number | null; fingerprint: string | null;
};
export type SessionMetadata = { id: string; application: "SkyDek DJ"; generator: string };
export type ExportSettings = { format: "wav" | "mp3"; sample_rate: 48000; channels: 2; mp3_bitrate_kbps: 320 };
export type MigrationInfo = { imported_from: "legacy-json" | "native-dek"; warnings: string[] };

export type MixSession = {
  format: typeof MIX_SESSION_FORMAT; version: typeof MIX_SESSION_VERSION;
  name: string; created_at: string; modified_at: string; metadata: SessionMetadata;
  source: { kind: "legacy-log"; audio_file: string; log: MixLog };
  source_refs: SourceReference[]; edits: Edit[]; triggers: Trigger[]; markers: Marker[];
  relinks: Record<string, string>; export_settings: ExportSettings; migration: MigrationInfo;
  [key: string]: unknown;
};

const limits: Record<EditLane, [number, number]> = {
  crossfader: [-1, 1], master: [0, 1], sampler_volume: [0, 1],
  volume_0: [0, 1], volume_1: [0, 1], rate_0: [0.5, 2], rate_1: [0.5, 2],
  filter_0: [-1, 1], filter_1: [-1, 1], wet_0: [0, 1], wet_1: [0, 1],
  effect_0: [0, 5], effect_1: [0, 5],
};
const finite = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value);
const fileName = (path: string) => path.split(/[\\/]/).pop() || path;
const fileType = (path: string): SourceReference["media_type"] => {
  const extension = path.split(".").pop()?.toLowerCase();
  return extension === "wav" || extension === "mp3" ? extension : "unknown";
};
const stableId = (prefix: string, value: string, index: number) =>
  `${prefix}-${index}-${Array.from(value).reduce((hash, char) => (hash * 31 + char.charCodeAt(0)) >>> 0, 2166136261).toString(16)}`;

export function duration(session: MixSession) { return session.source.log.frames.at(-1)?.seconds ?? 0; }
export function timelineSeconds(clientX: number, left: number, width: number, total: number, snap = 0.05) {
  if (![clientX, left, width, total, snap].every(Number.isFinite) || width <= 0 || total < 0) return 0;
  const raw = Math.max(0, Math.min(total, ((clientX - left) / width) * total));
  return snap > 0 ? Math.max(0, Math.min(total, Math.round(raw / snap) * snap)) : raw;
}

export function visibleTimelineItems<T extends { id: string; seconds: number }>(
  items: T[], start: number, end: number, selected: string | null, maximum = 1400,
) {
  const visible = items.filter((item) => item.seconds >= start && item.seconds <= end);
  if (visible.length <= maximum) return visible;
  const step = Math.ceil(visible.length / maximum);
  const sampled = visible.filter((_, index) => index % step === 0);
  const selectedItem = selected ? visible.find((item) => item.id === selected) : undefined;
  if (selectedItem && !sampled.includes(selectedItem)) sampled.push(selectedItem);
  return sampled;
}

function deriveTriggers(log: MixLog): Trigger[] {
  const triggers: Trigger[] = [];
  let active = new Set<string>();
  for (const frame of log.frames) {
    const next = new Set(frame.samples);
    for (const source of next) if (!active.has(source)) triggers.push({
      id: stableId("sample", `${frame.seconds}:${source}`, triggers.length),
      seconds: frame.seconds, kind: "sample", source, gain: 0.65,
    });
    active = next;
  }
  return triggers;
}

function deriveSourceReferences(log: MixLog, triggers: Trigger[]): SourceReference[] {
  const roles = new Map<string, SourceReference["role"]>();
  for (const frame of log.frames) {
    for (const deck of frame.decks) if (deck.path) roles.set(deck.path, "deck");
    for (const sample of frame.samples) if (!roles.has(sample)) roles.set(sample, "sample");
  }
  for (const trigger of triggers) if (!roles.has(trigger.source)) roles.set(trigger.source, "sample");
  if (log.audio_file && !roles.has(log.audio_file)) roles.set(log.audio_file, "recording");
  return [...roles.entries()].map(([path, role], index) => ({
    id: stableId("media", path, index), original_path: path, relative_path: null,
    display_name: fileName(path), media_type: fileType(path), role,
    duration: null, fingerprint: null,
  }));
}
function deriveMarkers(log: MixLog): Marker[] {
  return log.events.map((event, index) => ({
    id: stableId("event", `${event.seconds}:${event.label}`, index),
    seconds: event.seconds, label: event.label, kind: "recorded",
  }));
}
const defaultExportSettings = (): ExportSettings => ({
  format: "wav", sample_rate: 48000, channels: 2, mp3_bitrate_kbps: 320,
});

export function importLegacy(text: string, name: string): MixSession {
  const log = parseMixLog(text);
  const now = new Date().toISOString();
  const triggers = deriveTriggers(log);
  const warnings = ["Build 01 recordings do not contain independent EQ-band, loop, or hot-cue identity automation."];
  if (log.frames.some((frame) => frame.decks.some((deck) => deck.cue > 0))) {
    warnings.push("Cue positions were retained, but individual hot-cue actions were not identified by Build 01.");
  }
  return {
    format: MIX_SESSION_FORMAT, version: MIX_SESSION_VERSION, name: name.trim() || "SkyDek Mix",
    created_at: now, modified_at: now,
    metadata: { id: crypto.randomUUID(), application: "SkyDek DJ", generator: "Build 02 Mix Editor" },
    source: { kind: "legacy-log", audio_file: log.audio_file, log },
    source_refs: deriveSourceReferences(log, triggers), edits: [], triggers,
    markers: deriveMarkers(log), relinks: {}, export_settings: defaultExportSettings(),
    migration: { imported_from: "legacy-json", warnings },
  };
}

function validateSourceReferences(value: unknown): SourceReference[] {
  if (!Array.isArray(value)) return [];
  const roles = new Set(["deck", "sample", "recording"]);
  const types = new Set(["wav", "mp3", "unknown"]);
  return value.map((item, index) => {
    if (!item || typeof item !== "object") throw new Error(`Media reference ${index + 1} is invalid.`);
    const source = item as Partial<SourceReference>;
    if (typeof source.id !== "string" || typeof source.original_path !== "string" ||
        typeof source.display_name !== "string" || !roles.has(String(source.role)) ||
        !types.has(String(source.media_type)) ||
        (source.duration !== null && source.duration !== undefined && (!finite(source.duration) || source.duration < 0))) {
      throw new Error(`Media reference ${index + 1} is invalid.`);
    }
    return {
      id: source.id, original_path: source.original_path,
      relative_path: typeof source.relative_path === "string" ? source.relative_path : null,
      display_name: source.display_name, media_type: source.media_type as SourceReference["media_type"],
      role: source.role as SourceReference["role"], duration: source.duration ?? null,
      fingerprint: typeof source.fingerprint === "string" ? source.fingerprint : null,
    };
  });
}

export function parseSession(text: string): MixSession {
  let parsed: unknown;
  try { parsed = JSON.parse(text); }
  catch (cause) { throw new Error(`This .dek file is not valid JSON: ${cause instanceof Error ? cause.message : cause}`); }
  if (!parsed || typeof parsed !== "object") throw new Error("This is not a SkyDek Mix Editor session.");
  const source = parsed as Partial<MixSession>;
  if (source.format !== MIX_SESSION_FORMAT) throw new Error("This file is not a SkyDek Mix Editor session.");
  if (source.version !== MIX_SESSION_VERSION) throw new Error(`Unsupported .dek schema version ${String(source.version)}. This build supports version ${MIX_SESSION_VERSION}.`);
  if (typeof source.name !== "string" || !source.name.trim() || source.name.length > 512 ||
      !source.source || source.source.kind !== "legacy-log" || !Array.isArray(source.edits)) {
    throw new Error("This .dek session is missing required metadata or performance data.");
  }
  const log = parseMixLog(JSON.stringify(source.source.log));
  const maximum = log.frames.at(-1)?.seconds ?? 0;
  const seen = new Set<string>();
  for (const edit of source.edits) {
    const bounds = edit && limits[edit.lane];
    if (!edit || typeof edit.id !== "string" || seen.has(edit.id) || !bounds || !finite(edit.seconds) ||
        edit.seconds < 0 || edit.seconds > maximum || !finite(edit.value) ||
        edit.value < bounds[0] || edit.value > bounds[1]) {
      throw new Error("The session has invalid automation or a duplicate automation identifier.");
    }
    seen.add(edit.id);
  }
  const triggers = Array.isArray(source.triggers) ? source.triggers : deriveTriggers(log);
  for (const trigger of triggers) {
    if (!trigger || typeof trigger.id !== "string" || seen.has(trigger.id) || trigger.kind !== "sample" ||
        typeof trigger.source !== "string" || !trigger.source || !finite(trigger.seconds) ||
        trigger.seconds < 0 || trigger.seconds > maximum || !finite(trigger.gain) ||
        trigger.gain < 0 || trigger.gain > 2) {
      throw new Error("The session has an invalid or duplicate sample trigger.");
    }
    seen.add(trigger.id);
  }
  const relinks = source.relinks && typeof source.relinks === "object" && !Array.isArray(source.relinks)
    ? Object.fromEntries(Object.entries(source.relinks).filter(([from, to]) => from && typeof to === "string" && to)) : {};
  const markers = Array.isArray(source.markers) ? source.markers.map((marker, index) => {
    if (!marker || typeof marker.id !== "string" || typeof marker.label !== "string" || !finite(marker.seconds) ||
        marker.seconds < 0 || marker.seconds > maximum || (marker.kind !== "recorded" && marker.kind !== "editor")) {
      throw new Error(`Marker ${index + 1} is invalid.`);
    }
    return marker;
  }) : deriveMarkers(log);
  const sourceRefs = source.source_refs === undefined ? deriveSourceReferences(log, triggers) : validateSourceReferences(source.source_refs);
  const metadata = source.metadata && typeof source.metadata.id === "string"
    ? source.metadata : { id: crypto.randomUUID(), application: "SkyDek DJ" as const, generator: "Build 02 Mix Editor" };
  const exportSettings = source.export_settings ?? defaultExportSettings();
  if (exportSettings.format !== "wav" && exportSettings.format !== "mp3") throw new Error("The session export format is invalid.");
  return {
    ...source, format: MIX_SESSION_FORMAT, version: MIX_SESSION_VERSION, name: source.name,
    metadata, source: { ...source.source, log }, source_refs: sourceRefs, edits: source.edits,
    triggers, markers, relinks, export_settings: { ...defaultExportSettings(), ...exportSettings },
    migration: source.migration ?? { imported_from: "native-dek", warnings: [] },
  } as MixSession;
}

export function reconstructed(session: MixSession, seconds: number): Frame {
  const at = Math.max(0, Math.min(duration(session), seconds));
  const base = frameAt(session.source.log.frames, at);
  if (!base) throw new Error("Session has no recorded frames.");
  const frame: Frame = { ...base, decks: [{ ...base.decks[0] }, { ...base.decks[1] }], samples: [...base.samples] };
  for (const edit of [...session.edits].sort((a, b) => a.seconds - b.seconds || a.id.localeCompare(b.id))) {
    if (edit.seconds > at) break;
    if (edit.lane === "crossfader" || edit.lane === "master" || edit.lane === "sampler_volume") frame[edit.lane] = edit.value;
    else if (edit.lane === "effect_0" || edit.lane === "effect_1") frame.decks[Number(edit.lane.at(-1))].effect = Math.round(edit.value);
    else {
      const [key, which] = edit.lane.split("_") as ["volume" | "rate" | "filter" | "wet", "0" | "1"];
      frame.decks[Number(which)][key] = edit.value;
    }
  }
  return frame;
}

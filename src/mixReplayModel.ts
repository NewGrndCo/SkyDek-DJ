export type DeckFrame = { path: string | null; position: number; playing: boolean; rate: number; volume: number; filter: number; effect: number; wet: number; peak: number; cue: number };
export type Frame = { seconds: number; decks: [DeckFrame, DeckFrame]; crossfader: number; master: number; sampler_volume: number; samples: string[] };
export type MixLog = { version: 1; audio_file: string; frames: Frame[]; events: { seconds: number; label: string }[] };

const finite = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value);

export function parseMixLog(text: string): MixLog {
  const payload = text.trim().startsWith("{") ? text : text.match(/```(?:json)?\s*([\s\S]*?)```/i)?.[1];
  if (!payload) throw new Error("Select a SkyDek JSON log or Markdown file containing its JSON block.");
  const log: unknown = JSON.parse(payload);
  if (!log || typeof log !== "object") throw new Error("This is not a supported SkyDek mix log.");
  const candidate = log as Partial<MixLog>;
  if (candidate.version !== 1 || typeof candidate.audio_file !== "string" ||
      !Array.isArray(candidate.frames) || !candidate.frames.length || candidate.frames.length > 144000 ||
      !Array.isArray(candidate.events) || candidate.events.length > 144000) {
    throw new Error("This is not a supported SkyDek mix log.");
  }
  let previous = -1;
  for (const frame of candidate.frames) {
    if (!finite(frame?.seconds) || frame.seconds < 0 || frame.seconds < previous || frame.seconds > 6 * 60 * 60 || !Array.isArray(frame.decks) || frame.decks.length !== 2 ||
        !frame.decks.every((deck: DeckFrame) => deck && (deck.path === null || typeof deck.path === "string") &&
          finite(deck.position) && typeof deck.playing === "boolean" &&
          [deck.rate, deck.volume, deck.filter, deck.effect, deck.wet, deck.peak, deck.cue].every(finite) &&
          deck.position >= 0 && deck.cue >= 0 && deck.rate >= 0.5 && deck.rate <= 2 &&
          deck.volume >= 0 && deck.volume <= 1 && deck.filter >= -1 && deck.filter <= 1 &&
          deck.effect >= 0 && deck.effect <= 5 && deck.wet >= 0 && deck.wet <= 1) ||
        ![frame.crossfader, frame.master, frame.sampler_volume].every(finite) ||
        frame.crossfader < -1 || frame.crossfader > 1 || frame.master < 0 || frame.master > 1 ||
        frame.sampler_volume < 0 || frame.sampler_volume > 1 ||
        !Array.isArray(frame.samples) || !frame.samples.every((sample: unknown) => typeof sample === "string")) {
      throw new Error("The mix log contains an invalid or out-of-order frame.");
    }
    previous = frame.seconds;
  }
  const duration = candidate.frames.at(-1)?.seconds ?? 0;
  if (!candidate.events.every(event => event && finite(event.seconds) && event.seconds >= 0 && event.seconds <= duration && typeof event.label === "string")) {
    throw new Error("The mix log contains an invalid event.");
  }
  return candidate as MixLog;
}

export function frameAt(frames: Frame[], seconds: number): Frame | null {
  if (!frames.length) return null;
  let lo = 0, hi = frames.length - 1;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (frames[mid].seconds <= seconds) lo = mid;
    else hi = mid - 1;
  }
  return frames[lo];
}

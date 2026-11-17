export type DeckFrame = { path: string | null; position: number; playing: boolean; rate: number; volume: number; filter: number; effect: number; wet: number; peak: number; cue: number };
export type Frame = { seconds: number; decks: [DeckFrame, DeckFrame]; crossfader: number; master: number; sampler_volume: number; samples: string[]; mic?: { volume: number; mute: boolean; ducking: boolean } | null };
export type SampleCommand = { seconds: number; path: string; action: "press" | "release"; mode: "restart" | "toggle" | "overlap" | "hold" };
export type EffectCommand = { seconds: number; deck: number; effect: number; active: boolean };
export type MicCommand = { seconds: number; state: { volume: number; mute: boolean; ducking: boolean } };
export type MixLog = { version: 1; capture_revision?: number; audio_file: string; frames: Frame[]; events: { seconds: number; label: string }[]; sample_commands?: SampleCommand[]; effect_commands?: EffectCommand[]; mic_file?: string | null; mic_commands?: MicCommand[] };

const finite = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value);

export function parseMixLog(text: string): MixLog {
  const payload = text.trim().startsWith("{") ? text : text.match(/```(?:json)?\s*([\s\S]*?)```/i)?.[1];
  if (!payload) throw new Error("Select a SkyDek JSON log or Markdown file containing its JSON block.");
  const log: unknown = JSON.parse(payload);
  if (!log || typeof log !== "object") throw new Error("This is not a supported SkyDek mix log.");
  const candidate = log as Partial<MixLog>;
  if (candidate.version !== 1 || (candidate.capture_revision !== undefined && candidate.capture_revision !== 0 && candidate.capture_revision !== 1) || typeof candidate.audio_file !== "string" ||
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
        !Array.isArray(frame.samples) || !frame.samples.every((sample: unknown) => typeof sample === "string") ||
        (frame.mic !== undefined && frame.mic !== null && (!finite(frame.mic.volume) || frame.mic.volume < 0 || frame.mic.volume > 1 || typeof frame.mic.mute !== "boolean" || typeof frame.mic.ducking !== "boolean"))) {
      throw new Error("The mix log contains an invalid or out-of-order frame.");
    }
    previous = frame.seconds;
  }
  const duration = candidate.frames.at(-1)?.seconds ?? 0;
  if (!candidate.events.every(event => event && finite(event.seconds) && event.seconds >= 0 && event.seconds <= duration && typeof event.label === "string")) {
    throw new Error("The mix log contains an invalid event.");
  }
  if (candidate.sample_commands !== undefined && (!Array.isArray(candidate.sample_commands) || candidate.sample_commands.length > 500000 || !candidate.sample_commands.every(event => event && finite(event.seconds) && event.seconds >= 0 && event.seconds <= duration + 0.2 && typeof event.path === "string" && event.path.length > 0 && ["press", "release"].includes(event.action) && ["restart", "toggle", "overlap", "hold"].includes(event.mode)))) {
    throw new Error("The mix log contains an invalid sample command.");
  }
  if (candidate.effect_commands !== undefined && (!Array.isArray(candidate.effect_commands) || candidate.effect_commands.length > 500000 || !candidate.effect_commands.every(event => event && finite(event.seconds) && event.seconds >= 0 && event.seconds <= duration + 0.2 && (event.deck === 0 || event.deck === 1) && Number.isInteger(event.effect) && event.effect >= 0 && event.effect <= 5 && typeof event.active === "boolean"))) {
    throw new Error("The mix log contains an invalid effect command.");
  }
  if (candidate.mic_file !== undefined && candidate.mic_file !== null && typeof candidate.mic_file !== "string") throw new Error("The mix log microphone source is invalid.");
  if (candidate.mic_commands !== undefined && (!Array.isArray(candidate.mic_commands) || candidate.mic_commands.length > 500000 || !candidate.mic_commands.every(command => command && finite(command.seconds) && command.seconds >= 0 && command.seconds <= duration + 0.2 && command.state && finite(command.state.volume) && command.state.volume >= 0 && command.state.volume <= 1 && typeof command.state.mute === "boolean" && typeof command.state.ducking === "boolean"))) {
    throw new Error("The mix log contains an invalid microphone command.");
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

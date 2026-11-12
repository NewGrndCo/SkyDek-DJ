/** Visual prediction only. Every received audio snapshot replaces the anchor. */
export type PlaybackSnapshot = {
  position: number; playing?: boolean; rate?: number; effect?: number;
};
export type PlaybackClock = {
  track: string; position: number; duration: number; receivedAt: number;
  playing: boolean; rate: number; effect: number; velocity: number;
};
export const MAX_PREDICTION_MS = 150;
const finite = (n: number, fallback = 0) => Number.isFinite(n) ? n : fallback;
const bound = (n: number, min: number, max: number) => Math.min(max, Math.max(min, n));

export function anchorPlayback(previous: PlaybackClock | null, snapshot: PlaybackSnapshot,
  track: string, duration: number, now: number): PlaybackClock {
  duration = Math.max(0, finite(duration));
  const position = bound(finite(snapshot.position), 0, duration);
  const rate = bound(finite(snapshot.rate ?? 1, 1), .5, 2);
  const effect = snapshot.effect ?? 0;
  const playing = snapshot.playing === true && duration > 0;
  let velocity = playing ? rate : 0;
  if (playing && effect >= 1 && effect <= 3) {
    // Engine effect phase is not exposed. Infer local speed only from adjacent,
    // compatible observations; never carry a seek/load delta into prediction.
    velocity = 0;
    const elapsed = previous ? (now - previous.receivedAt) / 1000 : 0;
    if (previous?.track === track && previous.playing && previous.effect === effect &&
        previous.duration === duration && previous.rate === rate && elapsed > 0 && elapsed <= .15) {
      const observed = (position - previous.position) / elapsed;
      // Backspin's wet-controlled throw can reach six times the deck rate.
      if (effect === 1 && observed <= 0 && observed >= -rate * 6.1) velocity = Math.max(-rate * 6, observed);
      if (effect !== 1 && observed >= 0 && observed <= rate * 1.1) velocity = Math.min(rate, observed);
    }
  }
  return { track, position, duration, receivedAt: finite(now), playing, rate, effect, velocity };
}

export function playbackPosition(clock: PlaybackClock | null, now: number): number {
  if (!clock) return 0;
  const elapsed = bound(finite(now - clock.receivedAt), 0, MAX_PREDICTION_MS) / 1000;
  return bound(clock.position + (clock.playing ? elapsed * clock.velocity : 0), 0, clock.duration);
}

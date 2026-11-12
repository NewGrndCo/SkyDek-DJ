import { useEffect, useRef, useState, type CSSProperties } from "react";

type DeckStatus = {
  playing: boolean;
  previewing: boolean;
  looping: boolean;
  position: number;
  duration: number;
  rate: number;
  volume: number;
  end_events: number;
};

export function DeckGlow({ deck, index, path, crossfader, reducedMotion }: {
  deck?: DeckStatus;
  index: number;
  path?: string;
  crossfader: number;
  reducedMotion: boolean;
}) {
  const previous = useRef({ path, events: deck?.end_events ?? 0 });
  const [finished, setFinished] = useState(false);
  useEffect(() => {
    const events = deck?.end_events ?? 0;
    const ended = previous.current.path === path && events !== previous.current.events;
    previous.current = { path, events };
    setFinished(ended);
    if (!ended) return;
    const timer = window.setTimeout(() => setFinished(false), 1800);
    return () => window.clearTimeout(timer);
  }, [path, deck?.end_events]);

  const playing = Boolean(path && deck?.playing && !deck.previewing);
  const remaining = deck ? (deck.duration - deck.position) / Math.max(.01, deck.rate) : Infinity;
  const ending = playing && !deck?.looping && remaining > 0 && remaining <= 10;
  const mix = Math.max(0, Math.min(1, (crossfader + 1) / 2));
  const contribution = index === 0 ? 1 - mix : mix;
  const opacity = playing ? .25 + .65 * contribution * (deck?.volume ?? 0) : finished ? .8 : 0;
  return (
    <div
      className={["deck-glow", ending && "ending", finished && "finished", reducedMotion && "still"].filter(Boolean).join(" ")}
      style={{ "--glow-opacity": opacity } as CSSProperties}
      aria-hidden="true"
    />
  );
}

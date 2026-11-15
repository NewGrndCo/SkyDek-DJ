import { useEffect, useRef, useState } from "react";

type Props = {
  ready: boolean;
  connected: boolean;
  reducedMotion: boolean;
  onFinished: () => void;
};

export function SignalLockSplash({ ready, connected, reducedMotion, onFinished }: Props) {
  const started = useRef(performance.now());
  const finishRef = useRef(onFinished);
  const [exiting, setExiting] = useState(false);

  useEffect(() => {
    finishRef.current = onFinished;
  }, [onFinished]);

  useEffect(() => {
    if (!ready) return;
    const minimum = reducedMotion ? 250 : 1400;
    const wait = Math.max(0, minimum - (performance.now() - started.current));
    const exitTimer = window.setTimeout(() => setExiting(true), wait);
    const finishTimer = window.setTimeout(() => finishRef.current(), wait + (reducedMotion ? 20 : 460));
    return () => {
      window.clearTimeout(exitTimer);
      window.clearTimeout(finishTimer);
    };
  }, [ready, reducedMotion]);

  useEffect(() => {
    let finishTimer: number | undefined;
    const failSafe = window.setTimeout(() => {
      setExiting(true);
      finishTimer = window.setTimeout(() => finishRef.current(), reducedMotion ? 20 : 460);
    }, 7000);
    return () => {
      window.clearTimeout(failSafe);
      if (finishTimer !== undefined) window.clearTimeout(finishTimer);
    };
  }, [reducedMotion]);

  const status = ready
    ? connected
      ? "Xbox controller linked"
      : "Audio engine ready"
    : "Initializing audio engine";

  return (
    <div
      className={`signal-lock ${exiting ? "signal-lock-exit" : ""}`}
      data-motion={reducedMotion ? "reduced" : "full"}
      role="status"
      aria-live="polite"
      aria-label={`SkyDek DJ startup. ${status}.`}
    >
      <div className="signal-lock-grid" aria-hidden="true" />
      <div className="signal-lock-glow" aria-hidden="true" />
      <div className="signal-lock-content">
        <div className="signal-lock-logo-wrap">
          <span className="signal-lock-ring" aria-hidden="true" />
          <img src="/brand.png" alt="SkyDek DJ" />
        </div>
        <div className="signal-lock-wave" aria-hidden="true">
          {Array.from({ length: 40 }, (_, index) => (
            <i
              key={index}
              style={{
                animationDelay: `${index * -38}ms`,
                height: `${18 + ((index * 17) % 54)}%`,
              }}
            />
          ))}
        </div>
        <div className="signal-lock-copy">
          <strong>{status}</strong>
          <span><b className={ready ? "locked" : ""} /> SIGNAL {ready ? "LOCKED" : "SEARCHING"}</span>
        </div>
        <div className="signal-lock-progress" aria-hidden="true"><span /></div>
        <small>NEW GROUND MEDIA</small>
      </div>
    </div>
  );
}

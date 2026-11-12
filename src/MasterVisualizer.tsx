import { useEffect, useState } from "react";

const modes = ["Meter", "History bars", "Level trace", "Pulse"] as const;
type Mode = typeof modes[number];

export function MasterVisualizer({ peak }: { peak: number }) {
  const [mode, setMode] = useState<Mode>("Meter");
  const [history, setHistory] = useState<number[]>(() => Array(24).fill(0));
  const level = Math.max(0, Math.min(1, peak));
  useEffect(() => {
    if (mode === "Meter" || mode === "Pulse") return;
    const timer = window.setTimeout(() => setHistory(previous => [...previous.slice(1), level]), 40);
    return () => window.clearTimeout(timer);
  }, [level, mode]);

  return <div className="master-visualizer">
    {mode === "Meter" ? <div className="meter" aria-hidden="true">
      <div style={{ height: `${level * 100}%`, background: peak > 1 ? "#ff465c" : undefined }} />
    </div> : <svg viewBox="0 0 120 54" width="100%" height="54" aria-hidden="true">
      {mode === "History bars" && history.map((value, index) =>
        <rect key={index} x={index * 5} y={52 - value * 48} width="3" height={Math.max(1, value * 48)} fill="currentColor" />)}
      {mode === "Level trace" && <polyline fill="none" stroke="currentColor" strokeWidth="2"
        points={history.map((value, index) => `${index * 5},${52 - value * 48}`).join(" ")} />}
      {mode === "Pulse" && <circle cx="60" cy="27" r={5 + level * 20}
        fill="currentColor" fillOpacity={0.2 + level * 0.8} />}
    </svg>}
    <div className="visualizer-controls">
      <select aria-label="Master visualizer" value={mode} onChange={event => setMode(event.target.value as Mode)}>
        {modes.map(name => <option key={name}>{name}</option>)}
      </select>
      <button type="button" title="Random visualizer" aria-label="Random visualizer" onClick={() => {
        const choices = modes.filter(name => name !== mode);
        setMode(choices[Math.floor(Math.random() * choices.length)]);
      }}>⤨</button>
    </div>
  </div>;
}

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isEssentialMapping, mappingLabel, type ControllerMappings } from "./mappingLabels";

type Input = {
  connected: boolean; buttons: Record<string, boolean>;
  left_stick_x: number; left_stick_y: number; right_stick_x: number; right_stick_y: number;
  left_trigger: number; right_trigger: number;
};
const hotspots: [string, number, number, number][] = [
  ["a",72.8,44,6.3], ["b",79,34.8,6.3], ["x",67.1,34.8,6.3], ["y",72.8,25.9,6.3],
  ["view",43,33.8,4.4], ["menu",57,33.8,4.4],
  ["dpad-up",38.2,50.6,5], ["dpad-down",38.2,63.5,5],
  ["dpad-left",34,57,5], ["dpad-right",42.4,57,5],
];
type Props = {
  onSelect: (control: string) => void;
  mappings: ControllerMappings;
  layer: "normal" | "shift";
};

export function ControllerMonitor({ onSelect, mappings, layer }: Props) {
  const [input, setInput] = useState<Input | null>(null);
  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try { const next = await invoke<Input>("controller_input"); if (active) setInput(next); }
      catch { if (active) setInput(null); }
      if (active) timer = setTimeout(poll, 40);
    };
    void poll();
    return () => { active = false; clearTimeout(timer); };
  }, []);
  return <section className="controller-monitor" aria-label="Live Xbox controller input">
    <strong>LIVE INPUT · {input?.connected ? "Xbox connected" : "Controller disconnected"} · {layer.toUpperCase()}</strong>
    <div className="controller-shoulders">{["lt","lb","rb","rt"].map(control => {
      const amount = control === "lt" ? input?.left_trigger : control === "rt" ? input?.right_trigger : input?.buttons[control] ? 1 : 0;
      const action = mappings[layer][control];
      return <button key={control} className={`${amount && amount > .06 ? "lit" : ""} ${isEssentialMapping(action) ? "essential" : ""}`} onClick={() => onSelect(control)}><b>{control.toUpperCase()}{control.endsWith("t") ? ` ${Math.round((amount || 0) * 100)}%` : ""}</b><span>{mappingLabel(action)}</span></button>;
    })}</div>
    <div className="xbox-art">
      <img src="/xbox-series-controller-reference.png" alt="White Xbox Series controller with live button highlights" />
      {hotspots.map(([control,x,y,size]) => {
        const action = mappings[layer][control];
        return <div className={`controller-control control-${control}`} key={control} style={{left:`${x}%`,top:`${y}%`}}><button title={`Edit ${control}: ${mappingLabel(action)}`} aria-label={`Edit ${control}: ${mappingLabel(action)}`} className={`xbox-glow ${input?.buttons[control] ? "lit" : ""}`} style={{width:`${size}cqw`}} onClick={() => onSelect(control)} /><span className={isEssentialMapping(action) ? "essential" : ""}><b>{control.toUpperCase()}</b> {mappingLabel(action)}</span></div>;
      })}
      {(["left","right"] as const).map((side, i) => {
        const x = input?.[`${side}_stick_x`] || 0, y = input?.[`${side}_stick_y`] || 0;
        const control = i === 0 ? "l3" : "r3";
        const action = mappings[layer][control];
        return <div className={`controller-control control-${control}`} key={side} style={{left:`${(i === 0 ? 27.5 : 61.7) + x * 1.8}%`,top:`${(i === 0 ? 35.2 : 56.2) - y * 2.7}%`}}><button title={`Edit ${control}: ${mappingLabel(action)}`} aria-label={`Edit ${control}: ${mappingLabel(action)}`} className={`xbox-glow stick ${Math.abs(x) + Math.abs(y) > .12 || input?.buttons[control] ? "lit" : ""}`} onClick={() => onSelect(control)} /><span className={isEssentialMapping(action) ? "essential" : ""}><b>{control.toUpperCase()}</b> {mappingLabel(action)}</span></div>;
      })}
    </div>
    <small>Click a controller button to edit its assignment. Share is reserved by Windows.</small>
  </section>;
}

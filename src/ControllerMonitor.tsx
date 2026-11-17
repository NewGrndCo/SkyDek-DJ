import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isEssentialMapping, mappingLabel, type ControllerMappings } from "./mappingLabels";
import { readCalloutPreference, saveCalloutPreference } from "./controllerPreferences";
import { controlIntensity, stickDisplacement } from "./controllerVisualState";

type Input = {
  connected: boolean; buttons: Record<string, boolean>;
  left_stick_x: number; left_stick_y: number; right_stick_x: number; right_stick_y: number;
  left_trigger: number; right_trigger: number;
};
type Callout = { control: string; x: number; y: number; size: number; side: "left" | "right"; labelY: number };
const callouts: Callout[] = [
  { control:"lt", x:29, y:10, size:7, side:"left", labelY:8 }, { control:"lb", x:31, y:18, size:7, side:"left", labelY:19 },
  { control:"l3", x:27.5, y:35.2, size:11, side:"left", labelY:30 }, { control:"view", x:43, y:33.8, size:4.4, side:"left", labelY:41 },
  { control:"dpad-up", x:38.2, y:50.6, size:5, side:"left", labelY:52 }, { control:"dpad-left", x:34, y:57, size:5, side:"left", labelY:63 },
  { control:"dpad-down", x:38.2, y:63.5, size:5, side:"left", labelY:74 }, { control:"dpad-right", x:42.4, y:57, size:5, side:"left", labelY:85 },
  { control:"rt", x:71, y:10, size:7, side:"right", labelY:8 }, { control:"rb", x:69, y:18, size:7, side:"right", labelY:19 },
  { control:"y", x:72.8, y:25.9, size:6.3, side:"right", labelY:30 }, { control:"x", x:67.1, y:34.8, size:6.3, side:"right", labelY:41 },
  { control:"b", x:79, y:34.8, size:6.3, side:"right", labelY:52 }, { control:"a", x:72.8, y:44, size:6.3, side:"right", labelY:63 },
  { control:"menu", x:57, y:33.8, size:4.4, side:"right", labelY:74 }, { control:"r3", x:61.7, y:56.2, size:11, side:"right", labelY:85 },
];
const imageToStage = (value: number, axis: "x" | "y") => axis === "x" ? 19 + value * .62 : 8 + value * .807;
type Props = { onSelect:(control:string)=>void; mappings:ControllerMappings; layer:"normal"|"shift"; active:boolean };

export function ControllerMonitor({ onSelect, mappings, layer, active }: Props) {
  const [input,setInput]=useState<Input|null>(null);
  const [showCallouts,setShowCallouts]=useState(()=>readCalloutPreference(localStorage));
  useEffect(()=>{if(!active)return;let mounted=true;let timer:ReturnType<typeof setTimeout>;let previous="";const poll=async()=>{try{const next=await invoke<Input>("controller_input");const serialized=JSON.stringify(next);if(mounted&&serialized!==previous){previous=serialized;setInput(next);}}catch{if(mounted&&previous!=="null"){previous="null";setInput(null);}}if(mounted)timer=setTimeout(poll,40);};void poll();return()=>{mounted=false;clearTimeout(timer);};},[active]);
  const amountFor=(control:string)=>controlIntensity(input,control);
  const toggleCallouts=()=>setShowCallouts(current=>{const next=!current;saveCalloutPreference(localStorage,next);return next;});
  return <section className="controller-monitor" aria-label="Live Xbox controller input">
    <div className="controller-monitor-heading"><strong>LIVE INPUT · {input?.connected?"Xbox connected":"Controller disconnected"} · {layer.toUpperCase()}</strong><button className={showCallouts?"active":""} type="button" aria-pressed={showCallouts} onClick={toggleCallouts}>Mapping labels {showCallouts?"on":"off"}</button></div>
    <div className={`controller-diagram ${showCallouts?"show-callouts":"hide-callouts"}`}>
      {showCallouts&&<svg className="controller-callout-lines" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">{callouts.map(({control,x,y,side,labelY})=>{const [stickX,stickY]=stickDisplacement(input,control);const anchorX=imageToStage(x+stickX*1.8,"x"),anchorY=imageToStage(y-stickY*2.7,"y"),edgeX=side==="left"?18:82,labelX=side==="left"?12.5:87.5;return <polyline key={control} className={`${amountFor(control)>.06?"lit":""} ${isEssentialMapping(mappings[layer][control])?"essential":""}`} points={`${anchorX},${anchorY} ${edgeX},${labelY} ${labelX},${labelY}`}/>;})}</svg>}
      <div className="controller-image"><img src="/xbox-series-controller-reference.png" alt="White Xbox Series controller with live button highlights"/>{callouts.map(({control,x,y,size})=>{const action=mappings[layer][control];const [stickX,stickY]=stickDisplacement(input,control);const active=amountFor(control)>.06||Math.abs(stickX)+Math.abs(stickY)>.12;return <button key={control} title={`Edit ${control}: ${mappingLabel(action)}`} aria-label={`Edit ${control}: ${mappingLabel(action)}`} className={`controller-hotspot xbox-glow ${control==="l3"||control==="r3"?"stick":""} ${active?"lit":""}`} style={{left:`${x+stickX*1.8}%`,top:`${y-stickY*2.7}%`,width:`${size}cqw`}} onClick={()=>onSelect(control)}/>;})}</div>
      {showCallouts&&callouts.map(({control,side,labelY})=>{const action=mappings[layer][control],trigger=control==="lt"||control==="rt";return <button type="button" key={control} className={`controller-callout ${side} ${amountFor(control)>.06?"lit":""} ${isEssentialMapping(action)?"essential":""}`} style={{top:`${labelY}%`}} onClick={()=>onSelect(control)} title={`Edit ${control}: ${mappingLabel(action)}`}><b>{control.toUpperCase()}{trigger?` ${Math.round(amountFor(control)*100)}%`:""}</b><span>{mappingLabel(action)}</span></button>;})}
    </div>
    <small>Click a button or its callout to edit the assignment. Share is reserved by Windows.</small>
  </section>;
}

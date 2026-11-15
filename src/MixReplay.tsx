import { useEffect, useMemo, useRef, useState } from "react";
import { frameAt, parseMixLog, type MixLog } from "./mixReplayModel";
import "./mix-replay.css";

const name = (path: string | null) => path?.replace(/\\/g,"/").split("/").pop() || "No track";
const clock = (seconds: number) => `${Math.floor(seconds/60)}:${Math.floor(seconds%60).toString().padStart(2,"0")}`;
const effects = ["Off","Backspin","Brake","Brake start","Echo","Reverb"];
export function MixReplay({onClose}: {onClose:()=>void}) {
  const [log,setLog]=useState<MixLog|null>(null), [audioUrl,setAudioUrl]=useState<string|null>(null), [audioName,setAudioName]=useState(""), [seconds,setSeconds]=useState(0), [playing,setPlaying]=useState(false), [error,setError]=useState("");
  const audio=useRef<HTMLAudioElement>(null);
  useEffect(()=>()=>{ if(audioUrl) URL.revokeObjectURL(audioUrl); },[audioUrl]);
  useEffect(()=>{
    if (!playing) return;
    let id: number;
    let last = 0;
    const tick=(now: number)=>{ if(now-last>=33 && audio.current){setSeconds(audio.current.currentTime);last=now;} id=requestAnimationFrame(tick); };
    id=requestAnimationFrame(tick);
    return ()=>cancelAnimationFrame(id);
  },[playing]);
  const frame=useMemo(()=>log ? frameAt(log.frames,seconds) : null,[log,seconds]);
  const recent=useMemo(()=>log?.events.filter(e=>e.seconds<=seconds).slice(-8) || [],[log,seconds]);
  return <div className="mix-replay-backdrop"><section className="mix-replay" role="dialog" aria-modal="true" aria-label="Mix replay">
    <div className="mix-replay-head"><div><small>SKYDEK DJ</small><h2>Mix replay</h2></div><button onClick={onClose} aria-label="Close replay">×</button></div>
    <p>Import the JSON log saved beside your recording, then choose its MP3 or WAV file. Playback follows the recording while the controls show the captured mix.</p>
    <div className="mix-replay-files"><label>Mix log (.json or .md)<input type="file" accept=".json,.md,application/json,text/markdown" onChange={async e=>{const file=e.target.files?.[0];if(!file)return;try{if(file.size>64*1024*1024)throw new Error("Mix log is too large.");setLog(parseMixLog(await file.text()));setSeconds(0);setError("");}catch(error){setLog(null);setError(String(error));}}}/></label>
      <label>Recorded audio<input type="file" accept=".mp3,.wav,audio/*" onChange={e=>{const file=e.target.files?.[0];if(!file)return;setAudioName(file.name);setAudioUrl(URL.createObjectURL(file));setSeconds(0);setPlaying(false);setError(file.size ? "" : "The selected recording is empty.");}}/></label></div>
    {log && <p>Expected audio: <strong>{name(log.audio_file)}</strong>{audioName && name(log.audio_file).toLowerCase()!==audioName.toLowerCase() ? " · Check that you selected the matching recording." : ""}</p>}
    {audioUrl && <audio ref={audio} controls preload="metadata" src={audioUrl} onPlay={()=>setPlaying(true)} onPause={()=>setPlaying(false)} onEnded={()=>setPlaying(false)} onLoadedMetadata={()=>setError("")} onTimeUpdate={e=>setSeconds(e.currentTarget.currentTime)} onSeeked={e=>setSeconds(e.currentTarget.currentTime)} onError={e=>{setPlaying(false);setError(`The recording could not be loaded (media error ${e.currentTarget.error?.code ?? "unknown"}). Check that it is the matching MP3 or WAV file.`);}}/>}
    {log && audioUrl && !error && <p className="mix-replay-hint">Press play or drag the audio timeline. The deck and mixer controls follow the recording.</p>}
    <div className="mix-replay-time">{clock(seconds)} {log ? `/ ${clock(log.frames.at(-1)?.seconds || 0)}` : ""}</div>
    <div className="mix-replay-decks">{[0,1].map(i=>{const d=frame?.decks[i];return <article key={i} className={playing && d?.playing ? "playing" : ""}><small>DEK {i+1}</small><h3 title={d?.path || ""}>{name(d?.path || null)}</h3><div className="mix-replay-disc" style={{animationPlayState:playing && d?.playing?"running":"paused"}}>S</div><p>{d ? clock(d.position) : "0:00"} · {d?.playing ? "PLAYING" : "PAUSED"} · {effects[d?.effect || 0]}</p><div className="mix-replay-meter"><i style={{width:`${Math.min(100,Math.max(0,(d?.peak||0)*100))}%`}}/></div><div className="mix-replay-effect-list">{effects.slice(1).map((effect,index)=><span key={effect} className={d?.effect===index+1?"active":""}>{effect}</span>)}</div><div className="mix-replay-values"><span>Filter <b>{Math.round((d?.filter||0)*100)}%</b></span><span>Level <b>{Math.round((d?.volume||0)*100)}%</b></span><span>FX mix <b>{Math.round((d?.wet||0)*100)}%</b></span><span>Tempo <b>{d?.rate?.toFixed(2) || "1.00"}×</b></span><span>Cue <b>{clock(d?.cue||0)}</b></span></div></article>})}</div>
    <div className="mix-replay-controls"><span>Crossfader</span><div className="mix-replay-cross"><i style={{left:`${((frame?.crossfader||0)+1)*50}%`}}/></div><span>{Math.round((frame?.crossfader||0)*100)}%</span></div>
    <div className="mix-replay-values"><span>Master <b>{Math.round((frame?.master||0)*100)}%</b></span><span>Sampler <b>{Math.round((frame?.sampler_volume||0)*100)}%</b></span></div>
    <div className="mix-replay-samples">Active samples: {frame?.samples.map(name).join(", ") || "None"}</div>
    <div className="mix-replay-events">{recent.map((event,index)=><div key={`${event.seconds}-${index}`}><time>{clock(event.seconds)}</time> {event.label}</div>)}</div>
    {error && <p role="alert">{error}</p>}
  </section></div>;
}

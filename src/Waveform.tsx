import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { clamp, dragSeek, clickSeek } from "./workspaceMath";
import { anchorPlayback, playbackPosition, type PlaybackClock, type PlaybackSnapshot } from "./playbackClock";
type Track = { path:string; duration:number; waveform:number[]; bpm:number|null };
type Deck = PlaybackSnapshot & { cue:number; hot:(number|null)[] };
type Peaks = [number,number,number][];
const cache = new Map<string,Promise<Peaks>>();

export function Wave({track,deck,overview=false,onSeek,span:sharedSpan,onSpan,skin}: {
  track?:Track; deck?:Deck; overview?:boolean; onSeek?:(seconds:number, dragged?:boolean)=>void;
  span?:number; onSpan?:(seconds:number)=>void; skin: string;
}) {
  const canvas=useRef<HTMLCanvasElement>(null), [peaks,setPeaks]=useState<{path:string;data:Peaks}|null>(null);
  const [localSpan,setLocalSpan]=useState<number|null>(null), [size,setSize]=useState(0);
  const drag=useRef<{x:number;position:number;start:number;span:number;moved:boolean}|null>(null);
  const pending=useRef<number|null>(null), frame=useRef<number|null>(null);
  const clock=useRef<PlaybackClock|null>(null), visible=useRef({position:0,start:0});
  const latest=useRef({track,deck}); latest.current={track,deck};
  const duration=track?.duration || 0;
  useLayoutEffect(()=>{
    clock.current=track&&deck ? anchorPlayback(clock.current,deck,track.path,duration,performance.now()) : null;
  },[deck,track?.path,duration]);
  const span=overview ? Math.min(duration || 1, localSpan ?? (duration || 1)) : sharedSpan ?? 16;
  const zoom=(factor:number) => {
    const value=clamp(span*factor,.5,overview ? Math.max(.5,duration) : 128);
    if(overview) setLocalSpan(value); else onSpan?.(value);
  };
  const zoomRef=useRef(zoom); zoomRef.current=zoom;
  useEffect(() => {
    drag.current=null;pending.current=null;if(frame.current!==null){cancelAnimationFrame(frame.current);frame.current=null;}
    setLocalSpan(null); setPeaks(null);
    if(!track) return;
    if(!cache.has(track.path)) {
      if(cache.size>=8) cache.delete(cache.keys().next().value!);
      cache.set(track.path,invoke<Peaks>("waveform_data",{path:track.path}));
    }
    let active=true;
    cache.get(track.path)!.then(data=>{ if(active) setPeaks({path:track.path,data}); }).catch(()=>cache.delete(track.path));
    return ()=>{ active=false; };
  },[track?.path]);
  useEffect(()=>{
    const c=canvas.current; if(!c) return;
    const observer=new ResizeObserver(()=>setSize(n=>n+1)); observer.observe(c);
    const wheel=(e:WheelEvent)=>{ e.preventDefault(); zoomRef.current(e.deltaY>0 ? 1.25 : .8); };
    c.addEventListener("wheel",wheel,{passive:false});
    return ()=>{observer.disconnect(); c.removeEventListener("wheel",wheel); if(frame.current!==null) cancelAnimationFrame(frame.current);};
  },[]);
  useEffect(()=>{
    const c=canvas.current, ctx=c?.getContext("2d"); if(!c||!ctx) return;
    // The strip is three viewports, never a duration-sized canvas. Rebuild only
    // on zoom/resize/data changes or when playback leaves the cached region.
    const ratio=Math.min(window.devicePixelRatio || 1,3);
    const w=c.width=Math.max(1,Math.round(c.clientWidth*ratio));
    const h=c.height=Math.max(1,Math.round(c.clientHeight*ratio));
    const strip=document.createElement("canvas"); strip.width=w*3; strip.height=h;
    const ink=strip.getContext("2d"); if(!ink)return;
    let stripStart=NaN, animation=0;
    const detail=peaks && peaks.path===track?.path ? peaks.data : null;
    const secondDeck = Boolean(c.closest(".deck2,.redwave"));
    const color = skin === "xbox"
      ? (secondDeck ? "#39654b" : "#107c10")
      : (secondDeck ? "#e36783" : "#50bfd9");
    const light = skin !== "dark";
    const step=Math.max(1,ratio), secondsPerPixel=span/w;
    const rebuild=(start:number)=>{
      stripStart=Math.floor((start-span)/secondsPerPixel)*secondsPerPixel;
      ink.clearRect(0,0,strip.width,h);
      const fallback=latest.current.track?.waveform || [];
      const count=detail?.length || fallback.length;
      if(!count||!duration)return;
      const samples:Peaks=[];
      for(let x=0;x<=strip.width;x+=step){
        const sec=stripStart+x*secondsPerPixel, endSec=sec+step*secondsPerPixel;
        if(endSec<=0||sec>=duration){samples.push([0,0,0]);continue;}
        const first=clamp(Math.floor(sec/duration*count),0,count-1);
        const last=clamp(Math.ceil(endSec/duration*count),first+1,count);
        let lo=0,hi=0,rms=0;
        for(let i=first;i<last;i++){
          const p=detail?.[i];
          lo=Math.min(lo,p?.[0] ?? -fallback[i]);hi=Math.max(hi,p?.[1] ?? fallback[i]);rms=Math.max(rms,p?.[2] ?? fallback[i]*.5);
        }
        samples.push([lo,hi,rms]);
      }
      ink.fillStyle=color;ink.globalAlpha=.82;ink.beginPath();
      samples.forEach((p,i)=>{const x=i*step,y=h/2-p[1]*h*.43;if(i===0)ink.moveTo(x,y);else ink.lineTo(x,y);});
      for(let i=samples.length-1;i>=0;i--)ink.lineTo(i*step,h/2-samples[i][0]*h*.43);
      ink.closePath();ink.fill();
      ink.globalAlpha=.35;ink.fillStyle="#edf8ff";ink.beginPath();
      samples.forEach((p,i)=>{const x=i*step,y=h/2-p[2]*h*.34;if(i===0)ink.moveTo(x,y);else ink.lineTo(x,y);});
      for(let i=samples.length-1;i>=0;i--)ink.lineTo(i*step,h/2+samples[i][2]*h*.34);
      ink.closePath();ink.fill();ink.globalAlpha=1;
    };
    const draw=(now:number)=>{
      const {track:t,deck:d}=latest.current;
      const position=playbackPosition(clock.current,now);
      const start=overview ? clamp(position-span/2,0,Math.max(0,duration-span)) : position-span/2;
      visible.current={position,start};
      ctx.clearRect(0,0,w,h);
      ctx.strokeStyle=light?"#b4becc":"#2a323e";ctx.beginPath();ctx.moveTo(0,h/2);ctx.lineTo(w,h/2);ctx.stroke();
      if(t&&d&&duration){
        if(!Number.isFinite(stripStart)||start<stripStart||start+span>stripStart+span*3)rebuild(start);
        ctx.drawImage(strip,(stripStart-start)/secondsPerPixel,0);
        if(t.bpm&&Number.isFinite(t.bpm)&&t.bpm>0){
          const beat=60/t.bpm;
          // Keep corrupt BPM metadata from creating an unbounded drawing loop.
          if(span/beat<4096)for(let n=Math.ceil(Math.max(0,start)/beat);n*beat<Math.min(duration,start+span);n++){
            ctx.fillStyle=light?(n%4===0?"#4d5b70":"#8290a2"):(n%4===0?"#9ba8bc":"#576171");
            ctx.fillRect((n*beat-start)/span*w,0,ratio,(n%4===0?10:5)*ratio);
          }
        }
        [d.cue,...d.hot.slice(0,3)].forEach((p,i)=>{
          if(p==null)return;const x=(p-start)/span*w;if(x<0||x>w)return;
          ctx.fillStyle=skin==="xbox"?"#205723":light?"#7443a5":"#c8abff";ctx.fillRect(x,0,ratio,h);ctx.font=`${9*ratio}px Segoe UI`;ctx.fillText(i===0?"C":String(i),x+3*ratio,h-3*ratio);
        });
        ctx.fillStyle=light?"#243348":"#f0f5e7";ctx.fillRect((position-start)/span*w,0,Math.max(1,ratio),h);
      }
      animation=requestAnimationFrame(draw);
    };
    animation=requestAnimationFrame(draw);
    return ()=>cancelAnimationFrame(animation);
  },[track?.path,track?.bpm,track?.waveform.length,peaks,span,duration,size,overview,skin]);
  const send=(value:number)=>{
    pending.current=clamp(value,0,duration);
    if(frame.current===null)frame.current=requestAnimationFrame(()=>{frame.current=null;if(pending.current!==null)onSeek?.(pending.current);});
  };
  return <div className="waveform-view">
    <canvas ref={canvas} tabIndex={0} aria-label={overview?"Deck waveform: drag to seek; wheel to zoom":"Stacked waveform: drag to scrub; wheel to zoom"}
      onPointerDown={e=>{if(e.button!==0||!track||!deck)return;e.preventDefault();e.currentTarget.focus();e.currentTarget.setPointerCapture(e.pointerId);drag.current={x:e.clientX,position:visible.current.position,start:visible.current.start,span,moved:false};}}
      onPointerMove={e=>{const d=drag.current;if(!d)return;const dx=e.clientX-d.x;if(Math.abs(dx)>3)d.moved=true;if(d.moved)send(dragSeek(d.position,dx,e.currentTarget.clientWidth,d.span,duration));}}
      onPointerUp={e=>{const d=drag.current;if(!d)return;drag.current=null;if(frame.current!==null){cancelAnimationFrame(frame.current);frame.current=null;}const r=e.currentTarget.getBoundingClientRect();const time=d.moved?dragSeek(d.position,e.clientX-d.x,r.width,d.span,duration):clickSeek(d.start,(e.clientX-r.left)/r.width,d.span,duration);onSeek?.(time,d.moved);pending.current=null;}}
      onPointerCancel={()=>{drag.current=null;pending.current=null;if(frame.current!==null){cancelAnimationFrame(frame.current);frame.current=null;}}}
      onLostPointerCapture={()=>{drag.current=null;pending.current=null;if(frame.current!==null){cancelAnimationFrame(frame.current);frame.current=null;}}}
      onKeyDown={e=>{if(e.key==="ArrowLeft"||e.key==="ArrowRight"){e.preventDefault();e.stopPropagation();onSeek?.(clamp((deck?.position||0)+(e.key==="ArrowRight"?1:-1)*(e.shiftKey?1:.05),0,duration));}}}
    />
    <div className="waveform-zoom" aria-label="Waveform zoom">
      <button disabled={!track} aria-label="Zoom waveform out" onClick={()=>zoom(1.5)}>−</button>
      <span title="Visible time span. Tempo ticks are estimates; beat phase is not analyzed.">{overview&&span>=duration?"Full":span.toFixed(span<2?1:0)+"s"}</span>
      <button disabled={!track} aria-label="Zoom waveform in" onClick={()=>zoom(1/1.5)}>+</button>
      {overview&&localSpan!==null&&<button onClick={()=>setLocalSpan(null)}>Fit</button>}
    </div>
  </div>;
}

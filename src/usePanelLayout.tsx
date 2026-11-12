import { useRef, useState, type CSSProperties, type PointerEvent, type KeyboardEvent } from "react";
import { DEFAULT_LAYOUT as defaults, normalizeLayout as valid, type Layout } from "./workspaceMath";
export function usePanelLayout(){
  const [layout,setLayout]=useState<Layout>(()=>{try{return valid(JSON.parse(localStorage.getItem("skydek-panel-layout-v1")||"{}"));}catch{return {...defaults};}});
  const latest=useRef(layout);latest.current=layout;
  const drag=useRef<{key:keyof Layout;x:number;y:number;layout:Layout;width:number;height:number}|null>(null);
  const persist=(next:Layout)=>{try{localStorage.setItem("skydek-panel-layout-v1",JSON.stringify(next));}catch{/* Layout remains usable without storage. */}};
  const update=(next:Layout)=>{const bounded=valid(next);latest.current=bounded;setLayout(bounded);return bounded;};
  const separator=(key:keyof Layout)=>({
    role:"separator" as const, tabIndex:0,
    "aria-label":key==="height"?"Resize browser library and sampler height together":key==="left"?"Resize folder browser width":"Resize sampler width",
    "aria-orientation":key==="height"?"horizontal" as const:"vertical" as const,
    "aria-valuenow":Math.round(layout[key]*100),"aria-valuemin":key==="height"?20:key==="left"?12:19,"aria-valuemax":key==="height"?55:key==="left"?28:36,
    onPointerDown:(e:PointerEvent<HTMLDivElement>)=>{if(e.button!==0)return;e.preventDefault();e.currentTarget.focus();e.currentTarget.setPointerCapture(e.pointerId);drag.current={key,x:e.clientX,y:e.clientY,layout:latest.current,width:e.currentTarget.closest(".browser")?.clientWidth||innerWidth,height:innerHeight};},
    onPointerMove:(e:PointerEvent<HTMLDivElement>)=>{const d=drag.current;if(!d)return;const next={...d.layout};next[d.key]+=(d.key==="height"?-(e.clientY-d.y)/d.height:(e.clientX-d.x)/d.width*(d.key==="right"?-1:1));update(next);},
    onPointerUp:()=>{drag.current=null;persist(latest.current);},
    onPointerCancel:()=>{drag.current=null;persist(latest.current);},
    onLostPointerCapture:()=>{drag.current=null;},
    onKeyDown:(e:KeyboardEvent<HTMLDivElement>)=>{const allowed=key==="height"?["ArrowUp","ArrowDown"]:["ArrowLeft","ArrowRight"];if(!allowed.includes(e.key))return;e.preventDefault();e.stopPropagation();const sign=key==="height"?(e.key==="ArrowUp"?1:-1):(e.key==="ArrowRight"?1:-1)*(key==="right"?-1:1);persist(update({...latest.current,[key]:latest.current[key]+sign*.01}));},
  });
  const style={"--folder-share":layout.left+"fr","--sampler-share":layout.right+"fr","--library-share":(1-layout.left-layout.right)+"fr","--browser-height":layout.height*100+"vh"} as CSSProperties;
  return {style,separator,reset:()=>{update({...defaults});persist(defaults);}};
}

import { useEffect, useRef, type ReactNode } from "react";
export function Dialog({title,onClose,children}:{title:string;onClose:()=>void;children:ReactNode}) {
  const ref=useRef<HTMLElement>(null), close=useRef(onClose); close.current=onClose;
  useEffect(()=>{
    const previous=document.activeElement as HTMLElement|null;
    ref.current?.querySelector<HTMLButtonElement>("button")?.focus();
    const key=(e:KeyboardEvent)=>{
      if(e.key==="Escape"){e.preventDefault();e.stopPropagation();close.current();}
      if(e.key!=="Tab")return;
      const nodes=Array.from(ref.current?.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled),select:not(:disabled),summary,[tabindex="0"]')||[]).filter(n=>n.getClientRects().length>0);
      const first=nodes[0],last=nodes.at(-1);
      if(e.shiftKey&&document.activeElement===first){e.preventDefault();last?.focus();}
      else if(!e.shiftKey&&document.activeElement===last){e.preventDefault();first?.focus();}
    };
    document.addEventListener("keydown",key,true);
    return()=>{document.removeEventListener("keydown",key,true);previous?.focus();};
  },[]);
  return <div className="backdrop"><section ref={ref} role="dialog" aria-modal="true" aria-label={title} className="sample-dialog panel">
    <div className="dialog-heading"><h2>{title}</h2><button aria-label="Close sampler settings" onClick={onClose}>×</button></div>
    <div className="dialog-body">{children}</div>
  </section></div>;
}

import { useState, useEffect } from "react";
import { inFolder, normalizePath } from "./FolderTree";
import { invoke } from "@tauri-apps/api/core";
export type SoundFile = { path:string; name:string };
export function SoundBrowser({folder,sounds,onAdd,onAssign}:{
  folder:string|null; sounds:SoundFile[];
  onAdd:(path:string)=>Promise<number>; onAssign:(path:string)=>Promise<void>;
}) {
  const [directory,setDirectory]=useState(""),[search,setSearch]=useState(""),[selected,setSelected]=useState<SoundFile|null>(null);
  const [busy,setBusy]=useState(false),[message,setMessage]=useState(""),[page,setPage]=useState(0);
  useEffect(()=>{setDirectory("");setPage(0);setSelected(null);},[folder]);
  const directories=Array.from(new Set(sounds.map(s=>normalizePath(s.path).split("/").slice(0,-1).join("/")))).sort((a,b)=>a.localeCompare(b));
  const filtered=sounds.filter(s=>inFolder(s.path,directory)&&s.name.toLowerCase().includes(search.toLowerCase()));
  async function scan(refresh:boolean) {
    setBusy(true);
    try {const count=await invoke<number>("import_sound_folder",{refresh});setMessage(count+" sound files available");setPage(0);setSelected(null);}
    catch(e){setMessage(String(e));}finally{setBusy(false);}
  }
  async function assign(next:boolean) {
    if(!selected)return;
    setBusy(true);
    try {if(next){const pad=await onAdd(selected.path);setMessage(selected.name+" → Pad "+(pad+1));}else{await onAssign(selected.path);setMessage(selected.name+" assigned to selected pad.");}}
    catch(e){setMessage(String(e));}finally{setBusy(false);}
  }
  return <section className="sound-browser">
    <div className="sound-tools"><button disabled={busy} onClick={()=>scan(false)}>{folder?"Change SFX folder":"Choose SFX folder"}</button><button disabled={busy||!folder} onClick={()=>scan(true)}>Refresh</button></div>
    <small className="sound-root" title={folder||""}>{folder?normalizePath(folder).split("/").pop():"Choose a folder to browse your sound effects."}</small>
    <label className="sound-directory">Folder<select aria-label="Sound effects folder" disabled={busy||!folder} value={directory} onChange={e=>{setDirectory(e.target.value);setPage(0);setSelected(null);}}>
      <option value="">All folders</option>
      {directories.map(p=><option key={p} value={p}>{folder&&p.toLowerCase().startsWith(normalizePath(folder).toLowerCase())?(p.slice(normalizePath(folder).length+1)||"Root folder"):p}</option>)}
    </select></label>
    <div className="sound-tools"><input aria-label="Search sound effects" placeholder="Search sound effects…" value={search} onChange={e=>{setSearch(e.target.value);setPage(0);setSelected(null);}}/></div>
    <div className="sound-actions">
      <button disabled={busy||!selected} onClick={()=>assign(false)}>Assign to selected pad</button>
      <button disabled={busy||!selected} onClick={()=>assign(true)}>Add to next empty</button>
    </div>
    <small role="status">{busy?"Working…":message||"Select a sound below, then choose where to assign it."}</small>
    <div className="sound-files" role="listbox" aria-label="Available sound effects">
      {filtered.slice(page*20,page*20+20).map(sound=><button type="button" role="option" aria-selected={sound.path===selected?.path} className={"sound-file "+(sound.path===selected?.path?"active":"")} key={sound.path} disabled={busy} title={sound.path} onClick={()=>setSelected(sound)} draggable onDragStart={e=>{e.dataTransfer.setData("application/x-skydek-sound",sound.path);e.dataTransfer.setData("text/plain",sound.path);e.dataTransfer.effectAllowed="copy";}}>{sound.name}</button>)}
      {folder&&!filtered.length&&<p>No matching WAV or MP3 files.</p>}
    </div>
    {filtered.length>20&&<div className="sound-tools sound-pagination"><button disabled={page===0} onClick={()=>setPage(p=>p-1)}>Previous</button><span>{page+1} / {Math.ceil(filtered.length/20)}</span><button disabled={(page+1)*20>=filtered.length} onClick={()=>setPage(p=>p+1)}>Next</button></div>}
  </section>;
}

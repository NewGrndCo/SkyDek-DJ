import { useEffect, useId, useRef, useState, type ReactNode, type KeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { GeneralSettings, type GeneralSettingsProps } from "./GeneralSettings";
import { MapperPanel, type Mappings } from "./MapperPanel";
import "./settings.css";

export type SettingsSkin = "dark" | "light" | "xbox" | "matrix";
export type SettingsPanelProps = Omit<GeneralSettingsProps, "section"> & {
  mappings: Mappings;
  skin: SettingsSkin;
  onSkinChange: (skin: SettingsSkin) => void;
  reducedMotion: boolean;
  onReducedMotionChange: (value: boolean) => void;
  closing?: boolean;
  onClose: () => void;
  midiContent: ReactNode;
};
const pages = [
  ["general", "General"],
  ["appearance", "Appearance"],
  ["audio", "Audio"],
  ["controller", "Xbox Controller"],
  ["midi", "MIDI Controllers"],
] as const;
type Page = typeof pages[number][0];
const focusable = 'button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), summary, [tabindex]:not([tabindex="-1"])';

export function SettingsPanel(props: SettingsPanelProps) {
  const { skin, onSkinChange, reducedMotion, onReducedMotionChange, closing = false, onClose, midiContent } = props;
  const [page, setPage] = useState<Page>("general");
  const [visited, setVisited] = useState<Set<Page>>(() => new Set(["general"]));
  function selectPage(next: Page) {
    setVisited(previous => previous.has(next) ? previous : new Set([...previous, next]));
    setPage(next);
  }
  const [message, setMessage] = useState("");
  const [updateState, setUpdateState] = useState<"idle" | "checking" | "current" | "available" | "error">("idle");
  const [latestVersion, setLatestVersion] = useState("");
  const [releaseUrl, setReleaseUrl] = useState("");
  const [pendingUpdate, setPendingUpdate] = useState<any>(null);
  async function checkForUpdates() {
    setUpdateState("checking");
    try {
      const signedUpdate = await check();
      if (signedUpdate) {
        setPendingUpdate(signedUpdate);
        setLatestVersion(signedUpdate.version);
        setReleaseUrl("https://github.com/NewGrndCo/SkyDek-DJ/releases");
        setUpdateState("available");
        return;
      }
      const response = await fetch("https://api.github.com/repos/NewGrndCo/SkyDek-DJ/releases/latest", { headers: { Accept: "application/vnd.github+json" } });
      if (!response.ok) throw new Error(`GitHub returned ${response.status}`);
      const release = await response.json() as { tag_name?: string; html_url?: string };
      const latest = (release.tag_name || "").replace(/^v/i, "");
      setLatestVersion(latest); setReleaseUrl(release.html_url || "https://github.com/NewGrndCo/SkyDek-DJ/releases");
      const current = "0.10.1".split(".").map(Number), next = latest.split(".").map(Number);
      const newer = next.length === 3 && next.some((part, index) => part > (current[index] || 0)) && next.findIndex((part, index) => part !== (current[index] || 0)) >= 0;
      setUpdateState(newer ? "available" : "current");
    } catch (error) { setUpdateState("error"); setMessage(`Update check failed: ${String(error)}`); }
  }
  useEffect(() => {
    const handler = () => { setPage("appearance"); setVisited(previous => new Set([...previous, "appearance"])); void checkForUpdates(); };
    window.addEventListener("skydek-check-updates", handler);
    return () => window.removeEventListener("skydek-check-updates", handler);
  }, []);
  async function installUpdate() {
    if (!pendingUpdate) { window.open(releaseUrl, "_blank", "noopener,noreferrer"); return; }
    setMessage("Downloading update…");
    await pendingUpdate.downloadAndInstall();
    await relaunch();
  }
  const root = useRef<HTMLElement>(null);
  const scroll = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  const id = useId();
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const overflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    root.current?.querySelector<HTMLButtonElement>('[role="tab"][aria-selected="true"]')?.focus();
    function keepFocus(event: FocusEvent) {
      if (root.current && event.target instanceof Node && !root.current.contains(event.target)) {
        root.current.querySelector<HTMLElement>(focusable)?.focus();
      }
    }
    function handleKey(event: globalThis.KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault(); event.stopPropagation(); close.current(); return;
      }
      if (event.key !== "Tab" || !root.current) return;
      const items = Array.from(root.current.querySelectorAll<HTMLElement>(focusable)).filter(item => item.getClientRects().length > 0 && !item.closest("[hidden], [inert]"));
      const first = items[0], last = items[items.length - 1];
      if (!first) { event.preventDefault(); root.current.focus(); }
      else if (event.shiftKey && (document.activeElement === first || document.activeElement === root.current)) { event.preventDefault(); last.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
    }
    document.addEventListener("keydown", handleKey, true);
    document.addEventListener("focusin", keepFocus);
    return () => {
      document.removeEventListener("keydown", handleKey, true);
      document.removeEventListener("focusin", keepFocus);
      document.body.style.overflow = overflow;
      if (previous?.isConnected) previous.focus();
    };
  }, []);
  useEffect(() => { if (scroll.current) scroll.current.scrollTop = 0; }, [page]);
  function navigate(event: KeyboardEvent<HTMLButtonElement>, index: number) {
    let next = index;
    if (event.key === "ArrowDown" || event.key === "ArrowRight") next = (index + 1) % pages.length;
    else if (event.key === "ArrowUp" || event.key === "ArrowLeft") next = (index + pages.length - 1) % pages.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = pages.length - 1;
    else return;
    event.preventDefault(); selectPage(pages[next][0]);
    document.getElementById(`${id}-tab-${pages[next][0]}`)?.focus();
  }
  return <div className="settings-backdrop" data-settings-skin={skin} data-reduced-motion={reducedMotion} data-closing={closing}>
    <section className="settings-shell" role="dialog" aria-modal="true" aria-labelledby={`${id}-title`} ref={root} tabIndex={-1}>
      <div className="settings-shell-heading">
        <div><span className="settings-eyebrow">SKYDEK DJ</span><h2 id={`${id}-title`}>Settings</h2></div>
        <button type="button" className="settings-close" onClick={onClose} aria-label="Close settings">×</button>
      </div>
      <div className="settings-shell-body">
        <div className="settings-sidebar" role="tablist" aria-label="Settings categories" aria-orientation="vertical">
          {pages.map(([key, label], index) => <button type="button" key={key} role="tab" id={`${id}-tab-${key}`} aria-controls={`${id}-panel-${key}`} aria-selected={page === key} tabIndex={page === key ? 0 : -1} onClick={() => selectPage(key)} onKeyDown={event => navigate(event, index)}>{label}</button>)}
        </div>
        <div className="settings-content" ref={scroll}>
          {pages.map(([key, label]) => <div key={key} role="tabpanel" id={`${id}-panel-${key}`} aria-labelledby={`${id}-tab-${key}`} hidden={page !== key} tabIndex={0}>
            <div className="settings-page-heading"><h3>{label}</h3></div>
            {visited.has(key) && (key === "general" || key === "audio" || key === "controller") && <GeneralSettings {...props} section={key} />}
            {key === "controller" && visited.has(key) && <MapperPanel initial={props.mappings} profiles={props.preferences.profiles} activeProfile={props.preferences.active_profile} />}
            {key === "appearance" && <div className="settings-appearance">
              <label>Skin<select value={skin} onChange={event => onSkinChange(event.target.value as SettingsSkin)}><option value="dark">Dark</option><option value="light">Light</option><option value="xbox">Xbox</option><option value="matrix">Matrix</option></select></label>
              <label className="settings-check"><input type="checkbox" checked={reducedMotion} onChange={event => onReducedMotionChange(event.target.checked)} />Reduce motion</label>
              <p>Limit interface animations and transitions.</p>
              <h4>Workspace</h4><div className="settings-actions"><button type="button" onClick={props.resetLayout}>Reset panel sizes</button><button type="button" onClick={() => { void invoke("toggle_fullscreen").catch(error => setMessage(String(error))); }}>Toggle fullscreen (F11)</button></div>
              <p role="status">{message}</p>
              <h4>Updates</h4>
              <div className="settings-actions">
                <button type="button" onClick={() => void checkForUpdates()} disabled={updateState === "checking"}>{updateState === "checking" ? "Checking…" : "Check for updates"}</button>
                {updateState === "available" && <button type="button" onClick={() => void installUpdate()}>Install v{latestVersion}</button>}
              </div>
              <p role="status">{updateState === "current" ? `You are up to date (v0.10.1).` : updateState === "available" ? `Version ${latestVersion} is available.` : ""}</p>
            </div>}
            {key === "midi" && page === key && midiContent}
          </div>)}
        </div>
      </div>
      <div className="settings-shell-footer"><span>SkyDek DJ · New Ground Enterprise</span><button type="button" onClick={onClose}>Done</button></div>
    </section>
  </div>;
}
export default SettingsPanel;

import { useEffect, useRef, type ButtonHTMLAttributes } from "react";
type Props = ButtonHTMLAttributes<HTMLButtonElement> & { onPress: () => void; onRelease: () => void };
export function HoldButton({ onPress, onRelease, ...props }: Props) {
  const held = useRef(false), releaseRef = useRef(onRelease);
  releaseRef.current = onRelease;
  const release = () => { if (held.current) { held.current = false; releaseRef.current(); } };
  useEffect(() => {
    const cancel = () => { if(held.current) { held.current = false; releaseRef.current(); } };
    window.addEventListener("blur", cancel);
    return () => { window.removeEventListener("blur", cancel); cancel(); };
  }, []);
  return <button {...props}
    onPointerDown={e => { if (e.button !== 0) return; e.preventDefault(); e.currentTarget.focus(); e.currentTarget.setPointerCapture(e.pointerId); if (!held.current) { held.current = true; onPress(); } }}
    onPointerUp={release} onPointerCancel={release} onLostPointerCapture={release} onBlur={release}
    onKeyDown={e => { if (e.code === "Space" || e.code === "Enter") { e.preventDefault(); if (!e.repeat && !held.current) { held.current = true; onPress(); } } }}
    onKeyUp={e => { if (e.code === "Space" || e.code === "Enter") { e.preventDefault(); release(); } }}
  />;
}

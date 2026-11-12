export const clamp=(n:number,min:number,max:number)=>Math.max(min,Math.min(max,n));
export type Layout={left:number;right:number;height:number};
export const DEFAULT_LAYOUT:Layout={left:.16,right:.25,height:.34};
export function normalizeLayout(value:Partial<Layout>):Layout {
  const result={...DEFAULT_LAYOUT};
  for(const key of ["left","right","height"] as const)if(typeof value[key]==="number"&&Number.isFinite(value[key]))result[key]=value[key]!;
  result.left=clamp(result.left,.12,.28);result.right=clamp(result.right,.19,.36);result.height=clamp(result.height,.2,.55);
  return result;
}
export const dragSeek=(anchor:number,delta:number,width:number,span:number,duration:number)=>clamp(anchor-delta/Math.max(1,width)*span,0,duration);
export const clickSeek=(start:number,ratio:number,span:number,duration:number)=>clamp(start+clamp(ratio,0,1)*span,0,duration);

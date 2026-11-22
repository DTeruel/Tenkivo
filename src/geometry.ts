export interface Box {x:number; y:number; w:number; h:number; rotation?:number}
export function rotate(x:number,y:number,degrees:number) {
  const radians=degrees*Math.PI/180, c=Math.cos(radians), s=Math.sin(radians);
  return {x:x*c-y*s,y:x*s+y*c};
}
export function bounds(o:Box) {
  const angle=(o.rotation??0)*Math.PI/180;
  const w=Math.abs(Math.cos(angle))*o.w+Math.abs(Math.sin(angle))*o.h;
  const h=Math.abs(Math.cos(angle))*o.h+Math.abs(Math.sin(angle))*o.w;
  return {x:o.x+(o.w-w)/2,y:o.y+(o.h-h)/2,w,h};
}
export function snap(value:number,step:number){return step>0?Math.round(value/step)*step:value;}
export function enclosure(objects:Box[]):Box {
  if(objects.length===1)return objects[0];
  const boxes=objects.map(bounds), x=Math.min(...boxes.map(b=>b.x)),y=Math.min(...boxes.map(b=>b.y));
  return {x,y,w:Math.max(...boxes.map(b=>b.x+b.w))-x,h:Math.max(...boxes.map(b=>b.y+b.h))-y};
}
export function resize(o:Box,corner:string,dx:number,dy:number,step=0,proportional=false):Box {
  const angle=o.rotation??0, local=rotate(dx,dy,-angle);
  const sx=corner.includes('e')?1:-1, sy=corner.includes('s')?1:-1;
  const requestedW=o.w+local.x*sx, requestedH=o.h+local.y*sy;
  let w=Math.max(32,snap(requestedW,step)), h=Math.max(32,snap(requestedH,step));
  if(proportional){
    // Project the drag onto the original diagonal, keeping the opposite corner fixed.
    const scale=(requestedW*o.w+requestedH*o.h)/(o.w*o.w+o.h*o.h);
    const longest=Math.max(o.w,o.h);
    const factor=Math.max(32/o.w,32/o.h,snap(longest*scale,step)/longest);
    w=o.w*factor;h=o.h*factor;
  }
  const shift=rotate((w-o.w)*sx/2,(h-o.h)*sy/2,angle);
  return {x:o.x+(o.w-w)/2+shift.x,y:o.y+(o.h-h)/2+shift.y,w,h,rotation:angle};
}
export function mapChild(child:Box,from:Box,to:Box):Box {
  const local=rotate(child.x+child.w/2-from.x-from.w/2,child.y+child.h/2-from.y-from.h/2,-(from.rotation??0));
  const sx=to.w/from.w, sy=to.h/from.h;
  const center=rotate(local.x*sx,local.y*sy,to.rotation??0);
  const w=child.w*sx,h=child.h*sy;
  return {x:to.x+to.w/2+center.x-w/2,y:to.y+to.h/2+center.y-h/2,w,h,
    rotation:(child.rotation??0)+(to.rotation??0)-(from.rotation??0)};
}
export function contains(frame:Box,child:Box) {
  for(const x of [-.5,.5])for(const y of [-.5,.5]){
    const p=rotate(child.w*x,child.h*y,child.rotation??0);
    const q=rotate(child.x+child.w/2+p.x-frame.x-frame.w/2,child.y+child.h/2+p.y-frame.y-frame.h/2,-(frame.rotation??0));
    if(Math.abs(q.x)>frame.w/2 || Math.abs(q.y)>frame.h/2)return false;
  }
  return true;
}

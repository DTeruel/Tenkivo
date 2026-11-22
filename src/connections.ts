import {rotate} from './geometry';
import type {Item, PortSide} from './model';

export const portSides=['top','right','bottom','left'] as const;
export function portPosition(item:Item,side:PortSide){
  const directions={top:[0,-1],right:[1,0],bottom:[0,1],left:[-1,0]};
  const [dx,dy]=directions[side];
  const angle=item.rotation??0;
  const offset=rotate(dx*item.w/2,dy*item.h/2,angle);
  const direction=rotate(dx,dy,angle);
  return {x:item.x+item.w/2+offset.x,y:item.y+item.h/2+offset.y,dx:direction.x,dy:direction.y};
}
type Anchor={x:number;y:number;dx:number;dy:number};
export function connectionPath(from:Anchor,to:Anchor){
  const reach=Math.min(90,Math.hypot(to.x-from.x,to.y-from.y)/3);
  return `M ${from.x} ${from.y} C ${from.x+from.dx*reach} ${from.y+from.dy*reach}, ${to.x+to.dx*reach} ${to.y+to.dy*reach}, ${to.x} ${to.y}`;
}

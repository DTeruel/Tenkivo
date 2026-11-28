export const canvasEdges=['top','bottom','left','right'] as const;
export type CanvasEdge=typeof canvasEdges[number];

type Bounds=Pick<DOMRectReadOnly,'left'|'right'|'top'|'bottom'|'width'|'height'>;

export function canvasDropTarget(bounds:Bounds,x:number,y:number){
  const distances={top:y-bounds.top,bottom:bounds.bottom-y,left:x-bounds.left,right:bounds.right-x};
  if(canvasEdges.some(edge=>distances[edge]<0))return;

  const nearest=canvasEdges.reduce((best,edge)=>distances[edge]<distances[best]?edge:best);
  const span=nearest==='left' || nearest==='right'?bounds.width:bounds.height;
  return distances[nearest]<=Math.min(span/4,90)?nearest:undefined;
}

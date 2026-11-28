export const canvasEdges=['top','bottom','left','right'] as const;
export const canvasCorners=['top-left','top-right','bottom-left','bottom-right'] as const;
export type CanvasEdge=typeof canvasEdges[number];
export type CanvasCorner=typeof canvasCorners[number];

type Bounds=Pick<DOMRectReadOnly,'left'|'right'|'top'|'bottom'|'width'|'height'>;

export function canvasDropTarget(kind:'toolbar'|'add',bounds:Bounds,x:number,y:number){
  const distances={top:y-bounds.top,bottom:bounds.bottom-y,left:x-bounds.left,right:bounds.right-x};
  if(canvasEdges.some(edge=>distances[edge]<0))return;

  if(kind==='toolbar'){
    const nearest=canvasEdges.reduce((best,edge)=>distances[edge]<distances[best]?edge:best);
    const span=nearest==='left' || nearest==='right'?bounds.width:bounds.height;
    return distances[nearest]<=Math.min(span/4,90)?nearest:undefined;
  }
  const vertical=distances.top<=distances.bottom?'top':'bottom';
  const horizontal=distances.left<=distances.right?'left':'right';
  if(distances[vertical]<=Math.min(bounds.height/3,90) && distances[horizontal]<=Math.min(bounds.width/3,90)){
    return `${vertical}-${horizontal}` as CanvasCorner;
  }
}

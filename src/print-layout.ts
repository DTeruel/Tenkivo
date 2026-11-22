import {bounds} from './geometry';
import {treeRows,type Project,type Page,type Item} from './model';

export const width=1030, height=590;
export type Sheet={page?:Page; items:Item[]; x:number;y:number;scale:number;index?:Page[]};
export function layout(project:Project,single:string,filters:string[],tile:boolean):Sheet[]{
  const sheets:Sheet[]=[], pages:Page[]=[], inherited=new Map<string,string[]>();
  for(const {p} of treeRows(project.pages)){
    const tags=p.departments??inherited.get(p.parentId??'')??[];inherited.set(p.id,tags);
    if(single && p.id!==single)continue;
    const match=(ds:string[])=>!filters.length || !ds.length || ds.some(d=>filters.includes(d));
    const items=p.items.filter(o=>match(o.inheritDepartment?tags:o.departments??(o.department?[o.department]:[])));
    if(!single && !items.length && !match(tags))continue;
    pages.push(p);
    const boxes=items.map(bounds);
    const x=boxes.length?Math.min(...boxes.map(b=>b.x))-4:0;
    const y=boxes.length?Math.min(...boxes.map(b=>b.y))-4:0;
    const w=Math.max(1,...boxes.map(b=>b.x+b.w-x+4)),h=Math.max(1,...boxes.map(b=>b.y+b.h-y+4));
    if(!tile || !boxes.length){
      sheets.push({page:p,items,x,y,scale:Math.min(1,width/w,height/h)});continue;
    }
    const cells=new Map<string,{col:number;row:number;items:Item[]}>();
    for(const [i,b] of boxes.entries()){
      const first=Math.floor((b.x-x)/width),last=Math.floor((b.x+b.w-x-.000001)/width);
      const top=Math.floor((b.y-y)/height),bottom=Math.floor((b.y+b.h-y-.000001)/height);
      if((last-first+1)*(bottom-top+1)>2000)throw Error('This canvas needs too many sheets. Choose Fit canvas.');
      for(let row=top;row<=bottom;row++)for(let col=first;col<=last;col++){
        const key=col+','+row,cell=cells.get(key)??{col,row,items:[]};cell.items.push(items[i]);cells.set(key,cell);
        if(cells.size>2000)throw Error('This canvas needs too many sheets. Choose Fit canvas.');
      }
    }
    for(const c of [...cells.values()].sort((a,b)=>a.row-b.row || a.col-b.col))
      sheets.push({page:p,items:c.items,x:x+c.col*width,y:y+c.row*height,scale:1});
  }
  const contents:Sheet[]=[];
  if(!single)for(let i=0;i<pages.length;i+=16)contents.push({index:pages.slice(i,i+16),items:[],x:0,y:0,scale:1});
  return [...contents,...sheets];
}

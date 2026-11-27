import type {Connection, Destination, Item, Page} from './model';

export interface CanvasClipboard {
  token: string;
  pageId: string;
  roots: string[];
  items: Item[];
  connections: Connection[];
  cut: boolean;
  pastes: number;
}

export function collectSelection(page:Page, selection:string[], cut:boolean):CanvasClipboard|undefined {
  const roots=page.items.filter(item=>selection.includes(item.id)).map(item=>item.id);
  if(!roots.length)return;

  const children=new Map<string,string[]>();
  for(const item of page.items){
    if(!item.parent)continue;
    const siblings=children.get(item.parent)??[];
    siblings.push(item.id);
    children.set(item.parent,siblings);
  }
  const included=new Set(roots), pending=[...roots];
  for(let i=0;i<pending.length;i++){
    for(const child of children.get(pending[i])??[]){
      if(included.has(child))continue;
      included.add(child);
      pending.push(child);
    }
  }
  return {
    token:'Tenkivo canvas selection '+crypto.randomUUID(),
    pageId:page.id,roots,cut,pastes:0,
    items:page.items.filter(item=>included.has(item.id)),
    connections:(page.connections??[]).filter(({from,to})=>
      cut ? included.has(from.objectId) || included.has(to.objectId)
          : included.has(from.objectId) && included.has(to.objectId))
  };
}

export function remapLinks(item:Item, map:(to:Destination)=>Destination) {
  if(item.link)item.link=map(item.link);
  for(const link of item.textLinks??[])link.to=map(link.to);
}

export function preparePaste(clipboard:CanvasClipboard, page:Page, move:boolean, dx:number, dy:number, copyPage=false) {
  const ids=new Map(clipboard.items.map(item=>[item.id,move?item.id:crypto.randomUUID()]));
  const items=structuredClone(clipboard.items).map(item=>{
    const id=ids.get(item.id)!;
    item.parent=item.parent ? ids.get(item.parent) : undefined;
    item.id=id;
    item.x+=dx;
    item.y+=dy;
    if(copyPage && item.target===clipboard.pageId)item.target=page.id;
    remapLinks(item,to=>{
      const target=to.pageId===clipboard.pageId && to.itemId ? ids.get(to.itemId) : undefined;
      if(target)return {pageId:page.id,itemId:target};
      return copyPage && to.pageId===clipboard.pageId && !to.itemId ? {pageId:page.id} : to;
    });
    return item;
  });
  const existing=new Set(page.items.map(item=>item.id));
  const connections=clipboard.connections.flatMap(connection=>{
    const endpoint=(id:string)=>ids.get(id) ?? (move && page.id===clipboard.pageId && existing.has(id) ? id : undefined);
    const from=endpoint(connection.from.objectId), to=endpoint(connection.to.objectId);
    if(!from || !to)return [];
    return [{
      id:move?connection.id:crypto.randomUUID(),
      from:{...connection.from,objectId:from},to:{...connection.to,objectId:to}
    }];
  });
  return {items,connections,roots:clipboard.roots.map(id=>ids.get(id)!)};
}

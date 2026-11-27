export const departments = ['Design', 'Programming', 'Art', 'Sound'] as const;
export type Department = string;
export interface DepartmentInfo {id:string;name:string;color:string}
export const defaultCatalog:DepartmentInfo[]=departments.map((id,i)=>({id,name:id,color:['#bd9b56','#799ccf','#71aa8d','#b48fbd'][i]}));
export type Kind = 'Note' | 'Image' | 'Frame' | 'Page link' | 'Attachment' | 'Web link' | 'Drawing';
export interface Attachment {path:string;local:boolean;folder:boolean;shared?:{sha256:string;blob?:string}}
export function mediaKind(attachment:Attachment){
  if(attachment.folder)return;
  if(/\.(?:mp3|wav|m4a|aac|ogg|oga|opus|flac|weba)$/i.test(attachment.path))return 'audio';
  if(/\.(?:mp4|m4v|webm|mov|ogv|mkv)$/i.test(attachment.path))return 'video';
}
export function webAddress(text:string){
  try{const u=new URL(text.trim());return ['http:','https:'].includes(u.protocol) && !u.username && !u.password?u:undefined;}catch{return undefined;}
}
export function videoId(url:string){
  const u=webAddress(url);if(!u)return;
  const host=u.hostname.replace(/^(www\.|m\.)/,'');
  const id=host==='youtu.be'?u.pathname.slice(1):['youtube.com','youtube-nocookie.com'].includes(host)?
    (u.pathname==='/watch'?u.searchParams.get('v'):u.pathname.match(/^\/(?:embed|shorts|live)\/([^/]+)/)?.[1]):null;
  return id && /^[\w-]{11}$/.test(id)?id:undefined;
}
export interface Destination { pageId: string; itemId?: string; url?:string }
export interface TextLink { start: number; end: number; to: Destination }
export interface BrushStroke {
  color:string;
  thickness:number;
  points:{x:number;y:number}[];
}
export type Drawing = {width:number;height:number} & (BrushStroke | {strokes:BrushStroke[]});
function validStroke(stroke:BrushStroke){
  return !!stroke && Number.isFinite(stroke.thickness) && stroke.thickness>0 &&
    typeof stroke.color==='string' && /^#[a-f0-9]{6}$/i.test(stroke.color) &&
    Array.isArray(stroke.points) && stroke.points.length>0 &&
    stroke.points.every(p=>p && Number.isFinite(p.x) && Number.isFinite(p.y));
}
function validDrawing(d:Drawing|undefined){
  if(!d || ![d.width,d.height].every(n=>Number.isFinite(n) && n>0))return false;
  return 'strokes' in d
    ? Array.isArray(d.strokes) && d.strokes.length>0 && d.strokes.every(validStroke)
    : validStroke(d);
}
export type PortSide='top'|'right'|'bottom'|'left';
export interface ObjectPort {objectId:string;side:PortSide}
export interface Connection {id:string;from:ObjectPort;to:ObjectPort}

export function migrateLegacyDiagrams(project:Project){
  for(const page of [...project.pages,...project.trash??[],...(project.pageHistory??[]).map(v=>v.page)]){
    page.items=page.items.flatMap(item=>{
      if((item.kind as string)!=='Diagram')return [item];
      const {diagram,...frame}=item as Item & {diagram?:{
        nodes?:{id:string;title:string;description:string;parent?:string}[];
        edges?:{from:{node:string;side:PortSide};to:{node:string;side:PortSide}}[];
      }};
      if(!Array.isArray(diagram?.nodes))return [{...frame,kind:'Frame'}];
      const positions=new Map<string,string>();
      const nodes:Item[]=diagram.nodes.map((n,i)=>{
        const id=frame.id+'-node-'+n.id;positions.set(n.id,id);
        const parentTitle=diagram.nodes?.find(v=>v.id===n.parent)?.title;
        return {...frame,id,kind:'Note',parent:frame.id,rotation:0,
          x:frame.x+20+(i%3)*175,y:frame.y+48+Math.floor(i/3)*105,w:155,h:88,
          name:n.title,body:[parentTitle?'Subdiagram of '+parentTitle:'',n.description].filter(Boolean).join('\n')};
      });
      for(const [index,edge] of (diagram.edges??[]).entries()){
        const from=positions.get(edge.from.node),to=positions.get(edge.to.node);
        if(from && to)(page.connections??=[]).push({id:frame.id+'-arrow-'+index,from:{objectId:from,side:edge.from.side},to:{objectId:to,side:edge.to.side}});
      }
      return [{...frame,kind:'Frame',rotation:0,w:Math.max(frame.w,555),h:Math.max(frame.h,60+Math.ceil(nodes.length/3)*105)},...nodes];
    });
  }
}
export interface Item {
  styles?: {start:number;end:number;style:NoteStyle}[];
  drawing?:Drawing;
  id: string; kind: Kind; name: string; x: number; y: number;
  w: number; h: number; body: string; department: Department;
  parent?: string; source?: string; target?: string; size: number;
  link?: Destination; textLinks?: TextLink[];
  inheritDepartment?:boolean; departments?:Department[]; rotation?: number; autoFit?: boolean;
  showTitle?:boolean; attachment?:Attachment; url?:string;
}
function validDestination(value: Destination) {
  return value && typeof value.pageId === 'string' &&
    (value.itemId === undefined || typeof value.itemId === 'string');
}
function validLinks(o: Item) {
  if(o.styles!==undefined && (!Array.isArray(o.styles) || !o.styles.every(s=>s &&
    ['bold','italic','underline'].includes(s.style) && Number.isInteger(s.start) && Number.isInteger(s.end) &&
    s.start>=0 && s.end>s.start && s.end<=o.body.length)))return false;
  if (o.link !== undefined && !validDestination(o.link)) return false;
  if (o.textLinks === undefined) return true;
  return o.kind === 'Note' && Array.isArray(o.textLinks) && o.textLinks.every((link, index, links) =>
    link && Number.isInteger(link.start) && Number.isInteger(link.end) &&
    link.start >= (index ? links[index-1].end : 0) && link.start < link.end &&
    link.end <= o.body.length && validDestination(link.to));
}
export type NoteStyle='bold'|'italic'|'underline';
export function toggleStyle(o:Item,start:number,end:number,style:NoteStyle){
  if(start>=end)return;
  const spans=o.styles??[];
  const contained=spans.some(s=>s.style===style && s.start<=start && s.end>=end);
  const next=spans.filter(s=>s.style!==style);
  for(const s of spans.filter(s=>s.style===style)){
    if(s.end<start || s.start>end){next.push(s);continue;}
    if(contained){
      if(s.start<start)next.push({...s,end:start});
      if(s.end>end)next.push({...s,start:end});
    }else{start=Math.min(start,s.start);end=Math.max(end,s.end);}
  }
  if(!contained)next.push({start,end,style});
  o.styles=next.sort((a,b)=>a.start-b.start);
}
export function textParts(o:Item){
  const spans=o.styles??[],links=o.textLinks??[];
  const cuts=[...new Set([0,o.body.length,...[...spans,...links].flatMap(s=>[s.start,s.end])])].sort((a,b)=>a-b);
  return cuts.slice(1).map((end,i)=>{
    const start=cuts[i],styles=spans.filter(s=>s.start<=start && s.end>=end).map(s=>s.style);
    const css=(styles.includes('bold')?'font-weight:bold;':'')+(styles.includes('italic')?'font-style:italic;':'')+(styles.includes('underline')?'text-decoration:underline;':'');
    return {text:o.body.slice(start,end),css,to:links.find(s=>s.start<=start && s.end>=end)?.to};
  });
}

// Textarea offsets and string slices both use UTF-16 code units.
export function editBody(o: Item, value: string, selectionStart?: number) {
  const old = o.body;
  let start = 0, oldEnd = old.length, newEnd = value.length;
  while (start < Math.min(selectionStart ?? Infinity, oldEnd, newEnd) && old[start] === value[start]) start++;
  while (oldEnd > start && newEnd > start && old[oldEnd-1] === value[newEnd-1]) {oldEnd--; newEnd--;}
  const delta = newEnd-oldEnd;
  o.styles=o.styles?.flatMap(s=>{
    if(s.end<start || (s.end===start && oldEnd!==start))return [s];
    if(s.start>=oldEnd)return [{...s,start:s.start+delta,end:s.end+delta}];
    const left=Math.min(s.start,start),right=s.end>=oldEnd?s.end+delta:newEnd;
    return right>left?[{...s,start:left,end:right}]:[];
  });
  o.textLinks = o.textLinks?.flatMap(link => {
    if (link.end <= start) return [link];
    if (link.start >= oldEnd) return [{...link,start:link.start+delta,end:link.end+delta}];
    if (start <= link.start && oldEnd >= link.end) return [];
    const left = link.start < start ? link.start : newEnd;
    const right = link.end > oldEnd ? link.end+delta : start;
    return right > left ? [{...link,start:left,end:right}] : [];
  });
  o.body = value;
}
export interface Page { connections?:Connection[]; id: string; name: string; items: Item[]; parentId?:string; departments?:Department[] }
export function treeRows(pages:Page[]) {
  const children=new Map<string|undefined,Page[]>();
  for(const p of pages){
    const siblings=children.get(p.parentId)??[];
    siblings.push(p);children.set(p.parentId,siblings);
  }
  const rows:{p:Page;depth:number}[]=[];
  const pending=(children.get(undefined)??[]).map(p=>({p,depth:0})).reverse();
  while(pending.length){
    const row=pending.pop()!;rows.push(row);
    for(const p of [...children.get(row.p.id)??[]].reverse())pending.push({p,depth:row.depth+1});
  }
  return rows;
}
export const snapDefaults = {move:10, resize:10, rotate:10};
export type Snap = typeof snapDefaults;
export interface ProjectUser {id:string;name:string;icon:string}
export interface PageVersion {
  id:string;
  date:string|null;
  author?:ProjectUser;
  restoredFrom?:string;
  page:Page;
}
export interface Project { pageHistory?:PageVersion[]; users?:ProjectUser[]; version: 1; name: string; pages: Page[]; snap?:Snap; trash?:Page[]; catalog?:DepartmentInfo[]; description?:string }
export function item(kind: Kind, name: string, x: number, y: number, extra: Partial<Item> = {}): Item {
  return { id: crypto.randomUUID(), kind, name, x, y, w: 230, h: 145, body: '', size: 16, department: 'Design', ...extra };
}
export function page(name: string): Page { return { id: crypto.randomUUID(), name, items: [] }; }

function validConnections(page:Page){
  const edges=page.connections;
  if(!Array.isArray(edges))return false;
  const objects=new Set(page.items.map(o=>o?.id));
  const ids=new Set<string>();
  const validPort=(port:ObjectPort)=>port && objects.has(port.objectId) && ['top','right','bottom','left'].includes(port.side);
  for(const edge of edges){
    if(!edge || typeof edge.id!=='string' || ids.has(edge.id) || !validPort(edge.from) || !validPort(edge.to) || edge.from.objectId===edge.to.objectId)return false;
    ids.add(edge.id);
  }
  return true;
}
export function validProject(value: unknown): value is Project {
  if (!value || typeof value !== 'object') return false;
  const p = value as Project;
  if(p.pageHistory!==undefined && (!Array.isArray(p.pageHistory) || !p.pageHistory.every(v=>
    v && typeof v.id==='string' && (v.date===null || (typeof v.date==='string' && Number.isFinite(Date.parse(v.date)))) &&
    (!v.author || (typeof v.author.name==='string' && typeof v.author.icon==='string')) &&
    (v.restoredFrom===undefined || typeof v.restoredFrom==='string') &&
    v.page && validProject({version:1,name:'',pages:[{...v.page,parentId:undefined}]}))))return false;
  if(p.users!==undefined && (!Array.isArray(p.users) || !p.users.every(u=>u && typeof u.id==='string' && u.id.length>0 && typeof u.name==='string' && u.name.trim().length>0 && typeof u.icon==='string' && u.icon.length>0) || new Set(p.users.map(u=>u.id)).size!==p.users.length))return false;
  if(p.catalog!==undefined && (!Array.isArray(p.catalog) || !p.catalog.every(d=>d && typeof d.id==='string' && typeof d.name==='string' && /^#[a-f0-9]{6}$/i.test(d.color)) || new Set(p.catalog.map(d=>d.id)).size!==p.catalog.length))return false;
  if(p.snap && !Object.keys(snapDefaults).every(k=>Number.isFinite(p.snap![k as keyof Snap]) && p.snap![k as keyof Snap]>=0))return false;
  if(p.trash!==undefined && (!Array.isArray(p.trash) || !p.trash.every(v=>v && validProject({version:1,name:'',pages:[{...v,parentId:undefined}]}))))return false;
  return p.version === 1 && typeof p.name === 'string' && Array.isArray(p.pages) && p.pages.length > 0 &&
    new Set(p.pages.map(v=>v?.id)).size === p.pages.length &&
    p.pages.every(v=>v && typeof v.id === 'string' && typeof v.name === 'string' && Array.isArray(v.items) && (v.departments===undefined || (Array.isArray(v.departments) && v.departments.every(d=>typeof d==='string'))) &&
      new Set(v.items.map(o=>o?.id)).size === v.items.length &&
      (v.connections===undefined || validConnections(v)) &&
      v.items.every(o=>o && ['Note','Image','Frame','Page link','Web link','Attachment','Drawing'].includes(o.kind) && typeof o.id === 'string' &&
        (o.departments===undefined || (Array.isArray(o.departments) && o.departments.every(d=>typeof d==='string'))) &&
        (o.kind!=='Drawing' || validDrawing(o.drawing)) &&
        (o.showTitle===undefined || typeof o.showTitle==='boolean') &&
        (o.attachment===undefined || (o.attachment && typeof o.attachment.path==='string' && typeof o.attachment.local==='boolean' && typeof o.attachment.folder==='boolean' &&
          (o.attachment.shared===undefined || (o.attachment.shared && !o.attachment.folder && o.attachment.local &&
            /^[a-f0-9]{64}$/.test(o.attachment.shared.sha256) && (o.attachment.shared.blob===undefined || /^[a-f0-9]{40}$/.test(o.attachment.shared.blob)))))) &&
        (o.kind!=='Attachment' || !!o.attachment) && (o.kind!=='Web link' || (typeof o.url==='string' && !!webAddress(o.url))) &&
        typeof o.name === 'string' && typeof o.body === 'string' && validLinks(o) && typeof o.department==='string' && (o.rotation===undefined || Number.isFinite(o.rotation)) &&
        [o.x,o.y,o.w,o.h,o.size].every(Number.isFinite) && o.w>0 && o.h>0 && o.size>0 &&
        (o.kind !== 'Image' || !!o.attachment || (typeof o.source === 'string' && /^data:image\/(png|jpeg|webp|gif);base64,/.test(o.source))))) && treeRows(p.pages).length===p.pages.length;
}

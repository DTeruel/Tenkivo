<script lang="ts">
  import { onMount, tick } from 'svelte';
  import {connectionPath,portPosition,portSides} from './connections';
  import DrawingStroke from './DrawingStroke.svelte';
  import DockPanel from './DockPanel.svelte';
  import ExportDialog from './ExportDialog.svelte';
  import AttachmentImage from './AttachmentImage.svelte';
  import MediaPlayer from './MediaPlayer.svelte';
  import {getCurrentWebview} from '@tauri-apps/api/webview';
  import {getVersion} from '@tauri-apps/api/app';
  import {mediaKind,videoId,webAddress,type Attachment} from './model';
  import {invoke,diagnostics,recordError} from './diagnostics.svelte';
  import { isTauri } from '@tauri-apps/api/core';
  import { open, save, confirm } from '@tauri-apps/plugin-dialog';
  import { benchmark, defaultCatalog, editBody, item, page, sample, snapDefaults, textParts, treeRows, validProject, migrateLegacyDiagrams } from './model';
  import { bounds, contains, resize, mapChild, enclosure, snap } from './geometry';
  import type { Box } from './geometry';
  import type { Department, Destination, Item, Kind, Page, TextLink, ProjectUser, PageVersion, BrushStroke, ObjectPort, Project } from './model';

  const tenkivoIcon=new URL('../src-tauri/icons/icon.png',import.meta.url).href;
  let appVersion=$state('Loading…');

  function fitText(node:HTMLElement, options:{enabled?:boolean;size:number;text:string}){
    let frame=0;
    function measure(){
      cancelAnimationFrame(frame);frame=requestAnimationFrame(()=>{
        if(!options.enabled){node.style.fontSize=options.size+'px';return;}
        let low=1,high=options.size;
        while(low<high){const middle=Math.ceil((low+high)/2);node.style.fontSize=middle+'px';
          if(node.scrollHeight<=node.clientHeight+1 && node.scrollWidth<=node.clientWidth+1)low=middle;else high=middle-1;}
        node.style.fontSize=low+'px';
      });
    }
    const observer=new ResizeObserver(measure);observer.observe(node);measure();
    return {update(next:typeof options){options=next;measure();},destroy(){observer.disconnect();cancelAnimationFrame(frame);}};
  }
  const initial:ReturnType<typeof sample>={version:1,name:'',pages:[page('Overview')]};
  let welcome=$state(true),launching=$state(false),welcomeError=$state(''),creationError=$state('');
  let startup=$state(localStorage.getItem('studio-startup')??'welcome');
  type Recent={name:string;path:string};
  function readRecent():Recent[]{
    try{const list:unknown=JSON.parse(localStorage.getItem('studio-recent')??'[]');
      return Array.isArray(list)?list.filter(v=>v && typeof v.name==='string' && typeof v.path==='string'):[];}
    catch{return [];}
  }
  let recent=$state<Recent[]>(readRecent()),availability=$state<Record<string,string>>({});
  function rememberProject(location:string){
    if(!location)return;
    recent=[{name:project.name,path:location},...recent.filter(v=>v.path!==location)];
    localStorage.setItem('studio-recent',JSON.stringify(recent));
    localStorage.setItem('studio-last',location);
  }
  function forgetProject(location:string){
    recent=recent.filter(v=>v.path!==location);
    localStorage.setItem('studio-recent',JSON.stringify(recent));
    if(localStorage.getItem('studio-last')===location)localStorage.removeItem('studio-last');
  }
  async function checkRecent(){
    if(!isTauri())return;
    await Promise.all(recent.map(async v=>{
      try{availability[v.path]=await invoke<string>('project_availability',{path:v.path});}
      catch{availability[v.path]='Unavailable';}
    }));
  }
  async function openRecent(location:string){
    if(launching)return;
    launching=true;welcomeError='';
    try{await load(await invoke<string>('open_project',{path:location}),location);}
    catch(error){welcomeError='Could not open '+location+' · '+String(error);await checkRecent();}
    finally{launching=false;}
  }
  let project = $state(initial), current = $state(initial.pages[0].id);
  let board = $derived(project.pages.find(p=>p.id===current)!);
  let ids=$state<string[]>([]);
  let picked=$derived(board.items.filter(o=>ids.includes(o.id)));
  let active=$derived(picked.length===1?picked[0]:undefined);
  let liveBox=$state<Box>();
  let box=$derived(liveBox ?? (picked.length?enclosure(picked):undefined));
  let affected=$derived(board.items.filter(o=>ids.includes(o.id) || (o.parent && ids.includes(o.parent))));
  let marquee=$state<{x:number;y:number;endX:number;endY:number;prior:string[]}>();
  let settings:HTMLDialogElement;
  function readSnapDefaults():typeof snapDefaults{
    try{
      const values=JSON.parse(localStorage.getItem('studio-canvas-snap')??'null');
      if(values && [values.move,values.resize,values.rotate].every(n=>Number.isFinite(n) && n>=0))return values;
    }catch{/* Use the built-in defaults if local preferences are invalid. */}
    return {...snapDefaults};
  }
  let preferredSnap=$state(readSnapDefaults());
  let snapping=$derived((welcome?undefined:project.snap)??preferredSnap);
  function setSnap(key:keyof typeof snapDefaults,field:HTMLInputElement){
    if(!field.validity.valid || !Number.isFinite(field.valueAsNumber)){field.value=String(snapping[key]);return;}
    const next={...snapping,[key]:field.valueAsNumber};
    if(welcome){preferredSnap=next;localStorage.setItem('studio-canvas-snap',JSON.stringify(next));}
    else if(editing){project.snap=next;change();}
  }
  const snapKeys=['move','resize','rotate'] as const;
  let pagesSide=$state(localStorage.getItem('pages-side')??'left'), inspectorSide=$state(localStorage.getItem('inspector-side')??'right');
  let pagesOpen=$state(localStorage.getItem('pages-open')!=='false'), inspectorOpen=$state(localStorage.getItem('inspector-open')!=='false');
  $effect(()=>{localStorage.setItem('pages-side',pagesSide);localStorage.setItem('inspector-side',inspectorSide);localStorage.setItem('pages-open',String(pagesOpen));localStorage.setItem('inspector-open',String(inspectorOpen));});
  let menu=$state<{x:number;y:number;page?:Page;canvas?:boolean;fromAdd?:boolean;point?:{x:number;y:number}}>(), pendingPage=$state<Page>();
  let focusArea:'tree'|'canvas'='canvas';
  let history=$state<{page:string;x:number;y:number;zoom:number;ids:string[]}[]>([]), cursor=$state(-1);
  let historyLimit=$state(normalizeLimit(Number(localStorage.getItem('navigation-limit')??10)));
  let historyMenu=$state<HTMLDetailsElement>(null!), fileMenu=$state<HTMLDetailsElement>(null!);
  let pageParent:string|undefined;
  function normalizeLimit(n:number){return Number.isFinite(n)?Math.max(1,Math.min(100,Math.round(n))):10;}
  function trimHistory(){
    if(history.length<=historyLimit)return;
    const start=Math.min(history.length-historyLimit,Math.max(0,cursor-historyLimit+1));
    history=history.slice(start,start+historyLimit);cursor-=start;
  }
  function setLimit(n:number){historyLimit=normalizeLimit(n);localStorage.setItem('navigation-limit',String(historyLimit));trimHistory();}
  function view(){return {page:current,x:tx,y:ty,zoom,ids:[...ids]};}
  function resetViews(){wire=undefined;selectedConnection='';history=[];cursor=-1;filters=[];ids=[];menu=undefined;}
  function rememberView(){if(cursor>=0)history[cursor]=view();}
  function travel(step:number){
    if(historyMenu)historyMenu.open=false;
    const dest=history[cursor+step];if(!dest)return;
    stopDrawing();wire=undefined;selectedConnection='';rememberView();cursor+=step;current=dest.page;tx=dest.x;ty=dest.y;zoom=dest.zoom;ids=dest.ids.filter(id=>board.items.some(o=>o.id===id && (matches(itemTags(o),filters))));
    inline='';range=undefined;gesture=undefined;liveBox=undefined;drag=undefined;marquee=undefined;menu=undefined;
  }
  function setFilters(next:Department[]){filters=next;ids=picked.filter(o=>matches(itemTags(o),next)).map(o=>o.id);}
  function context(e:MouseEvent,p?:Page,o?:Item){e.preventDefault();e.stopPropagation();selectedConnection='';if(o && !ids.includes(o.id))ids=[o.id];menu={x:Math.min(e.clientX,innerWidth-190),y:Math.min(e.clientY,innerHeight-260),page:p};}
  function removeObjects(){if(!editing)return;board.connections=board.connections?.filter(c=>!ids.includes(c.from.objectId) && !ids.includes(c.to.objectId));board.items=board.items.filter(o=>!ids.includes(o.id));for(const o of board.items)if(o.parent && ids.includes(o.parent))o.parent=undefined;ids=[];menu=undefined;change();}
  function requestDelete(p:Page){if(!editing)return;pendingPage=p;menu=undefined;ask('Delete page');}
  async function deletePage(){
    const p=pendingPage;if(!p || !editing)return;
    project.trash=[...(project.trash??[]),p];for(const child of project.pages)if(child.parentId===p.id)child.parentId=p.parentId;
    project.pages=project.pages.filter(v=>v.id!==p.id);if(!project.pages.length)project.pages.push(page('Overview'));
    const next=project.pages.find(v=>v.id===current)??project.pages[0];history=[];cursor=-1;await navigate(next);dialog.close();change();
  }
  function restorePage(p:Page){if(!editing)return;if(!project.pages.some(v=>v.id===p.parentId))p.parentId=undefined;project.pages.push(p);project.trash=project.trash?.filter(v=>v.id!==p.id);change();}
  function align(axis:'x'|'y',factor:number){
    if(!editing || !box)return;
    const roots=picked.filter(o=>!o.parent || !ids.includes(o.parent));
    const group=enclosure(roots),size=axis==='x'?'w':'h';
    for(const o of roots){const b=bounds(o),d=group[axis]+group[size]*factor-b[axis]-b[size]*factor;o[axis]+=d;for(const c of board.items)if(c.parent===o.id)c[axis]+=d;}
    change();menu=undefined;
  }
  let editing = $state(false), inline = $state(''), dirty = $state(false), busy = $state(false);
  let theme = $state(localStorage.getItem('studio-theme') || 'System'), path = $state(''), status = $state('');
  let fileDialog:HTMLDialogElement, incoming=$state<string[]>([]), copying=$state(false), importError=$state(''), playing=$state('');
  let dropAt={x:0,y:0};
  function offerFiles(paths:string[],x:number,y:number){
    if(!editing){status=readHint;return;}
    if(!paths.length || document.querySelector('dialog[open]'))return;
    incoming=paths;dropAt={x,y};importError='';fileDialog.showModal();
  }
  async function attach(local:boolean){
    if(copying)return;copying=true;
    try{
      if(local && !path){await saveFile();if(!path){importError='Save the project to choose where its assets will live.';return;}}
      const items=await invoke<{name:string;image:boolean;attachment:Attachment}[]>('import_attachments',{project:path,paths:[...incoming],local});
      for(const [i,v] of items.entries()){
        const media=mediaKind(v.attachment);
        add(v.image?'Image':'Attachment',{name:v.name,attachment:v.attachment,department:v.image?'Art':'Design',
          x:dropAt.x+i*22,y:dropAt.y+i*22,w:media==='video'?360:v.image || media==='audio'?300:230,
          h:media==='video'?240:v.image?200:media==='audio'?120:100});
      }
      fileDialog.close();
    }catch(err){importError=String(err);}finally{copying=false;}
  }
  let insertAt:{x:number;y:number}|undefined,linkPage=$state(''),linkUrl=$state('');
  function canvasMenu(e:MouseEvent,fromButton=false){
    e.preventDefault();e.stopPropagation();focusArea='canvas';
    const rect=(e.currentTarget as HTMLElement).getBoundingClientRect();
    menu={canvas:true,fromAdd:fromButton,x:Math.max(0,Math.min(fromButton?rect.left:e.clientX,innerWidth-200)),
      y:Math.max(0,Math.min(fromButton?rect.bottom+3:e.clientY,innerHeight-308)),point:fromButton?undefined:worldPoint(e)};
  }
  async function insert(kind:string){
    if(!editing)return;
    insertAt=menu?.point;menu=undefined;
    if(kind==='Drawing'){drawingMode=true;ids=[];inline='';return;}
    if(kind==='Note' || kind==='Frame'){add(kind,insertAt);return;}
    if(kind==='Page link'){linkPage=project.pages.find(p=>p.id!==current)?.id??current;ask('Add page link');return;}
    if(kind==='Web link'){linkUrl='';ask('Add web link');return;}
    if(!isTauri()){imageInput.click();return;}
    try{
      const value=await open({multiple:true,directory:kind==='Folder',filters:kind==='Image'?[{name:'Images',extensions:['png','jpg','jpeg','gif','webp']}]:undefined});
      if(value)offerFiles(value,insertAt?.x??(width/2-tx)/zoom,insertAt?.y??(height/2-ty)/zoom);
    }catch(err){recordError('Choose resource',err);status=String(err);}
  }
  async function openResource(o:Item){
    try{if(o.attachment)await invoke('open_attachment',{project:path,attachment:o.attachment});
      else if(o.url && isTauri())await invoke('open_web',{url:o.url});
      else if(o.url)window.open(o.url,'_blank','noopener,noreferrer');
    }catch(err){status='Could not open '+o.name+'. '+String(err);}
  }
  async function revealAttachment(o:Item){
    if(!o.attachment)return;
    try{await invoke('reveal_attachment',{project:path,attachment:o.attachment});}
    catch(error){status='Could not show the attachment in its folder. '+String(error);}
  }
  let copyingItem=$state('');
  async function copyToProject(o:Item){
    const original=o.attachment, owner=project, page=board;
    if(!editing || copyingItem || !original || original.local)return;
    copyingItem=o.id;
    try{
      if(!path){await saveFile();if(!path || project!==owner)return;}
      const [copy]=await invoke<{attachment:Attachment}[]>('import_attachments',{
        project:path,paths:[original.path],local:true
      });
      if(project!==owner || !editing || !owner.pages.includes(page) || !page.items.includes(o) || o.attachment!==original)return;
      if(!copy?.attachment.local)throw Error('The project copy could not be created.');
      o.attachment=copy.attachment;change();status='Attachment copied to project.';
    }catch(error){status='Could not copy attachment. '+String(error);}
    finally{copyingItem='';}
  }
  async function relink(o:Item){
    if(!o.attachment)return;
    const file=await open({directory:o.attachment.folder});if(file){o.attachment={...o.attachment,path:file,local:false};change();}
  }
  function insertUrl(text:string,x?:number,y?:number){
    const u=webAddress(text);if(!u)return false;
    if(!editing){status=readHint;return true;}
    const owner=board,video=videoId(u.href);
    const o=add('Web link',{url:u.href,name:video?'YouTube video':u.hostname,w:video?360:280,h:video?252:145,...(x!==undefined && y!==undefined?{x,y}:{})});
    if(o && isTauri()){
      void invoke<[string,string]>('web_preview',{url:u.href}).then(([title,description])=>{
        const entry=owner.items.find(v=>v.id===o.id);
        if(!entry || !project.pages.includes(owner))return;
        if(title && entry.name===o.name)entry.name=title;
        if(description)entry.body=description;
        if(title || description)change();
      }).catch(()=>{});
    }
    return true;
  }
  function paste(e:ClipboardEvent){
    if(welcome)return;
    if(document.querySelector('dialog[open]') || (e.target as HTMLElement)?.closest?.('textarea,input,[contenteditable=true]'))return;
    if(insertUrl(e.clipboardData?.getData('text/plain')??''))e.preventDefault();
  }
  let zoom = $state(1), tx = $state(30), ty = $state(40), width = $state(800), height = $state(600);
  let viewport=$state<HTMLDivElement>(null!), imageInput: HTMLInputElement, fileInput: HTMLInputElement, dialog: HTMLDialogElement;
  let dialogType = $state(''), name = $state(''), fps = $state(0), showFps = $state(false);
  let filters = $state<Department[]>([]), drag: {x:number;y:number;tx:number;ty:number;objects:{o:Item;x:number;y:number}[];pan:boolean} | undefined;
  const catalog=$derived(project.catalog??defaultCatalog);
  const departments=$derived(catalog.map(d=>d.id));
  const colors=$derived(Object.fromEntries(catalog.map(d=>[d.id,d.color])));
  const names=$derived(Object.fromEntries(catalog.map(d=>[d.id,d.name])));
  let draftName=$state(''),draftDescription=$state(''),draftCatalog=$state<typeof defaultCatalog>([]);
  const userIcons=['👤','🦊','🐻','🐱','🐼','🐙','🌙','🌿','🎨','🎧'];
  let draftUsers=$state<ProjectUser[]>([]), editorId=$state('');
  let pickerUsers=$state<ProjectUser[]>([]), selectedUser=$state(''),registering=$state(false);
  let userName=$state(''),userIcon=$state(userIcons[0]);
  let editor=$derived(project.users?.find(u=>u.id===editorId));
  function normalizeName(value:string){return value.trim().toLowerCase();}
  let validUsers=$derived(draftUsers.every(u=>u.name.trim()) && new Set(draftUsers.map(u=>normalizeName(u.name))).size===draftUsers.length);
  let validRegistration=$derived(userName.trim()!=='' && !project.users?.some(u=>normalizeName(u.name)===normalizeName(userName)));
  function recentEditors():string[]{
    try{const ids:unknown=JSON.parse(localStorage.getItem('studio-recent-editors')??'[]');return Array.isArray(ids)?ids.filter(id=>typeof id==='string'):[];}
    catch{return [];}
  }
  function selectEditor(){
    const users=project.users??[];
    const last=recentEditors().map(id=>users.find(u=>u.id===id)).find(Boolean);
    pickerUsers=last?[last,...users.filter(u=>u.id!==last.id)]:[...users];
    selectedUser=pickerUsers[0]?.id??'';registering=!users.length;userName='';userIcon=userIcons[0];ask('Select editing user');
  }
  function enterEditing(){
    if(registering){
      if(!validRegistration)return;
      const user={id:crypto.randomUUID(),name:userName.trim(),icon:userIcon};
      project.users=[...project.users??[],user];selectedUser=user.id;change();
    }
    if(!project.users?.some(u=>u.id===selectedUser))return;
    editorId=selectedUser;editing=true;
    localStorage.setItem('studio-recent-editors',JSON.stringify([editorId,...recentEditors().filter(id=>id!==editorId)]));
    if(status===readHint)status='';
    dialog.close();
  }
  let section=$state(''), priorDepartments:typeof defaultCatalog=[],priorUsers:ProjectUser[]=[];
  function openSection(value:string){
    priorDepartments=$state.snapshot(draftCatalog);priorUsers=$state.snapshot(draftUsers);section=value;
  }
  function leaveSection(apply:boolean){
    if(!apply){draftCatalog=priorDepartments;draftUsers=priorUsers;}
    else if(dialogType==='Project settings' && editing){applyOptions(false);}
    section='';
  }
  function projectOptions(){section='';
    draftUsers=(project.users??[]).map(u=>({...u}));draftName=project.name;draftDescription=project.description??'';draftCatalog=catalog.map(d=>({...d}));ask('Project settings');
  }
  function applyOptions(close=true){
    if(!editing || !validUsers || !draftName.trim() || draftCatalog.some(d=>!d.name.trim()))return;
    const allowed=new Set(draftCatalog.map(d=>d.id));
    for(const p of [...project.pages,...project.trash??[]]){
      if(p.departments)p.departments=p.departments.filter(id=>allowed.has(id));
      for(const o of p.items){
        if(!allowed.has(o.department))o.department='';
        if(o.departments)o.departments=o.departments.filter(id=>allowed.has(id));
      }
    }
    project.users=draftUsers.map(u=>({...u,name:u.name.trim()}));
    project.name=draftName.trim();project.description=draftDescription.trim();
    project.catalog=draftCatalog.map(d=>({...d,name:d.name.trim()}));setFilters(filters.filter(id=>allowed.has(id)));change();if(close)dialog.close();
  }
  let visible = $derived(board.items.filter(o=>{
    const b=bounds(o);
    return (matches(itemTags(o),filters)) && b.x*zoom+tx<width && (b.x+b.w)*zoom+tx>0 && b.y*zoom+ty<height && (b.y+b.h)*zoom+ty>0;
  }));
  let treeDrop = $state(''), draggingPage=$state('');
  let frameOnly=$state('');
  let pageTags=$derived.by(()=>{
    const map=new Map<string,Department[]>();
    for(const {p} of treeRows(project.pages))map.set(p.id,p.departments??map.get(p.parentId??'')??[]);
    return map;
  });
  function matches(tags:Department[],filter:Department[]){return !filter.length || !tags.length || tags.some(d=>filter.includes(d));}
  function itemTags(o:Item){return o.inheritDepartment?pageTags.get(current)??[]:(o.departments??(departments.includes(o.department)?[o.department]:[]));}
  function toggleObjectTag(id:string,enabled:boolean){
    if(!editing)return;
    for(const o of picked){
      const tags=itemTags(o);
      o.departments=enabled?[...new Set([...tags,id])]:tags.filter(d=>d!==id);
      o.inheritDepartment=false;o.department=o.departments[0]??'';
    }
    change();
  }
  function inheritTags(enabled:boolean){
    if(!editing)return;
    for(const o of picked){
      if(!enabled){o.departments=[...itemTags(o)];o.department=o.departments[0]??'';}
      o.inheritDepartment=enabled;
    }
    change();
  }
  let pageRows=$derived.by(()=>{
    const rows=treeRows(project.pages),keep=new Set<string>();
    for(const {p} of [...rows].reverse())if(matches(pageTags.get(p.id)??[],filters) || keep.has(p.id)){keep.add(p.id);if(p.parentId)keep.add(p.parentId);}
    return rows.filter(({p})=>keep.has(p.id));
  });
  function newPage(parent?:Page){pageParent=parent?.id;ask('New page');}
  function togglePageTag(p:Page,id:string,on:boolean){
    const tags=pageTags.get(p.id)??[];
    p.departments=on?[...new Set([...tags,id])]:tags.filter(d=>d!==id);change();
  }
  type Placement='before'|'inside'|'after'|'root';
  function placement(e:DragEvent):Placement {
    const r=(e.currentTarget as HTMLElement).getBoundingClientRect(), fraction=(e.clientY-r.top)/r.height;
    return fraction<.25?'before':fraction>.75?'after':'inside';
  }
  function movePage(e:DragEvent,to?:Page){
    e.preventDefault();treeDrop='';
    const source=project.pages.find(p=>p.id===e.dataTransfer?.getData('studio/page'));
    reposition(source,to,to?placement(e):'root');
  }
  function reposition(source:Page|undefined,to:Page|undefined,where:Placement){
    if(!editing || !source || source===to)return;
    const parent=where==='inside'?to?.id:to?.parentId;
    for(let id=parent;id;id=project.pages.find(p=>p.id===id)?.parentId)if(id===source.id)return;
    const ordered=project.pages.filter(p=>p.id!==source.id);
    const at=to && where!=='inside'?ordered.findIndex(p=>p.id===to.id)+(where==='after'?1:0):ordered.length;
    source.parentId=parent;ordered.splice(at,0,source);project.pages=ordered;change();
  }
  let pagePointer:{id:string;x:number;y:number;pointer:number}|undefined, suppressPageClick=false;
  function pageDown(e:PointerEvent,p:Page){
    if(isTauri() && editing && e.button===0){suppressPageClick=false;pagePointer={id:p.id,x:e.clientX,y:e.clientY,pointer:e.pointerId};}
  }
  function pagePointerMove(e:PointerEvent,finish=false){
    if(!pagePointer || pagePointer.pointer!==e.pointerId)return;
    const from=pagePointer;
    if(finish)pagePointer=undefined;
    if(!draggingPage && Math.hypot(from.x-e.clientX,from.y-e.clientY)<5)return;
    draggingPage=from.id;
    const hit=document.elementFromPoint(e.clientX,e.clientY);
    const row=hit?.closest<HTMLElement>('[data-page]'),to=project.pages.find(p=>p.id===row?.dataset.page);
    const rect=row?.getBoundingClientRect(),frac=rect?(e.clientY-rect.top)/rect.height:0;
    const where:Placement=frac<.25?'before':frac>.75?'after':'inside';
    treeDrop=to?to.id+' '+where:'';
    if(!finish)return;
    suppressPageClick=true;draggingPage='';treeDrop='';
    const src=project.pages.find(p=>p.id===from.id);
    if(to || hit?.closest('.root-drop'))reposition(src,to,to?where:'root');
    else if(src && hit?.closest('.viewport')){const {x,y}=worldPoint(e);add('Page link',{name:src.name,target:src.id,x,y,w:210,h:84});}
  }
  let gesture: {box:Box; handle:string; px:number; py:number; startAngle:number; children:{o:Item;box:Box}[]} | undefined;
  function worldPoint(e:MouseEvent){
    const r=viewport.getBoundingClientRect();
    return {x:(e.clientX-r.left-tx)/zoom,y:(e.clientY-r.top-ty)/zoom};
  }
  function beginHandle(e:PointerEvent,handle:string){
    if(!editing || !box || e.button!==0)return;
    e.preventDefault();e.stopPropagation();inline='';
    const {x,y}=worldPoint(e),o=box;
    gesture={box:{...o},handle,px:x,py:y,startAngle:Math.atan2(y-o.y-o.h/2,x-o.x-o.w/2),
      children:(active && active.id===frameOnly?[active]:affected).map(v=>({o:v,box:{...v}}))};
    viewport.setPointerCapture(e.pointerId);
  }
  const fileFilters = [{name:'Tenkivo',extensions:['gdstudio']}];
  let release=$state<{current:string;version:string;notes:string;portable:boolean}|null>(null);
  let updateDialog:HTMLDialogElement, installing=$state(false), updateError=$state('');
  const updateInterval=60*60*1000;
  let checking=$state(false),cooldown=$state(false),updateTimer:ReturnType<typeof setTimeout>;
  let updateChecksActive=false;
  const updateLabel=$derived(release?'Update available · '+release.version:cooldown?'Up to date · Next check within one hour':checking?'Checking for updates…':'Check for updates');
  function scheduleUpdateCheck(){
    clearTimeout(updateTimer);
    if(!updateChecksActive)return;
    updateTimer=setTimeout(()=>{
      cooldown=false;
      void checkUpdates();
    },updateInterval);
  }
  async function checkUpdates(manual=false){
    if(!updateChecksActive || checking || cooldown)return;
    if(installing){scheduleUpdateCheck();return;}
    checking=true;
    try{
      const result=await invoke<typeof release>('check_update');
      if(!updateChecksActive || installing)return;
      const newVersion=result && result.version!==release?.version;
      release=result;
      if(newVersion){
        updateError='';
        if(!updateDialog.open)updateDialog.showModal();
      }else if(!result){
        if(updateDialog.open)updateDialog.close();
        if(!manual)return;
        status='No newer public portable release found.';cooldown=true;
      }
    }catch{if(manual)status='Could not check GitHub Releases. Try again when online.';}
    finally{checking=false;scheduleUpdateCheck();}
  }
  function openUpdates(){
    if(release){
      if(!updateDialog.open)updateDialog.showModal();
    }else void checkUpdates(true);
  }
  async function applyUpdate(){
    if(!release || installing)return;
    const version=release.version;
    installing=true;
    try{
      if(dirty){
        await saveFile();
        if(dirty){installing=false;updateError='Save your project before restarting.';return;}
      }
      await invoke('install_update',{version});
      await invoke('restart_after_update');
    }
    catch(err){installing=false;updateError=String(err);}
  }
  let range = $state<{id:string; start:number; end:number}>();
  let draftRange = $state<{start:number; end:number}>();
  let targetPage = $state(''), targetItem = $state('');
  let choices = $derived(project.pages.find(p=>p.id===targetPage)?.items ?? []);
  let beforeStart: number | undefined;
  function objectLink(o: Item): Destination | undefined {
    return o.link ?? (o.kind==='Page link' && o.target ? {pageId:o.target} : undefined);
  }
  function labelFor(to: Destination) {
    const p=project.pages.find(p=>p.id===to.pageId);
    if(!p)return 'Missing page';
    return to.itemId ? p.name+' / '+(p.items.find(o=>o.id===to.itemId)?.name ?? 'Missing object') : p.name;
  }
  async function follow(to: Destination) {
    const p=project.pages.find(p=>p.id===to.pageId);
    const target=p?.items.find(o=>o.id===to.itemId);
    if(!p || (to.itemId && !target)){status='Link destination is missing.';return;}
    await navigate(p);
    if(target){if(!matches(itemTags(target),filters)){status='Destination hidden by department filters.';return;}ids=[target.id];fit([target]);}
  }
  function remember(o:Item, field:HTMLTextAreaElement) {
    range={id:o.id,start:field.selectionStart,end:field.selectionEnd};
  }
  function updateText(o:Item, field:HTMLTextAreaElement) {
    editBody(o,field.value,Math.min(beforeStart ?? Infinity,field.selectionStart));
    beforeStart=undefined;remember(o,field);change();
  }
  function chooseLink(span?: {start:number;end:number}) {
    if(!editing || !active)return;
    draftRange=span && {start:span.start,end:span.end};
    const existing=span ? active.textLinks?.find(l=>l.start===span.start && l.end===span.end)?.to : objectLink(active);
    targetPage=existing?.pageId ?? current;targetItem=existing?.itemId ?? '';
    ask('Internal link');
  }
  function storeLink(to?:Destination) {
    if(!editing || !active)return;
    if(draftRange){
      const {start,end}=draftRange;
      const kept:TextLink[]=[];
      for(const old of active.textLinks ?? []){
        if(old.end<=start || old.start>=end){kept.push(old);continue;}
        if(old.start<start)kept.push({...old,end:start});
        if(old.end>end)kept.push({...old,start:end});
      }
      if(to)kept.push({start,end,to});
      active.textLinks=kept.sort((a,b)=>a.start-b.start);
    }else{active.link=to;if(active.kind==='Page link')active.target=undefined;}
    change();dialog.close();range=undefined;
  }
  type Revision={doc:typeof initial;page:string};
  let revisions:Revision[]=[{doc:structuredClone(initial),page:initial.pages[0].id}];
  let revision=$state(0), revisionCount=$state(1);
  let savedRevision:Revision|undefined=revisions[0], pending=false;
  let typingField:HTMLElement|null=null,lastTyped=0;
  let historyPageId=$state(''), chosenVersionId=$state('');
  let historyPage=$derived(project.pages.find(p=>p.id===historyPageId));
  let pageVersions=$derived((project.pageHistory??[]).filter(v=>v.page.id===historyPageId).slice().reverse());
  let chosenVersion=$derived(pageVersions.find(v=>v.id===chosenVersionId));
  let changedBy=new Map<string,ProjectUser>();
  function pageSnapshot(p:Page){
    const snapshot=$state.snapshot(p);
    delete snapshot.parentId;
    return snapshot;
  }
  function versionFor(p:Page, date:string|null, author?:ProjectUser, restoredFrom?:string):PageVersion{
    return {id:crypto.randomUUID(),date,author:author?{...author}:undefined,restoredFrom,page:pageSnapshot(p)};
  }
  function checkpointPages(only?:Page){
    const versions=[...project.pageHistory??[]];
    const latestByPage=new Map(versions.map(v=>[v.page.id,v]));
    for(const page of only?[only]:[...project.pages,...project.trash??[]]){
      const latest=latestByPage.get(page.id);
      if(!latest || JSON.stringify(latest.page)!==JSON.stringify(pageSnapshot(page))){
        versions.push(versionFor(page,new Date().toISOString(),changedBy.get(page.id)??editor));
      }
    }
    return versions;
  }
  function showPageHistory(p:Page){
    historyPageId=p.id;chosenVersionId='';ask('Page history');menu=undefined;
  }
  function restorePageVersion(){
    const p=historyPage, source=chosenVersion;
    if(!p || !source || !editor || !editing || busy)return;
    commit();
    const versions=checkpointPages(p);
    const saved=structuredClone($state.snapshot(source.page));
    p.name=saved.name;p.items=saved.items;p.departments=saved.departments;p.connections=saved.connections;
    versions.push(versionFor(p,new Date().toISOString(),editor,source.id));
    project.pageHistory=versions;
    change();
    chosenVersionId='';
    status='Page restored. Save to keep this version.';
  }
  function resetEdits(saved:boolean){
    changedBy.clear();
    if(saved && !project.pageHistory){
      project.pageHistory=[...project.pages,...project.trash??[]].map(p=>versionFor(p,null));
    }
    revisions=[{doc:$state.snapshot(project),page:current}];revision=0;revisionCount=1;
    savedRevision=saved?revisions[0]:undefined;pending=false;typingField=null;dirty=!saved;
  }
  function commit(){
    if(!pending)return;
    pending=false;
    const field=document.activeElement;
    const textField=field instanceof HTMLTextAreaElement || (field instanceof HTMLInputElement && field.type==='text')?field:null;
    const merge=textField && textField===typingField && performance.now()-lastTyped<750 &&
      revision>0 && revision===revisions.length-1 && revisions[revision]!==savedRevision;
    if(editor){
      const previous=revisions[revision].doc;
      const oldPages=new Map([...previous.pages,...previous.trash??[]].map(p=>[p.id,p]));
      for(const p of [...project.pages,...project.trash??[]]){
        const old=oldPages.get(p.id);
        if(!old || JSON.stringify(pageSnapshot(p))!==JSON.stringify(pageSnapshot(old)))changedBy.set(p.id,{...editor});
      }
    }
    const {pageHistory:_,...editable}=project;
    const entry={doc:$state.snapshot(editable),page:current};
    if(merge)revisions[revision]=entry;
    else{revisions.splice(revision+1);revisions.push(entry);if(revisions.length>101)revisions.shift();revision=revisions.length-1;}
    revisionCount=revisions.length;typingField=textField;lastTyped=performance.now();
  }
  let exportedPdf=$state('');
  function exportNotice(message:string,destination=''){
    exportedPdf=destination;status=message;
  }
  async function openExport(folder:boolean){
    try{await invoke('open_pdf_location',{destination:exportedPdf,folder});}
    catch(error){status=String(error);}
  }
  function change(){dirty=true;status='';pending=true;queueMicrotask(()=>{if(!gesture && !drag)commit();});}
  function restoreEdit(direction:number){
    if(!editing || busy || copying)return;
    commit();const next=revision+direction,entry=revisions[next];if(!entry)return;
    typingField=null;gesture=undefined;liveBox=undefined;drag=undefined;marquee=undefined;
    range=undefined;menu=undefined;inline='';frameOnly='';playing='';
    const oldPage=current,affectedPage=direction<0?revisions[revision].page:entry.page;
    const pageHistory=project.pageHistory;
    const {pageHistory:_,...editable}=entry.doc;
    project={...structuredClone(editable),pageHistory};revision=next;
    if(!project.users?.some(u=>u.id===editorId)){stopDrawing();editing=false;editorId='';}
    current=project.pages.find(p=>p.id===affectedPage)?.id??project.pages.find(p=>p.id===entry.page)?.id??project.pages[0].id;
    ids=ids.filter(id=>board.items.some(o=>o.id===id));
    filters=filters.filter(d=>departments.includes(d));
    history=history.filter(v=>project.pages.some(p=>p.id===v.page));cursor=history.map(v=>v.page).lastIndexOf(current);
    if(cursor<0){history.push(view());cursor=history.length-1;}trimHistory();
    if(current!==oldPage)fit();
    dirty=entry!==savedRevision || JSON.stringify(pageHistory)!==JSON.stringify(savedRevision?.doc.pageHistory);status=direction<0?'Undone':'Redone';
  }
  const readHint='Read mode is on. Press E to enable editing.';
  function toggleEdit() { stopDrawing();wire=undefined;selectedConnection='';if(!editing){selectEditor();return;} frameOnly='';editing=false;editorId=''; if(status===readHint)status='';inline='';gesture=undefined;liveBox=undefined;drag=undefined; }
  function fit(objects: Item[] = picked.length ? picked : board.items.filter(o=>matches(itemTags(o),filters))) {
    if (!objects.length) {zoom=1;tx=30;ty=40;return;}
    const boxes=objects.map(bounds);
    const left=Math.min(...boxes.map(o=>o.x)), top=Math.min(...boxes.map(o=>o.y));
    const w=Math.max(...boxes.map(o=>o.x+o.w))-left, h=Math.max(...boxes.map(o=>o.y+o.h))-top;
    zoom=Math.max(.05,Math.min(1.35,(width-70)/w,(height-70)/h));
    tx=(width-w*zoom)/2-left*zoom; ty=(height-h*zoom)/2-top*zoom;
  }
  async function navigate(p: Page) { stopDrawing();wire=undefined;selectedConnection=''; typingField=null;frameOnly='';playing='';
    rememberView();current=p.id; ids=[]; inline=''; range=undefined; drag=undefined;gesture=undefined;liveBox=undefined;marquee=undefined;menu=undefined;await tick();fit();
    history=history.slice(0,cursor+1);history.push(view());cursor=history.length-1;trimHistory();
  }
  function scale(factor:number, x=width/2,y=height/2) {
    const next=Math.max(.05,Math.min(5,zoom*factor));
    tx=x-(x-tx)*next/zoom; ty=y-(y-ty)*next/zoom; zoom=next;
  }
  function wheel(e: WheelEvent) {
    e.preventDefault(); const r=viewport.getBoundingClientRect();
    scale(Math.exp(-Math.max(-200,Math.min(200,e.deltaY))*.002),e.clientX-r.left,e.clientY-r.top);
  }
  let altHeld=$state(false),selectedConnection=$state('');
  let wire=$state<{pointer:number;from:ObjectPort;x:number;y:number;target?:ObjectPort}>();
  const connectable=$derived(board.items.filter(o=>matches(itemTags(o),filters)));
  const connectionObjects=$derived(new Map(connectable.map(o=>[o.id,o])));
  const arrows=$derived((board.connections??[]).flatMap(edge=>{
    const from=connectionObjects.get(edge.from.objectId),to=connectionObjects.get(edge.to.objectId);
    return from && to?[{id:edge.id,path:connectionPath(portPosition(from,edge.from.side),portPosition(to,edge.to.side))}]:[];
  }));
  const previewArrow=$derived.by(()=>{
    if(!wire)return;
    const from=connectionObjects.get(wire.from.objectId);
    if(!from)return;
    const target=wire.target && connectionObjects.get(wire.target.objectId);
    return connectionPath(portPosition(from,wire.from.side),target && wire.target
      ?portPosition(target,wire.target.side):{x:wire.x,y:wire.y,dx:0,dy:0});
  });
  function beginConnection(e:PointerEvent,from:ObjectPort){
    if(e.button!==0 || !editing)return;
    e.stopPropagation();e.preventDefault();
    const {x,y}=worldPoint(e);
    wire={pointer:e.pointerId,from,x,y};viewport.setPointerCapture(e.pointerId);
  }
  function updateConnection(e:PointerEvent){
    if(!wire || wire.pointer!==e.pointerId)return;
    const p=worldPoint(e);
    wire.x=p.x;wire.y=p.y;
    const hit=document.elementFromPoint(e.clientX,e.clientY)?.closest<HTMLElement>('[data-port-side]');
    const objectId=hit?.dataset.portObject,side=hit?.dataset.portSide as ObjectPort['side']|undefined;
    wire.target=objectId && side && objectId!==wire.from.objectId?{objectId,side}:undefined;
  }
  function finishConnection(e:PointerEvent){
    if(!wire || wire.pointer!==e.pointerId)return;
    updateConnection(e);
    const {from,target}=wire;wire=undefined;
    if(!target || !editing || !connectionObjects.has(from.objectId) || !connectionObjects.has(target.objectId))return;
    const same=(a:ObjectPort,b:ObjectPort)=>a.objectId===b.objectId && a.side===b.side;
    const existing=board.connections?.find(c=>same(c.from,from) && same(c.to,target));
    const id=existing?.id??crypto.randomUUID();
    if(!existing){board.connections=[...board.connections??[],{id,from,to:target}];change();}
    selectConnection(id);
  }
  function selectConnection(id:string){selectedConnection=id;ids=[];focusArea='canvas';menu=undefined;}
  function deleteConnection(){
    if(!editing || !selectedConnection)return;
    board.connections=board.connections?.filter(c=>c.id!==selectedConnection);selectedConnection='';change();
  }
  let drawingMode=$state(false),brushColor=$state('#4b8ee8'),brushSize=$state(5);
  let stroke=$state<(BrushStroke & {pointer:number})>();
  let draftStrokes=$state<BrushStroke[]>([]);
  function stopDrawing(){drawingMode=false;stroke=undefined;draftStrokes=[];}
  function acceptDrawing(continueDrawing:boolean){
    if(!drawingMode || !editing)return;
    const strokes=$state.snapshot(draftStrokes);
    if(stroke){
      const {points,color,thickness}=$state.snapshot(stroke);
      strokes.push({points,color,thickness});
    }
    if(strokes.length){
      let left=Infinity,right=-Infinity,top=Infinity,bottom=-Infinity;
      for(const {points,thickness} of strokes){
        const padding=thickness/2+2;
        for(const p of points){
          left=Math.min(left,p.x-padding);right=Math.max(right,p.x+padding);
          top=Math.min(top,p.y-padding);bottom=Math.max(bottom,p.y+padding);
        }
      }
      const w=right-left,h=bottom-top;
      const object=add('Drawing',{x:left,y:top,w,h,drawing:{width:w,height:h,strokes:strokes.map(s=>({
        ...s,points:s.points.map(p=>({x:p.x-left,y:p.y-top}))
      }))}});
      if(object)object.parent=board.items.find(f=>f.kind==='Frame' && contains(f,object))?.id;
    }
    draftStrokes=[];stroke=undefined;
    if(!continueDrawing)stopDrawing();
  }
  function extendStroke(e:PointerEvent){
    if(!stroke || stroke.pointer!==e.pointerId)return;
    const p=worldPoint(e);
    const last=stroke.points[stroke.points.length-1];
    if(Math.hypot(last.x-p.x,last.y-p.y)>=.5/zoom)stroke.points.push(p);
  }
  function finishStroke(e:PointerEvent){
    if(!stroke || stroke.pointer!==e.pointerId)return;
    extendStroke(e);
    const {points,color,thickness}=$state.snapshot(stroke);
    draftStrokes.push({points,color,thickness});
    stroke=undefined;
  }
  function down(e:PointerEvent) {
    if(e.button>1 || (e.button===0 && (e.target as HTMLElement).closest('input,textarea,button,audio,video,.drawing-toolbar')))return;
    focusArea='canvas';menu=undefined;selectedConnection='';
    if(drawingMode && editing && e.button===0){
      if(stroke || drag)return;
      e.preventDefault();
      stroke={pointer:e.pointerId,points:[worldPoint(e)],color:brushColor,thickness:brushSize/zoom};
      viewport.setPointerCapture(e.pointerId);
      return;
    }
    if(stroke)return;
    const hit=(e.target as HTMLElement).closest<HTMLElement>('[data-id]');
    const o=board.items.find(v=>v.id===hit?.dataset.id);
    if(titleId)(document.activeElement as HTMLElement)?.blur();const pan=e.button===1;if(!pan && (o?.id!==frameOnly || e.shiftKey))frameOnly='';
    if(!pan){
      inline='';range=undefined;
      if(!o){const p=worldPoint(e);marquee={...p,endX:p.x,endY:p.y,prior:e.shiftKey?[...ids]:[]};ids=marquee.prior;viewport.setPointerCapture(e.pointerId);return;}
      if(e.shiftKey){ids=ids.includes(o.id)?ids.filter(id=>id!==o.id):[...ids,o.id];return;}
      if(!ids.includes(o.id))ids=[o.id];
      if(!editing)return;
    }
    e.preventDefault();drag={x:e.clientX,y:e.clientY,tx,ty,objects:pan?[]:(active && active.id===frameOnly?[active]:affected).map(o=>({o,x:o.x,y:o.y})),pan};
    (hit?(e.target as Element):viewport).setPointerCapture(e.pointerId);
  }
  function move(e:PointerEvent) {
    if(wire){updateConnection(e);return;}
    if(stroke){extendStroke(e);return;}
    if(marquee){
      const p=worldPoint(e),m=marquee;m.endX=p.x;m.endY=p.y;
      const l=Math.min(m.x,p.x),r=Math.max(m.x,p.x),t=Math.min(m.y,p.y),b=Math.max(m.y,p.y);
      ids=[...new Set([...m.prior,...board.items.filter(o=>{const v=bounds(o);return (matches(itemTags(o),filters)) && v.x>=l && v.y>=t && v.x+v.w<=r && v.y+v.h<=b;}).map(o=>o.id)])];return;
    }
    if(gesture && editing){
      const {box,px,py,handle,startAngle,children}=gesture;
      const {x,y}=worldPoint(e);
      const next=handle==='rotate'?{...box,rotation:(box.rotation??0)+(Math.atan2(y-box.y-box.h/2,x-box.x-box.w/2)-startAngle)*180/Math.PI}:resize(box,handle,x-px,y-py,e.ctrlKey?snapping.resize:0,e.shiftKey);
      if(handle==='rotate' && e.ctrlKey)next.rotation=snap(next.rotation??0,snapping.rotate)%360;
      liveBox=next;
      for(const child of children)Object.assign(child.o,mapChild(child.box,box,next));
      change();return;
    }
    if(!drag) return;
    const dx=e.clientX-drag.x,dy=e.clientY-drag.y;
    if(drag.pan){tx=drag.tx+dx;ty=drag.ty+dy;}
    else if(dx || dy){const anchor=drag.objects[0];if(!anchor)return;const step=e.ctrlKey?snapping.move:0;
      const sx=snap(anchor.x+dx/zoom,step)-anchor.x,sy=snap(anchor.y+dy/zoom,step)-anchor.y;
      for(const v of drag.objects){v.o.x=v.x+sx;v.o.y=v.y+sy;}change();}
  }
  function up(e:PointerEvent) {
    if(wire){finishConnection(e);return;}
    if(stroke){finishStroke(e);return;}
    marquee=undefined;gesture=undefined;liveBox=undefined;
    if(drag && !drag.pan && Math.hypot(e.clientX-drag.x,e.clientY-drag.y)>3){
      for(const o of picked){
        if(o.kind!=='Frame' && (!o.parent || !ids.includes(o.parent)))
          o.parent=board.items.find(f=>f.kind==='Frame' && contains(f,o))?.id;
      }
    }
    drag=undefined;commit();
  }
  let titleId=$state(''), titleDraft=$state('');
  function finishTitle(o:Item,apply:boolean){
    if(titleId!==o.id)return;
    titleId='';
    if(apply && titleDraft.trim() && titleDraft.trim()!==o.name){o.name=titleDraft.trim();change();}
  }
  async function activate(o:Item,event:MouseEvent) {
    const target=event.target as Element;
    if(target.closest('input,textarea,audio,video'))return;
    if(editing && o.kind==='Note' && target.closest('.caption')){
      ids=[o.id];inline='';titleDraft=o.name;titleId=o.id;await tick();
      const field=viewport.querySelector<HTMLInputElement>('.note-title');field?.focus();field?.select();return;
    }
    if(editing && o.kind==='Frame'){ids=[o.id];frameOnly=frameOnly===o.id?'':o.id;return;}
    if(o.attachment || o.url){await openResource(o);return;}
    const to=objectLink(o);
    if(to && (!editing || o.kind==='Page link'))await follow(to);
    else if(!editing)status=readHint;
    else if(editing && o.kind==='Note'){inline=o.id;await tick();viewport.querySelector('textarea')?.focus();}
  }
  function add(kind:Kind, extra:Partial<Item>={}) {
    if(!editing)return;
    const o=item(kind,kind==='Note'?'New note':kind,(width/2-tx)/zoom-110,(height/2-ty)/zoom-60,{inheritDepartment:true,...extra});
    if(kind==='Frame'){o.w=480;o.h=300;board.items.unshift(o);}else board.items.push(o);
    ids=[o.id];focusArea='canvas';change();return o;
  }
  async function importImages(files:File[], x?:number,y?:number) {
    if(!editing || !files.length)return;
    const sourcePage=current;
    for(const [i,f] of files.entries()) {
      if(!['image/png','image/jpeg','image/gif','image/webp'].includes(f.type)){status='Choose a PNG, JPEG, GIF or WebP image.';continue;}
      try {
        const source=await new Promise<string>((resolve,reject)=>{const r=new FileReader();r.onload=()=>resolve(String(r.result));r.onerror=reject;r.readAsDataURL(f);});
        const img=new Image();img.src=source;await img.decode();
        if(current!==sourcePage || !editing)return;
        const w=Math.min(360,img.naturalWidth), h=w*img.naturalHeight/img.naturalWidth+32;
        add('Image',{name:f.name,source,w,h,department:'Art',x:(x??(width/2-tx)/zoom)+i*18,y:(y??(height/2-ty)/zoom)+i*18});
      } catch(err){recordError('Image import',err);status='This image could not be loaded.';}
    }
  }
  function drop(e:DragEvent){
    e.preventDefault();if(!editing){status=readHint;return;}if(!e.dataTransfer)return;
    const r=viewport.getBoundingClientRect(),x=(e.clientX-r.left-tx)/zoom,y=(e.clientY-r.top-ty)/zoom;
    const p=project.pages.find(v=>v.id===e.dataTransfer!.getData('studio/page'));
    if(p)add('Page link',{name:p.name,target:p.id,x,y,w:210,h:84});
    else if(insertUrl(e.dataTransfer.getData('text/uri-list') || e.dataTransfer.getData('text/plain'),x,y))return;
    else void importImages([...e.dataTransfer.files],x,y);
  }
  function newProject(){section='';
    creationError='';draftUsers=[];draftName='';draftDescription='';draftCatalog=defaultCatalog.map(d=>({...d}));ask('New project');
  }
  function ask(type:string){dialogType=type;name='';dialog.showModal();}
  async function discard(){return !dirty || (isTauri()?await confirm('Discard unsaved changes?'):window.confirm('Discard unsaved changes?'));}
  async function closeProject(){
    if(busy || !await discard())return;
    stopDrawing();welcome=true;welcomeError='';path='';status='';editing=false;
    project=initial;current=initial.pages[0].id;resetViews();resetEdits(true);
    inline='';frameOnly='';titleId='';playing='';range=undefined;marquee=undefined;
    gesture=undefined;drag=undefined;liveBox=undefined;
    localStorage.removeItem('studio-last');
    await checkRecent();
  }
  async function named(){
    if(dialogType==='New project'){
      if(busy || !validUsers || !draftName.trim() || draftCatalog.some(d=>!d.name.trim()))return;
      busy=true;creationError='';
      try{
        if(!await discard())return;
        const p=page('Overview');
        const next:typeof initial={version:1,users:draftUsers.map(u=>({...u,name:u.name.trim()})),name:draftName.trim(),description:draftDescription.trim(),catalog:draftCatalog.map(d=>({...d,name:d.name.trim()})),pages:[p]};
        const location=isTauri()?await save({defaultPath:next.name+'.gdstudio',filters:fileFilters}):'';
        if(location===null)return;
        if(location)await invoke('save_project',{path:location,document:JSON.stringify(next)});
        project=next;path=location;welcome=false;editorId='';editing=false;showFps=false;resetViews();await navigate(p);resetEdits(!!location);rememberProject(location);status='Project created';dialog.close();selectEditor();
      }catch(error){creationError=String(error);recordError('Create project',error);}
      finally{busy=false;}
      return;
    }
    if(!name.trim())return;
    const p=page(name.trim());p.parentId=pageParent;project.pages.push(p);await navigate(p);
    change();dialog.close();
  }
  async function load(raw:string,location:string){
    const data:unknown=JSON.parse(raw);
    if(data && typeof data==='object' && 'pages' in data && Array.isArray(data.pages)){
      migrateLegacyDiagrams(data as Project);
    }
    if(!validProject(data))throw Error('Unsupported or invalid project.');
    if(!await discard())return;
    project=data;path=location;welcome=false;rememberProject(location);dirty=false;editorId='';editing=false;showFps=false;resetViews();await navigate(project.pages[0]);resetEdits(true);status='Project opened';
  }
  async function openFile(){
    if(!isTauri()){fileInput.click();return;}
    try{const f=await open({filters:fileFilters});if(f)await load(await invoke('open_project',{path:f}),f);}
    catch(e){recordError('Project file',e);status=String(e);if(welcome)welcomeError=status;}
  }
  async function saveFile(){
    if(busy)return;busy=true;
    try{
      const location=isTauri()?(path || await save({defaultPath:project.name+'.gdstudio',filters:fileFilters})):'';
      if(isTauri() && !location)return;
      commit();typingField=null;const saving=revisions[revision];
      const pageHistory=checkpointPages();
      const raw=JSON.stringify({...project,pageHistory});
      if(location){await invoke('save_project',{path:location,document:raw});path=location;rememberProject(location);}
      else{const a=document.createElement('a');const url=URL.createObjectURL(new Blob([raw]));a.href=url;a.download=project.name+'.gdstudio';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}
      project.pageHistory=pageHistory;
      savedRevision={...saving,doc:{...saving.doc,pageHistory}};
      if(revisions[revision]===saving)revisions[revision]=savedRevision;
      dirty=JSON.stringify(project)!==raw;status='Project saved';
    }catch(e){recordError('Project file',e);status=String(e);if(welcome)welcomeError=status;}finally{busy=false;}
  }
  let exporter:ExportDialog;
  async function ensureExportSaved(){
    await saveFile();
    return !!path && !dirty;
  }
  async function convertExternal(paths:string[]){
    if(!path && !await ensureExportSaved())throw Error('Choose a project location before copying files.');
    const copies=await invoke<{attachment:Attachment}[]>('import_attachments',{project:path,paths,local:true});
    if(copies.length!==paths.length)throw Error('Some files could not be copied.');
    const replacements=new Map(paths.map((file,i)=>[file,copies[i].attachment]));
    for(const p of [...project.pages,...project.trash??[],...(project.pageHistory??[]).map(v=>v.page)])for(const o of p.items){
      if(!o.attachment || o.attachment.local)continue;
      const copy=replacements.get(o.attachment.path);
      if(copy)o.attachment=copy;
    }
    change();
    if(!await ensureExportSaved())throw Error(status || 'Unable to save the updated references.');
  }
  function keys(e:KeyboardEvent){
    if(e.key==='Alt' && !welcome && editing && !drawingMode && !document.querySelector('dialog[open]')){
      e.preventDefault();altHeld=true;return;
    }
    if(e.key==='Escape' && (wire || selectedConnection)){
      e.preventDefault();wire=undefined;selectedConnection='';return;
    }
    if(welcome || (e.target as HTMLElement).closest?.('audio,video'))return;
    if(drawingMode && !document.querySelector('dialog[open]')){
      if(e.key==='Escape'){e.preventDefault();stopDrawing();menu=undefined;return;}
      if(e.key==='Enter' && !e.ctrlKey && !e.metaKey && !e.altKey){
        e.preventDefault();if(!e.repeat)acceptDrawing(true);return;
      }
    }
    if(e.key==='Escape' && (menu || fileMenu.open)){menu=undefined;fileMenu.open=false;return;}
    if(e.key==='Escape' && historyMenu?.open){historyMenu.open=false;historyMenu.querySelector('summary')?.focus();return;}
    if(document.querySelector('dialog[open]'))return;
    if((e.target as HTMLElement).closest?.('.note-title')){
      if((e.ctrlKey || e.metaKey) && e.key.toLowerCase()==='s' && active){
        e.preventDefault();finishTitle(active,true);void saveFile();
      }
      return;
    }
    const key=e.key.toLowerCase();
    if((e.ctrlKey || e.metaKey) && !e.altKey && (key==='z' || key==='y')){
      e.preventDefault();restoreEdit(key==='y' || e.shiftKey?1:-1);return;
    }
    if(e.key==='Escape'){commit();frameOnly='';menu=undefined;gesture=undefined;liveBox=undefined;drag=undefined;marquee=undefined;}
    if((e.target as HTMLElement).closest('input,textarea,select') && e.key==='Delete')return;
    if(!editing && e.key==='Delete' && (ids.length || focusArea==='tree')){e.preventDefault();status=readHint;return;}
    if((e.ctrlKey||e.metaKey) && e.shiftKey && ['ArrowLeft','ArrowRight'].includes(e.key)){e.preventDefault();travel(e.key==='ArrowLeft'?-1:1);return;}
    if(editing && e.key==='Delete' && !e.repeat){e.preventDefault();const focusedPage=(e.target as HTMLElement).closest<HTMLElement>('[data-page]')?.dataset.page;
      if(menu?.page || (focusArea==='tree' && focusedPage))requestDelete(menu?.page??project.pages.find(p=>p.id===focusedPage)??board);
      else if(selectedConnection)deleteConnection();
      else if(ids.length)removeObjects();return;}
    if(editing && active && (e.ctrlKey||e.metaKey) && e.key.toLowerCase()==='k'){
      e.preventDefault();
      if(e.target instanceof HTMLTextAreaElement && e.target.selectionEnd>e.target.selectionStart)
        chooseLink({start:e.target.selectionStart,end:e.target.selectionEnd});
      else chooseLink();
      return;
    }
    if((e.ctrlKey||e.metaKey) && e.key.toLowerCase()==='s'){e.preventDefault();void saveFile();return;}
    if((e.target as HTMLElement).closest('input,textarea,select') || e.ctrlKey || e.metaKey || e.altKey)return;
    if(e.key.toLowerCase()==='f')fit();
    if(e.key.toLowerCase()==='e' && !e.repeat)toggleEdit();
    if(e.key==='Escape'){inline='';ids=[];}
  }
  async function createTemplate(){
    if(busy)return;
    busy=true;
    try{
      const folder=isTauri()?await open({directory:true,title:'Choose a folder for the template project'}):'';
      if(folder===null || !await discard())return;
      const example=sample();example.name='Template Project';example.pages.push(benchmark());
      const location=isTauri()?await invoke<string>('create_template',{folder,document:JSON.stringify(example)}):'';
      project=example;path=location;welcome=false;rememberProject(location);editorId='';editing=false;showFps=true;resetViews();await navigate(example.pages[0]);resetEdits(!!location);
      status=location?'Template created in '+folder:'Template ready. Save to download it.';selectEditor();
    }catch(err){status=String(err);}finally{busy=false;}
  }
  onMount(()=>{
    if(isTauri())void getVersion().then(version=>{appVersion=version;}).catch(()=>{appVersion='Unavailable';});
    const listener=isTauri()?getCurrentWebview().onDragDropEvent(({payload})=>{
      if(welcome || payload.type!=='drop')return;
      const r=viewport.getBoundingClientRect(),x=payload.position.x/devicePixelRatio-r.left,y=payload.position.y/devicePixelRatio-r.top;
      if(x>=0 && y>=0 && x<=r.width && y<=r.height)offerFiles(payload.paths,(x-tx)/zoom,(y-ty)/zoom);
    }):undefined;
    updateChecksActive=isTauri();
    if(updateChecksActive)void invoke<boolean>('update_status').then(failed=>{
      if(failed){
        status='Update could not replace the executable. Close other copies and try again.';
        scheduleUpdateCheck();
      }else void checkUpdates();
    }).catch(()=>{void checkUpdates();});
    const last=localStorage.getItem('studio-last');
    if(isTauri() && startup==='last' && last)void openRecent(last);
    else void checkRecent();
     let id=0,start=performance.now(),frames=0;
    function frame(t:number){frames++;if(t-start>=750){fps=Math.round(frames*1000/(t-start));start=t;frames=0;}id=requestAnimationFrame(frame);}
    id=requestAnimationFrame(frame);
    const before=(e:BeforeUnloadEvent)=>{if(dirty){e.preventDefault();e.returnValue='';}};
    const refresh=()=>{if(welcome)void checkRecent();};
    window.addEventListener('focus',refresh);
    window.addEventListener('beforeunload',before);
    return ()=>{window.removeEventListener('focus',refresh);updateChecksActive=false;clearTimeout(updateTimer);void listener?.then(stop=>stop());cancelAnimationFrame(id);window.removeEventListener('beforeunload',before);};
  });
</script>

{#snippet updatesButton()}
  <button class="update-button" aria-label={updateLabel} title={updateLabel} disabled={!isTauri() || (!release && (checking || cooldown))} onclick={openUpdates}>
    <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v12m-5-5 5 5 5-5M4 16v4h16v-4"/></svg>
    {#if release}<span class="update-badge" aria-hidden="true"></span>{/if}
  </button>
{/snippet}

{#snippet closePanel(close:()=>void)}
  <button type="button" class="close-panel" title="Close" aria-label="Close" onclick={close}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8"/></svg></button>
{/snippet}
{#snippet imageTitle(image:Item)}
  <label class="check image-title"><input type="checkbox" disabled={!editing} checked={image.showTitle!==false}
    onchange={e=>{image.showTitle=e.currentTarget.checked;change();}}/> Show title</label>
{/snippet}
{#snippet departmentPicker()}
  <details class="object-departments">
    <summary>Departments</summary>
    <fieldset disabled={!editing}>
      <label><input type="checkbox" checked={picked.length>0 && picked.every(o=>o.inheritDepartment)}
        indeterminate={picked.some(o=>o.inheritDepartment) && picked.some(o=>!o.inheritDepartment)}
        onchange={e=>inheritTags(e.currentTarget.checked)}/> Inherit from page</label>
      {#each catalog as d (d.id)}
        {@const count=picked.filter(o=>itemTags(o).includes(d.id)).length}
        <label><input type="checkbox" checked={count>0 && count===picked.length} indeterminate={count>0 && count<picked.length}
          onchange={e=>toggleObjectTag(d.id,e.currentTarget.checked)}/><i class="department-dot" style:background={d.color}></i>{d.name}</label>
      {/each}
    </fieldset>
  </details>
{/snippet}
<svelte:window onpaste={paste} onpointermove={e=>pagePointerMove(e)} onpointerup={e=>pagePointerMove(e,true)} onpointercancel={()=>{pagePointer=undefined;draggingPage='';treeDrop='';}} onkeydown={keys} onkeyup={e=>{if(e.key==='Alt')altHeld=false;}} onblur={()=>{altHeld=false;wire=undefined;}} oncontextmenu={e=>e.preventDefault()} onpointerdown={e=>{if(!(e.target as HTMLElement).closest('.app-menu'))menu=undefined;if(historyMenu && !historyMenu.contains(e.target as Node))historyMenu.open=false;if(fileMenu && !fileMenu.contains(e.target as Node))fileMenu.open=false;}}/>
<div class="studio" data-theme={theme} class:launcher={welcome}>
  {#if welcome}
    <main class="welcome">
      <div class="welcome-head"><h1><img src={tenkivoIcon} alt="" width="48" height="48"/>Tenkivo</h1><div class="app-actions"><button title="Settings" aria-label="Settings" onclick={()=>settings.showModal()}>⚙</button>{@render updatesButton()}</div></div>
      <div class="welcome-actions"><button disabled={launching} onclick={newProject}>New project…</button><button disabled={launching} onclick={openFile}>Open project…</button></div>
      <h2>Recent projects</h2>
      {#if welcomeError}<p role="alert">{welcomeError}</p>{/if}
      {#if launching}<p role="status">Opening project…</p>{/if}
      {#each recent as entry (entry.path)}
        {@const state=availability[entry.path]??'Checking…'}
        <div class="recent-project"><button disabled={launching || state!=='Available'} onclick={()=>openRecent(entry.path)}><strong>{entry.name}</strong><small>{entry.path}</small></button>
          <span class:available={state==='Available'}><b aria-hidden="true">{state==='Available'?'✓':state==='Checking…'?'…':'⚠'}</b> {state}</span>
          {#if state==='Missing' || state==='Unavailable'}<button title="Remove from recent projects" onclick={()=>forgetProject(entry.path)}>Remove</button>{/if}
        </div>
      {:else}<p>No recent projects.</p>{/each}
    </main>
  {:else}
  <header>
    <nav><details class="file-menu" bind:this={fileMenu}>
      <summary>File</summary><div class="file-list">
        <button onclick={()=>{fileMenu.open=false;newProject();}}>New…</button>
        <button disabled={busy} onclick={()=>{fileMenu.open=false;void createTemplate();}}>New Template…</button>
        <button onclick={()=>{fileMenu.open=false;void openFile();}}>Open…</button>
        <button disabled={busy} onclick={()=>{fileMenu.open=false;void closeProject();}}>Close</button>
        <hr/>
        <button disabled={busy} onclick={()=>{fileMenu.open=false;void saveFile();}}>Save</button>
        <hr/>
        <button disabled={busy} onclick={()=>{fileMenu.open=false;exporter.show();}}>Export…</button>
      </div></details><button disabled={busy} title={dirty?'Save unsaved changes':'Save project'} onclick={saveFile}>{dirty?'Save *':'Save'}</button></nav>
    <div class="app-actions">
      <button aria-label="Keyboard shortcuts" title="Keyboard shortcuts" onclick={()=>ask('Keyboard shortcuts')}><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="2" y="5" width="20" height="14" rx="3"/><path d="M5 9h2m2 0h2m2 0h2m2 0h2M5 12h2m2 0h2m2 0h2m2 0h2M8 15h8"/></svg></button>
      <button class="settings-button" aria-label="Settings" title="Settings" onclick={()=>settings.showModal()}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m10 3-.6 2.3-2 .9-2.1-.8-2 3.4L5 10.4v2.8l-1.7 1.6 2 3.5 2.2-.8 2 .9.5 2.2h4l.6-2.2 2-.9 2.1.8 2-3.5-1.7-1.6v-2.8l1.7-1.6-2-3.4-2.2.8-2-.9L14 3z"/><circle cx="12" cy="12" r="3"/></svg></button>
      {@render updatesButton()}
      <button aria-label="Editor logs" title="Editor logs" onclick={()=>ask('Editor logs')}><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="3" width="16" height="18" rx="2"/><path d="M8 8h8M8 12h8M8 16h5"/></svg></button>
    </div>
  </header>
  <div class="project-tools"><div class="filters"><button class:chosen={!filters.length} onclick={()=>setFilters([])}>All</button>{#each departments as d}<button aria-pressed={filters.includes(d)} class:chosen={filters.includes(d)} onclick={()=>setFilters(filters.includes(d)?filters.filter(v=>v!==d):[...filters,d])}><i style:background={colors[d]}></i>{names[d]}</button>{/each}</div>
    <button class="mode-switch" class:edit={editing} class:read-mode={!editing} onclick={toggleEdit} title={editing?'Switch to read mode (E)':'Enable editing (E)'}>{#if !editing}<svg aria-hidden="true" width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.7"><rect x="4" y="8" width="12" height="9" rx="2"/><path d="M6.5 8V6a3.5 3.5 0 0 1 7 0v2"/></svg>{/if}{editing?'Editing · '+(editor?.name??''):'Read mode'}</button>
  </div>
  <div class="workspace">
  <DockPanel name="Pages" panelClass="sidebar" bind:side={pagesSide} bind:open={pagesOpen}><div class="project-heading"><h2>{project.name}</h2><button aria-label="Project settings" title="Project settings" onclick={projectOptions}>⚙</button></div>
    <div class="pages-tools"><span class="section">Pages</span><button title="Add root page" aria-label="Add root page" disabled={!editing} onclick={()=>newPage()}>+</button></div>
    <div class="page-scroll">
    {#each pageRows as {p,depth} (p.id)}
      <div class="tree-row" class:ancestor={!matches(pageTags.get(p.id)??[],filters)}>
      <button class="page" class:chosen={p.id===current} style:padding-left={9+depth*17+'px'} data-drop={treeDrop.startsWith(p.id+' ')?treeDrop.split(' ').at(-1):''}
        onpointerdown={e=>pageDown(e,p)} draggable={editing && !isTauri()} ondragstart={e=>{draggingPage=p.id;e.dataTransfer?.setData('studio/page',p.id);}}
        ondragend={()=>{treeDrop='';draggingPage='';}} ondragover={e=>{if(editing && draggingPage){e.preventDefault();treeDrop=p.id+' '+placement(e);}}}
        ondrop={e=>movePage(e,p)} oncontextmenu={e=>{focusArea='tree';context(e,p);}} data-page={p.id} onfocus={()=>focusArea='tree'} onclick={()=>{if(suppressPageClick){suppressPageClick=false;return;}focusArea='tree';void navigate(p);}}><span>▧</span><span class="page-name">{p.name}</span></button>
      <span class="page-tags" title={(pageTags.get(p.id)??[]).map(d=>names[d]).join(', ')}>{#each pageTags.get(p.id)??[] as d}<i style:background={colors[d]}></i>{/each}</span>
      <button class="page-child" disabled={!editing} title={'Add child page to '+p.name} aria-label={'Add child page to '+p.name} onclick={()=>newPage(p)}>+</button></div>
    {/each}
    {#if draggingPage}<button class="root-drop" ondragover={e=>e.preventDefault()} ondrop={e=>movePage(e)}>Move to project root</button>{/if}
    </div>
    <details class="page-trash"><summary><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M3 6h18M9 6V3h6v3M6 6l1 15h10l1-15M10 9v9m4-9v9"/></svg>Trash ({project.trash?.length??0})</summary>
      <div>{#each project.trash??[] as p}<button disabled={!editing} onclick={()=>restorePage(p)}>Restore {p.name}</button>{:else}<small>Trash is empty.</small>{/each}</div>
    </details>
  </DockPanel>
  <main><div class="boardhead"><div><div class="canvas-label"><span class="section">Canvas</span>{#if !editing}<span class="read-caption">Editing is locked</span><button class="enable-edit" onclick={toggleEdit}>Enable editing <kbd>E</kbd></button>{/if}</div><h1>{board.name}</h1></div></div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="viewport" class:drawing-mode={drawingMode} bind:this={viewport} bind:clientWidth={width} bind:clientHeight={height} oncontextmenu={e=>canvasMenu(e)} onwheel={wheel} onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={()=>{wire=undefined;stroke=undefined;drag=undefined;gesture=undefined;liveBox=undefined;marquee=undefined;commit();}} ondragover={e=>e.preventDefault()} ondrop={drop}>
      <button class="canvas-add" aria-label="Add canvas element" title="Add canvas element" disabled={!editing} onpointerdown={e=>e.stopPropagation()} onclick={e=>canvasMenu(e,true)}>+</button>
      {#if drawingMode}
        <div class="drawing-input"></div>
        <div class="drawing-toolbar" role="group" aria-label="Drawing tools">
          <label>Color <input type="color" bind:value={brushColor}/></label>
          <label>Size <input type="range" min="1" max="32" bind:value={brushSize}/><output>{brushSize}</output></label>
          <button onclick={()=>acceptDrawing(false)}>Done</button>
          <button onclick={stopDrawing}>Cancel <kbd>Esc</kbd></button>
        </div>
      {/if}
      <div class="world" style:transform={'translate('+tx+'px,'+ty+'px) scale('+zoom+')'}>
        {#each visible as o (o.id)}
          {@const destination=objectLink(o)}
          {@const media=o.attachment && mediaKind(o.attachment)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div data-id={o.id} class="object" class:drawing-object={o.kind==='Drawing'} class:frame={o.kind==='Frame'} class:selected={ids.includes(o.id)} oncontextmenu={e=>{focusArea='canvas';context(e,undefined,o);}} style:left={o.x+'px'} style:top={o.y+'px'} style:width={o.w+'px'} style:height={o.h+'px'} style:transform={'rotate('+(o.rotation??0)+'deg)'} style:--dept={colors[itemTags(o)[0]]??'var(--line)'} ondblclick={e=>activate(o,e)}>
            {#if o.kind==='Image' && o.attachment}<AttachmentImage project={path} attachment={o.attachment} name={o.name}/>{:else if o.kind==='Image'}<img draggable="false" src={o.source} alt={o.name} decoding="async"/>{/if}
            {#if o.drawing}<DrawingStroke drawing={o.drawing}/>{/if}
            {#if o.kind!=='Drawing' && (o.kind!=='Image' || o.showTitle!==false)}<div class="caption">{#if editing && titleId===o.id}<input class="note-title" aria-label="Note title" bind:value={titleDraft}
              onblur={()=>finishTitle(o,true)} onkeydown={e=>{if(e.key==='Enter' || e.key==='Escape'){e.preventDefault();e.stopPropagation();finishTitle(o,e.key==='Enter');}}}/>
              {:else}{#if o.kind==='Frame'}▣　{:else if o.kind==='Page link'}↗　{/if}{o.name}{/if}{#if editing && active?.id===o.id && frameOnly===o.id}<small>Frame only</small>{/if}</div>{/if}
            {#if o.kind==='Attachment' && o.attachment && media}
              <MediaPlayer kind={media} project={path} attachment={o.attachment} name={o.name}/>
            {:else if o.kind==='Attachment'}<small>{o.attachment?.folder?'Folder':'File'} · {o.attachment?.local?'Project copy':'Linked original'}</small>{/if}
            {#if o.url}
              {@const video=videoId(o.url)}
              {#if video && playing===o.id}<iframe title={o.name} src={'https://www.youtube-nocookie.com/embed/'+video+'?autoplay=1'} referrerpolicy="strict-origin-when-cross-origin" allow="autoplay; encrypted-media; picture-in-picture" allowfullscreen></iframe>
              {:else if video}<button onclick={()=>playing=o.id}>▶ Play video</button>{/if}
              <small>↗ {webAddress(o.url)?.hostname}</small>{#if o.body}<div class="body">{o.body}</div>{/if}
            {/if}
            {#if o.kind==='Note'}
              {#if inline===o.id}
                <textarea aria-label="Edit note" use:fitText={{enabled:o.autoFit,size:o.size,text:o.body}} value={o.body}
                  onbeforeinput={e=>beforeStart=e.currentTarget.selectionStart}
                  onselect={e=>remember(o,e.currentTarget)} oninput={e=>updateText(o,e.currentTarget)} onblur={()=>inline=''}></textarea>
              {:else}
                <div class="body" use:fitText={{enabled:o.autoFit,size:o.size,text:o.body}}>{#each textParts(o) as part}{#if part.to}<button class="inline-link" title={labelFor(part.to)} onclick={()=>follow(part.to!)}>{part.text}</button>{:else}{part.text}{/if}{/each}</div>
              {/if}
            {/if}
            {#if destination}<button class="object-link" title={'Open '+labelFor(destination)} aria-label={'Open '+labelFor(destination)} onclick={()=>follow(destination)}>↗</button>{/if}
          </div>
        {/each}
        <svg class="canvas-connections" width="1" height="1" aria-label="Canvas connections">
          <defs><marker id="canvas-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto"><path d="M0 0L10 5L0 10Z" fill="context-stroke"/></marker></defs>
          {#each arrows as arrow (arrow.id)}
            <path d={arrow.path} class="connection-line" class:chosen={selectedConnection===arrow.id} marker-end="url(#canvas-arrow)"/>
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <path d={arrow.path} class="connection-hit" onpointerdown={e=>{if(e.button===0){e.stopPropagation();selectConnection(arrow.id);}}}/>
          {/each}
          {#if previewArrow}<path d={previewArrow} class="connection-line chosen" marker-end="url(#canvas-arrow)"/>{/if}
        </svg>
        {#if editing && !drawingMode && (altHeld || wire)}
          {#each visible as o (o.id)}
            {#each portSides as side}
              {@const port=portPosition(o,side)}
              <button class="connection-port" data-port-object={o.id} data-port-side={side}
                class:target-port={wire?.target?.objectId===o.id && wire?.target?.side===side}
                aria-label={'Connect '+o.name+' '+side} style:left={port.x+'px'} style:top={port.y+'px'}
                style:--port-size={14/zoom+'px'} onpointerdown={e=>beginConnection(e,{objectId:o.id,side})}></button>
            {/each}
          {/each}
        {/if}
        {#if draftStrokes.length || stroke}
          <div class="drawing-preview"><DrawingStroke drawing={{width:1,height:1,strokes:stroke?[...draftStrokes,stroke]:draftStrokes}}/></div>
        {/if}
        {#if marquee}<div class="marquee" style:left={Math.min(marquee.x,marquee.endX)+'px'} style:top={Math.min(marquee.y,marquee.endY)+'px'} style:width={Math.abs(marquee.endX-marquee.x)+'px'} style:height={Math.abs(marquee.endY-marquee.y)+'px'}></div>{/if}
        {#if box && editing && !drawingMode}
          <div class="handles" class:group={picked.length>1} style:left={box.x+'px'} style:top={box.y+'px'} style:width={box.w+'px'} style:height={box.h+'px'} style:transform={'rotate('+(box.rotation??0)+'deg)'} style:--unit={1/zoom+'px'}>
            {#each ['nw','ne','sw','se','rotate'] as handle}
              <button class={'handle '+handle} aria-label={handle==='rotate'?'Rotate object':'Resize '+handle} title={handle==='rotate'?'Drag to rotate':'Drag to resize'} onpointerdown={e=>beginHandle(e,handle)}></button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
    <footer>
    <div class="navigation-tools">
    <button aria-label="Back" title="Back (Ctrl+Shift+Left)" disabled={cursor<=0} onclick={()=>travel(-1)}>←</button>
    <details class="nav-history" bind:this={historyMenu}>
      <summary aria-label="Navigation history" title="Navigation history">▾</summary>
      <div class="history-list">{#each history as entry,i}
        <button aria-current={i===cursor?'page':undefined} onclick={()=>travel(i-cursor)}>{project.pages.find(p=>p.id===entry.page)?.name??'Page'}{#if i===cursor}<span aria-hidden="true">✓</span>{/if}</button>
      {/each}</div>
    </details>
    <button aria-label="Forward" title="Forward (Ctrl+Shift+Right)" disabled={cursor>=history.length-1} onclick={()=>travel(1)}>→</button>
    </div>
    <span class="divider" aria-hidden="true"></span>
    <button aria-label="Undo" title="Undo (Ctrl/Cmd + Z)" disabled={!editing || revision===0} onclick={()=>restoreEdit(-1)}>↶</button>
    <button aria-label="Redo" title="Redo (Ctrl/Cmd + Shift + Z)" disabled={!editing || revision>=revisionCount-1} onclick={()=>restoreEdit(1)}>↷</button>
<div class="spacer"></div><button onclick={()=>scale(.8)}>−</button><span>{Math.round(zoom*100)}%</span><button onclick={()=>scale(1.25)}>+</button><button onclick={()=>fit()}>Fit <kbd>F</kbd></button></footer>
  </main>
  <DockPanel name="Inspector" panelClass="inspector" bind:side={inspectorSide} bind:open={inspectorOpen}>
    {#if selectedConnection && arrows.some(a=>a.id===selectedConnection)}<h2>Connection</h2><button disabled={!editing} onclick={deleteConnection}>Delete connection</button>
    {:else if active}<h2>{active.kind}</h2><fieldset disabled={!editing}>
      <label>Title<input bind:value={active.name} oninput={change}/></label>
      {#if active.kind==='Image'}{@render imageTitle(active)}{/if}
      {@render departmentPicker()}
      {#if active.kind==='Note'}<label>Text<textarea value={active.body}
        onbeforeinput={e=>beforeStart=e.currentTarget.selectionStart}
        onselect={e=>active && remember(active,e.currentTarget)} oninput={e=>active && updateText(active,e.currentTarget)}></textarea></label>
        <button disabled={range?.id!==active.id || range.start===range.end} onclick={()=>chooseLink(range)}>Link selected text</button>
        {#each active.textLinks ?? [] as link}<button class="link-entry" title={labelFor(link.to)} onclick={()=>chooseLink(link)}>↗ {active.body.slice(link.start,link.end)}</button>{/each}
        <label class="check"><input type="checkbox" bind:checked={active.autoFit} onchange={change}/> Auto-fit text to box</label><label>Font size<input type="number" min="8" max="96" value={active.size} onchange={e=>{if(active){active.size=Math.max(8,Math.min(96,Number(e.currentTarget.value)||16));change();}}}/></label>{/if}
      <div class="dimensions">{#each ['w','h'] as key}<label>{key==='w'?'Width':'Height'}<input type="number" min="40" value={active[key as 'w'|'h']} onchange={e=>{if(active){active[key as 'w'|'h']=Math.max(40,Number(e.currentTarget.value)||40);change();}}}/></label>{/each}</div>
      <button onclick={()=>chooseLink()}>{objectLink(active)?'Edit object link':'Link object'}</button>
    </fieldset>
      {#if active.attachment}<p>{active.attachment.local?'Project copy':'Linked original'}<br>{active.attachment.path}</p><div class="attachment-actions">
        <button disabled={!editing || copyingItem===active.id} onclick={()=>relink(active!)}>Relink…</button>
        <button onclick={()=>revealAttachment(active!)}>Open folder</button>
        {#if !active.attachment.local}<button disabled={!editing || !!copyingItem} onclick={()=>copyToProject(active!)}>{copyingItem===active.id?'Copying…':'Copy to project'}</button>{/if}
      </div>{/if}
    {:else if picked.length>1}<h2>{picked.length} objects</h2><p>Move the selection or drag a handle to transform it.</p>{:else}<h2>Page details</h2><label>Page name<input disabled={!editing} bind:value={board.name} oninput={change}/></label><div class="stat">Objects <b>{board.items.length}</b></div><p>Select an object to inspect its properties.</p>{/if}
  </DockPanel>
  </div>
  {#if menu}
    <div class="app-menu" style:left={menu.x+'px'} style:top={Math.max(0,menu.y)+'px'}>
      {#if menu.page}
        {@const target=menu.page}
        {#if target.id!==current}<button onclick={()=>{void navigate(target);menu=undefined;}}>Open page</button>{/if}
        <button disabled={!editing} onclick={()=>{newPage(target);menu=undefined;}}>Add child page…</button>
        <details class="object-departments"><summary>Departments</summary><fieldset disabled={!editing}>
        {#if target.parentId}<label><input type="checkbox" checked={target.departments===undefined} onchange={e=>{target.departments=e.currentTarget.checked?undefined:[...pageTags.get(target.id)??[]];change();}}/> Inherit from parent</label>{/if}
        {#each catalog as d}<label><input type="checkbox" checked={pageTags.get(target.id)?.includes(d.id)??false} onchange={e=>togglePageTag(target,d.id,e.currentTarget.checked)}/><i class="department-dot" style:background={d.color}></i>{d.name}</label>{/each}
        </fieldset></details>
        <button onclick={()=>showPageHistory(target)}>View page history…</button>
        <button onclick={()=>{exporter.show(target);menu=undefined;}}>Export page as PDF…</button>
        <hr/>
        <button disabled={!editing} onclick={()=>requestDelete(target)}>Delete page…</button>
      {:else if menu.canvas}
        <div class="menu-heading">New</div>
        {#each ['Note','Image','Frame','Drawing','Page link','Web link',...(isTauri()?['File','Folder']:[])] as kind}
          <button disabled={!editing} onclick={()=>insert(kind)}>{kind}</button>
        {/each}
        {#if !menu.fromAdd}<button class="menu-export" onclick={()=>{menu=undefined;exporter.show(board);}}>Export page as PDF…</button>{/if}
      {:else}
        <button onclick={()=>{fit();menu=undefined;}}>Focus selection</button>
        {@render departmentPicker()}
        {#if active?.kind==='Image'}{@render imageTitle(active)}{/if}
        {#if active?.attachment || active?.url}<button onclick={()=>{menu=undefined;void openResource(active!);}}>Open</button>{/if}
        {#if picked.length===1}<button disabled={!editing} onclick={()=>{menu=undefined;chooseLink();}}>Link object…</button>{/if}
        {#if picked.length>1}
          {#each ['Horizontal','Vertical'] as axis}
            <div class="align-group" role="group" aria-label={axis+' alignment'}><small>{axis} alignment</small><div>
              {#each [0,.5,1] as factor,i}
                {@const label='Align '+(axis==='Horizontal'?['left','horizontal center','right']:['top','vertical center','bottom'])[i]}
                <button disabled={!editing} aria-label={label} title={label} onclick={()=>align(axis==='Horizontal'?'x':'y',factor)}>
                  <svg aria-hidden="true" viewBox="0 0 24 24" width="19" height="19"><g transform={axis==='Vertical'?'translate(24 0) rotate(90)':undefined}>
                    <path d={'M'+(4+16*factor)+' 2v20'} stroke="currentColor" fill="none"/>
                    <rect x={4+6*factor} y="6" width="10" height="4" rx="1" fill="currentColor"/><rect x={4+2*factor} y="14" width="14" height="4" rx="1" fill="currentColor"/>
                  </g></svg>
                </button>
              {/each}
            </div></div>
          {/each}
        {/if}
        <button disabled={!editing} onclick={removeObjects}>Delete selection</button>
      {/if}
    </div>
  {/if}
  <div class="status" role="status"><i></i>{#if status==='PDF exported' && exportedPdf}
    <button class="export-result" title="Open exported PDF" onclick={()=>openExport(false)}>PDF exported ↗</button>
    <button class="export-result" onclick={()=>openExport(true)}>Open folder</button>
    {:else}{status || 'Local project'}{/if} <span>{showFps?fps+' fps　·　':''}{board.items.length} objects</span></div>
  {/if}
  <dialog bind:this={fileDialog} oncancel={e=>{if(copying)e.preventDefault();}}>
    <h2>Add to canvas</h2><p>{incoming.length===1?incoming[0]:incoming.length+' items selected'}</p>
    <p>Copy files and folders beside the project, or keep a link to their original location.</p>
    {#if importError}<p role="alert">{importError}</p>{/if}
    <button disabled={copying} onclick={()=>fileDialog.close()}>Cancel</button>
    <button disabled={copying} onclick={()=>attach(false)}>Link original</button><button disabled={copying} onclick={()=>attach(true)}>{copying?'Adding…':'Copy to project'}</button>
  </dialog>
  <dialog class="settings" bind:this={settings}>
    {@render closePanel(()=>settings.close())}<h2>Settings</h2>
    {#if isTauri()}<label>On startup<select bind:value={startup} onchange={()=>localStorage.setItem('studio-startup',startup)}><option value="welcome">Show welcome screen</option><option value="last">Reopen last project</option></select></label>{/if}
    <h3>Appearance</h3>
    <label>Theme<select aria-label="Appearance" bind:value={theme} onchange={()=>localStorage.setItem('studio-theme',theme)}><option>System</option><option>Light</option><option>Dark</option></select></label>
    <label>Navigation history limit<input type="number" min="1" max="100" value={historyLimit} onchange={e=>{setLimit(e.currentTarget.valueAsNumber);e.currentTarget.value=String(historyLimit);}}/></label>
    <h3>Canvas Snapping</h3><p>Hold Ctrl while dragging. Set a step to zero to disable snapping for that transform.</p>
    {#each snapKeys as key}<label>{key==='rotate'?'Rotation (degrees)':key==='resize'?'Resize (canvas units)':'Move (canvas units)'}
      <input type="number" min="0" max={key==='rotate'?360:10000} step="any" value={snapping[key]} disabled={!welcome && !editing}
        onchange={e=>setSnap(key,e.currentTarget)}/></label>{/each}
    {#if welcome}<p>These defaults apply to projects without custom snapping values.</p>{:else if !editing}<p>Enable editing to change this project’s canvas snapping.</p>{/if}
    {#if isTauri()}<p class="app-version">Tenkivo · Version {appVersion}</p>{/if}
  </dialog>
  <dialog bind:this={updateDialog} oncancel={e=>{if(installing)e.preventDefault();}}>
    {#if release}<h2>Update available</h2><p>Current version {release.current} · New version {release.version}</p>
      <p style="white-space:pre-wrap;max-height:40vh;overflow:auto">{release.notes}</p>
      {#if updateError}<p role="alert">{updateError}</p>{/if}
      <button disabled={installing} onclick={()=>updateDialog.close()}>Later</button>
      {#if release.portable}<button disabled={installing} onclick={applyUpdate}>{installing?'Updating…':'Update and restart'}</button>
      {:else}<p>Automatic replacement is available in the Windows portable build.</p>{/if}
    {/if}
  </dialog>
  <dialog bind:this={dialog}>{#if ['Project settings','Keyboard shortcuts','Editor logs','Page history'].includes(dialogType)}{@render closePanel(()=>dialog.close())}{/if}<h2>{(dialogType==='Project settings' || dialogType==='New project') && section?section:dialogType}</h2>{#if dialogType==='Internal link' && active}
    <form onsubmit={e=>{e.preventDefault();storeLink(targetItem?{pageId:targetPage,itemId:targetItem}:{pageId:targetPage});}}>
      <p>{draftRange ? '“'+active.body.slice(draftRange.start,draftRange.end)+'”' : active.name}</p>
      <label>Page<select bind:value={targetPage} onchange={()=>targetItem=''}>{#each project.pages as p}<option value={p.id}>{p.name}</option>{/each}</select></label>
      <label>Destination<select bind:value={targetItem}><option value="">Entire page</option>{#each choices as o}<option value={o.id}>{o.name} · {o.kind}</option>{/each}</select></label>
      {#if draftRange ? active.textLinks?.some(l=>l.start<draftRange!.end && l.end>draftRange!.start) : objectLink(active)}
        <button type="button" onclick={()=>storeLink()}>Remove link</button>{/if}
      <button type="button" onclick={()=>dialog.close()}>Cancel</button><button disabled={!project.pages.some(p=>p.id===targetPage) || (!!targetItem && !choices.some(o=>o.id===targetItem))}>Apply link</button>
    </form>
  {:else if dialogType==='Project settings' || dialogType==='New project'}
    {@const creating=dialogType==='New project'}
    <form class="project-form" onsubmit={e=>{e.preventDefault();if(section)leaveSection(true);else if(creating)void named();else applyOptions();}}>
      {#if !section}<fieldset disabled={!creating && !editing}><label>Project name<input required bind:value={draftName}/></label>
      <label>Description<textarea bind:value={draftDescription}></textarea></label>
      </fieldset>
      <button class="add-project-entry" type="button" onclick={()=>openSection('Departments')}>Departments　›</button>
      <button class="add-project-entry" type="button" onclick={()=>openSection('Users')}>Users　›</button>
      {:else if section==='Departments'}<fieldset disabled={!creating && !editing}>
      {#each draftCatalog as d (d.id)}
        <div class="department-editor"><input type="color" aria-label={'Color for '+d.name} bind:value={d.color}/>
        <input required aria-label="Department name" bind:value={d.name}/><button type="button" title="Remove department" aria-label={'Remove '+d.name} onclick={()=>draftCatalog=draftCatalog.filter(v=>v.id!==d.id)}>×</button></div>
      {/each}
      <button class="add-project-entry" type="button" onclick={()=>draftCatalog.push({id:crypto.randomUUID(),name:'New department',color:'#8888bb'})}>Add New Department</button>
      </fieldset>{:else}<fieldset disabled={!creating && !editing}>
      {#each draftUsers as u (u.id)}
        <div class="department-editor"><select aria-label={'Icon for '+u.name} bind:value={u.icon}>{#each userIcons as icon}<option value={icon}>{icon}</option>{/each}</select>
        <input required aria-label="User name" placeholder="User name" bind:value={u.name}/>
        <button type="button" disabled={u.id===editorId && !creating} aria-label={'Remove user '+u.name} onclick={()=>draftUsers=draftUsers.filter(v=>v.id!==u.id)}>×</button></div>
      {/each}
      <button class="add-project-entry" type="button" onclick={()=>draftUsers.push({id:crypto.randomUUID(),name:'',icon:userIcons[0]})}>Add New User</button>
      {#if !validUsers && draftUsers.every(u=>u.name.trim())}<p role="alert">Choose a distinct name for each user.</p>{/if}
      </fieldset>{/if}
      {#if section}<button type="button" onclick={()=>leaveSection(false)}>Back</button>{/if}
      {#if !creating && !editing}<p>Read mode is on. Enable editing to change project settings.</p>{/if}
      {#if creating && !section}<button type="button" onclick={()=>dialog.close()}>Cancel</button>{#if creationError}<p role="alert">{creationError}</p>{/if}{/if}<button disabled={busy || !validUsers || (!creating && !editing) || ((!section || !creating) && !draftName.trim()) || draftCatalog.some(d=>!d.name.trim())}>{creating && !section?'Create':'Apply changes'}</button>
    </form>
  {:else if dialogType==='Select editing user'}
    <form class="user-picker" onsubmit={e=>{e.preventDefault();enterEditing();}}>
      {#if registering}
        <label>User name<input required bind:value={userName}/></label>
        <label>Icon<select bind:value={userIcon}>{#each userIcons as icon}<option value={icon}>{icon}</option>{/each}</select></label>
        {#if userName.trim() && !validRegistration}<p role="alert">This user name is already in use.</p>{/if}
        {#if pickerUsers.length}<button type="button" onclick={()=>registering=false}>Choose existing user</button>{/if}
      {:else}
        <label>User<select bind:value={selectedUser}>{#each pickerUsers as u}<option value={u.id}>{u.icon} {u.name}</option>{/each}</select></label>
        <button type="button" onclick={()=>registering=true}>Add New User</button>
      {/if}
      <div><button type="button" onclick={()=>dialog.close()}>Cancel</button><button disabled={registering?!validRegistration:!selectedUser}>Start editing</button></div>
    </form>
  {:else if dialogType==='Page history' && historyPage}
    {@const preview=chosenVersion?.page??historyPage}
    <div class="page-history">
      <nav aria-label="Page versions">
        <button class:chosen={!chosenVersion} aria-pressed={!chosenVersion} onclick={()=>chosenVersionId=''}>Current state</button>
        {#each pageVersions as v (v.id)}
          <button class:chosen={v.id===chosenVersionId} aria-pressed={v.id===chosenVersionId} onclick={()=>chosenVersionId=v.id}>
            <strong>{v.restoredFrom?'Restored version':v.date?'Saved version':'Initial snapshot'}</strong>
            <span>{v.author?v.author.icon+' '+v.author.name:'Unknown author'}</span>
            {#if v.date}<time datetime={v.date}>{new Date(v.date).toLocaleString()}</time>{:else}<small>Original date unavailable</small>{/if}
          </button>
        {:else}<p>Saved versions appear here after saving.</p>{/each}
      </nav>
      <section aria-label="Version preview">
        <h3>{preview.name}</h3>
        <p>{preview.items.length} objects</p>
        <ul>{#each preview.items as object}<li><strong>{object.name}</strong><small>{object.kind}</small>{#if object.kind==='Note' && object.body}<div>{object.body}</div>{/if}</li>{/each}</ul>
        {#if chosenVersion}
          {#if !editing}<p>Enable editing to restore a version.</p>
          {:else}<p>Restoring creates a new version and keeps the current state in history. Save to keep the change.</p>{/if}
          <button disabled={!editing || busy} onclick={restorePageVersion}>Restore as new version</button>
        {/if}
      </section>
    </div>
  {:else if dialogType==='Add page link'}
    <form onsubmit={e=>{e.preventDefault();const p=project.pages.find(v=>v.id===linkPage);if(p)add('Page link',{...insertAt,target:p.id,name:p.name,w:210,h:84});dialog.close();}}>
      <label>Page<select bind:value={linkPage}>{#each project.pages as p}<option value={p.id}>{p.name}</option>{/each}</select></label><button type="button" onclick={()=>dialog.close()}>Cancel</button><button>Add link</button></form>
  {:else if dialogType==='Add web link'}
    <form onsubmit={e=>{e.preventDefault();if(insertUrl(linkUrl,insertAt?.x,insertAt?.y))dialog.close();}}><label>URL<input type="url" required bind:value={linkUrl}/></label><button type="button" onclick={()=>dialog.close()}>Cancel</button><button disabled={!webAddress(linkUrl)}>Add link</button></form>
  {:else if dialogType==='Editor logs'}
    <div class="log-list">{#each [...diagnostics.entries].reverse() as entry}<article><time>{new Date(entry.time).toLocaleString()}</time><strong>{entry.context}</strong><pre>{entry.message}</pre></article>{:else}<p>No errors recorded.</p>{/each}</div>
  {:else if dialogType==='Delete page' && pendingPage}<p>Move “{pendingPage.name}” to the trash? Its child pages will move up one level.</p><button onclick={()=>dialog.close()}>Cancel</button><button onclick={deletePage}>Move to trash</button>
  {:else if dialogType==='Keyboard shortcuts'}<p>E　 Toggle read / edit<br>F　 Focus selection or page<br>Ctrl / Cmd + Z　 Undo<br>Ctrl / Cmd + Shift + Z or Ctrl + Y　 Redo<br>Ctrl / Cmd + K　 Link selected text or object<br>Ctrl / Cmd + S　 Save project<br>Wheel　 Zoom at pointer<br>Middle-drag　 Pan<br>Left-drag　 Box selection<br>Shift + click　 Extend selection<br>Alt + drag between ports　 Connect canvas objects<br>Enter　 Finish drawing and start another<br>Esc　 Cancel current drawing<br>Shift + corner drag　 Resize proportionally<br>Ctrl + drag　 Snap transform<br>Ctrl + Shift + Left / Right　 Navigation history<br>Delete　 Delete selection or focused page</p>{:else}<form onsubmit={e=>{e.preventDefault();void named();}}><label>Name<input required bind:value={name}/></label><button type="button" onclick={()=>dialog.close()}>Cancel</button><button disabled={!name.trim()}>Create</button></form>{/if}</dialog>
  <input hidden bind:this={imageInput} type="file" accept="image/png,image/jpeg,image/gif,image/webp" multiple onchange={e=>{void importImages([...(e.currentTarget.files??[])],insertAt?.x,insertAt?.y);e.currentTarget.value='';}}/>
  <input hidden bind:this={fileInput} type="file" accept=".gdstudio" onchange={async e=>{const f=e.currentTarget.files?.[0];e.currentTarget.value='';if(f)try{await load(await f.text(),'');}catch(err){recordError('Open file',err);status=String(err);if(welcome)welcomeError=status;}}}/>
</div>
<ExportDialog bind:this={exporter} {project} {path} {filters} {pageTags} convert={convertExternal} ensureSaved={ensureExportSaved} notify={exportNotice}/>

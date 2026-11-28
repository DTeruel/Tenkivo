<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import {connectionPath,portPosition,portSides} from './connections';
  import {collectSelection,preparePaste,remapLinks,type CanvasClipboard} from './canvasClipboard';
  import DrawingStroke from './DrawingStroke.svelte';
  import DockPanel from './DockPanel.svelte';
  import {canvasEdges,canvasDropTarget,type CanvasEdge} from './canvasDock';
  import ExportDialog from './ExportDialog.svelte';
  import AttachmentImage from './AttachmentImage.svelte';
  import MediaPlayer from './MediaPlayer.svelte';
  import {materializeAttachment} from './sharedAttachment';
  import {getCurrentWebview} from '@tauri-apps/api/webview';
  import {getCurrentWindow} from '@tauri-apps/api/window';
  import {getVersion} from '@tauri-apps/api/app';
  import {mediaKind,videoId,webAddress,type Attachment} from './model';
  import {invoke,diagnostics,recordActivity,recordError,collectNativeDiagnostics} from './diagnostics.svelte';
  import GitHubProjectUsers from './GitHubProjectUsers.svelte';
  import KeyboardShortcuts from './KeyboardShortcuts.svelte';
  import {PagePresence,type PageEditor} from './pagePresence.svelte';
  function showLogs(){
    if(!dialog.open)ask('Editor logs');
    if(dialogType==='Editor logs')diagnostics.unreadErrors=0;
  }
  import { isTauri } from '@tauri-apps/api/core';
  import { open, save, confirm } from './diagnostics.svelte';
  import { frameHeaderText, defaultCatalog, tagPresets, availablePresetTags, addTagPreset, editBody, item, page, snapDefaults, textParts, treeRows, validProject, migrateLegacyDiagrams } from './model';
  import {richNote} from './richNote';
  import {toggleStyle,type NoteStyle} from './model';
  const framePalette=[
    '#4ea0ff','#ff785a','#51c78b','#b785ee','#edc450','#32bdc8',
    '#e66fa8','#9bb24e','#6876dc','#d58d42','#8c664e','#a1aebb',
  ];
  const noteStyles:NoteStyle[]=['bold','italic','underline'];
  function formatNote(style:NoteStyle){
    if(!editing || !active || range?.id!==active.id || range.start===range.end)return;
    toggleStyle(active,range.start,range.end,style);change();
  }
  import { bounds, contains, resize, mapChild, enclosure, snap } from './geometry';
  import type {CommentThread} from './model';
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
  const initial:Project={version:1,name:'',pages:[page('Overview')]};
  let welcome=$state(true),launching=$state(false),welcomeError=$state(''),creationError=$state('');
  let startup=$state(localStorage.getItem('studio-startup')??'welcome');
  type ProjectOpeningMode='read'|'edit';
  let projectOpeningMode=$state<ProjectOpeningMode>(localStorage.getItem('studio-project-opening-mode')==='edit'?'edit':'read');
  let openingModeDialog:HTMLDialogElement;
  let draftOpeningMode=$state<ProjectOpeningMode>('read');
  let openingModePromptShown=false;
  let openNextProjectInReadMode=false;
  function setProjectOpeningMode(mode:ProjectOpeningMode){
    projectOpeningMode=mode;
    localStorage.setItem('studio-project-opening-mode',mode);
  }
  function resetOpeningModePrompt(){
    openingModePromptShown=false;
    localStorage.removeItem('studio-hide-opening-mode-prompt');
    recordActivity('Settings','The opening mode prompt will appear on the next manual switch to edit mode.');
  }
  function requestEditing(){
    const canChooseOpeningMode=!onlineProject || (onlineIdentity && onlineProject.writable);
    if(canChooseOpeningMode && !openingModePromptShown && localStorage.getItem('studio-hide-opening-mode-prompt')!=='true'){
      openingModePromptShown=true;
      draftOpeningMode=projectOpeningMode;
      openingModeDialog.showModal();
      return;
    }
    selectEditor();
  }
  function finishOpeningModePrompt(apply:boolean){
    if(apply){
      setProjectOpeningMode(draftOpeningMode);
      localStorage.setItem('studio-hide-opening-mode-prompt','true');
    }
    openingModeDialog.close();
    selectEditor();
  }
  let openingEditPending=false;
  type Recent={name:string;path:string};
  function readRecent():Recent[]{
    try{const list:unknown=JSON.parse(localStorage.getItem('studio-recent')??'[]');
      return Array.isArray(list)?list.filter(v=>v && typeof v.name==='string' && typeof v.path==='string'):[];}
    catch{return [];}
  }
  let recent=$state<Recent[]>(readRecent()),availability=$state<Record<string,string>>({});
  const lastOnlineProjectKey='studio-last-online-project';
  function readLastOnlineProject():Pick<OnlineProject,'repo'|'branch'>|undefined{
    try{
      const value=JSON.parse(localStorage.getItem(lastOnlineProjectKey)??'null');
      if(typeof value?.repo==='string' && typeof value.branch==='string')return value;
    }catch{/* An invalid saved location cannot be reopened. */}
  }
  function rememberOnlineProject(remote:OnlineProject){
    localStorage.setItem(lastOnlineProjectKey,JSON.stringify({repo:remote.repo,branch:remote.branch}));
    localStorage.removeItem('studio-last');
  }
  function rememberProject(location:string){
    if(!location)return;
    recent=[{name:project.name,path:location},...recent.filter(v=>v.path!==location)];
    localStorage.setItem('studio-recent',JSON.stringify(recent));
    localStorage.setItem('studio-last',location);
    localStorage.removeItem(lastOnlineProjectKey);
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
    if(launching || busy)return;
    launching=true;welcomeError='';
    try{if(await confirmUnsaved('open'))await load(await invoke<string>('open_project',{path:location}),location);}
    catch(error){welcomeError='Could not open '+location+' · '+String(error);await checkRecent();}
    finally{launching=false;}
  }
  let project = $state(initial), current = $state(initial.pages[0].id);
  let board = $derived(project.pages.find(p=>p.id===current)!);
  let ids=$state<string[]>([]);
  let canvasClipboard=$state.raw<CanvasClipboard>();
  let pageClipboard=$state.raw<{page:Page; selection:CanvasClipboard}>();
  let clipboardSource:{generation:number;path:string;repo?:string;departments:{id:string;name:string}[]};
  let projectGeneration=0;
  function rememberClipboardSource(){
    clipboardSource={generation:projectGeneration,path,repo:onlineProject?.repo,
      departments:catalog.map(({id,name})=>({id,name}))};
  }
  function copyPage(p:Page,event?:ClipboardEvent){
    const selection=collectSelection(p,p.items.map(item=>item.id),false)??{
      token:'Tenkivo page '+crypto.randomUUID(),pageId:p.id,roots:[],items:[],connections:[],cut:false,pastes:0
    };
    pageClipboard=$state.snapshot({page:{...p,departments:pageTags.get(p.id)??[]},selection});
    canvasClipboard=undefined;
    rememberClipboardSource();
    if(event){event.clipboardData?.setData('text/plain',selection.token);event.preventDefault();}
    else void navigator.clipboard?.writeText(selection.token).catch(()=>{});
    menu=undefined;status='Copied page: '+p.name;
  }
  async function transferClipboardItems(items:Item[],source:typeof clipboardSource){
    if(source.generation===projectGeneration)return;
    for(const item of items){
      const attachment=item.attachment;
      if(!attachment)continue;
      if(!onlineProject && !attachment.local)continue;
      if(onlineProject && attachment.shared?.blob && source.repo?.toLowerCase()===onlineProject.repo.toLowerCase())continue;
      const file=attachment.shared
        ? await invoke<string>('online_attachment_path',{repo:source.repo,attachment})
        : await invoke<string>('clipboard_attachment_path',{project:source.path,attachment});
      if(onlineProject){
        const repo=onlineProject.repo;
        const size=await invoke<number>('online_attachment_size',{path:file});
        if(size>5*1024*1024 && !await confirm('This attachment is larger than 5 MiB. Copy it to the online project?'))throw Error('Paste canceled.');
        item.attachment=await invoke<Attachment>('stage_online_attachment',{repo,path:file});
      }else{
        const [imported]=await invoke<{attachment:Attachment}[]>('import_attachments',{project:path,paths:[file],local:!!path});
        item.attachment=imported.attachment;
      }
    }
  }
  function clipboardTags(tags:string[],source:typeof clipboardSource){
    return tags.flatMap(id=>{
      const name=source.departments.find(d=>d.id===id)?.name;
      const match=catalog.find(d=>name && d.name.toLowerCase()===name.toLowerCase());
      return match?[match.id]:[];
    });
  }
  function adaptClipboardItems(items:Item[],source:typeof clipboardSource,destination:Page){
    if(source.generation===projectGeneration)return;
    const pages=[...project.pages,destination];
    const valid=(to:{pageId:string;itemId?:string})=>pages.some(p=>p.id===to.pageId && (!to.itemId || p.items.some(item=>item.id===to.itemId)));
    for(const item of items){
      item.departments=clipboardTags(item.departments??[item.department],source);
      if(item.link && !valid(item.link))delete item.link;
      item.textLinks=item.textLinks?.filter(link=>valid(link.to));
      if(item.target && !valid({pageId:item.target}))delete item.target;
    }
  }
  async function pastePage(){
    if(!editing || busy || !pageClipboard)return;
    const {page:original,selection}=pageClipboard;
    const source=clipboardSource;
    const newPage:Page={id:crypto.randomUUID(),name:original.name,items:[],
      departments:source.generation===projectGeneration?original.departments:clipboardTags(original.departments??[],source)};
    const pasted=preparePaste(selection,newPage,false,0,0,true);
    newPage.items=pasted.items;newPage.connections=pasted.connections;
    busy=true;menu=undefined;
    try{
      await transferClipboardItems(newPage.items,source);
      adaptClipboardItems(newPage.items,source,newPage);
      commit();project.pages.push(newPage);change();await navigate(newPage);
      status='Pasted page: '+newPage.name;
    }catch(error){status='Could not paste page: '+String(error);recordError('Paste page',error);}
    finally{busy=false;}
  }
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
  let canvasToolbarSide=$state<CanvasEdge>(canvasEdges.find(edge=>edge===localStorage.getItem('canvas-toolbar-side'))??'bottom');
  let canvasArea=$state<HTMLDivElement>(null!);
  let dockDrag=$state<{id:number;x:number;y:number;moving:boolean;target?:CanvasEdge}>();
  $effect(()=>{
    localStorage.setItem('canvas-toolbar-side',canvasToolbarSide);
  });
  function beginCanvasDock(e:PointerEvent){
    e.stopPropagation();
    if(e.button!==0 || !e.isPrimary)return;
    menu=undefined;
    if(historyMenu)historyMenu.open=false;
    dockDrag={id:e.pointerId,x:e.clientX,y:e.clientY,moving:false};
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function moveCanvasDock(e:PointerEvent){
    if(!dockDrag || e.pointerId!==dockDrag.id)return;
    e.stopPropagation();
    if(!dockDrag.moving && Math.hypot(e.clientX-dockDrag.x,e.clientY-dockDrag.y)<5)return;
    dockDrag.moving=true;
    dockDrag.target=canvasDropTarget(canvasArea.getBoundingClientRect(),e.clientX,e.clientY);
  }
  function finishCanvasDock(e:PointerEvent){
    if(!dockDrag || e.pointerId!==dockDrag.id)return;
    moveCanvasDock(e);
    if(dockDrag.moving && dockDrag.target)canvasToolbarSide=dockDrag.target;
    dockDrag=undefined;
  }
  function cancelCanvasDock(){
    dockDrag=undefined;
  }
  function canvasDockKey(e:KeyboardEvent){
    if(e.key==='Escape' && dockDrag){cancelCanvasDock();e.stopPropagation();return;}
    if(e.altKey || e.ctrlKey || e.metaKey || e.shiftKey)return;
    const edge=canvasEdges.find(edge=>e.key==='Arrow'+({top:'Up',bottom:'Down',left:'Left',right:'Right'}[edge]));
    if(!edge)return;
    e.preventDefault();e.stopPropagation();
    canvasToolbarSide=edge;
  }
  let pagesOpen=$state(localStorage.getItem('pages-open')!=='false'), inspectorOpen=$state(localStorage.getItem('inspector-open')!=='false');
  $effect(()=>{localStorage.setItem('pages-side',pagesSide);localStorage.setItem('inspector-side',inspectorSide);localStorage.setItem('pages-open',String(pagesOpen));localStorage.setItem('inspector-open',String(inspectorOpen));});
  let menu=$state<{x:number;y:number;item?:Item;page?:Page;canvas?:boolean;point?:{x:number;y:number}}>(), pendingPage=$state<Page>();
  let focusArea:'tree'|'canvas'='canvas';
  let history=$state<{page:string;x:number;y:number;zoom:number;ids:string[]}[]>([]), cursor=$state(-1);
  let historyLimit=$state(normalizeLimit(Number(localStorage.getItem('navigation-limit')??10)));
  let historyMenu=$state<HTMLDetailsElement>(null!), fileMenu=$state<HTMLDetailsElement>(null!);
  let githubMenu=$state<HTMLDetailsElement>();
  let pageParent:string|undefined;
  function normalizeLimit(n:number){return Number.isFinite(n)?Math.max(1,Math.min(100,Math.round(n))):10;}
  function trimHistory(){
    if(history.length<=historyLimit)return;
    const start=Math.min(history.length-historyLimit,Math.max(0,cursor-historyLimit+1));
    history=history.slice(start,start+historyLimit);cursor-=start;
  }
  function setLimit(n:number){historyLimit=normalizeLimit(n);localStorage.setItem('navigation-limit',String(historyLimit));trimHistory();}
  function view(){return {page:current,x:tx,y:ty,zoom,ids:[...ids]};}
  function resetViews(){modifiedPage='';wire=undefined;selectedConnection='';history=[];cursor=-1;filters=[];ids=[];menu=undefined;projectGeneration++;}
  function rememberView(){if(cursor>=0)history[cursor]=view();}
  function travel(step:number){
    if(historyMenu)historyMenu.open=false;
    const dest=history[cursor+step];if(!dest)return;
    stopDrawing();wire=undefined;selectedConnection='';rememberView();cursor+=step;current=dest.page;tx=dest.x;ty=dest.y;zoom=dest.zoom;ids=dest.ids.filter(id=>board.items.some(o=>o.id===id && (matches(itemTags(o),filters))));
    inline='';range=undefined;gesture=undefined;liveBox=undefined;drag=undefined;marquee=undefined;menu=undefined;
  }
  function setFilters(next:Department[]){filters=next;ids=picked.filter(o=>matches(itemTags(o),next)).map(o=>o.id);}
  function context(e:MouseEvent,p?:Page,o?:Item){e.preventDefault();e.stopPropagation();selectedConnection='';if(o && !ids.includes(o.id))ids=[o.id];menu={x:Math.min(e.clientX,innerWidth-190),y:Math.min(e.clientY,innerHeight-260),item:o,page:p};}
  function removeObjects(selection=ids){
    if(!editing || !selection.length)return;
    const removed=new Set(selection);
    board.connections=board.connections?.filter(c=>!removed.has(c.from.objectId) && !removed.has(c.to.objectId));
    board.items=board.items.filter(o=>!removed.has(o.id));
    for(const o of board.items)if(o.parent && removed.has(o.parent))o.parent=undefined;
    ids=[];menu=undefined;change();
  }
  function canvasClipboardBlocked(target:EventTarget|null){
    return welcome || drawingMode || !!document.querySelector('dialog[open]') ||
      (target instanceof Element && !!target.closest('textarea,input,select,[contenteditable]:not([contenteditable=false]),audio,video'));
  }
  function copyObjects(cut:boolean,event?:ClipboardEvent){
    if(event && (canvasClipboardBlocked(event.target) || window.getSelection()?.isCollapsed===false))return;
    if(event && !cut && focusArea==='tree' && event.target instanceof Element){
      const id=event.target.closest<HTMLElement>('[data-page]')?.dataset.page;
      const p=project.pages.find(p=>p.id===id);
      if(p){copyPage(p,event);return;}
    }
    if(cut && !editing){if(ids.length)status=readHint;return;}
    const selection=collectSelection(board,ids,cut);
    if(!selection)return;
    canvasClipboard=$state.snapshot(selection);
    pageClipboard=undefined;rememberClipboardSource();
    if(event){
      event.clipboardData?.setData('text/plain',selection.token);
      event.preventDefault();
    }else{
      void navigator.clipboard?.writeText(selection.token).catch(()=>{});
    }
    if(cut){
      commit();typingField=null;
      removeObjects(selection.items.map(o=>o.id));
    }
    menu=undefined;
    status=(cut?'Cut ':'Copied ')+selection.items.length+(selection.items.length===1?' object':' objects');
  }
  async function pasteObjects(point?:{x:number;y:number}){
    const clipboard=canvasClipboard;
    if(!clipboard || busy)return;
    const sourceProject=clipboardSource;
    const sameProject=sourceProject.generation===projectGeneration;
    if(!editing){status=readHint;return;}
    const movedIds=new Set(clipboard.items.map(o=>o.id));
    const pages=[...project.pages,...project.trash??[]];
    const move=sameProject && clipboard.cut && !clipboard.pastes && !pages.some(p=>p.items.some(o=>movedIds.has(o.id)));
    const source=enclosure(clipboard.items);
    const center={x:(width/2-tx)/zoom,y:(height/2-ty)/zoom};
    const offset=(move?0:clipboard.pastes+1)*24/zoom;
    const samePage=sameProject && clipboard.pageId===current;
    const sourceVisible=source.x*zoom+tx<width && source.y*zoom+ty<height &&
      (source.x+source.w)*zoom+tx>0 && (source.y+source.h)*zoom+ty>0;
    const origin=point ?? (samePage && sourceVisible
      ? {x:source.x+offset,y:source.y+offset}
      : {x:center.x-source.w/2+offset,y:center.y-source.h/2+offset});
    const pasted=preparePaste(clipboard,board,move,origin.x-source.x,origin.y-source.y);

    const destinationId=current;
    if(!sameProject){
      busy=true;
      try{await transferClipboardItems(pasted.items,sourceProject);}
      catch(error){status='Could not paste objects: '+String(error);recordError('Paste objects',error);return;}
      finally{busy=false;}
      if(destinationId!==current || !editing)return;
      adaptClipboardItems(pasted.items,sourceProject,{...board,items:[...board.items,...pasted.items]});
    }
    commit();typingField=null;
    if(move && !samePage){
      for(const p of pages)for(const o of p.items)remapLinks(o,to=>
        to.pageId===clipboard.pageId && to.itemId && movedIds.has(to.itemId) ? {...to,pageId:current} : to);
    }
    board.items.push(...pasted.items);
    if(pasted.connections.length)board.connections=[...board.connections??[],...pasted.connections];
    canvasClipboard={...clipboard,pastes:clipboard.pastes+1};
    const roots=new Set(pasted.roots);
    ids=pasted.items.filter(o=>roots.has(o.id) && matches(itemTags(o),filters)).map(o=>o.id);
    selectedConnection='';frameOnly='';focusArea='canvas';menu=undefined;
    change();
    const hidden=pasted.items.some(o=>!matches(itemTags(o),filters));
    status='Pasted '+pasted.items.length+(pasted.items.length===1?' object':' objects')+
      (hidden?' · Tag filters hide some pasted content':'');
  }
  function requestDelete(p:Page){if(!editing)return;pendingPage=p;menu=undefined;ask('Delete page');}
  async function deletePage(){
    const p=pendingPage;if(!p || !editing)return;
    project.trash=[...(project.trash??[]),p];for(const child of project.pages)if(child.parentId===p.id)child.parentId=p.parentId;
    project.pages=project.pages.filter(v=>v.id!==p.id);if(!project.pages.length)project.pages.push(page('Overview'));
    const next=project.pages.find(v=>v.id===current)??project.pages[0];history=[];cursor=-1;await navigate(next);dialog.close();change();
  }
  function restorePage(p:Page){if(!editing)return;if(!project.pages.some(v=>v.id===p.parentId))p.parentId=undefined;project.pages.push(p);project.trash=project.trash?.filter(v=>v.id!==p.id);change();}
  function emptyTrash(){
    if(!editing || !project.trash?.length)return;
    commit();
    project.trash=[];
    change();
    dialog.close();
  }
  function align(axis:'x'|'y',factor:number){
    if(!editing || !box)return;
    const roots=picked.filter(o=>!o.parent || !ids.includes(o.parent));
    const group=enclosure(roots),size=axis==='x'?'w':'h';
    for(const o of roots){const b=bounds(o),d=group[axis]+group[size]*factor-b[axis]-b[size]*factor;o[axis]+=d;for(const c of board.items)if(c.parent===o.id)c[axis]+=d;}
    change();menu=undefined;
  }
  let editing = $state(false), inline = $state(''), dirty = $state(false), busy = $state(false);
  let theme = $state(localStorage.getItem('studio-theme') || 'System'), path = $state(''), status = $state('');
  type GitHubAccount={id:number;login:string};
  type OnlineProject={repo:string;branch:string;sha:string;writable:boolean};
  type DiscoveredProject=Pick<OnlineProject,'repo'|'writable'>;
  let account=$state<GitHubAccount>(),onlineProject=$state<OnlineProject>();
  const presence=new PagePresence();
  const savedPresenceInterval=Number(localStorage.getItem('tenkivo-presence-seconds'));
  let presenceSeconds=$state([15,30,60].includes(savedPresenceInterval)?savedPresenceInterval:30);
  let modifiedPage=$state('');
  $effect(()=>presence.start(presenceSeconds));
  $effect(()=>{
    if(!editing || modifiedPage!==current)modifiedPage='';
  });
  let signingOut=$state(false);
  const editorsByPage=$derived.by(()=>{
    const pages=new Map<string,PageEditor[]>();
    for(const editor of presence.editors){
      const editors=pages.get(editor.page)??[];
      editors.push(editor);
      pages.set(editor.page,editors);
    }
    return pages;
  });
  $effect(()=>{
    const repo=onlineProject?.repo, branch=onlineProject?.branch, accountId=account?.id;
    const target=repo && branch && accountId && !signingOut
      ? {repo,branch,accountId,page:editing && onlineProject?.writable && modifiedPage===current?current:null}
      : null;
    untrack(()=>presence.setTarget(target));
  });
  function editingNotice(editors:PageEditor[]){
    const names=editors.map(editor=>'@'+editor.login).join(', ');
    return names+(editors.length===1?' is editing this page':' are editing this page');
  }
  const canSaveProject=$derived(!busy && (!onlineProject || !!(account && onlineProject.writable)));
  function refreshOnlineProject(){
    if(!onlineProject || busy || restoringAccount)return;
    if(account)void accessOnline('reload');
    else showGitHubAccount();
  }
  let restoringAccount=$state(isTauri()),restoreAccountError=$state('');
  let restoreSkipped=$state(false), restoreAttempt=0;
  let onlineMode=$state<'open'|'create'|'save'>('open');
  let discoveredProjects=$state<DiscoveredProject[]>([]),discovering=$state(false),discoveryError=$state(''),discoveryPaused=$state(false);
  let discoveryAttempt=0;
  let onlineRepo=$state(localStorage.getItem('studio-online-repo')??'');
  let onlineNewName=$state('');
  let clientId=$state(localStorage.getItem('studio-github-client-id')??'');
  let configuredClientId=$state('');
  let onlineError=$state(''),deviceCode=$state(''),onlineWorking=$state(false);
  let projectCacheBusy=$state(false),projectCacheStatus=$state(''),projectCacheError=$state('');
  let signInTimer:ReturnType<typeof setTimeout>|undefined, signInAttempt=0;
  const onlineIdentity=$derived(account?{id:'github:'+account.id,name:account.login,icon:'🐙'}:undefined);

  async function discoverProjects(){
    if(!account)return;
    const attempt=++discoveryAttempt;
    discovering=true;discoveryError='';discoveryPaused=false;
    try{
      const found=await invoke<DiscoveredProject[]>('online_discover',{requestId:attempt});
      if(attempt===discoveryAttempt)discoveredProjects=found;
    }catch(error){if(attempt===discoveryAttempt)discoveryError=String(error);}
    finally{if(attempt===discoveryAttempt)discovering=false;}
  }

  function stopDiscovery(){
    const requestId=++discoveryAttempt;
    discovering=false;discoveryPaused=true;
    void invoke('online_stop_discovery',{requestId}).catch(()=>{});
  }

  function showGitHubAccount(){
    if(restoringAccount)return;
    onlineError=restoreAccountError;
    ask('GitHub account');
  }

  function showOnline(mode:'open'|'create'|'save'){
    if(restoringAccount)return;
    onlineMode=mode;
    onlineRepo=onlineProject?.repo??onlineRepo;
    if(mode==='create'){onlineNewName='';draftCatalog=[];selectedTagPreset='';}
    onlineError='';
    ask('Online project');
  }
  async function requestSignIn(){
    const id=configuredClientId || clientId.trim();
    const attempt=++signInAttempt;
    clearTimeout(signInTimer);
    onlineWorking=true;onlineError='';restoreAccountError='';deviceCode='';
    try{
      const prompt=await invoke<{user_code:string;interval:number}>('online_sign_in',{clientId:id});
      if(attempt!==signInAttempt)return;
      if(!configuredClientId)localStorage.setItem('studio-github-client-id',id);
      deviceCode=prompt.user_code;
      signInTimer=setTimeout(()=>void pollSignIn(attempt),prompt.interval*1000);
    }catch(error){if(attempt===signInAttempt)onlineError=String(error);}
    finally{if(attempt===signInAttempt)onlineWorking=false;}
  }
  async function pollSignIn(attempt:number){
    try{
      const result=await invoke<{status:'pending';interval:number}|{status:'signed_in';account:GitHubAccount}>('online_poll');
      if(attempt!==signInAttempt)return;
      if(result.status==='pending'){
        signInTimer=setTimeout(()=>void pollSignIn(attempt),result.interval*1000);
      }else{
        commit();typingField=null;
        account=result.account;deviceCode='';
        void discoverProjects();
      }
    }catch(error){if(attempt===signInAttempt){deviceCode='';onlineError=String(error);}}
  }
  async function restoreGitHubAccount(){
    const attempt=++restoreAttempt;
    restoringAccount=true;restoreSkipped=false;restoreAccountError='';
    try{
      const restored=await invoke<GitHubAccount|null>('online_restore');
      if(attempt!==restoreAttempt)return;
      account=restored??undefined;
      if(account){
        commit();typingField=null;
        void discoverProjects();
      }
    }catch(error){if(attempt===restoreAttempt)restoreAccountError=String(error);}
    finally{
      if(attempt===restoreAttempt){
        restoringAccount=false;
        applyProjectOpeningMode();
      }
    }
  }
  function skipGitHubRestore(){
    ++restoreAttempt;
    restoringAccount=false;restoreSkipped=true;
    applyProjectOpeningMode();
  }
  async function signOut(){
    if(onlineWorking || busy)return;
    ++signInAttempt;clearTimeout(signInTimer);deviceCode='';onlineError='';
    onlineWorking=true;
    signingOut=true;
    try{
      await presence.leave();
      await invoke('online_sign_out');
      if(editing && editorId===onlineIdentity?.id){commit();typingField=null;toggleEdit();}
      account=undefined;
      restoreAccountError='';
      if(discovering)stopDiscovery();
      discoveredProjects=[];discoveryPaused=false;discoveryError='';
      if(onlineProject)onlineProject={...onlineProject,writable:false};
    }catch(error){onlineError=String(error);}
    finally{onlineWorking=false;signingOut=false;}
  }
  function projectAttachments(doc:Project){
    const pages=[...doc.pages,...doc.trash??[],...(doc.pageHistory??[]).map(version=>version.page)];
    return pages.flatMap(page=>page.items.flatMap(item=>item.attachment?[item.attachment]:[]));
  }
  function checkOnlineAttachments(doc:Project){
    if(projectAttachments(doc).some(attachment=>!attachment.shared))
      throw Error('This document contains local file paths, including possible entries in page history. Online projects require shared attachments.');
  }
  function snapshotForSave(online=false){
    commit();typingField=null;
    const pageHistory=checkpointPages();
    const document={...project,pageHistory};
    if(online)checkOnlineAttachments(document);
    return {pageHistory,raw:JSON.stringify(document),revision:revisions[revision]};
  }
  function acceptSaved(snapshot:ReturnType<typeof snapshotForSave>){
    const {pageHistory,raw,revision:saving}=snapshot;
    project.pageHistory=pageHistory;
    savedRevision={...saving,doc:{...saving.doc,pageHistory}};
    if(revisions[revision]===saving)revisions[revision]=savedRevision;
    dirty=JSON.stringify(project)!==raw;
  }
  async function accessOnline(action:'open'|'save'|'reload'|'create',selectedRepo?:string,selectedBranch?:string){
    if(onlineWorking || busy || !account)return;
    const repo=action==='reload' ? onlineProject?.repo : selectedRepo??onlineRepo.trim();
    if(!repo || (action==='save' && (welcome || onlineProject)))return;
    const newName=onlineNewName.trim();
    if(action==='create' && (!newName || invalidDraftTags()))return;
    if(action!=='save' && !await confirmUnsaved(action))return;
    onlineWorking=true;busy=true;onlineError='';
    try{
      if(action==='create'){
        const document=JSON.stringify({version:1,name:newName,catalog:saveDraftTags(),pages:[page('Overview')] } satisfies Project);
        const saved=await invoke<{sha:string;branch:string}>('online_save',{repo,branch:null,sha:null,document});
        await load(document,'',{repo,...saved,writable:true});
        onlineNewName='';
        status='Online project created · '+repo;
        void discoverProjects();
      }else if(action==='save'){
        const snapshot=snapshotForSave(true);
        const saved=await invoke<{sha:string;branch:string}>('online_save',{repo,branch:null,sha:null,document:snapshot.raw});
        onlineProject={repo,...saved,writable:true};path='';
        rememberOnlineProject(onlineProject);
        if(editing && onlineIdentity)editorId=onlineIdentity.id;
        acceptSaved(snapshot);
        status='Project saved online · '+repo;
        void discoverProjects();
      }else{
        const {document,...remote}=await invoke<Omit<OnlineProject,'repo'> & {document:string}>('online_open',{repo,branch:action==='reload'?onlineProject?.branch:selectedBranch??null});
        await load(document,'',{repo,...remote});
        status='Online project opened · '+repo;
      }
      onlineRepo=repo;
      localStorage.setItem('studio-online-repo',repo);
      if(dialogType==='Online project')dialog.close();
    }catch(error){
      onlineError=String(error);
      recordError('Online project · '+action,error);
      if(!dialog.open){if(welcome)welcomeError=onlineError;else status=onlineError;}
    }
    finally{onlineWorking=false;busy=false;}
  }
  async function saveAsLocal(copyOnly=false){
    if(busy || !onlineProject)return;
    const repo=onlineProject.repo;
    const wasEditing=editing;
    busy=true;onlineError='';
    try{
      const destination=await save({defaultPath:project.name+'.tkv',filters:fileFilters});
      if(destination){
        const snapshot=snapshotForSave();
        if(copyOnly){
          await invoke('save_online_project_locally',{repo,path:destination,document:snapshot.raw});
          status='Local recovery copy saved';
          return;
        }
        if(projectAttachments(project).some(attachment=>attachment.shared)){
          editing=false;
          const document=await invoke<string>('save_online_project_locally',{repo,path:destination,document:snapshot.raw});
          const local=JSON.parse(document) as Project;
          project=local;
          resetEdits(true);
        }else{
          await invoke('save_project',{path:destination,document:snapshot.raw});
          acceptSaved(snapshot);
        }
        path=destination;onlineProject=undefined;rememberProject(destination);
        status='Project saved locally';
      }
    }catch(error){recordError('Save as local',error);status=String(error);}
    finally{editing=wasEditing;busy=false;}
  }
  async function manageProjectCache(action:'open'|'clear'){
    const repo=onlineProject?.repo;
    if(!repo || projectCacheBusy)return;
    projectCacheBusy=true;projectCacheStatus='';projectCacheError='';
    try{
      if(action==='open'){
        await invoke('open_project_cache',{repo});
      }else if(await confirm('Clear downloaded files from this device? Tenkivo will download them again when needed.')){
        const removed=await invoke<number>('clear_project_cache',{repo});
        projectCacheStatus=removed===0?'The cache was already empty.':'Project cache cleared.';
      }
    }catch(error){projectCacheError=String(error);}
    finally{projectCacheBusy=false;}
  }
  let fileDialog:HTMLDialogElement, incoming=$state<string[]>([]), copying=$state(false), importError=$state(''), playing=$state('');
  let dropAt={x:0,y:0};
  function offerFiles(paths:string[],x:number,y:number){
    if(!editing){status=readHint;return;}
    if(!paths.length || document.querySelector('dialog[open]'))return;
    if(onlineProject){
      void importOnlineFiles(onlineProject.repo,paths,x,y);
      return;
    }
    incoming=paths;dropAt={x,y};importError='';fileDialog.showModal();
  }
  async function confirmOnlineFile(name:string,bytes:number){
    return bytes<=5*1024*1024 || await confirm(name+' ('+(bytes/(1024*1024)).toFixed(1)+' MiB) will be shared at its original size. Add this file?');
  }
  async function importOnlineFiles(repo:string,paths:string[],x:number,y:number){
    const owner=project,sourcePage=current;
    const stillEditing=()=>project===owner && current===sourcePage && editing && onlineProject?.repo===repo;
    let failure='';
    for(const [index,file] of paths.entries()){
      const name=file.split(/[\\/]/).at(-1)??'File';
      try{
        const bytes=await invoke<number>('online_attachment_size',{path:file});
        if(!stillEditing())return;
        if(!await confirmOnlineFile(name,bytes))continue;
        if(!stillEditing())return;
        const left=x+index*18,top=y+index*18;
        if(/\.(png|jpg|jpeg|gif|webp)$/i.test(name)){
          await importEmbeddedImages([{name,read:()=>invoke<string>('read_embedded_image',{path:file})}],left,top,false);
        }else{
          const attachment=await invoke<Attachment>('stage_online_attachment',{repo,path:file});
          if(!stillEditing())return;
          const media=mediaKind(attachment);
          add('Attachment',{name:attachment.path,attachment,department:'Design',x:left,y:top,
            w:media==='video'?360:media==='audio'?300:230,h:media==='video'?240:media==='audio'?120:100});
        }
      }catch(error){failure='Could not add '+name+'. '+String(error);}
    }
    if(failure && stillEditing())status=failure;
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
  function canvasMenu(e:MouseEvent){
    e.preventDefault();e.stopPropagation();focusArea='canvas';
    menu={canvas:true,x:Math.max(0,Math.min(e.clientX,innerWidth-200)),
      y:Math.max(0,Math.min(e.clientY,innerHeight-308)),point:worldPoint(e)};
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
    try{if(o.attachment)await invoke('open_attachment',{project:path,attachment:await materializeAttachment(o.attachment,onlineProject?.repo)});
      else if(o.url && isTauri())await invoke('open_web',{url:o.url});
      else if(o.url)window.open(o.url,'_blank','noopener,noreferrer');
    }catch(err){status='Could not open '+o.name+'. '+String(err);}
  }
  async function revealAttachment(o:Item){
    if(!o.attachment)return;
    try{await invoke('reveal_attachment',{project:path,attachment:await materializeAttachment(o.attachment,onlineProject?.repo)});}
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
    const previous=o.attachment,owner=project,sourcePage=board,repo=onlineProject?.repo;
    if(!previous)return;
    try{
      const file=await open({directory:previous.folder});
      if(!file)return;
      let replacement:Attachment;
      if(previous.shared && repo){
        const bytes=await invoke<number>('online_attachment_size',{path:file});
        if(!await confirmOnlineFile(file.split(/[\\/]/).at(-1)??'File',bytes))return;
        replacement=await invoke<Attachment>('stage_online_attachment',{repo,path:file});
      }else replacement={...previous,path:file,local:false};
      if(project!==owner || !editing || !owner.pages.includes(sourcePage) || !sourcePage.items.includes(o) || o.attachment!==previous)return;
      o.attachment=replacement;change();
    }catch(error){status='Could not replace the attachment. '+String(error);}
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
    if(canvasClipboardBlocked(e.target))return;
    const text=e.clipboardData?.getData('text/plain')??'';
    if(pageClipboard && (!text || text===pageClipboard.selection.token)){
      e.preventDefault();void pastePage();
    }else if(canvasClipboard && (!text || text===canvasClipboard.token)){
      e.preventDefault();
      pasteObjects();
    }else if(insertUrl(text))e.preventDefault();
  }
  let zoom = $state(1), tx = $state(30), ty = $state(40), width = $state(800), height = $state(600);
  let viewport=$state<HTMLDivElement>(null!), imageInput: HTMLInputElement, fileInput: HTMLInputElement, dialog: HTMLDialogElement;
  let dialogType = $state(''), name = $state('');
  let filters = $state<Department[]>([]), drag: {x:number;y:number;tx:number;ty:number;objects:{o:Item;x:number;y:number}[];pan:boolean} | undefined;
  const catalog=$derived(project.catalog??defaultCatalog);
  const departments=$derived(catalog.map(d=>d.id));
  const colors=$derived(Object.fromEntries(catalog.map(d=>[d.id,d.color])));
  const names=$derived(Object.fromEntries(catalog.map(d=>[d.id,d.name])));
  let draftName=$state(''),draftDescription=$state(''),draftCatalog=$state<typeof defaultCatalog>([]);
  let selectedTagPreset=$state<keyof typeof tagPresets|''>('');
  let excludedPresetTags=$state<string[]>([]);
  const presetPreview=$derived(selectedTagPreset?availablePresetTags(draftCatalog,selectedTagPreset):[]);
  const selectedPresetTags=$derived(presetPreview.filter(name=>!excludedPresetTags.includes(name)));
  function applyTagPreset(){
    if(!selectedTagPreset || !selectedPresetTags.length)return;
    draftCatalog=addTagPreset(draftCatalog,selectedTagPreset,selectedPresetTags);
    selectedTagPreset='';
  }
  function invalidDraftTags(){return draftCatalog.some(tag=>!tag.name.trim());}
  function saveDraftTags(){return draftCatalog.map(tag=>({...tag,name:tag.name.trim()}));}
  const userIcons=['👤','🦊','🐻','🐱','🐼','🐙','🌙','🌿','🎨','🎧'];
  let draftUsers=$state<ProjectUser[]>([]), editorId=$state('');
  let pickerUsers=$state<ProjectUser[]>([]), selectedUser=$state(''),registering=$state(false);
  let userName=$state(''),userIcon=$state(userIcons[0]);
  let editor=$derived(editorId===onlineIdentity?.id?onlineIdentity:project.users?.find(u=>u.id===editorId));
  let commentItem=$state<Item|Page>(), threadId=$state(''), draftComment=$state('');
  const reader=$derived(onlineProject?onlineIdentity:editor??lastLocalEditor([...(onlineIdentity?[onlineIdentity]:[]),...project.users??[]])??onlineIdentity);
  const readKey=$derived('tenkivo-read-comments:'+JSON.stringify([onlineProject?.repo??(path||project.pages[0]?.id),reader?.id??'guest']));
  let receiptVersion=$state(0);
  const receipts=$derived.by(()=>{
    void receiptVersion;
    try{return new Set<string>(JSON.parse(localStorage.getItem(readKey)??'[]'));}catch{return new Set<string>();}
  });
  function unread(t:CommentThread){return t.messages.filter(m=>m.user!==reader?.id && !receipts.has(m.id)).length;}
  function unreadComments(o:Item|Page){return (o.threads??[]).reduce((count,t)=>count+unread(t),0);}
  function viewThread(t:CommentThread){
    threadId=t.id;draftComment='';
    if(unread(t)){
      localStorage.setItem(readKey,JSON.stringify([...new Set([...receipts,...t.messages.map(m=>m.id)])]));
      receiptVersion++;
    }
  }
  function showComments(o:Item|Page, selected?:CommentThread|null){
    menu=undefined;commentItem=o;threadId='';draftComment='';ask('Conversations');
    if(selected===null)return;
    const t=selected??o.threads?.find(t=>unread(t))??o.threads?.find(t=>!t.archived)??o.threads?.[0];
    if(t)viewThread(t);
  }
  function groupComments(messages:CommentThread['messages']){
    const groups:CommentThread['messages'][]=[];
    for(const message of messages){
      const lastGroup=groups.at(-1);
      const first=lastGroup?.[0];
      if(lastGroup && first?.user===message.user && Math.floor(first.time/60_000)===Math.floor(message.time/60_000)){
        lastGroup.push(message);
      }else groups.push([message]);
    }
    return groups;
  }
  function postComment(){
    if(!editing || !editor || !commentItem || !draftComment.trim())return;
    const message={id:crypto.randomUUID(),user:editor.id,author:editor.name,icon:editor.icon,text:draftComment.trim(),time:Date.now()};
    const t=commentItem.threads?.find(t=>t.id===threadId);
    if(t?.archived)return;
    if(t)t.messages.push(message);
    else{
      threadId=crypto.randomUUID();
      commentItem.threads=[...commentItem.threads??[],{id:threadId,archived:false,messages:[message]}];
    }
    change();draftComment='';
  }
  function normalizeName(value:string){return value.trim().toLowerCase();}
  let validUsers=$derived(draftUsers.every(u=>u.name.trim()) && new Set(draftUsers.map(u=>normalizeName(u.name))).size===draftUsers.length);
  let validRegistration=$derived(userName.trim()!=='' && !project.users?.some(u=>normalizeName(u.name)===normalizeName(userName)));
  function recentEditors():string[]{
    try{const ids:unknown=JSON.parse(localStorage.getItem('studio-recent-editors')??'[]');return Array.isArray(ids)?ids.filter(id=>typeof id==='string'):[];}
    catch{return [];}
  }
  function lastLocalEditor(users:ProjectUser[]){
    const saved=path?localStorage.getItem('studio-project-editor:'+path):null;
    if(saved)return users.find(user=>user.id===saved);
    // Older versions only remembered a shared list. Local profile IDs can be matched to this project.
    return recentEditors()
      .filter(id=>!path || project.users?.some(user=>user.id===id))
      .map(id=>users.find(user=>user.id===id)).find(Boolean);
  }
  function applyProjectOpeningMode(){
    if(!openingEditPending || restoringAccount)return;
    openingEditPending=false;
    if(welcome || editing || projectOpeningMode!=='edit')return;
    selectEditor(true);
  }
  function selectEditor(automatic=false){
    if(onlineProject && !onlineIdentity){showGitHubAccount();return;}
    if(onlineProject && !onlineProject.writable){status='Reload the project after signing in with write access.';return;}
    const users=[...(onlineIdentity?[onlineIdentity]:[]),...project.users??[]];
    const last=onlineProject?undefined:lastLocalEditor(users);
    const automaticEditor=last??onlineIdentity??(users.length===1?users[0]:undefined);
    const preferred=onlineProject?onlineIdentity:automatic?automaticEditor:undefined;
    if(preferred){
      editorId=preferred.id;editing=true;
      if(status===readHint)status='';
      return;
    }
    pickerUsers=last?[last,...users.filter(u=>u.id!==last.id)]:[...users];
    selectedUser=pickerUsers[0]?.id??'';registering=!users.length;userName='';userIcon=userIcons[0];ask('Select editing user');
    if(!registering)void tick().then(()=>{
      if(dialog.open && dialogType==='Select editing user' && !registering)
        dialog.querySelector<HTMLSelectElement>('.user-picker select')?.focus();
    });
  }
  function confirmSelectedUser(e:KeyboardEvent){
    if(registering || !selectedUser || e.key!=='Enter' || e.isComposing || e.altKey || e.ctrlKey || e.metaKey || e.shiftKey)return;
    if(!(e.target instanceof HTMLSelectElement))return;
    e.preventDefault();
    if(!e.repeat)enterEditing();
  }
  function enterEditing(){
    if(registering){
      if(!validRegistration)return;
      const user={id:crypto.randomUUID(),name:userName.trim(),icon:userIcon};
      project.users=[...project.users??[],user];selectedUser=user.id;change();
    }
    if(selectedUser!==onlineIdentity?.id && !project.users?.some(u=>u.id===selectedUser))return;
    editorId=selectedUser;editing=true;
    if(path && !onlineProject)localStorage.setItem('studio-project-editor:'+path,editorId);
    localStorage.setItem('studio-recent-editors',JSON.stringify([editorId,...recentEditors().filter(id=>id!==editorId)]));
    if(status===readHint)status='';
    dialog.close();
  }
  let section=$state(''), priorDepartments:typeof defaultCatalog=[],priorUsers:ProjectUser[]=[];
  function openSection(value:string){
    selectedTagPreset='';priorDepartments=$state.snapshot(draftCatalog);priorUsers=$state.snapshot(draftUsers);section=value;
  }
  function leaveSection(apply:boolean){
    if(!apply){draftCatalog=priorDepartments;draftUsers=priorUsers;}
    else if(dialogType==='Project settings' && editing){applyOptions(false);}
    section='';
  }
  function projectOptions(){section='';
    projectCacheStatus='';projectCacheError='';
    draftUsers=(project.users??[]).map(u=>({...u}));draftName=project.name;draftDescription=project.description??'';draftCatalog=catalog.map(d=>({...d}));ask('Project settings');
  }
  function applyOptions(close=true){
    if(!editing || !validUsers || !draftName.trim() || invalidDraftTags())return;
    const allowed=new Set(draftCatalog.map(d=>d.id));
    for(const p of [...project.pages,...project.trash??[]]){
      if(p.departments)p.departments=p.departments.filter(id=>allowed.has(id));
      for(const o of p.items){
        if(!allowed.has(o.department))o.department='';
        if(o.departments)o.departments=o.departments.filter(id=>allowed.has(id));
      }
    }
    if(!onlineProject)project.users=draftUsers.map(u=>({...u,name:u.name.trim()}));
    project.name=draftName.trim();project.description=draftDescription.trim();
    project.catalog=saveDraftTags();setFilters(filters.filter(id=>allowed.has(id)));change();if(close)dialog.close();
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
  const fileFilters = [{name:'Tenkivo',extensions:['tkv']}];
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
    recordActivity('Updates',manual?'Checking GitHub Releases on request.':'Checking GitHub Releases automatically.');
    try{
      const {notice:result,message}=await invoke<{notice:typeof release;message:string}>('check_update');
      recordActivity('Updates',message);
      if(!updateChecksActive || installing)return;
      const newVersion=result && result.version!==release?.version;
      release=result;
      if(newVersion){
        updateError='';
        if(!updateDialog.open)updateDialog.showModal();
      }else if(!result){
        if(updateDialog.open)updateDialog.close();
        if(!manual)return;
        status=message;cooldown=true;
      }
    }catch(error){if(manual)status='Update check failed: '+String(error);}
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
      recordActivity('Updates','Downloading version '+version+'.');
      await invoke('install_update',{version});
      recordActivity('Updates','Version '+version+' is ready. Restarting Tenkivo.');
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
    if(to.url)return to.url;
    const p=project.pages.find(p=>p.id===to.pageId);
    if(!p)return 'Missing page';
    return to.itemId ? p.name+' / '+(p.items.find(o=>o.id===to.itemId)?.name ?? 'Missing object') : p.name;
  }
  async function follow(to: Destination) {
    if(to.url){
      const url=webAddress(to.url);
      if(url)try{if(isTauri())await invoke('open_web',{url:url.href});else window.open(url.href,'_blank','noopener');}
      catch(error){status=String(error);}
      return;
    }
    const p=project.pages.find(p=>p.id===to.pageId);
    const target=p?.items.find(o=>o.id===to.itemId);
    if(!p || (to.itemId && !target)){status='Link destination is missing.';return;}
    await navigate(p);
    if(target){if(!matches(itemTags(target),filters)){status='Destination hidden by tag filters.';return;}ids=[target.id];fit([target]);}
  }
  function remember(o:Item, field:HTMLTextAreaElement) {
    range={id:o.id,start:field.selectionStart,end:field.selectionEnd};
  }
  function updateText(o:Item, field:HTMLTextAreaElement) {
    editBody(o,field.value,Math.min(beforeStart ?? Infinity,field.selectionStart));
    beforeStart=undefined;remember(o,field);change();
  }
  let webTarget=$state(''), linkType=$state('project');
  function chooseLink(span?: {start:number;end:number}) {
    if(!editing || !active)return;
    draftRange=span && {start:span.start,end:span.end};
    const existing=span ? active.textLinks?.find(l=>l.start===span.start && l.end===span.end)?.to : objectLink(active);
    webTarget=existing?.url??'';linkType=webTarget?'web':'project';
    targetPage=existing?.pageId || current;targetItem=existing?.itemId ?? '';
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
        if(!old || JSON.stringify(pageSnapshot(p))!==JSON.stringify(pageSnapshot(old))){
          changedBy.set(p.id,{...editor});
          if(onlineProject && editing && p.id===current)modifiedPage=current;
        }
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
    if(editorId!==onlineIdentity?.id && !project.users?.some(u=>u.id===editorId)){stopDrawing();editing=false;editorId='';}
    current=project.pages.find(p=>p.id===affectedPage)?.id??project.pages.find(p=>p.id===entry.page)?.id??project.pages[0].id;
    ids=ids.filter(id=>board.items.some(o=>o.id===id));
    filters=filters.filter(d=>departments.includes(d));
    history=history.filter(v=>project.pages.some(p=>p.id===v.page));cursor=history.map(v=>v.page).lastIndexOf(current);
    if(cursor<0){history.push(view());cursor=history.length-1;}trimHistory();
    if(current!==oldPage)fit();
    dirty=entry!==savedRevision || JSON.stringify(pageHistory)!==JSON.stringify(savedRevision?.doc.pageHistory);status=direction<0?'Undone':'Redone';
  }
  const readHint='Read mode is on. Press E to enable editing.';
  function toggleEdit() { openingEditPending=false;stopDrawing();wire=undefined;selectedConnection='';if(!editing){requestEditing();return;} frameOnly='';editing=false;editorId=''; if(status===readHint)status='';inline='';gesture=undefined;liveBox=undefined;drag=undefined; }
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
    if(e.button>1 || (e.button===0 && (e.target as HTMLElement).closest('input,textarea,[contenteditable],button,audio,video,.drawing-toolbar')))return;
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
    if(target.closest('input,textarea,[contenteditable],audio,video'))return;
    if(editing && o.kind==='Note' && target.closest('.caption')){
      ids=[o.id];inline='';titleDraft=o.name;titleId=o.id;await tick();
      const field=viewport.querySelector<HTMLInputElement>('.note-title');field?.focus();field?.select();return;
    }
    if(editing && o.kind==='Frame'){ids=[o.id];frameOnly=frameOnly===o.id?'':o.id;return;}
    if(o.attachment || o.url){await openResource(o);return;}
    const to=objectLink(o);
    if(to && (!editing || o.kind==='Page link'))await follow(to);
    else if(!editing)status=readHint;
    else if(editing && o.kind==='Note'){inline=o.id;await tick();viewport.querySelector<HTMLElement>('[contenteditable]')?.focus();}
  }
  function frameColorFor(frame:Item){
    return frame.frameBackground??colors[itemTags(frame)[0]]??'#808080';
  }
  function add(kind:Kind, extra:Partial<Item>={}) {
    if(!editing)return;
    const o=item(kind,kind==='Note'?'New note':kind,(width/2-tx)/zoom-110,(height/2-ty)/zoom-60,{inheritDepartment:true,...extra});
    if(kind==='Frame'){
      o.w=480;o.h=300;
      const frames=board.items.filter(item=>item.kind==='Frame');
      const usedColors=new Set(frames.map(frame=>frameColorFor(frame).toLowerCase()));
      const nextUnused=framePalette.find(color=>!usedColors.has(color));
      const previousColor=frames[0]?.frameBackground?.toLowerCase()??'';
      o.frameBackground=nextUnused??framePalette[(framePalette.indexOf(previousColor)+1)%framePalette.length];
      board.items.unshift(o);
    }else board.items.push(o);
    ids=[o.id];focusArea='canvas';change();return o;
  }
  async function importEmbeddedImages(files:{name:string;read:()=>Promise<string>}[], x=(width/2-tx)/zoom,y=(height/2-ty)/zoom,warnLarge=true) {
    if(!editing || !files.length)return;
    const owner=project,sourcePage=current;
    let failure='';
    for(const [i,f] of files.entries()) {
      try {
        const source=await f.read();
        if(project!==owner || current!==sourcePage || !editing)return;
        if(onlineProject && warnLarge){
          const encodedLength=source.length-source.indexOf(',')-1;
          const padding=source.endsWith('==')?2:source.endsWith('=')?1:0;
          const bytes=Math.floor(encodedLength*3/4)-padding;
          if(!await confirmOnlineFile(f.name,bytes))continue;
          if(project!==owner || current!==sourcePage || !editing)return;
        }
        const img=new Image();img.src=source;await img.decode();
        if(project!==owner || current!==sourcePage || !editing)return;
        const w=Math.min(360,img.naturalWidth), h=w*img.naturalHeight/img.naturalWidth+32;
        add('Image',{name:f.name,source,w,h,department:'Art',x:x+i*18,y:y+i*18});
      }catch(error){recordError('Image import',error);failure='Could not import '+f.name+'. '+String(error);}
    }
    if(failure && project===owner && current===sourcePage)status=failure;
  }
  function importImages(files:File[],x?:number,y?:number) {
    return importEmbeddedImages(files.map(file=>({
      name:file.name,
      read:()=>new Promise<string>((resolve,reject)=>{
        if(!['image/png','image/jpeg','image/gif','image/webp'].includes(file.type)){
          reject('Choose a PNG, JPEG, GIF or WebP image.');return;
        }
        const reader=new FileReader();
        reader.onload=()=>resolve(String(reader.result));
        reader.onerror=()=>reject('The image could not be read.');
        reader.readAsDataURL(file);
      })
    })),x,y);
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
    creationError='';draftUsers=[];draftName='';draftDescription='';draftCatalog=[];selectedTagPreset='';ask('New project');
  }
  function ask(type:string){dialogType=type;name='';dialog.showModal();}
  type UnsavedAction='exit'|'close'|'open'|'create'|'reload';
  type UnsavedChoice='save'|'discard'|'cancel';
  const unsavedLabels:Record<UnsavedAction,{action:string;save:string}> = {
    exit:{action:'closing Tenkivo',save:'Save and Close'},
    close:{action:'closing the project',save:'Save and Close'},
    open:{action:'opening another project',save:'Save and Open'},
    create:{action:'creating a new project',save:'Save and Continue'},
    reload:{action:'reloading from GitHub',save:'Save and Reload'},
  };
  let unsavedDialog:HTMLDialogElement;
  let unsavedAction=$state<UnsavedAction>('exit');
  let resolveUnsaved:((choice:UnsavedChoice)=>void)|undefined;
  function finishUnsaved(choice:UnsavedChoice){
    const resolve=resolveUnsaved;
    resolveUnsaved=undefined;
    unsavedDialog.close();
    resolve?.(choice);
  }
  async function confirmUnsaved(action:UnsavedAction){
    if(busy || resolveUnsaved)return false;
    if(!dirty)return true;
    unsavedAction=action;
    const choice=await new Promise<UnsavedChoice>(resolve=>{
      resolveUnsaved=resolve;
      unsavedDialog.showModal();
    });
    if(choice==='cancel')return false;
    if(choice==='save'){
      await saveFile();
      return !dirty;
    }
    return true;
  }
  async function closeProject(){
    if(!await confirmUnsaved('close'))return;
    openingEditPending=false;
    stopDrawing();welcome=true;welcomeError='';path='';onlineProject=undefined;status='';editing=false;
    project=initial;current=initial.pages[0].id;resetViews();resetEdits(true);
    inline='';frameOnly='';titleId='';playing='';range=undefined;marquee=undefined;
    gesture=undefined;drag=undefined;liveBox=undefined;
    localStorage.removeItem('studio-last');
    localStorage.removeItem(lastOnlineProjectKey);
    await checkRecent();
  }
  async function named(){
    if(dialogType==='New project'){
      if(busy || !validUsers || !draftName.trim() || invalidDraftTags())return;
      if(!await confirmUnsaved('create'))return;
      busy=true;creationError='';
      try{
        const p=page('Overview');
        const next:typeof initial={version:1,users:draftUsers.map(u=>({...u,name:u.name.trim()})),name:draftName.trim(),description:draftDescription.trim(),catalog:saveDraftTags(),pages:[p]};
        const location=isTauri()?await save({defaultPath:next.name+'.tkv',filters:fileFilters}):'';
        if(location===null)return;
        if(location)await invoke('save_project',{path:location,document:JSON.stringify(next)});
        openingEditPending=false;
        project=next;path=location;onlineProject=undefined;welcome=false;editorId='';editing=false;resetViews();await navigate(p);resetEdits(!!location);rememberProject(location);status='Project created';dialog.close();selectEditor(true);
      }catch(error){creationError=String(error);recordError('Create project',error);}
      finally{busy=false;}
      return;
    }
    if(!name.trim())return;
    const p=page(name.trim());p.parentId=pageParent;project.pages.push(p);await navigate(p);
    change();dialog.close();
  }
  async function load(raw:string,location:string,remote?:OnlineProject){
    const data:unknown=JSON.parse(raw);
    if(data && typeof data==='object' && 'pages' in data && Array.isArray(data.pages)){
      migrateLegacyDiagrams(data as Project);
    }
    if(!validProject(data))throw Error('Unsupported or invalid project.');
    if(remote)checkOnlineAttachments(data);
    project=data;path=location;onlineProject=remote;welcome=false;
    if(remote)rememberOnlineProject(remote);else rememberProject(location);
    dirty=false;editorId='';editing=false;resetViews();await navigate(project.pages[0]);resetEdits(true);status='Project opened';
    openingEditPending=projectOpeningMode==='edit' && !openNextProjectInReadMode;
    openNextProjectInReadMode=false;
    applyProjectOpeningMode();
    return true;
  }
  async function openFile(){
    if(!isTauri()){fileInput.click();return;}
    try{const f=await open({filters:[{name:'Tenkivo projects',extensions:['tkv','gdstudio']}]});if(f && await confirmUnsaved('open'))await load(await invoke('open_project',{path:f}),f);}
    catch(e){recordError('Project file',e);status=String(e);if(welcome)welcomeError=status;}
  }
  async function saveFile(){
    if(busy)return;busy=true;
    try{
      const remote=onlineProject;
      if(remote && (!account || !remote.writable)){status='Sign in to GitHub with write access to save this project.';return;}
      const location=isTauri() && !remote?(path || await save({defaultPath:project.name+'.tkv',filters:fileFilters})):'';
      if(isTauri() && !remote && !location)return;
      const snapshot=snapshotForSave(!!remote);
      const raw=snapshot.raw;
      if(remote){
        const {sha}=await invoke<{sha:string}>('online_save',{repo:remote.repo,branch:remote.branch,sha:remote.sha,document:raw});
        onlineProject={...remote,sha};
      }else if(location){await invoke('save_project',{path:location,document:raw});path=location;rememberProject(location);}
      else{const a=document.createElement('a');const url=URL.createObjectURL(new Blob([raw]));a.href=url;a.download=project.name+'.tkv';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}
      acceptSaved(snapshot);
      status=remote?'Project saved to GitHub':'Project saved';
    }catch(e){
      recordError('Project file',e);status=String(e);
      if(onlineProject && status.startsWith('Online save conflict:'))ask('Online save conflict');
      if(welcome)welcomeError=status;
    }finally{busy=false;}
  }
  let closingWindow=false;
  async function exitApp(){
    if(busy || closingWindow)return;
    closingWindow=true;
    try{
      if(!await confirmUnsaved('exit'))return;
      await presence.leave();
      await getCurrentWindow().destroy();
    }catch(error){
      status='Could not close Tenkivo: '+String(error);
      recordError('Close Tenkivo',error);
    }finally{closingWindow=false;}
  }
  let exporter:ExportDialog;
  async function ensureExportSaved(){
    if(onlineProject){status='Use File → Save as Local before creating a project archive.';return false;}
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
    if(e.key==='Escape' && (menu || fileMenu?.open || githubMenu?.open)){
      menu=undefined;
      if(fileMenu)fileMenu.open=false;
      if(githubMenu)githubMenu.open=false;
      return;
    }
    if(e.key==='Escape' && historyMenu?.open){historyMenu.open=false;historyMenu.querySelector('summary')?.focus();return;}
    if(document.querySelector('dialog[open]'))return;
    if((e.target as HTMLElement).closest?.('.note-title')){
      if((e.ctrlKey || e.metaKey) && e.key.toLowerCase()==='s' && active){
        e.preventDefault();finishTitle(active,true);void saveFile();
      }
      return;
    }
    const key=e.key.toLowerCase();
    if(editing && active?.kind==='Note' && (e.ctrlKey||e.metaKey) && (e.target as HTMLElement).closest('textarea,[contenteditable]') && range?.id===active.id && range.start!==range.end){
      const style=({b:'bold',i:'italic',u:'underline'} as Record<string,NoteStyle>)[key];
      if(style){e.preventDefault();formatNote(style);return;}
      if(key==='k'){e.preventDefault();chooseLink(range);return;}
    }
    if((e.ctrlKey || e.metaKey) && !e.altKey && (key==='z' || key==='y')){
      e.preventDefault();restoreEdit(key==='y' || e.shiftKey?1:-1);return;
    }
    if(e.key==='Escape'){commit();frameOnly='';menu=undefined;gesture=undefined;liveBox=undefined;drag=undefined;marquee=undefined;}
    if((e.target as HTMLElement).closest('input,textarea,select,[contenteditable]') && e.key==='Delete')return;
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
    if((e.target as HTMLElement).closest('input,textarea,select,[contenteditable]') || e.ctrlKey || e.metaKey || e.altKey)return;
    if(e.key.toLowerCase()==='f')fit();
    if(e.key.toLowerCase()==='e' && !e.repeat)toggleEdit();
    if(e.key==='Escape'){inline='';ids=[];}
  }
  onMount(()=>{
    const openingModeStartup=isTauri()?invoke<boolean>('should_reset_opening_mode_prompt').then(reset=>{
      if(!reset)return;
      resetOpeningModePrompt();
      openNextProjectInReadMode=true;
    }).catch(()=>{}):undefined;

    let diagnosticsTimer:ReturnType<typeof setInterval>|undefined;
    let collectingDiagnostics=false;
    const collect=()=>{
      if(collectingDiagnostics)return;
      collectingDiagnostics=true;
      void collectNativeDiagnostics().catch(()=>clearInterval(diagnosticsTimer))
        .finally(()=>{collectingDiagnostics=false;});
    };
    if(isTauri()){diagnosticsTimer=setInterval(collect,2000);collect();}
    if(isTauri())void invoke<string|null>('github_client_id').then(id=>{configuredClientId=id??'';}).catch(()=>{});
    let restoreFrame:number|undefined;
    const githubRestore=isTauri()?new Promise<void>(resolve=>{
      restoreFrame=requestAnimationFrame(()=>{
        if(restoreSkipped)resolve();
        else void restoreGitHubAccount().finally(resolve);
      });
    }):undefined;
    if(isTauri())void getVersion().then(version=>{
      appVersion=version;
      recordActivity('Startup','Tenkivo '+version+' started.');
    }).catch(error=>{
      appVersion='Unavailable';
      recordError('Startup version',error);
    });
    else recordActivity('Startup','Tenkivo browser preview started.');
    const listener=isTauri()?getCurrentWebview().onDragDropEvent(({payload})=>{
      if(welcome || payload.type!=='drop')return;
      const r=viewport.getBoundingClientRect(),x=payload.position.x/devicePixelRatio-r.left,y=payload.position.y/devicePixelRatio-r.top;
      if(x>=0 && y>=0 && x<=r.width && y<=r.height)offerFiles(payload.paths,(x-tx)/zoom,(y-ty)/zoom);
    }).catch(error=>{recordError('Drag and drop listener',error);return undefined;}):undefined;
    updateChecksActive=isTauri();
    if(updateChecksActive)void invoke<boolean>('update_status').then(failed=>{
      if(failed){
        status='Update could not replace the executable. Close other copies and try again.';
        recordError('Updates',status);
        scheduleUpdateCheck();
      }else void checkUpdates();
    }).catch(()=>{void checkUpdates();});
    if(openingModeStartup)void openingModeStartup.then(()=>invoke<string|null>('startup_project')).then(async file=>{
      if(file){await openRecent(file);return;}
      if(startup==='last'){
        const remote=readLastOnlineProject();
        if(remote){
          await githubRestore;
          if(!welcome || busy || launching || dialog.open || restoreSkipped || startup!=='last')return;
          if(account)await accessOnline('open',remote.repo,remote.branch);
          else welcomeError='Sign in to GitHub to reopen '+remote.repo+'.';
          if(!welcome)return;
        }else{
          const local=localStorage.getItem('studio-last');
          if(local){await openRecent(local);return;}
        }
      }
      await checkRecent();
    }).catch(()=>void checkRecent());
    else void checkRecent();
    const before=(e:BeforeUnloadEvent)=>{if(dirty){e.preventDefault();e.returnValue='';}};
    const refresh=()=>{if(welcome)void checkRecent();};
    window.addEventListener('focus',refresh);
    const closeListener=isTauri()?getCurrentWindow().onCloseRequested(event=>{
      event.preventDefault();
      if(closingWindow || resolveUnsaved)return;
      if(busy){status='Wait for the current operation to finish before closing Tenkivo.';return;}
      void exitApp();
    }).catch(error=>{recordError('Window close listener',error);return undefined;}):undefined;
    if(!isTauri())window.addEventListener('beforeunload',before);
    return ()=>{
      presence.setTarget(null);
      window.removeEventListener('focus',refresh);updateChecksActive=false;clearTimeout(updateTimer);++signInAttempt;clearTimeout(signInTimer);++restoreAttempt;++discoveryAttempt;if(restoreFrame!==undefined)cancelAnimationFrame(restoreFrame);clearInterval(diagnosticsTimer);void listener?.then(stop=>stop?.()).catch(error=>recordError('Drag and drop listener',error));window.removeEventListener('beforeunload',before);
      void closeListener?.then(stop=>stop?.()).catch(error=>recordError('Window close listener',error));
    };
  });
</script>

{#snippet tagEditor()}
  {#each draftCatalog as tag (tag.id)}
    <div class="department-editor">
      <input type="color" aria-label={'Color for '+tag.name} bind:value={tag.color}/>
      <input required aria-label="Tag name" bind:value={tag.name}/>
      <button type="button" title="Remove tag" aria-label={'Remove '+tag.name} onclick={()=>draftCatalog=draftCatalog.filter(other=>other.id!==tag.id)}>×</button>
    </div>
  {/each}
  <button class="add-project-entry" type="button" onclick={()=>draftCatalog.push({id:crypto.randomUUID(),name:'New tag',color:'#8888bb'})}>Add New Tag</button>
  <label class="tag-preset-picker">Tag preset
    <select bind:value={selectedTagPreset} onchange={()=>excludedPresetTags=[]}>
      <option value="">Choose a preset</option>
      {#each Object.keys(tagPresets) as preset}<option value={preset}>{preset}</option>{/each}
    </select>
  </label>
  {#if selectedTagPreset}
    {#if presetPreview.length}
      <fieldset class="tag-preset-preview">
        <legend>Tags to add</legend>
        {#each presetPreview as name (name)}
          <label>
            <input type="checkbox" checked={!excludedPresetTags.includes(name)}
              onchange={event=>excludedPresetTags=event.currentTarget.checked
                ? excludedPresetTags.filter(tag=>tag!==name):[...excludedPresetTags,name]}/>
            {name}
          </label>
        {/each}
      </fieldset>
    {:else}
      <p>All tags from this preset are already in the project.</p>
    {/if}
    <button class="add-project-entry" type="button" disabled={!selectedPresetTags.length} onclick={applyTagPreset}>Add Preset Tags</button>
  {/if}
{/snippet}

{#snippet updatesButton()}
  <button class="update-button" aria-label={updateLabel} title={updateLabel} disabled={!isTauri() || (!release && (checking || cooldown))} onclick={openUpdates}>
    <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v12m-5-5 5 5 5-5M4 16v4h16v-4"/></svg>
    {#if release}<span class="update-badge" aria-hidden="true"></span>{/if}
  </button>
{/snippet}

{#snippet logsButton()}
  {@const label=diagnostics.unreadErrors?'Editor logs · '+diagnostics.unreadErrors+' new '+(diagnostics.unreadErrors===1?'error':'errors'):'Editor logs'}
  <button class="update-button" aria-label={label} title={label} onclick={()=>showLogs()}><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="3" width="16" height="18" rx="2"/><path d="M8 8h8M8 12h8M8 16h5"/></svg>
    {#if diagnostics.unreadErrors}<span class="update-badge log-badge" aria-hidden="true"></span>{/if}
  </button>
{/snippet}

{#snippet gitHubAccountButton()}
  {#if isTauri()}
    {#if restoringAccount}
      {#if !welcome}{@render githubRestoreProgress()}{/if}
    {:else if restoreSkipped && !welcome}
      <button onclick={()=>restoreGitHubAccount()}>Reconnect to GitHub</button>
    {:else}
      <button title="GitHub account" onclick={showGitHubAccount}>{account?'@'+account.login:'Sign in with GitHub'}</button>
    {/if}
  {/if}
{/snippet}

{#snippet githubRestoreProgress()}
  <div class="github-startup-status">
    <span role="status">Restoring your GitHub session…</span>
    <button onclick={skipGitHubRestore}>Skip for now</button>
  </div>
{/snippet}

{#snippet discoverableProjects()}
  <div class="row"><h3>Available projects</h3><button disabled={discovering || busy} onclick={discoverProjects}>Refresh</button></div>
  {#if discovering}
    <div class="github-startup-status"><span role="status">Finding your GitHub projects…</span><button onclick={stopDiscovery}>Cancel search</button></div>
  {:else if discoveryPaused}<p role="status">Search cancelled. Select Refresh to try again.</p>{/if}
  {#if discoveryError}<p role="alert">{discoveryError}</p>{/if}
  {#each discoveredProjects as entry (entry.repo)}
    <div class="recent-project">
      <button disabled={busy || launching} onclick={()=>accessOnline('open',entry.repo)}><strong>{entry.repo}</strong></button>
      <span>{entry.writable?'Can edit':'Read only'}</span>
    </div>
  {:else}
    {#if !discovering && !discoveryPaused && !discoveryError}<p>Online projects appear here after you accept their GitHub invitation.</p>{/if}
  {/each}
{/snippet}

{#snippet closePanel(close:()=>void,disabled=false)}
  <button type="button" class="close-panel" title="Close" aria-label="Close" {disabled} onclick={close}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8"/></svg></button>
{/snippet}
{#snippet frameBackgroundPicker(frame:Item)}
        <div class="frame-background">
          <label>Background color<input type="color" disabled={!editing} value={frameColorFor(frame)}
            onchange={e=>{frame.frameBackground=e.currentTarget.value;change();}}/></label>
          <button disabled={!editing || !frame.frameBackground} onclick={()=>{delete frame.frameBackground;change();}}>Reset</button>
        </div>
{/snippet}

{#snippet imageTitle(image:Item)}
  <label class="check image-title"><input type="checkbox" disabled={!editing} checked={image.showTitle!==false}
    onchange={e=>{image.showTitle=e.currentTarget.checked;change();}}/> Show title</label>
{/snippet}
{#snippet departmentPicker()}
  <details class="object-departments">
    <summary>Tags</summary>
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
<svelte:window oncopy={e=>copyObjects(false,e)} oncut={e=>copyObjects(true,e)} onpaste={paste} onpointermove={e=>pagePointerMove(e)} onpointerup={e=>pagePointerMove(e,true)} onpointercancel={()=>{pagePointer=undefined;draggingPage='';treeDrop='';}} onkeydown={keys} onkeyup={e=>{if(e.key==='Alt')altHeld=false;}} onblur={()=>{altHeld=false;wire=undefined;cancelCanvasDock();}} oncontextmenu={e=>e.preventDefault()} onpointerdown={e=>{if(!(e.target as HTMLElement).closest('.app-menu'))menu=undefined;if(historyMenu && !historyMenu.contains(e.target as Node))historyMenu.open=false;if(fileMenu && !fileMenu.contains(e.target as Node))fileMenu.open=false;if(githubMenu && !githubMenu.contains(e.target as Node))githubMenu.open=false;}}/>
<div class="studio" data-theme={theme} class:launcher={welcome}>
  {#if welcome}
    <main class="welcome">
      <div class="welcome-head"><h1><img src={tenkivoIcon} alt="" width="48" height="48"/>Tenkivo</h1><div class="app-actions">{@render gitHubAccountButton()}<button title="Settings" aria-label="Settings" onclick={()=>settings.showModal()}>⚙</button>{@render updatesButton()}{@render logsButton()}</div></div>
      {#if status}<p role="status">{status}</p>{/if}
      {#if welcomeError}<p role="alert">{welcomeError}</p>{/if}
      {#if launching}<p role="status">Opening project…</p>{/if}
      <div class="welcome-projects">
        <section class="welcome-project-panel">
          <h2>Local projects</h2>
          <div class="welcome-actions">
            <button disabled={launching || busy} onclick={newProject}>New local project…</button>
            <button disabled={launching || busy} onclick={openFile}>Open local project…</button>
          </div>
          <h3>Recent local projects</h3>
          {#each recent as entry (entry.path)}
            {@const state=availability[entry.path]??'Checking…'}
            <div class="recent-project"><button disabled={launching || busy || state!=='Available'} onclick={()=>openRecent(entry.path)}><strong>{entry.name}</strong><small>{entry.path}</small></button>
              <span class:available={state==='Available'}><b aria-hidden="true">{state==='Available'?'✓':state==='Checking…'?'…':'⚠'}</b> {state}</span>
              {#if state==='Missing' || state==='Unavailable'}<button title="Remove from recent projects" onclick={()=>forgetProject(entry.path)}>Remove</button>{/if}
            </div>
          {:else}<p>No recent local projects.</p>{/each}
        </section>
        {#if isTauri()}
          <section class="welcome-project-panel">
            <h2>Online projects</h2>
            <div class="welcome-actions">
              <button disabled={launching || busy || restoringAccount} onclick={()=>showOnline('create')}>New online project…</button>
              <button disabled={launching || busy || restoringAccount} onclick={()=>showOnline('open')}>Open online project…</button>
            </div>
            {#if restoringAccount}{@render githubRestoreProgress()}
            {:else if account}{@render discoverableProjects()}
            {:else if restoreSkipped}
              <p role="status">GitHub was skipped for now. Local projects are ready.</p>
              <button class="github-reconnect" onclick={()=>restoreGitHubAccount()}>Reconnect to GitHub</button>
            {:else}<p>Sign in with GitHub to use online projects.</p>{/if}
          </section>
        {/if}
      </div>
    </main>
  {:else}
  <header>
    <nav><details class="file-menu" bind:this={fileMenu} ontoggle={event=>{if(event.currentTarget.open && githubMenu)githubMenu.open=false;}}>
      <summary>File</summary><div class="file-list">
        <button disabled={busy} onclick={()=>{fileMenu.open=false;newProject();}}>New Local Project…</button>
        {#if isTauri()}<button disabled={busy || restoringAccount} onclick={()=>{fileMenu.open=false;showOnline('create');}}>New Online Project…</button>{/if}
        <hr/>
        <button disabled={busy} onclick={()=>{fileMenu.open=false;void openFile();}}>Open Local Project…</button>
        {#if isTauri()}<button disabled={busy || restoringAccount} onclick={()=>{fileMenu.open=false;showOnline('open');}}>Open Online Project…</button>{/if}
        <hr/>
        <button disabled={!canSaveProject} onclick={()=>{fileMenu.open=false;void saveFile();}}>Save</button>
        {#if onlineProject}
          <button disabled={busy} onclick={()=>{fileMenu.open=false;void saveAsLocal();}}>Save as Local…</button>
        {:else if isTauri()}
          <button disabled={busy || restoringAccount} onclick={()=>{fileMenu.open=false;showOnline('save');}}>Save as Online…</button>
        {/if}
        <hr/>
        <button disabled={busy} onclick={()=>{fileMenu.open=false;exporter.show();}}>Export as PDF…</button>
        <hr/>
        <button disabled={busy} onclick={()=>{fileMenu.open=false;void closeProject();}}>Close Project</button>
      </div></details>
      {#if onlineProject}
        <details class="file-menu" bind:this={githubMenu} ontoggle={event=>{if(event.currentTarget.open && fileMenu)fileMenu.open=false;}}>
          <summary>GitHub</summary>
          <div class="file-list">
            <button disabled={busy || restoringAccount} onclick={()=>{if(githubMenu)githubMenu.open=false;refreshOnlineProject();}}>Refresh Project</button>
            <button onclick={()=>{
              if(githubMenu)githubMenu.open=false;
              const repo=onlineProject?.repo;
              if(repo)void invoke('open_web',{url:'https://github.com/'+repo.split('/').map(encodeURIComponent).join('/')})
                .catch(error=>{status='Could not open the GitHub repository. '+String(error);});
            }}>Open Repository</button>
          </div>
        </details>
      {/if}
      <button disabled={!canSaveProject} title={onlineProject && !canSaveProject && !busy?'GitHub write access is required':dirty?'Save unsaved changes':'Save project'} onclick={saveFile}>{dirty?'Save *':'Save'}</button>
    </nav>
    <div class="app-actions">
      {@render gitHubAccountButton()}
      <button aria-label="Keyboard shortcuts" title="Keyboard shortcuts" onclick={()=>ask('Keyboard shortcuts')}><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="2" y="5" width="20" height="14" rx="3"/><path d="M5 9h2m2 0h2m2 0h2m2 0h2M5 12h2m2 0h2m2 0h2m2 0h2M8 15h8"/></svg></button>
      <button class="settings-button" aria-label="Settings" title="Settings" onclick={()=>settings.showModal()}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m10 3-.6 2.3-2 .9-2.1-.8-2 3.4L5 10.4v2.8l-1.7 1.6 2 3.5 2.2-.8 2 .9.5 2.2h4l.6-2.2 2-.9 2.1.8 2-3.5-1.7-1.6v-2.8l1.7-1.6-2-3.4-2.2.8-2-.9L14 3z"/><circle cx="12" cy="12" r="3"/></svg></button>
      {@render updatesButton()}
      {@render logsButton()}
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
      {@const unreadCount=unreadComments(p)+p.items.reduce((count,o)=>count+unreadComments(o),0)}
      <div class="tree-row" class:ancestor={!matches(pageTags.get(p.id)??[],filters)}>
      <button class="page" class:chosen={p.id===current} style:padding-left={9+depth*17+'px'} data-drop={treeDrop.startsWith(p.id+' ')?treeDrop.split(' ').at(-1):''}
        onpointerdown={e=>pageDown(e,p)} draggable={editing && !isTauri()} ondragstart={e=>{draggingPage=p.id;e.dataTransfer?.setData('studio/page',p.id);}}
        ondragend={()=>{treeDrop='';draggingPage='';}} ondragover={e=>{if(editing && draggingPage){e.preventDefault();treeDrop=p.id+' '+placement(e);}}}
        ondrop={e=>movePage(e,p)} oncontextmenu={e=>{focusArea='tree';context(e,p);}} data-page={p.id} onfocus={()=>focusArea='tree'} onclick={()=>{if(suppressPageClick){suppressPageClick=false;return;}focusArea='tree';void navigate(p);}}><span>▧</span><span class="page-name">{p.name}</span></button>
      <span class="page-tags" title={(pageTags.get(p.id)??[]).map(d=>names[d]).join(', ')}>{#each pageTags.get(p.id)??[] as d}<i style:background={colors[d]}></i>{/each}</span>
      {#if unreadCount}<button class="unread-comments" title="Read unread comments" aria-label={'Unread comments on '+p.name} onclick={()=>showComments(unreadComments(p)?p:p.items.find(o=>unreadComments(o))!)}>● {unreadCount}</button>{/if}
      {#if editorsByPage.get(p.id)?.length}
        {@const editors=editorsByPage.get(p.id)!}
        <span class="page-editors" title={editingNotice(editors)} role="img" aria-label={editingNotice(editors)}>
          {#each editors.slice(0,3) as editor (editor.user_id)}
            <span class="page-editor-avatar">
              {editor.login.slice(0,1).toUpperCase()}
              <img src={'https://avatars.githubusercontent.com/u/'+editor.user_id+'?s=40'} alt="" width="18" height="18" onerror={event=>{event.currentTarget.setAttribute('hidden','');}}/>
            </span>
          {/each}
          {#if editors.length>3}<small>+{editors.length-3}</small>{/if}
        </span>
      {/if}
      <button class="page-child" disabled={!editing} title={'Add child page to '+p.name} aria-label={'Add child page to '+p.name} onclick={()=>newPage(p)}>+</button></div>
    {/each}
    {#if draggingPage}<button class="root-drop" ondragover={e=>e.preventDefault()} ondrop={e=>movePage(e)}>Move to project root</button>{/if}
    </div>
    <details class="page-trash"><summary><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M3 6h18M9 6V3h6v3M6 6l1 15h10l1-15M10 9v9m4-9v9"/></svg>Trash ({project.trash?.length??0})</summary>
      <div>{#each project.trash??[] as p}<button disabled={!editing} onclick={()=>restorePage(p)}>Restore {p.name}</button>{:else}<small>Trash is empty.</small>{/each}</div>
      {#if project.trash?.length}
        <button class="empty-trash" disabled={!editing} onclick={()=>ask('Empty trash')}>Empty trash…</button>
      {/if}
    </details>
  </DockPanel>
  <main><div class="boardhead"><div><div class="canvas-label">
    <span class="section">Canvas</span>
    <span class="editing-presence" aria-live="polite">{#if editorsByPage.get(current)?.length}{editingNotice(editorsByPage.get(current)!)}{/if}</span>
    {#if !editing}<span class="read-caption">Editing is locked</span><button class="enable-edit" onclick={toggleEdit}>Enable editing <kbd>E</kbd></button>{/if}
  </div><h1>{board.name}</h1></div></div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="canvas-area" data-toolbar-side={canvasToolbarSide} bind:this={canvasArea}>
    <div class="viewport" class:drawing-mode={drawingMode} bind:this={viewport} bind:clientWidth={width} bind:clientHeight={height} oncontextmenu={e=>canvasMenu(e)} onwheel={wheel} onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={()=>{wire=undefined;stroke=undefined;drag=undefined;gesture=undefined;liveBox=undefined;marquee=undefined;commit();}} ondragover={e=>e.preventDefault()} ondrop={drop}>
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
          {@const frameColor=o.kind==='Frame'?frameColorFor(o):undefined}
          {@const media=o.attachment && mediaKind(o.attachment)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div data-id={o.id} class="object" class:drawing-object={o.kind==='Drawing'} class:frame={o.kind==='Frame'} class:selected={ids.includes(o.id)} oncontextmenu={e=>{focusArea='canvas';context(e,undefined,o);}} style:left={o.x+'px'} style:top={o.y+'px'} style:width={o.w+'px'} style:height={o.h+'px'} style:transform={'rotate('+(o.rotation??0)+'deg)'} style:--dept={colors[itemTags(o)[0]]??'var(--line)'} style:--frame-color={frameColor} style:--frame-text={frameColor && frameHeaderText(frameColor)} ondblclick={e=>activate(o,e)}>
            {#if o.kind==='Image' && o.attachment}<AttachmentImage project={path} attachment={o.attachment} name={o.name}/>{:else if o.kind==='Image'}<img draggable="false" src={o.source} alt={o.name} decoding="async"/>{/if}
            {#if o.drawing}<DrawingStroke drawing={o.drawing}/>{/if}
            {#if o.kind!=='Drawing' && (o.kind!=='Image' || o.showTitle!==false)}<div class="caption">{#if editing && titleId===o.id}<input class="note-title" aria-label="Note title" bind:value={titleDraft}
              onblur={()=>finishTitle(o,true)} onkeydown={e=>{if(e.key==='Enter' || e.key==='Escape'){e.preventDefault();e.stopPropagation();finishTitle(o,e.key==='Enter');}}}/>
              {:else}{#if o.kind==='Frame'}▣　{:else if o.kind==='Page link'}↗　{/if}{o.name}{/if}{#if editing && active?.id===o.id && frameOnly===o.id}<small>Frame only</small>{/if}</div>{/if}
            {#if o.kind==='Attachment' && o.attachment && media}
              <MediaPlayer kind={media} project={path} attachment={o.attachment} name={o.name} repo={onlineProject?.repo}/>
            {:else if o.kind==='Attachment'}<small>{o.attachment?.folder?'Folder':'File'} · {o.attachment?.local?'Project copy':'Linked original'}</small>{/if}
            {#if o.url}
              {@const video=videoId(o.url)}
              {#if video && playing===o.id}<iframe title={o.name} src={'https://www.youtube-nocookie.com/embed/'+video+'?autoplay=1'} referrerpolicy="strict-origin-when-cross-origin" allow="autoplay; encrypted-media; picture-in-picture" allowfullscreen></iframe>
              {:else if video}<button onclick={()=>playing=o.id}>▶ Play video</button>{/if}
              <small>↗ {webAddress(o.url)?.hostname}</small>{#if o.body}<div class="body">{o.body}</div>{/if}
            {/if}
            {#if o.kind==='Note'}
              {#if inline===o.id}
                <div class="body rich-editor" contenteditable="plaintext-only" role="textbox" aria-multiline="true" aria-label="Edit note" tabindex="0"
                  use:richNote={{parts:textParts(o),select:(start,end)=>{range={id:o.id,start,end};},edit:(value,start)=>{editBody(o,value,start);change();}}}
                  use:fitText={{enabled:o.autoFit,size:o.size,text:o.body}} onblur={()=>inline=''}></div>
              {:else}
                <div class="body" use:fitText={{enabled:o.autoFit,size:o.size,text:o.body}}>{#each textParts(o) as part}<span style={part.css}>{#if part.to}<button class="inline-link" title={labelFor(part.to)} onclick={()=>follow(part.to!)}>{part.text}</button>{:else}{part.text}{/if}</span>{/each}</div>
              {/if}
            {/if}
            {#if o.threads?.length}<button class="comment-pin" title="Open conversations" aria-label={'Open conversations on '+o.name} onclick={()=>showComments(o)}>
              <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M4 3.5h12a1.5 1.5 0 0 1 1.5 1.5v8a1.5 1.5 0 0 1-1.5 1.5H8L3 18v-4a2 2 0 0 1-1-1.7V5a1.5 1.5 0 0 1 2-1.5Z"/></svg></button>{/if}
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
    <footer class="canvas-toolbar" aria-label="Canvas toolbar">
    <button class="panel-grip canvas-toolbar-grip" aria-label="Move canvas toolbar"
      title="Drag the toolbar to a canvas edge, or use the arrow keys"
      onpointerdown={beginCanvasDock} onpointermove={moveCanvasDock} onpointerup={finishCanvasDock}
      onpointercancel={cancelCanvasDock} onlostpointercapture={cancelCanvasDock} onkeydown={canvasDockKey}>
      <span class="grip-dots" aria-hidden="true"></span>
    </button>
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
    <div class="spacer"></div>
    <button aria-label="Zoom out" onclick={()=>scale(.8)}>−</button>
    <span>{Math.round(zoom*100)}%</span>
    <button aria-label="Zoom in" onclick={()=>scale(1.25)}>+</button>
    <button onclick={()=>fit()}>Fit <kbd>F</kbd></button>
    </footer>
    {#if dockDrag?.moving}
      {#each canvasEdges as edge}
        <div class="canvas-dock-target" class:over={dockDrag.target===edge} data-edge={edge} aria-hidden="true"></div>
      {/each}
    {/if}
    </div>
  </main>
  {#snippet inspectorConversations(subject:Item|Page)}
      <section class="inspector-section inspector-threads">
        <h3>Conversations</h3>
        <button class="new-comment-thread" aria-label="New comment thread" disabled={!editing || !editor} onclick={()=>showComments(subject,null)}>New</button>
        {#each subject.threads??[] as t (t.id)}
            {@const last=t.messages[t.messages.length-1]}
            <button onclick={()=>showComments(subject,t)}>
              <span class="thread-preview-title"><strong>{t.messages[0].text}</strong>{#if t.archived}<span class="archived-label">Archived</span>{/if}</span><small>{unread(t)?'Unread · ':''}{last.author} · {last.text}</small>
            </button>
        {/each}
      </section>
  {/snippet}
  <DockPanel name="Inspector" panelClass="inspector" bind:side={inspectorSide} bind:open={inspectorOpen}>
    {#if selectedConnection && arrows.some(a=>a.id===selectedConnection)}<h2>Connection</h2><button disabled={!editing} onclick={deleteConnection}>Delete connection</button>
    {:else if active}<h2>{active.kind}</h2>
      <section class="inspector-section"><h3>Properties</h3><fieldset disabled={!editing}>
      <label>Title<input bind:value={active.name} oninput={change}/></label>
      {#if active.kind==='Image'}{@render imageTitle(active)}{/if}
      {#if active.kind==='Frame'}{@render frameBackgroundPicker(active)}{/if}
      {@render departmentPicker()}
      {#if active.kind==='Note'}<label>Text<textarea value={active.body}
        onbeforeinput={e=>beforeStart=e.currentTarget.selectionStart}
        onselect={e=>active && remember(active,e.currentTarget)} oninput={e=>active && updateText(active,e.currentTarget)}></textarea></label>
        <div class="note-format" role="group" aria-label="Selected text formatting">
          {#each noteStyles as style}<button title={style} aria-label={style} aria-pressed={!!range && active.styles?.some(s=>s.style===style && s.start<=range!.start && s.end>=range!.end) || false}
            disabled={range?.id!==active.id || range.start===range.end} onmousedown={e=>e.preventDefault()} onclick={()=>formatNote(style)}>{#if style==='bold'}<b>B</b>{:else if style==='italic'}<i>I</i>{:else}<u>U</u>{/if}</button>{/each}
          <button disabled={range?.id!==active.id || range.start===range.end} onmousedown={e=>e.preventDefault()} onclick={()=>chooseLink(range)}>Link…</button></div>
        {#each active.textLinks ?? [] as link}<button class="link-entry" title={labelFor(link.to)} onclick={()=>chooseLink(link)}>↗ {active.body.slice(link.start,link.end)}</button>{/each}
        <label class="check"><input type="checkbox" bind:checked={active.autoFit} onchange={change}/> Auto-fit text to box</label><label>Font size<input type="number" min="8" max="96" value={active.size} onchange={e=>{if(active){active.size=Math.max(8,Math.min(96,Number(e.currentTarget.value)||16));change();}}}/></label>{/if}
      <div class="dimensions">{#each ['w','h'] as key}<label>{key==='w'?'Width':'Height'}<input type="number" min="40" value={active[key as 'w'|'h']} onchange={e=>{if(active){active[key as 'w'|'h']=Math.max(40,Number(e.currentTarget.value)||40);change();}}}/></label>{/each}</div>
      <button onclick={()=>chooseLink()}>{objectLink(active)?'Edit object link':'Link object'}</button>
    </fieldset></section>
      {@render inspectorConversations(active)}
      {#if active.attachment}<section class="inspector-section inspector-file"><h3>File</h3><p>{active.attachment.shared?'Shared file':active.attachment.local?'Project copy':'Linked original'}<br>{active.attachment.path}</p><div class="attachment-actions">
        <button disabled={!editing || copyingItem===active.id} onclick={()=>relink(active!)}>Relink…</button>
        <button onclick={()=>revealAttachment(active!)}>Open folder</button>
        {#if !active.attachment.local}<button disabled={!editing || !!copyingItem} onclick={()=>copyToProject(active!)}>{copyingItem===active.id?'Copying…':'Copy to project'}</button>{/if}
      </div></section>{/if}
    {:else if picked.length>1}<h2>{picked.length} objects</h2><p>Move the selection or drag a handle to transform it.</p>{:else}<h2>Page details</h2><label>Page name<input disabled={!editing} bind:value={board.name} oninput={change}/></label><div class="stat">Objects <b>{board.items.length}</b></div><p>Select an object to inspect its properties.</p>{@render inspectorConversations(board)}{/if}
  </DockPanel>
  </div>
  {#if menu}
    <div class="app-menu" style:left={menu.x+'px'} style:top={Math.max(0,menu.y)+'px'} style:max-height={'calc(100dvh - '+(Math.max(0,menu.y)+8)+'px)'}>
      {#if menu.page}
        {@const target=menu.page}
        {#if target.id!==current}<button onclick={()=>{void navigate(target);menu=undefined;}}>Open page</button>{/if}
        <button disabled={!editing} onclick={()=>{newPage(target);menu=undefined;}}>Add child page…</button>
        <details class="object-departments"><summary>Tags</summary><fieldset disabled={!editing}>
        {#if target.parentId}<label><input type="checkbox" checked={target.departments===undefined} onchange={e=>{target.departments=e.currentTarget.checked?undefined:[...pageTags.get(target.id)??[]];change();}}/> Inherit from parent</label>{/if}
        {#each catalog as d}<label><input type="checkbox" checked={pageTags.get(target.id)?.includes(d.id)??false} onchange={e=>togglePageTag(target,d.id,e.currentTarget.checked)}/><i class="department-dot" style:background={d.color}></i>{d.name}</label>{/each}
        </fieldset></details>
        <button onclick={()=>copyPage(target)}>Copy page</button>
        <button disabled={!editing || !pageClipboard} onclick={()=>void pastePage()}>Paste page</button>
        <button onclick={()=>showComments(target)}>Conversations…</button>
        <button onclick={()=>showPageHistory(target)}>View page history…</button>
        <button onclick={()=>{exporter.show(target);menu=undefined;}}>Export page as PDF…</button>
        <hr/>
        <button disabled={!editing} onclick={()=>requestDelete(target)}>Delete page…</button>
      {:else if menu.canvas}
        {#key menu}
          <details class="canvas-new-menu">
            <summary>New</summary>
            <div>
              {#each ['Note','Image','Frame','Drawing','Page link','Web link',...(isTauri()?(onlineProject?['File']:['File','Folder']):[])] as kind}
                <button disabled={!editing} onclick={()=>insert(kind)}>{kind}</button>
              {/each}
            </div>
          </details>
        {/key}
        <hr/>
        <button disabled={!editing || (!canvasClipboard && !pageClipboard)} onclick={()=>pageClipboard?void pastePage():void pasteObjects(menu?.point)}>Paste</button>
        <hr/>
        <button onclick={()=>{menu=undefined;exporter.show(board);}}>Export page as PDF…</button>
      {:else}
        {#if menu.item}<button onclick={()=>showComments(menu!.item!)}>Conversations…</button>{/if}
        <button disabled={!picked.length} onclick={()=>copyObjects(false)}>Copy</button>
        <button disabled={!editing || !picked.length} onclick={()=>copyObjects(true)}>Cut</button>
        <button disabled={!editing || (!canvasClipboard && !pageClipboard)} onclick={()=>pageClipboard?void pastePage():void pasteObjects()}>Paste</button>
        <hr/>
        <button onclick={()=>{fit();menu=undefined;}}>Focus selection</button>
        {@render departmentPicker()}
        {#if menu.item?.kind==='Frame'}{@render frameBackgroundPicker(menu.item)}{/if}
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
        <button disabled={!editing} onclick={()=>removeObjects()}>Delete selection</button>
      {/if}
    </div>
  {/if}
  <div class="status" role="status"><i></i>{#if status==='PDF exported' && exportedPdf}
    <button class="export-result" title="Open exported PDF" onclick={()=>openExport(false)}>PDF exported ↗</button>
    <button class="export-result" onclick={()=>openExport(true)}>Open folder</button>
    {:else}{status || (onlineProject?'GitHub · '+onlineProject.repo:'Local project')}{/if} <span>{board.items.length} objects</span></div>
  {/if}
  <dialog bind:this={fileDialog} oncancel={e=>{if(copying)e.preventDefault();}}>
    <h2>Add to canvas</h2><p>{incoming.length===1?incoming[0]:incoming.length+' items selected'}</p>
    <p>Copy files and folders beside the project, or keep a link to their original location.</p>
    {#if importError}<p role="alert">{importError}</p>{/if}
    <div class="dialog-actions">
      <button disabled={copying} onclick={()=>fileDialog.close()}>Cancel</button>
      <button disabled={copying} onclick={()=>attach(false)}>Link original</button>
      <button disabled={copying} onclick={()=>attach(true)}>{copying?'Adding…':'Copy to project'}</button>
    </div>
  </dialog>
  <dialog bind:this={unsavedDialog} aria-labelledby="unsaved-dialog-title" oncancel={event=>{event.preventDefault();finishUnsaved('cancel');}}>
    <h2 id="unsaved-dialog-title">Save changes before {unsavedLabels[unsavedAction].action}?</h2>
    <p>Save your changes to “{project.name}”, or discard them to continue.</p>
    <div class="dialog-actions unsaved-actions">
      <button onclick={()=>finishUnsaved('cancel')}>Cancel</button>
      <button onclick={()=>finishUnsaved('discard')}>Don’t Save</button>
      <button onclick={()=>finishUnsaved('save')}>{unsavedLabels[unsavedAction].save}</button>
    </div>
  </dialog>
  <dialog bind:this={openingModeDialog} aria-labelledby="opening-mode-title" oncancel={event=>{event.preventDefault();finishOpeningModePrompt(false);}}>
    <h2 id="opening-mode-title">How should projects open?</h2>
    <p>You can switch modes at any time with <kbd>E</kbd>.</p>
    <form onsubmit={event=>{event.preventDefault();finishOpeningModePrompt(true);}}>
      <label>Open projects in
        <select bind:value={draftOpeningMode}>
          <option value="read">Read mode</option>
          <option value="edit">Edit mode</option>
        </select>
      </label>
      <div class="dialog-actions">
        <button type="button" onclick={()=>finishOpeningModePrompt(false)}>Close</button>
        <button type="submit">Apply</button>
      </div>
    </form>
  </dialog>
  <dialog class="settings" bind:this={settings}>
    {@render closePanel(()=>settings.close())}<h2>Settings</h2>
    {#if isTauri()}<label>On startup<select bind:value={startup} onchange={()=>localStorage.setItem('studio-startup',startup)}><option value="welcome">Show welcome screen</option><option value="last">Reopen last project</option></select></label>{/if}
    <label>Open projects in<select value={projectOpeningMode} onchange={event=>setProjectOpeningMode(event.currentTarget.value==='edit'?'edit':'read')}>
      <option value="read">Read mode</option><option value="edit">Edit mode</option>
    </select></label>
    <h3>Appearance</h3>
    <label>Theme<select aria-label="Appearance" bind:value={theme} onchange={()=>localStorage.setItem('studio-theme',theme)}><option>System</option><option>Light</option><option>Dark</option></select></label>
    <label>Online editor presence refresh<select bind:value={presenceSeconds} onchange={e=>localStorage.setItem('tenkivo-presence-seconds',e.currentTarget.value)}>
      {#each [15,30,60] as seconds}<option value={seconds}>{seconds} seconds</option>{/each}
    </select></label>
    <label>Navigation history limit<input type="number" min="1" max="100" value={historyLimit} onchange={e=>{setLimit(e.currentTarget.valueAsNumber);e.currentTarget.value=String(historyLimit);}}/></label>
    <h3>Canvas Snapping</h3><p>Hold Ctrl while dragging. Set a step to zero to disable snapping for that transform.</p>
    <div class="snap-fields">
      {#each snapKeys as key}
        <label>{key==='rotate'?'Rotation (degrees)':key==='resize'?'Resize (canvas units)':'Move (canvas units)'}
          <input type="number" min="0" max={key==='rotate'?360:10000} step="any" value={snapping[key]} disabled={!welcome && !editing}
            onchange={e=>setSnap(key,e.currentTarget)}/>
        </label>
      {/each}
    </div>
    {#if welcome}<p>These defaults apply to projects without custom snapping values.</p>{:else if !editing}<p>Enable editing to change this project’s canvas snapping.</p>{/if}
    {#if isTauri()}<p class="app-version">Tenkivo · Version {appVersion}</p>{/if}
  </dialog>
  <dialog bind:this={updateDialog} oncancel={e=>{if(installing)e.preventDefault();}}>
    {#if release}<h2>Update available</h2><p>Current version {release.current} · New version {release.version}</p>
      <p style="white-space:pre-wrap;max-height:40vh;overflow:auto">{release.notes}</p>
      {#if updateError}<p role="alert">{updateError}</p>{/if}
      {#if !release.portable}<p>Automatic replacement is available in the Windows portable build.</p>{/if}
      <div class="dialog-actions">
        <button disabled={installing} onclick={()=>updateDialog.close()}>Later</button>
        {#if release.portable}<button disabled={installing} onclick={applyUpdate}>{installing?'Updating…':'Update and restart'}</button>{/if}
      </div>
    {/if}
  </dialog>
  <dialog bind:this={dialog} class:comments-dialog={dialogType==='Conversations'} class:shortcuts-dialog={dialogType==='Keyboard shortcuts'} oncancel={e=>{if(dialogType==='Online project' && onlineWorking)e.preventDefault();}}>
    {#if ['Online project','GitHub account'].includes(dialogType)}
      {@render closePanel(()=>dialog.close(),busy || onlineWorking)}
    {:else if ['Project settings','Keyboard shortcuts','Editor logs','Page history','Conversations'].includes(dialogType)}
      {@render closePanel(()=>dialog.close())}
    {/if}
    <h2>{dialogType==='Online project'?{create:'New Online Project',open:'Open Online Project',save:'Save as Online'}[onlineMode]:(dialogType==='Project settings' || dialogType==='New project') && section?section:dialogType==='New project'?'New Local Project':dialogType}</h2>{#if dialogType==='Conversations' && commentItem}
      {@const thread=commentItem.threads?.find(t=>t.id===threadId)}
      {@const threads=commentItem.threads??[]}
      <section class="comments-panel">
        <strong>{commentItem.name}</strong>
        {#if threads.length>1 || (!thread && threads.length)}
          <label>Thread<select value={threadId} onchange={e=>{const t=commentItem?.threads?.find(t=>t.id===e.currentTarget.value);if(t)viewThread(t);}}>
            {#if !thread}<option value="">New thread</option>{/if}
            {#each threads as t}<option value={t.id}>{t.archived?'Archived · ':''}{t.messages[0].text.slice(0,48)}{unread(t)?' · Unread':''}</option>{/each}
          </select></label>
        {/if}
        {#if thread}
          <div class="comment-list">{#each groupComments(thread.messages) as group (group[0].id)}
            {@const m=group[0]}
            {@const githubId=/^github:(\d+)$/.exec(m.user)?.[1]}
            <article>
            <strong class="comment-author">
              {#if githubId}<span class="page-editor-avatar" aria-hidden="true">{m.author.slice(0,1).toUpperCase()}<img src={'https://avatars.githubusercontent.com/u/'+githubId+'?s=40'} alt="" width="18" height="18" onerror={e=>e.currentTarget.setAttribute('hidden','')}/></span>
              {:else}<span aria-hidden="true">{m.icon??project.users?.find(user=>user.id===m.user)?.icon??'👤'}</span>{/if}
              {m.author}
            </strong> <small>{new Date(m.time).toLocaleString(undefined,{dateStyle:'short',timeStyle:'short'})}</small>{#each group as message (message.id)}<p>{message.text}</p>{/each}</article>{/each}</div>
        {/if}
        {#if editing && editor && !thread?.archived}
          <textarea aria-label="Comment" placeholder={thread?'Write a reply…':'Start a thread…'} bind:value={draftComment} rows="2" onkeydown={e=>{
            if(e.key==='Enter' && (e.ctrlKey || e.metaKey) && !e.isComposing){e.preventDefault();e.stopPropagation();if(!e.repeat)postComment();}
          }}></textarea>
          <button class="send-comment" title="Send (Ctrl / Cmd + Enter)" disabled={!draftComment.trim()} onclick={postComment}>Send</button>
        {:else if !editing}<small>Open edit mode to add comments.</small>{/if}
        {#if thread && (editing || thread.archived)}
          <footer class="thread-actions">
            {#if thread.archived}<button disabled={!editing} onclick={()=>{
              if(!editing)return;
              thread.archived=false;
              change();
            }}>Reopen</button>
            {:else}<button onclick={()=>{thread.archived=true;change();}}>Archive</button>{/if}
            {#if editing}<button onclick={()=>{threadId='';draftComment='';}}>New thread</button>{/if}
          </footer>
        {/if}
      </section>
    {:else if dialogType==='Internal link' && active}
    <form onsubmit={e=>{e.preventDefault();if(linkType==='web'){const url=webAddress(webTarget);if(url)storeLink({pageId:'',url:url.href});}else storeLink(targetItem?{pageId:targetPage,itemId:targetItem}:{pageId:targetPage});}}>
      <p>{draftRange ? '“'+active.body.slice(draftRange.start,draftRange.end)+'”' : active.name}</p>
      {#if draftRange}<label>Link to<select bind:value={linkType}><option value="project">Page or object</option><option value="web">Website</option></select></label>{/if}
      {#if linkType==='web'}<label>Web address<input type="url" bind:value={webTarget} placeholder="https://"/></label>{:else}
      <label>Page<select bind:value={targetPage} onchange={()=>targetItem=''}>{#each project.pages as p}<option value={p.id}>{p.name}</option>{/each}</select></label>
      <label>Destination<select bind:value={targetItem}><option value="">Entire page</option>{#each choices as o}<option value={o.id}>{o.name} · {o.kind}</option>{/each}</select></label>
      {/if}
      <div class="dialog-actions">
        {#if draftRange ? active.textLinks?.some(l=>l.start<draftRange!.end && l.end>draftRange!.start) : objectLink(active)}
          <button type="button" onclick={()=>storeLink()}>Remove link</button>
        {/if}
        <button type="button" onclick={()=>dialog.close()}>Cancel</button>
        <button disabled={linkType==='web'?!webAddress(webTarget):!project.pages.some(p=>p.id===targetPage) || (!!targetItem && !choices.some(o=>o.id===targetItem))}>Apply link</button>
      </div>
    </form>
  {:else if dialogType==='Online project' || dialogType==='GitHub account'}
    {@const managingAccount=dialogType==='GitHub account'}
    {#if account}
      <p>Signed in as <strong>@{account.login}</strong>.</p>
      {#if managingAccount}
        <p>Tenkivo uses this account as your editing identity.</p>
        <div class="dialog-actions"><button disabled={busy} onclick={signOut}>Sign out</button></div>
      {:else}
      {#if onlineMode==='open'}
        {@render discoverableProjects()}
        {#if !discovering && (discoveryError || !discoveredProjects.length)}
          <details class="manual-online-open">
            <summary>Open by repository name…</summary>
            <form onsubmit={e=>{e.preventDefault();void accessOnline('open');}}>
              <label>Repository name<input required placeholder="owner/repository" bind:value={onlineRepo} disabled={busy}/></label>
              <div class="dialog-actions"><button disabled={busy || !onlineRepo.trim()}>Open repository</button></div>
            </form>
          </details>
        {/if}
      {:else}
        {#if onlineMode==='save'}<p>Save this project to a private GitHub repository and continue editing the online copy.</p>{/if}
        <form onsubmit={e=>{e.preventDefault();void accessOnline(onlineMode==='create'?'create':'save');}}>
          <label>Private repository<input required placeholder="owner/repository" bind:value={onlineRepo} disabled={busy}/></label>
          {#if onlineMode==='create'}
            <label>New project name<input required bind:value={onlineNewName} disabled={busy}/></label>
            <details><summary>Tags</summary><fieldset disabled={busy}>{@render tagEditor()}</fieldset></details>
            {#if invalidDraftTags()}<p>Enter a name for each tag.</p>{/if}
          {/if}
          <div class="dialog-actions"><button disabled={busy || !onlineRepo.trim() || (onlineMode==='create' && (!onlineNewName.trim() || invalidDraftTags()))}>{onlineMode==='create'?'Create online project':'Save as Online'}</button></div>
        </form>
      {/if}
      {/if}
    {:else if deviceCode}
      <p>Enter this code on GitHub to connect your account.</p>
      <p><strong>{deviceCode}</strong></p>
      <div class="dialog-actions"><button type="button" onclick={()=>void invoke('open_web',{url:'https://github.com/login/device'}).catch(error=>onlineError=String(error))}>Open GitHub sign-in ↗</button></div>
    {:else}
      <form onsubmit={e=>{e.preventDefault();void requestSignIn();}}>
        {#if !configuredClientId}
          <p>Set up a GitHub App with Device Flow, then copy its public Client ID here. GitHub will ask you to sign in in your browser.</p>
          <label>Public Client ID from GitHub App settings
            <input required bind:value={clientId} disabled={onlineWorking} placeholder="GitHub App Client ID" autocomplete="off" spellcheck="false"/>
          </label>
        {/if}
        <div class="dialog-actions">
          {#if !configuredClientId}<button type="button" onclick={()=>void invoke('open_web',{url:'https://github.com/settings/apps'}).catch(error=>onlineError=String(error))}>Open GitHub App settings ↗</button>{/if}
          <button disabled={onlineWorking || !(configuredClientId || clientId.trim())}>Sign in with GitHub</button>
        </div>
      </form>
    {/if}
    {#if onlineError}<p role="alert">{onlineError}</p>{/if}
    {#if deviceCode}
      <div class="dialog-actions online-dialog-footer">
        <button onclick={signOut}>Cancel sign-in</button>
      </div>
    {/if}
  {:else if dialogType==='Online save conflict'}
    <p>Someone saved a newer version on GitHub. Your changes are still open. Save a local recovery copy before reloading, or discard them to open the latest version.</p>
    <div class="dialog-actions">
      <button onclick={()=>{dialog.close();void saveAsLocal(true);}}>Save a Local Copy…</button>
      <button onclick={()=>{dialog.close();void accessOnline('reload');}}>Discard Changes and Reload…</button>
      <button onclick={()=>dialog.close()}>Keep Editing</button>
    </div>
  {:else if dialogType==='GitHub users' && onlineProject}
    <GitHubProjectUsers repo={onlineProject.repo} onback={()=>{dialogType='Project settings';}}/>
  {:else if dialogType==='Project settings' || dialogType==='New project'}
    {@const creating=dialogType==='New project'}
    <form class="project-form" onsubmit={e=>{e.preventDefault();if(section)leaveSection(true);else if(creating)void named();else applyOptions();}}>
      {#if !section}<fieldset disabled={!creating && !editing}><label>Project name<input required bind:value={draftName}/></label>
      <label>Description<textarea bind:value={draftDescription}></textarea></label>
      </fieldset>
      <button class="add-project-entry" type="button" onclick={()=>openSection('Tags')}>Tags　›</button>
      {#if onlineProject}<button class="add-project-entry" type="button" disabled={!account} onclick={()=>{dialogType='GitHub users';}}>Users on GitHub　›</button>
      {:else}<button class="add-project-entry" type="button" onclick={()=>openSection('Users')}>Local profiles　›</button>{/if}
      {#if !creating && onlineProject}
        <section class="project-cache" aria-label="Project cache">
          <h3>Project cache</h3>
          <p>Downloaded files are kept on this device so they open faster.</p>
          <div class="row">
            <button type="button" disabled={projectCacheBusy} onclick={()=>manageProjectCache('open')}>Open cache folder</button>
            <button type="button" disabled={projectCacheBusy} onclick={()=>manageProjectCache('clear')}>Clear cache…</button>
          </div>
          {#if projectCacheStatus}<p role="status">{projectCacheStatus}</p>{/if}
          {#if projectCacheError}<p role="alert">{projectCacheError}</p>{/if}
        </section>
      {/if}
      {:else if section==='Tags'}<fieldset disabled={!creating && !editing}>
      {@render tagEditor()}
      </fieldset>{:else}<fieldset disabled={!creating && !editing}>
      {#each draftUsers as u (u.id)}
        <div class="department-editor"><select aria-label={'Icon for '+u.name} bind:value={u.icon}>{#each userIcons as icon}<option value={icon}>{icon}</option>{/each}</select>
        <input required aria-label="User name" placeholder="User name" bind:value={u.name}/>
        <button type="button" disabled={u.id===editorId && !creating} aria-label={'Remove user '+u.name} onclick={()=>draftUsers=draftUsers.filter(v=>v.id!==u.id)}>×</button></div>
      {/each}
      <button class="add-project-entry" type="button" onclick={()=>draftUsers.push({id:crypto.randomUUID(),name:'',icon:userIcons[0]})}>Add New User</button>
      {#if !validUsers && draftUsers.every(u=>u.name.trim())}<p role="alert">Choose a distinct name for each user.</p>{/if}
      </fieldset>{/if}
      {#if !creating && !editing}<p>Read mode is on. Enable editing to change project settings.</p>{/if}
      {#if creating && !section && creationError}<p role="alert">{creationError}</p>{/if}
      <div class="dialog-actions settings-footer">
        {#if creating && !section}<button type="button" onclick={()=>dialog.close()}>Cancel</button>{/if}
        <button disabled={busy || !validUsers || (!creating && !editing) || ((!section || !creating) && !draftName.trim()) || invalidDraftTags()}>{creating && !section?'Create':'Apply changes'}</button>
        {#if section}<button class="settings-back" type="button" onclick={()=>leaveSection(false)}>Back</button>{/if}
      </div>
    </form>
  {:else if dialogType==='Select editing user'}
    <form class="user-picker" onsubmit={e=>{e.preventDefault();enterEditing();}}>
      {#if registering}
        <label>User name<input required bind:value={userName}/></label>
        <label>Icon<select bind:value={userIcon}>{#each userIcons as icon}<option value={icon}>{icon}</option>{/each}</select></label>
        {#if userName.trim() && !validRegistration}<p role="alert">This user name is already in use.</p>{/if}
        {#if pickerUsers.length}<button type="button" onclick={()=>registering=false}>Choose existing user</button>{/if}
      {:else}
        <label>User<select bind:value={selectedUser} onkeydown={confirmSelectedUser}>{#each pickerUsers as u}<option value={u.id}>{u.icon} {u.id===onlineIdentity?.id?'@'+u.name+' (GitHub)':u.name}</option>{/each}</select></label>
        <button type="button" onclick={()=>registering=true}>Add New User</button>
      {/if}
      <div class="dialog-actions"><button type="button" onclick={()=>dialog.close()}>Cancel</button><button disabled={registering?!validRegistration:!selectedUser}>Start editing</button></div>
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
      <label>Page<select bind:value={linkPage}>{#each project.pages as p}<option value={p.id}>{p.name}</option>{/each}</select></label><div class="dialog-actions"><button type="button" onclick={()=>dialog.close()}>Cancel</button><button>Add link</button></div></form>
  {:else if dialogType==='Add web link'}
    <form onsubmit={e=>{e.preventDefault();if(insertUrl(linkUrl,insertAt?.x,insertAt?.y))dialog.close();}}><label>URL<input type="url" required bind:value={linkUrl}/></label><div class="dialog-actions"><button type="button" onclick={()=>dialog.close()}>Cancel</button><button disabled={!webAddress(linkUrl)}>Add link</button></div></form>
  {:else if dialogType==='Editor logs'}
    <div class="log-list">{#each [...diagnostics.entries].reverse() as entry}<article><time>{new Date(entry.time).toLocaleString()}</time><strong>{entry.level.toUpperCase()} · {entry.context}</strong><pre>{entry.message}</pre></article>{:else}<p>No activity recorded in this session.</p>{/each}</div>
  {:else if dialogType==='Empty trash'}<p>Remove {project.trash?.length} {project.trash?.length===1?'page':'pages'} from this project’s trash?</p>
    <div class="dialog-actions"><button onclick={()=>dialog.close()}>Cancel</button><button onclick={emptyTrash}>Empty trash</button></div>
  {:else if dialogType==='Delete page' && pendingPage}<p>Move “{pendingPage.name}” to the trash? Its child pages will move up one level.</p><div class="dialog-actions"><button onclick={()=>dialog.close()}>Cancel</button><button onclick={deletePage}>Move to trash</button></div>
  {:else if dialogType==='Keyboard shortcuts'}
    <KeyboardShortcuts/>
  {:else}<form onsubmit={e=>{e.preventDefault();void named();}}><label>Name<input required bind:value={name}/></label><div class="dialog-actions"><button type="button" onclick={()=>dialog.close()}>Cancel</button><button disabled={!name.trim()}>Create</button></div></form>{/if}</dialog>
  <input hidden bind:this={imageInput} type="file" accept="image/png,image/jpeg,image/gif,image/webp" multiple onchange={e=>{void importImages([...(e.currentTarget.files??[])],insertAt?.x,insertAt?.y);e.currentTarget.value='';}}/>
  <input hidden bind:this={fileInput} type="file" accept=".tkv,.gdstudio" onchange={async e=>{const f=e.currentTarget.files?.[0];e.currentTarget.value='';if(f)try{if(await confirmUnsaved('open'))await load(await f.text(),'');}catch(err){recordError('Open file',err);status=String(err);if(welcome)welcomeError=status;}}}/>
</div>
<ExportDialog bind:this={exporter} {project} {path} {filters} {pageTags} convert={convertExternal} ensureSaved={ensureExportSaved} notify={exportNotice}/>

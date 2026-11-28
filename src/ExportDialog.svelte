<script lang="ts">
  import {connectionPath,portPosition} from "./connections";
  import DrawingStroke from "./DrawingStroke.svelte";
  import './export.css';
  import {tick} from 'svelte';
  import {invoke} from './diagnostics.svelte';
  import {isTauri} from '@tauri-apps/api/core';
  import {save} from './diagnostics.svelte';
  import {frameHeaderText,defaultCatalog,textParts,webAddress,type Page,type Project,type Destination} from './model';
  import {layout,width,height,type Sheet} from './print-layout';
  let {project,path,filters,pageTags,convert,ensureSaved,notify}:{
    project:Project;path:string;filters:string[];pageTags:Map<string,string[]>;convert:(paths:string[])=>Promise<void>;ensureSaved:()=>Promise<boolean>;notify:(message:string,destination?:string)=>void
  }=$props();
  let dialog:HTMLDialogElement, printRoot:HTMLDivElement;
  let shown=$state(false),single=$state(''),mode=$state('pdf'),tile=$state(false),filtered=$state(false);
  let cursor=$state(0),error=$state(''),busy=$state(false),images=$state<Record<string,string>>({}),loading=$state(false),generation=0;
  const plan=$derived.by(()=>{try{return {sheets:shown && mode==='pdf'?layout(project,single,filtered?filters:[],tile):[],error:''};}
    catch(e){return {sheets:[] as Sheet[],error:String(e)};}});
  const sheets=$derived(plan.sheets);
  const links=$derived.by(()=>{
    const files=new Map<string,Set<string>>();
    for(const page of [...project.pages,...project.trash??[],...(project.pageHistory??[]).map(v=>v.page)])for(const o of page.items){
      if(!o.attachment || o.attachment.local)continue;
      const places=files.get(o.attachment.path)??new Set();places.add(page.name);files.set(o.attachment.path,places);
    }
    return [...files];
  });
  function sheetFor(d:Destination){if(d.url)return webAddress(d.url)?.href;const index=sheets.findIndex(s=>s.page?.id===d.pageId && (!d.itemId || s.items.some(o=>o.id===d.itemId)));return index<0?undefined:'#pdf-sheet-'+index;}
  export function show(p?:Page){
    single=p?.id??'';cursor=0;mode='pdf';tile=false;filtered=false;error='';shown=true;dialog.showModal();void loadImages();
  }
  async function loadImages(){
    const token=++generation;loading=true;images={};
    const sources:Record<string,string>={};
    for(const p of project.pages){
      if(single && p.id!==single)continue;
      for(const o of p.items){
        if(token!==generation)return;
        if(o.kind==='Image' && o.attachment){
          const key=JSON.stringify(o.attachment);
          if(!(key in sources))try{sources[key]=await invoke<string>('attachment_image',{project:path,attachment:o.attachment});}catch{sources[key]='';}
        }
      }
    }
    if(token===generation){images=sources;loading=false;}
  }
  function close(){generation++;shown=false;images={};dialog.close();}
  async function localize(paths:string[]){
    if(busy)return;busy=true;error='';
    try{await convert(paths);await loadImages();}
    catch(e){error=String(e);}finally{busy=false;}
  }
  async function run(){
    busy=true;error='';
    try{
      const label=(single?project.pages.find(p=>p.id===single)?.name:project.name)??'Project';
      const name=label.replace(/[<>:"/\\|?*\x00-\x1F]/g,'_').trim()||'Project';
      if(mode==='zip'){
        if(!path && !await ensureSaved())throw Error('Save the project first to create its package.');
        const destination=await save({defaultPath:name+'.zip',filters:[{name:'Project archive',extensions:['zip']}]});
        if(!destination)return;
        await invoke('export_zip',{project:path,destination,document:JSON.stringify(project)});
      }else{
        await tick();await document.fonts.ready;
        await Promise.all([...printRoot.querySelectorAll('img')].map(img=>img.decode().catch(()=>{})));
        if(isTauri() && navigator.userAgent.includes('Windows')){
          const destination=await save({defaultPath:name+'.pdf',filters:[{name:'PDF document',extensions:['pdf']}]});
          if(!destination)return;
          await invoke('export_pdf',{destination});
          notify('PDF exported',destination);close();return;
        }else if(isTauri()){await invoke('export_pdf',{destination:''});return;}
        else{window.print();return;}
      }
      notify('Project archive exported');close();
    }catch(e){error=String(e);}finally{busy=false;}
  }
</script>

{#snippet sheet(s:Sheet,n:number,print:boolean)}
  <section class="pdf-paper" id={print?'pdf-sheet-'+n:undefined}>
    <div class="pdf-kicker">{project.name}</div>
    <h1>{s.page?.name??'Contents'}</h1>
    {#if s.index}
      <div class="pdf-index">{#each s.index as p}<a href={sheetFor({pageId:p.id})}><span>{p.name}</span><span>{sheets.findIndex(v=>v.page===p)+1}</span></a>{/each}</div>
    {:else}
      <div class="pdf-window" style:width={width+'px'} style:height={height+'px'}>
        <div class="pdf-world" style:transform={'scale('+s.scale+') translate('+(-s.x)+'px,'+(-s.y)+'px)'}>
          <svg class="pdf-connections" width="1" height="1" aria-hidden="true">
            <defs><marker id={'pdf-arrow-'+n+'-'+print} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto"><path d="M0 0L10 5L0 10Z" fill="#485d77"/></marker></defs>
            {#each s.page?.connections??[] as edge}
              {@const from=s.items.find(o=>o.id===edge.from.objectId)}
              {@const to=s.items.find(o=>o.id===edge.to.objectId)}
              {#if from && to}<path d={connectionPath(portPosition(from,edge.from.side),portPosition(to,edge.to.side))} fill="none" stroke="#485d77" stroke-width="2" marker-end={'url(#pdf-arrow-'+n+'-'+print+')'}/>{/if}
            {/each}
          </svg>
          {#each s.items as o}
            {@const tags=o.inheritDepartment?(pageTags.get(s.page?.id??'')??[]):o.departments??[o.department]}
            {@const color=(project.catalog??defaultCatalog).find(d=>d.id===tags[0])?.color??'#808080'}
            {@const frameColor=o.kind==='Frame'?(o.frameBackground??color):undefined}
            {@const src=o.source??(o.attachment?images[JSON.stringify(o.attachment)]:undefined)}
            {@const dest=o.link??(o.target?{pageId:o.target}:undefined)}
            <div class="pdf-object" class:drawing-object={!!o.drawing} class:frame={o.kind==='Frame'} style:left={o.x+'px'} style:top={o.y+'px'} style:width={o.w+'px'} style:height={o.h+'px'} style:border-color={color} style:--frame-color={frameColor} style:--frame-text={frameColor && frameHeaderText(frameColor)} style:transform={'rotate('+(o.rotation??0)+'deg)'}>
              {#if o.kind==='Image' && src}<img src={src} alt={o.name}/>{/if}
              {#if o.drawing}<DrawingStroke drawing={o.drawing}/>
              {:else if o.kind==='Image' && o.showTitle===false}
                {#if dest && sheetFor(dest)}<a class="pdf-image-link" href={sheetFor(dest)} aria-label={o.name}></a>{/if}
              {:else}<strong>{#if frameColor}<span aria-hidden="true">▣ </span>{/if}{#if dest && sheetFor(dest)}<a href={sheetFor(dest)}>{o.name}</a>{:else if o.url}<a href={o.url}>{o.name}</a>{:else}{o.name}{/if}</strong>{/if}
              {#if o.kind==='Note'}<div class="pdf-text" use:fitNote={{active:!!o.autoFit,size:o.size}} style:font-size={o.size+'px'}>{#each textParts(o) as part}<span style={part.css}>{#if part.to && sheetFor(part.to)}<a href={sheetFor(part.to)}>{part.text}</a>{:else}{part.text}{/if}</span>{/each}</div>
              {:else if o.url}<small>{o.url}</small><small>{o.body}</small>
              {:else if o.kind==='Attachment'}<small>{o.attachment?.folder?'Folder':'File'} · {o.attachment?.path.split(/[\\/]/).at(-1)}</small>
              {:else if o.kind==='Image' && !src}<small>Image unavailable</small>{/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}
    <div class="pdf-foot"><span>{s.page?'Canvas':'Project index'}</span><span>{n+1} / {sheets.length}</span></div>
  </section>
{/snippet}

<script module lang="ts">
  function fitNote(node:HTMLElement,config:{active:boolean;size:number}){
    function measure(){if(!config.active)return;let lo=1,hi=Math.ceil(config.size);while(lo<hi){
      const mid=Math.ceil((lo+hi)/2);node.style.fontSize=mid+'px';if(node.scrollHeight<=node.clientHeight+1 && node.scrollWidth<=node.clientWidth+1)lo=mid;else hi=mid-1;
    }node.style.fontSize=lo+'px';}
    const observer=new ResizeObserver(measure);observer.observe(node);return {destroy(){observer.disconnect();}};
  }
</script>
<dialog class="export-dialog" bind:this={dialog} oncancel={e=>{e.preventDefault();if(!busy)close();}}>
  <h2>{single?'Export page as PDF':'Export project'}</h2>
  {#if !single}<label>Format<select bind:value={mode} disabled={busy} onchange={()=>{cursor=0;error='';}}><option value="pdf">PDF document</option>{#if isTauri()}<option value="zip">Project archive (.zip)</option>{/if}</select></label>{/if}
  {#if mode==='pdf'}
    <label>Page layout<select bind:value={tile} disabled={busy} onchange={()=>cursor=0}><option value={false}>Fit canvas on one A4 sheet</option><option value={true}>Tile at original scale across A4 sheets</option></select></label>
    {#if filters.length}<label class="export-check"><input type="checkbox" bind:checked={filtered} onchange={()=>cursor=0}/> Apply active tag filters</label>{/if}
    {#if sheets[cursor]}<div class="pdf-preview">{@render sheet(sheets[cursor],cursor,false)}</div>
    <div class="preview-nav"><button disabled={cursor===0} onclick={()=>cursor--}>Previous</button><span>{cursor+1} / {sheets.length}</span><button disabled={cursor>=sheets.length-1} onclick={()=>cursor++}>Next</button></div>{/if}
    {#if loading}<p>Loading project images…</p>{/if}
  {:else}
    <p>The archive includes the project and its assets folder.</p>
    {#if links.length}<h3>External local paths ({links.length})</h3><p>These files or folders may be unavailable on another computer. Converting copies them into the project and saves the updated references. You can also export with the original paths.</p>
      <button disabled={busy} onclick={()=>localize(links.map(([file])=>file))}>Convert all to local</button>
      <ul>{#each links as [file,places]}<li><div><strong>{file}</strong><small>{[...places].join(' · ')}</small></div><button disabled={busy} onclick={()=>localize([file])}>Convert to local</button></li>{/each}</ul>
    {:else}<p>No external file references.</p>{/if}
  {/if}
  {#if mode==='zip' && [...project.pages,...project.trash??[],...(project.pageHistory??[]).map(v=>v.page)].some(p=>p.items.some(o=>o.attachment?.path.toLowerCase().endsWith('.lnk')))}<p>Windows shortcuts keep their original targets when copied.</p>{/if}
  {#if error || plan.error}<p role="alert">{error || plan.error}</p>{/if}
  <div class="dialog-actions export-actions"><button disabled={busy} onclick={close}>Cancel</button><button disabled={busy || (mode==='pdf' && (loading || !sheets.length))} onclick={run}>{busy?'Working…':mode==='zip'?'Export ZIP':'Export PDF'}</button></div>
</dialog>
<div class="print-root" bind:this={printRoot}>{#if shown && mode==='pdf'}{#each sheets as s,n}{@render sheet(s,n,true)}{/each}{/if}</div>

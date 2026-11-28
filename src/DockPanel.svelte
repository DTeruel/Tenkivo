<script module lang="ts">
  export type DockId='sidebar'|'inspector'|'search-panel';
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';

  let {name, panelClass, rank, onreorder, side=$bindable(), open=$bindable(), children}: {
    name:string; panelClass:DockId; rank:number; side:string; open:boolean; children:Snippet;
    onreorder:(dock:DockId,neighbor:DockId,outer:boolean)=>void;
  } = $props();
  let panel:HTMLElement;
  const minWidth=170, maxWidth=640;
  const widthKey='studio-pages-width';
  let preferredWidth=$state(Number(localStorage.getItem(widthKey)) || 0);
  let resizeStart:{x:number;width:number}|undefined;

  function setWidth(width:number){
    const limit=Math.min(maxWidth,Math.max(minWidth,Math.floor(panel.parentElement!.clientWidth*.55)));
    preferredWidth=Math.max(minWidth,Math.min(limit,Math.round(width)));
    localStorage.setItem(widthKey,String(preferredWidth));
  }
  function beginResize(event:PointerEvent){
    if(event.button!==0 || !event.isPrimary)return;
    resizeStart={x:event.clientX,width:panel.getBoundingClientRect().width};
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }
  function resizePanel(event:PointerEvent){
    if(!resizeStart)return;
    setWidth(resizeStart.width+(event.clientX-resizeStart.x)*(side==='left'?1:-1));
  }
  function resizeWithKeys(event:KeyboardEvent){
    if(event.key!=='ArrowLeft' && event.key!=='ArrowRight')return;
    event.preventDefault();
    event.stopPropagation();
    const direction=(event.key==='ArrowRight')===(side==='left')?1:-1;
    setWidth(panel.getBoundingClientRect().width+direction*16);
  }
  let target=$state(''), dragging=$state(false);
  let insertion=$state<{neighbor:DockId;outer:boolean;x:number}>();
  const distance=$derived((open?10:20)-rank);
  let start:{id:number;x:number;y:number;area:DOMRect}|undefined;

  function begin(e:PointerEvent){
    if(e.button!==0 || !e.isPrimary)return;
    start={id:e.pointerId,x:e.clientX,y:e.clientY,area:panel.parentElement!.getBoundingClientRect()};
    e.preventDefault();
  }
  function move(e:PointerEvent){
    if(!start || start.id!==e.pointerId)return;
    if(!dragging && Math.hypot(e.clientX-start.x,e.clientY-start.y)<5)return;
    dragging=true;
    const r=start.area, edge=Math.min(250,r.width*.25);
    target='';
    insertion=undefined;
    if(e.clientY<r.top || e.clientY>r.bottom || e.clientX<r.left || e.clientX>r.right)return;
    const neighbor=document.elementFromPoint(e.clientX,e.clientY)?.closest<HTMLElement>('aside[data-dock]');
    if(neighbor && neighbor.parentElement===panel.parentElement){
      if(neighbor===panel)return;
      target=neighbor.dataset.side!;
      if(open && !neighbor.classList.contains('collapsed')){
        const rect=neighbor.getBoundingClientRect();
        const before=e.clientX<(rect.left+rect.right)/2;
        insertion={neighbor:neighbor.dataset.dock as DockId,outer:before===(target==='left'),x:(before?rect.left:rect.right)-r.left};
      }
      return;
    }
    if(e.clientX<=r.left+edge)target='left';
    if(e.clientX>=r.right-edge)target='right';
  }
  function cancel(){dragging=false;target='';start=undefined;insertion=undefined;}
  function finish(e:PointerEvent){
    if(start?.id!==e.pointerId)return;
    move(e);
    if(dragging && target){
      side=target;
      if(insertion)onreorder(panelClass,insertion.neighbor,insertion.outer);
    }
    cancel();
  }
  function key(e:KeyboardEvent){
    if(e.key==='Escape'){cancel();e.stopPropagation();return;}
    if(e.ctrlKey || e.shiftKey || e.metaKey || e.altKey)return;
    if(e.key==='ArrowLeft' || e.key==='ArrowRight'){
      e.preventDefault();side=e.key==='ArrowLeft'?'left':'right';
    }
  }
</script>

<svelte:window onpointermove={move} onpointerup={finish} onpointercancel={cancel} onblur={cancel}/>
<aside bind:this={panel} class={panelClass} class:collapsed={!open} aria-label={name}
  data-dock={panelClass} data-side={side} style:order={side==='left'?-distance:distance}
  style:width={panelClass==='sidebar' && open && preferredWidth? `clamp(${minWidth}px, ${preferredWidth}px, 55%)`:undefined}>
  <div class="panel-head">
    <button class="panel-grip" title={'Drag '+name+' to either edge, or use the arrow keys'} aria-label={'Move '+name}
      onpointerdown={begin} onkeydown={key}>
      <span aria-hidden="true" class="grip-dots"></span><span>{name}</span>
    </button>
    <button class="panel-fold" aria-expanded={open} aria-label={(open?'Collapse ':'Expand ')+name}
      title={(open?'Collapse ':'Expand ')+name} onclick={()=>open=!open}>
      <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" aria-hidden="true" style:transform={(side==='left')===open?'':'rotate(180deg)'}><path d="M10 3.5 5.5 8l4.5 4.5"/></svg>
    </button>
  </div>
  {#if open}<div class="panel-content">{@render children()}</div>{/if}
  {#if open && panelClass==='sidebar'}
    <button class="panel-resize" aria-label="Resize Pages" title="Drag to resize Pages, or use the arrow keys"
      onpointerdown={beginResize} onpointermove={resizePanel}
      onlostpointercapture={()=>resizeStart=undefined} onkeydown={resizeWithKeys}></button>
  {/if}
</aside>
{#if insertion}
  <div class="dock-insertion" style:left={insertion.x+'px'} aria-hidden="true"></div>
{:else if dragging}
  {#each ['left','right'] as edge}
    <div class={'dock-target '+edge} class:over={edge===target} aria-hidden="true">Dock {edge}</div>
  {/each}
{/if}

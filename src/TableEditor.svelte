<script lang="ts">
  type Axis = 'row' | 'column';
  type Track = {axis:Axis;index:number};

  let {rows, selected, update}:{rows:string[][];selected:boolean;update:(rows:string[][])=>void}=$props();
  let row=$state(0), column=$state(0), typing=$state(false);
  let menu=$state<Track>(), dragging=$state<Track>(), dropIndex=$state<number>();
  let editor:HTMLDivElement;
  const axes=['column','row'] as const;

  $effect(()=>{
    if(!selected){
      typing=false;
      menu=undefined;
      endDrag();
    }
  });

  function count(axis:Axis){return axis==='row'?rows.length:rows[0].length;}

  function changeTrack(axis:Axis,index:number,action:'insert'|'delete'|'move',destination=index){
    const size=count(axis);
    if(index<0 || index>size || (action!=='insert' && index===size))return;
    if(action==='delete' && size===1)return;
    if(action==='move' && (destination===index || destination<0 || destination>=size))return;

    function apply<T>(values:T[], empty:T){
      const result=values.slice();
      if(action==='insert')result.splice(index,0,empty);
      else if(action==='delete')result.splice(index,1);
      else {
        const [moved]=result.splice(index,1);
        result.splice(destination,0,moved);
      }
      return result;
    }

    const next=axis==='row'
      ? apply(rows,Array<string>(rows[0].length).fill(''))
      : rows.map(values=>apply(values,''));
    const lastIndex=(axis==='row'?next.length:next[0].length)-1;
    const position=action==='move'?destination:Math.min(index,lastIndex);
    if(axis==='row')row=position;else column=position;
    menu=undefined;
    typing=false;
    update(next);
  }

  function openMenu(axis:Axis,index:number){
    typing=false;
    menu=menu?.axis===axis && menu.index===index?undefined:{axis,index};
    if(axis==='row')row=index;else column=index;
  }

  let gripPointer:{id:number;x:number;y:number;track:Track;grips:HTMLElement[]}|undefined;
  let suppressGripClick=false;

  function grabGrip(event:PointerEvent,axis:Axis,index:number){
    if(event.button!==0)return;
    const grip=event.currentTarget as HTMLButtonElement;
    const grips=Array.from(grip.closest('.track-tools')!.querySelectorAll<HTMLElement>('.grip'));
    gripPointer={id:event.pointerId,x:event.clientX,y:event.clientY,track:{axis,index},grips};
    suppressGripClick=false;
    grip.setPointerCapture(event.pointerId);
  }

  function moveGrip(event:PointerEvent){
    const pointer=gripPointer;
    if(!pointer || pointer.id!==event.pointerId || pointer.grips.length<2)return;
    const centers=pointer.grips.map(grip=>{
      const rect=grip.getBoundingClientRect();
      return {x:rect.left+rect.width/2,y:rect.top+rect.height/2};
    });
    const first=centers[0], last=centers[centers.length-1];
    const length=Math.hypot(last.x-first.x,last.y-first.y);
    if(!length)return;
    const displacement=((event.clientX-pointer.x)*(last.x-first.x)+(event.clientY-pointer.y)*(last.y-first.y))/length;
    if(!dragging && Math.abs(displacement)<4)return;
    suppressGripClick=true;
    menu=undefined;
    dragging=pointer.track;
    const target=pointer.track.index+Math.round(displacement*(centers.length-1)/length);
    dropIndex=Math.max(0,Math.min(centers.length-1,target));
  }

  function releaseGrip(event:PointerEvent){
    if(gripPointer?.id!==event.pointerId)return;
    if(event.type==='pointerup'){
      moveGrip(event);
      if(dragging && dropIndex!==undefined)changeTrack(dragging.axis,dragging.index,'move',dropIndex);
    }
    endDrag();
  }

  function endDrag(){gripPointer=undefined;dragging=undefined;dropIndex=undefined;}

  function focusCell(node:HTMLTextAreaElement){node.focus();node.setSelectionRange(node.value.length,node.value.length);}
</script>

<svelte:window
  onpointerdown={event=>{if(menu && !editor.contains(event.target as Node))menu=undefined;}}
  onkeydown={event=>{if(event.key==='Escape')menu=undefined;}}/>

<div class="grid" bind:this={editor}>
  <div class="table-layout" class:interactive={selected} style:--columns={rows[0].length} style:--rows={rows.length}>
    {#if selected}
      {#each axes as axis}
        <div class="track-tools" class:columns={axis==='column'} class:rows={axis==='row'} role="group" aria-label={axis==='row'?'Rows':'Columns'}>
          {#each Array(count(axis)) as _,index}
            <div class="track" class:target={dragging?.axis===axis && dropIndex===index}
              role="group" aria-label={axis+' '+(index+1)}>
              <button class="insert" title={'Insert '+axis+' before '+(index+1)} aria-label={'Insert '+axis+' before '+(index+1)} onclick={()=>changeTrack(axis,index,'insert')}>+</button>
              <button class="grip" class:current={menu?.axis===axis && menu.index===index}
                title={'Drag to move '+axis+' '+(index+1)+', or click for actions'} aria-label={'Actions for '+axis+' '+(index+1)}
                aria-expanded={menu?.axis===axis && menu.index===index}
                onpointerdown={event=>grabGrip(event,axis,index)} onpointermove={moveGrip} onpointerup={releaseGrip}
                onpointercancel={releaseGrip} onlostpointercapture={releaseGrip}
                onclick={event=>{if(!suppressGripClick || event.detail===0)openMenu(axis,index);}}>⠿</button>
              {#if menu?.axis===axis && menu.index===index}
                <div class="track-menu" role="group" aria-label={axis+' actions'}>
                  <button disabled={index===0} onclick={()=>changeTrack(axis,index,'move',index-1)}>Move {axis==='row'?'up':'left'}</button>
                  <button disabled={index===count(axis)-1} onclick={()=>changeTrack(axis,index,'move',index+1)}>Move {axis==='row'?'down':'right'}</button>
                  <button disabled={count(axis)===1} onclick={()=>changeTrack(axis,index,'delete')}>Delete {axis}</button>
                </div>
              {/if}
            </div>
          {/each}
        </div>
        <button class="append" class:append-row={axis==='row'} class:append-column={axis==='column'}
          title={'Add '+axis} aria-label={'Add '+axis} onclick={()=>changeTrack(axis,count(axis),'insert')}>+</button>
      {/each}
    {/if}
    <table aria-label="Table contents"><tbody>
      {#each rows as values,r}<tr>{#each values as value,c}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <td class:chosen={selected && !menu && !dragging && row===r && column===c}
          class:highlighted={selected && (menu?.axis==='row' && menu.index===r || menu?.axis==='column' && menu.index===c || dragging?.axis==='row' && dropIndex===r || dragging?.axis==='column' && dropIndex===c)}

          onpointerdown={e=>{if(e.button===0){menu=undefined;if(row!==r || column!==c){typing=false;row=r;column=c;}}}}
          ondblclick={e=>{if(selected){e.stopPropagation();row=r;column=c;typing=true;}}}>
          {#if selected && typing && row===r && column===c}
            <textarea aria-label={'Row '+(r+1)+', column '+(c+1)} use:focusCell value={value} oninput={e=>{const text=e.currentTarget.value;update(rows.map((v,i)=>i===r?v.map((old,j)=>j===c?text:old):v));}} onblur={()=>typing=false} onkeydown={e=>{if(e.key==='Escape'){e.stopPropagation();typing=false;}}}></textarea>
          {:else}<span class="cell-text">{value}</span>{/if}
        </td>
      {/each}</tr>{/each}
    </tbody></table>
  </div>
</div>

<style>
  .grid{flex:1;min-height:0;overflow:auto;scrollbar-width:thin}
  .table-layout{position:relative;box-sizing:border-box;height:100%;min-height:calc(var(--rows) * 24px);min-width:calc(var(--columns) * 48px)}
  .interactive{padding:23px;min-height:calc(var(--rows) * 24px + 46px);min-width:calc(var(--columns) * 48px + 46px)}
  table{width:100%;height:100%;border-collapse:collapse;table-layout:fixed;font-size:12px}
  tr{height:calc(100% / var(--rows))}
  td{position:relative;border:1px solid var(--line)}
  .cell-text{position:absolute;inset:0;padding:4px 6px;overflow:hidden;white-space:pre-wrap;overflow-wrap:anywhere}
  .chosen{outline:1px solid var(--accent);outline-offset:-1px}
  .highlighted{background:color-mix(in srgb,var(--accent) 15%,transparent)}
  textarea{position:absolute;inset:0;box-sizing:border-box;width:100%;height:100%;min-height:0;padding:3px 5px;resize:none;font:inherit;margin:0!important}
  .track-tools{position:absolute;display:grid}
  .columns{left:23px;right:23px;top:0;height:23px;grid-template-columns:repeat(var(--columns),minmax(0,1fr))}
  .rows{top:23px;bottom:23px;left:0;width:23px;grid-template-rows:repeat(var(--rows),minmax(0,1fr))}
  .track{position:relative;display:grid;place-items:center}
  .grip,.insert,.append{display:grid;place-items:center;padding:0;line-height:1}
  .grip{width:22px;height:15px;border-radius:5px;font-size:13px;color:var(--muted);cursor:grab}
  .rows .grip{width:15px;height:19px}
  .grip:hover,.grip.current,.target .grip{border-color:var(--accent);color:var(--accent)}
  .insert,.append{position:absolute;width:16px;height:16px;border-radius:50%;color:var(--muted);font-size:13px}
  .insert{z-index:1;opacity:0}
  .track-tools:hover .insert,.track-tools:focus-within .insert{opacity:1}
  .columns .insert{left:0;transform:translateX(-50%)}
  .rows .insert{top:0;transform:translateY(-50%)}
  .append-row{bottom:3px;left:50%;transform:translateX(-50%)}
  .append-column{right:3px;top:50%;transform:translateY(-50%)}
  .track-menu{position:absolute;z-index:2;display:grid;min-width:max-content;padding:3px;background:var(--panel);border:1px solid var(--line);border-radius:6px;box-shadow:0 4px 12px #0003}
  .columns .track-menu{top:100%;right:0}
  .columns .track:first-child .track-menu{right:auto;left:0}
  .rows .track-menu{top:0;left:100%}
  .rows .track:last-child .track-menu{top:auto;bottom:0}
  .track-menu button{border:0;text-align:left;padding:3px 7px;font-size:11px}
</style>

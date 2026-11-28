<script lang="ts">
  let {rows, selected, update}:{rows:string[][];selected:boolean;update:(rows:string[][])=>void}=$props();
  let row=$state(0), column=$state(0), typing=$state(false);
  $effect(()=>{if(!selected) typing=false;});
  function modify(axis:'Row'|'Column', action:'add'|'remove'|'previous'|'next'){
    const index=Math.min(axis==='Row'?row:column,(axis==='Row'?rows.length:rows[0].length)-1);
    function edit<T>(values:T[], empty:T):T[]{
      const result=values.slice();
      if(action==='add')result.splice(index+1,0,empty);
      else if(action==='remove')result.splice(index,1);
      else {
        const dest=index+(action==='previous'?-1:1);
        [result[index],result[dest]]=[result[dest],result[index]];
      }
      return result;
    }
    const next=axis==='Row'?edit(rows,Array<string>(rows[0].length).fill('')):rows.map(values=>edit(values,''));
    const position=Math.min(index+(action==='previous'?-1:action==='remove'?0:1),(axis==='Row'?next.length:next[0].length)-1);
    if(axis==='Row')row=position;else column=position;
    update(next);
  }
  function focusCell(node:HTMLTextAreaElement){node.focus();node.setSelectionRange(node.value.length,node.value.length);}
</script>

{#if selected}
  <div class="tools">
    {#each ['Row','Column'] as const as axis}
      {@const index=axis==='Row'?row:column}
      {@const count=axis==='Row'?rows.length:rows[0].length}
      <span>{axis}</span>
      {#each ['add','remove','previous','next'] as const as action}
        {@const label=action==='add'?'Insert '+axis.toLowerCase()+' after selection':action==='remove'?'Delete selected '+axis.toLowerCase():'Move '+axis.toLowerCase()+' '+(axis==='Row'?(action==='previous'?'up':'down'):(action==='previous'?'left':'right'))}
        <button title={label} aria-label={label} disabled={(action==='remove' && count===1)||(action==='previous' && index===0)||(action==='next' && index>=count-1)} onclick={()=>modify(axis,action)}>{action==='add'?'+':action==='remove'?'−':axis==='Row'?(action==='previous'?'↑':'↓'):(action==='previous'?'←':'→')}</button>
      {/each}
    {/each}
  </div>
{/if}
<div class="grid">
<table aria-label="Table contents"><tbody>
  {#each rows as values,r}<tr>{#each values as value,c}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <td class:chosen={selected && row===r && column===c} onpointerdown={e=>{if(e.button===0 && (row!==r || column!==c)){typing=false;row=r;column=c;}}} ondblclick={e=>{if(selected){e.stopPropagation();row=r;column=c;typing=true;}}}>
      {#if selected && typing && row===r && column===c}
        <textarea aria-label={'Row '+(r+1)+', column '+(c+1)} use:focusCell value={value} oninput={e=>{const text=e.currentTarget.value;update(rows.map((v,i)=>i===r?v.map((old,j)=>j===c?text:old):v));}} onblur={()=>typing=false} onkeydown={e=>{if(e.key==='Escape'){e.stopPropagation();typing=false;}}}></textarea>
      {:else}{value}{/if}
    </td>
  {/each}</tr>{/each}
</tbody></table>
</div>
<style>
  .tools{display:flex;align-items:center;gap:2px;flex-wrap:wrap;font-size:11px;padding-bottom:3px}
  .tools span{margin:0 3px}
  .tools button{padding:0 3px;min-width:19px;height:20px;line-height:1}
  .grid{flex:1;min-height:0;overflow:auto}
  table{width:100%;height:100%;border-collapse:collapse;table-layout:fixed;font-size:12px}
  td{position:relative;min-width:0;border:1px solid var(--line);padding:4px 6px;white-space:pre-wrap;overflow-wrap:anywhere;vertical-align:top}
  .chosen{outline:1px solid var(--accent);outline-offset:-1px}
  textarea{position:absolute;inset:0;box-sizing:border-box;width:100%;height:100%;min-height:0;padding:3px 5px;resize:none;font:inherit}
</style>

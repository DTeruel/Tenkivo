<script lang="ts">
  import {untrack} from 'svelte';
  let {rows,apply,cancel}:{rows:string[][];apply:(rows:string[][])=>void;cancel:()=>void}=$props();
  let draft=$state(untrack(()=>rows.map(row=>[...row])));
  let selectedRow=$state(0), selectedColumn=$state(0);
  const columns=$derived(draft[0].length);

  function removeRow(){
    draft.splice(selectedRow,1);
    selectedRow=Math.min(selectedRow,draft.length-1);
  }
  function removeColumn(){
    for(const row of draft)row.splice(selectedColumn,1);
    selectedColumn=Math.min(selectedColumn,columns-1);
  }
</script>

<form onsubmit={e=>{e.preventDefault();apply($state.snapshot(draft));}}>
  <div class="table-tools">
    <button type="button" onclick={()=>draft.push(Array<string>(columns).fill(''))}>Add row</button>
    <button type="button" onclick={()=>{for(const row of draft)row.push('');}}>Add column</button>
    <button type="button" disabled={draft.length===1} onclick={removeRow}>Remove row {selectedRow+1}</button>
    <button type="button" disabled={columns===1} onclick={removeColumn}>Remove column {selectedColumn+1}</button>
  </div>
  <div class="table-scroll">
    <table aria-label="Edit table cells"><thead><tr><td></td>{#each draft[0] as _, c}<th scope="col">{c+1}</th>{/each}</tr></thead>
      <tbody>{#each draft as row,r}
        <tr><th scope="row">{r+1}</th>{#each row as _,c}
          <td><textarea rows="2" aria-label={'Row '+(r+1)+', column '+(c+1)} bind:value={draft[r][c]}
            onfocus={()=>{selectedRow=r;selectedColumn=c;}}></textarea></td>
        {/each}</tr>
      {/each}</tbody>
    </table>
  </div>
  <div class="dialog-actions"><button type="button" onclick={cancel}>Cancel</button><button type="submit">Apply</button></div>
</form>

<style>
  .table-tools {display:flex;flex-wrap:wrap;gap:6px;}
  .table-scroll {max-width:min(760px,78vw);max-height:55vh;overflow:auto;margin-block:10px;}
  table {border-spacing:4px;}
  th {font-weight:500;padding:0 4px;}
  textarea {display:block;width:120px;min-height:38px;resize:vertical;margin:0;}
</style>

<script lang="ts">
  let {rows}:{rows:string[][]}=$props();
</script>

<table aria-label="Table contents">
  <tbody>
    {#each rows as row}
      <tr>{#each row as cell}<td>{cell}</td>{/each}</tr>
    {/each}
  </tbody>
</table>

<style>
  table {width:100%;height:100%;table-layout:fixed;border-collapse:collapse;font-size:12px;}
  td {border:1px solid var(--line,#aaa);padding:4px 6px;white-space:pre-wrap;overflow-wrap:anywhere;vertical-align:top;}
</style>

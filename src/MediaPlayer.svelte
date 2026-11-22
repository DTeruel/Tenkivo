<script lang="ts">
  import {convertFileSrc} from '@tauri-apps/api/core';
  import {invoke} from './diagnostics.svelte';
  import type {Attachment} from './model';

  let {project,attachment,name,kind}:{kind:"audio"|"video";project:string;attachment:Attachment;name:string}=$props();
  let failed=$state('');
  let source=$derived(invoke<string>('attachment_media',{project,attachment:{...attachment}}).then(file=>convertFileSrc(file)));
</script>

{#await source}
  <small>Loading {kind}…</small>
{:then url}
  {#if kind==='video'}
    <!-- svelte-ignore a11y_media_has_caption (user-provided local video) -->
    <video controls playsinline preload="metadata" src={url} aria-label={'Play '+name}
      onerror={()=>failed=url} onloadedmetadata={()=>failed=''}></video>
  {:else}
  <audio controls preload="none" src={url} aria-label={'Play '+name}
    onerror={()=>failed=url} onloadedmetadata={()=>failed=''}></audio>
  {/if}
  {#if failed===url}<small>{kind==='video'?'Video':'Audio'} unavailable. Open the file externally or relink it in the inspector.</small>{/if}
{:catch}
  <small>{kind==='video'?'Video':'Audio'} unavailable. Check the file location in the inspector.</small>
{/await}

<style>
  video {display:block;width:100%;min-height:0;flex:1;object-fit:contain;background:#101010}
  audio {display:block;width:calc(100% - 12px);min-width:0;height:38px;flex:none;margin:4px 6px}
</style>

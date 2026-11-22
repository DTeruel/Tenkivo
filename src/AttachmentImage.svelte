<script lang="ts">
  import {invoke} from './diagnostics.svelte';
  import type {Attachment} from './model';
  let {project,attachment,name}:{project:string;attachment:Attachment;name:string}=$props();
  let data=$derived(invoke<string>('attachment_image',{project,attachment:{...attachment}}));
</script>
{#await data}<small>Loading image…</small>{:then src}<img draggable="false" {src} alt={name} decoding="async"/>{:catch}<small>× Image unavailable</small>{/await}

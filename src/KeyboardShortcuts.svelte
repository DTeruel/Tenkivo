<script lang="ts">
  const groups:{title:string;shortcuts:{action:string;keys:string[]}[]}[]=[
    {title:'Project and navigation',shortcuts:[
      {action:'Toggle read / edit mode',keys:['E']},
      {action:'Save project',keys:['Ctrl / Cmd + S']},
      {action:'Focus selection or page',keys:['F']},
      {action:'Back or forward in history',keys:['Ctrl / Cmd + Shift + ← / →']},
    ]},
    {title:'Canvas and selection',shortcuts:[
      {action:'Zoom at pointer',keys:['Mouse wheel']},
      {action:'Pan',keys:['Middle-button drag']},
      {action:'Select multiple objects',keys:['Left-button drag']},
      {action:'Extend selection',keys:['Shift + click']},
    ]},
    {title:'Clipboard',shortcuts:[
      {action:'Copy page or selection',keys:['Ctrl / Cmd + C']},
      {action:'Cut canvas selection',keys:['Ctrl / Cmd + X']},
      {action:'Paste page, selection, or link',keys:['Ctrl / Cmd + V']},
    ]},
    {title:'Edit and transform',shortcuts:[
      {action:'Undo',keys:['Ctrl / Cmd + Z']},
      {action:'Redo',keys:['Ctrl / Cmd + Shift + Z','Ctrl / Cmd + Y']},
      {action:'Delete selection or focused page',keys:['Delete']},
      {action:'Resize proportionally',keys:['Shift + corner drag']},
      {action:'Snap a transform',keys:['Ctrl + drag']},
    ]},
    {title:'Text and links',shortcuts:[
      {action:'Bold, italic, or underline',keys:['Ctrl / Cmd + B / I / U']},
      {action:'Link selected text or object',keys:['Ctrl / Cmd + K']},
      {action:'Connect canvas objects',keys:['Alt + drag between ports']},
    ]},
    {title:'Drawing',shortcuts:[
      {action:'Finish drawing and start another',keys:['Enter']},
      {action:'Cancel current drawing',keys:['Esc']},
    ]},
  ];
</script>

<div class="shortcut-groups">
  {#each groups as group (group.title)}
    <section aria-label={group.title}>
      <h3>{group.title}</h3>
      <dl>
        {#each group.shortcuts as shortcut (shortcut.action)}
          <div>
            <dt>{shortcut.action}</dt>
            <dd>
              {#each shortcut.keys as keys,i (keys)}
                {#if i}<span>or</span>{/if}
                <kbd>{keys}</kbd>
              {/each}
            </dd>
          </div>
        {/each}
      </dl>
    </section>
  {/each}
</div>

<style>
  .shortcut-groups{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,275px),1fr));gap:14px;align-items:start}
  section{min-width:0;padding:12px;border:1px solid var(--line);border-radius:8px;background:var(--paper)}
  h3{margin:0 0 7px;font-size:13px}
  dl{margin:0}
  dl>div{display:flex;align-items:baseline;justify-content:space-between;gap:6px 12px;flex-wrap:wrap;padding:6px 0}
  dl>div+div{border-top:1px solid var(--line)}
  dt{font-size:12px;color:var(--muted)}
  dd{display:flex;align-items:baseline;justify-content:flex-end;flex-wrap:wrap;gap:4px;margin:0 0 0 auto;font-size:11px;color:var(--muted)}
  kbd{display:inline-block;padding:2px 5px;background:var(--panel);color:var(--ink);font-size:11px;line-height:1.35;overflow-wrap:anywhere}
</style>

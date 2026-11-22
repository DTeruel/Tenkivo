<script lang="ts">
  import type {Drawing} from './model';
  let {drawing}:{drawing:Drawing}=$props();
  let strokes=$derived('strokes' in drawing?drawing.strokes:[drawing]);
</script>

<svg aria-hidden="true" width="100%" height="100%" viewBox={'0 0 '+drawing.width+' '+drawing.height} preserveAspectRatio="none">
  {#each strokes as stroke}
    {#if stroke.points.length===1}
      <circle cx={stroke.points[0].x} cy={stroke.points[0].y} r={stroke.thickness/2} fill={stroke.color}/>
    {:else}
      <polyline points={stroke.points.map(p=>p.x+','+p.y).join(' ')} fill="none" stroke={stroke.color}
        stroke-width={stroke.thickness} stroke-linejoin="round" stroke-linecap="round"/>
    {/if}
  {/each}
</svg>

<style>
  svg {display:block;overflow:visible;pointer-events:none}
</style>

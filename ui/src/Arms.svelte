<script>
  // The logo: the arm-wrestle handshake from the theme pack, cut out, the arms fading where they
  // run off the art. Small sizes are smooth-scaled so both arms still read; at 104 px and up it's
  // drawn pixel for pixel. A pack without a handshake gets the hunter's three dots instead.
  import { PK, url } from './lib/packdata.svelte.js';

  /** @type {{ size?: number }} */
  let { size = 36 } = $props();
</script>

{#if PK.handshake}
  <img class="arms" class:px={size % 104 === 0} src={url(PK.handshake)} alt="" width={size} height={size} aria-hidden="true" />
{:else}
  <span class="dots" style="--u:{size / 44}px" aria-hidden="true"><i></i><i></i><i></i></span>
{/if}

<style>
  .arms {
    display: block;
    flex: none;
    -webkit-mask-image: linear-gradient(#000 70%, transparent 100%), linear-gradient(90deg, transparent, #000 8%, #000 92%, transparent);
    -webkit-mask-composite: source-in;
    mask-image: linear-gradient(#000 70%, transparent 100%), linear-gradient(90deg, transparent, #000 8%, #000 92%, transparent);
    mask-composite: intersect;
  }
  .px { image-rendering: pixelated; }
  .dots { position: relative; display: block; flex: none; width: calc(var(--u) * 44); height: calc(var(--u) * 40); }
  .dots i { position: absolute; width: calc(var(--u) * 11); height: calc(var(--u) * 11); border-radius: 50%; background: var(--color-hit); box-shadow: 0 0 calc(var(--u) * 14) rgb(255 75 58 / 0.9); }
  .dots i:nth-child(1) { left: calc(var(--u) * 16); top: 0; }
  .dots i:nth-child(2) { left: 0; top: calc(var(--u) * 27); }
  .dots i:nth-child(3) { left: calc(var(--u) * 32); top: calc(var(--u) * 27); }
</style>

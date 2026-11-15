<script>
  // The squad's helicopter: original pixel art from the YSOB game (176x72 body, nose to the
  // right, no markings), with its 4-frame main and tail rotor sheets spinning on top. Drawn at a
  // whole-number scale (pixel for pixel) or under 1 (smooth, so it still reads small);
  // `face="left"` mirrors it.
  /** @type {{ scale?: number, face?: 'left' | 'right' }} */
  let { scale = 2, face = 'right' } = $props();
  const sharp = $derived(Number.isInteger(scale));
</script>

<div class="huey" class:left={face === 'left'} style="--s:{scale}px; width:{200 * scale}px; height:{72 * scale}px" aria-hidden="true">
  <img class="body" class:px={sharp} src="/heli/heli_body.png" alt="" width={176 * scale} height={72 * scale} />
  <span class="rotor" class:px={sharp}></span>
  <span class="tail" class:px={sharp}></span>
</div>

<style>
  .huey { position: relative; flex: none; }
  .px { image-rendering: pixelated; }
  .left { transform: scaleX(-1); }
  .body { position: absolute; left: 0; top: 0; display: block; max-width: none; }
  .rotor, .tail { position: absolute; display: block; background-repeat: no-repeat; background-size: 400% 100%; animation: spin 0.17s steps(4) infinite; }
  /* frame centred on the mast, 3 px down the body (as in the game's heli.gd) */
  .rotor { left: 0; top: calc(var(--s) * -5); width: calc(var(--s) * 200); height: calc(var(--s) * 16); background-image: url('/heli/heli_rotor.png'); }
  .tail { left: calc(var(--s) * -0.5); top: calc(var(--s) * 3.5); width: calc(var(--s) * 24); height: calc(var(--s) * 24); background-image: url('/heli/heli_tail_rotor.png'); animation-duration: 0.13s; }
  @keyframes spin { to { background-position: 133.333% 0; } }
  @media (prefers-reduced-motion: reduce) { .rotor, .tail { animation: none; } }
</style>

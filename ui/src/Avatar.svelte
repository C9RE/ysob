<script>
  import { PK, url, member, slugOf } from './lib/packdata.svelte.js';

  /** @type {{ name: string, face?: string, size?: number, status?: string, dying?: boolean }} */
  let { name, face = '', size = 36, status = 'idle', dying = false } = $props();

  const m = $derived(member(name));
  const slug = $derived(slugOf({ name, face }));
  const entry = $derived(PK.crew[slug]);
  const portrait = $derived(entry?.portrait ? url(entry.portrait) : '');
  /** No portrait: the first frame of the idle sprite sheet, scaled to fit. */
  const idle = $derived(entry?.anims?.idle);
  const hue = $derived(m?.hue ?? 120);
  /** Where the head is in the portrait ([centre x, top, body height], from the pack), so the
   *  face fills the frame whatever the pose. Without it: top centre, the usual place. */
  const zoom = $derived.by(() => {
    const h = /** @type {number[] | undefined} */ (entry?.anchor?.head);
    if (!h) return '';
    const z = 1 / Math.max(0.2, h[2] * 0.4);
    return `transform-origin:${h[0] * 100}% ${h[1] * 100}%; transform:translate(${(0.5 - h[0]) * 100}%, ${(0.08 - h[1]) * 100}%) scale(${z.toFixed(2)})`;
  });
  let broken = $state(false);
  $effect(() => {
    portrait;
    broken = false;
  });
</script>

<span
  class="av"
  class:dying
  class:held={status === 'held'}
  class:ask={status === 'waiting'}
  style="--s:{size}px; --h:{hue}"
  aria-hidden="true"
>
  {#if portrait && !broken}
    <img class="pixel" src={portrait} alt="" style={zoom} onerror={() => (broken = true)} />
  {:else if idle}
    <span class="frame pixel" style="background-image:url({url(idle.sheet)}); --fw:{idle.w}; --fh:{idle.h}; --n:{idle.frames}"></span>
  {:else}
    <svg viewBox="0 0 16 16" class="pixel">
      <path d="M5 2h6v1h1v4h-1v1h-1v1h3v1h1v5H2v-5h1V9h3V8H5V7H4V3h1z" fill="hsl(var(--h) 22% 30%)" />
      <path d="M5 4h6v1H5zM6 6h4v1H6z" fill="hsl(var(--h) 30% 18%)" />
    </svg>
    <b>{name.slice(0, 1)}</b>
  {/if}
  {#if dying}
    <i class="dot d1"></i><i class="dot d2"></i><i class="dot d3"></i>
  {/if}
</span>

<style>
  .av {
    position: relative;
    display: inline-grid;
    place-items: center;
    width: var(--s);
    height: var(--s);
    flex: none;
    border-radius: calc(var(--s) * 0.28);
    background: radial-gradient(120% 120% at 30% 20%, hsl(var(--h) 25% 22%), hsl(var(--h) 20% 11%));
    overflow: hidden;
    transition: filter 0.3s;
  }
  img,
  svg {
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: 50% 12%;
  }
  /* Portrait renders are full-body; zoom to head and shoulders. */
  img {
    transform-origin: 50% 2%;
    transform: scale(2.5);
  }
  .frame {
    width: 100%;
    height: 100%;
    background-repeat: no-repeat;
    background-size: calc(var(--n) * 100% * var(--fw) / var(--fh)) 100%;
    background-position: 0 0;
  }
  b {
    position: absolute;
    right: 3px;
    bottom: 1px;
    font: 600 calc(var(--s) * 0.3) / 1 var(--font-pixel);
    color: hsl(var(--h) 60% 75%);
  }
  .held {
    filter: saturate(0.3) brightness(0.8);
    box-shadow: inset 0 0 0 1.5px rgb(255 75 58 / 0.7);
  }
  .ask {
    box-shadow: inset 0 0 0 1.5px rgb(244 181 74 / 0.8);
  }
  /* The hunter's three dots converge, then the squad member burns out. */
  .dot {
    position: absolute;
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: #ff2a1a;
    box-shadow: 0 0 6px 2px rgb(255 42 26 / 0.8);
    animation: lock 0.9s var(--ease) forwards;
  }
  .d1 { --x: -40px; --y: -30px; }
  .d2 { --x: 40px; --y: -24px; animation-delay: 0.08s; }
  .d3 { --x: 0px; --y: 40px; animation-delay: 0.16s; }
  @keyframes lock {
    from { transform: translate(var(--x), var(--y)); opacity: 0; }
    60% { opacity: 1; }
    to { transform: translate(calc(var(--x) * 0.06), calc(var(--y) * 0.06 - 4px)); opacity: 1; }
  }
  .dying {
    animation: taken 1.9s steps(1, end) forwards;
  }
  .dying img,
  .dying svg {
    animation: burn 1s 0.9s steps(6) forwards;
  }
  @keyframes taken {
    0%, 47% { filter: none; }
    48% { filter: brightness(3) saturate(0); }
    52% { filter: hue-rotate(-30deg) saturate(3); }
    100% { filter: none; }
  }
  @keyframes burn {
    to { clip-path: inset(100% 0 0 0); opacity: 0; transform: translateY(-3px); }
  }
</style>

<script>
  import { PK, url, member, slugOf } from './lib/packdata.svelte.js';
  import { scene } from './lib/scene.svelte.js';

  /**
   * A full-body squad member in the jungle, always side-on (the way the sheets are drawn).
   * pose: 'side' stands, 'walk' walks. dir: 1 faces right, -1 faces left (the sheets face
   * right and are mirrored). A pack with no walk sheet stands instead; one with no sheets at
   * all shows its portrait, and with no art there is a silhouette.
   * @type {{ name: string, face?: string, height?: number, status?: string, dying?: boolean, hurt?: boolean, win?: boolean, pose?: 'walk' | 'side', dir?: number, walkMs?: number }}
   */
  let { name, face = '', height = 72, status = 'idle', dying = false, hurt = false, win = false, pose = 'side', dir = 1, walkMs = 100 } = $props();

  const m = $derived(member(name));
  const C = $derived(PK.crew[slugOf({ name, face })]);
  const front = $derived(C?.portrait ? url(C.portrait) : '');
  /** Working: the firing animation, side-on. */
  const fire = $derived(status === 'working' ? (C?.anims?.fire ?? C?.anims?.run) : null);
  const walk = $derived(!fire && pose === 'walk' ? C?.anims?.walk : null);
  const side = $derived(!fire && !walk ? C?.anims?.idle : null);
  /** Taken by the hunter: the death sheet plays once and the body stays down. */
  const death = $derived(dying ? C?.anims?.death : null);
  /** A failed tool call: takes a hit, flinches, recovers. */
  const flinch = $derived(hurt && !dying ? C?.anims?.hurt : null);
  /** A proven job: their close-range move, once. */
  const move = $derived(win && !dying && !hurt ? C?.anims?.attack : null);
  const loop = $derived(fire ?? side ?? null);
  /** Where the gun's muzzle is in the frame (found by the pack sync), for tracers and light. */
  const muzzle = $derived(fire?.muzzle ?? [0.84, 0.45]);
  const fx = $derived(status === 'working' && !dying && scene.effects !== 'off');

  // Every visual is placed by where the character stands in it (measured by the pack sync), so
  // poses, animations and mirroring all line up on one spot and nothing jumps.
  const k = $derived(height / 128); // screen px per art px
  const flip = $derived(dir < 0);
  /** @param {number} art frame size in art px @param {[number, number] | undefined} a anchor @param {boolean} mirror */
  function place(art, a, mirror) {
    const size = Math.round(art * k); // whole pixels: the strip steps one frame at a time, exactly
    const [cx, fy] = a ?? [0.5, art === 128 ? 0.97 : 0.85];
    const x = mirror ? 1 - cx : cx;
    return `width:${size}px; height:${size}px; left:calc(50% - ${(x * size).toFixed(1)}px); bottom:${(-(1 - fy) * size).toFixed(1)}px; --S:${size}px`;
  }
  const sheetArt = (/** @type {any} */ anim) => anim?.w ?? 172;
  const rich = $derived(scene.effects === 'full');
  const GORE = Array.from({ length: 18 }, (_, i) => {
    const a = Math.PI * (0.75 + (((i * 37) % 18) / 18) * 0.55);
    const v = 16 + ((i * 53) % 30);
    return { dx: Math.cos(a) * v, dy: Math.sin(a) * v * 0.7, s: 1 + (i % 3), d: (i % 5) * 0.02, dark: i % 3 === 0 };
  });
</script>

<div class="fig" class:smooth={!Number.isInteger(k)} class:work={status === 'working'} class:held={status === 'held'} class:ask={status === 'waiting'} class:dying style="--h:{height}px; --hue:{m?.hue ?? 120}; --mx:{muzzle[0]}; --my:{muzzle[1]}">
  {#if death}
    <span class="sheet pixel once" class:m={flip} style="{place(sheetArt(death), death.anchor, flip)}; --n:{death.frames}; --ms:{death.ms}ms"><i style="background-image:url({url(death.sheet)}); animation-timing-function: steps({death.frames}, jump-none)"></i></span>
  {:else if flinch}
    <span class="sheet pixel hit" class:m={flip} style="{place(sheetArt(flinch), flinch.anchor, flip)}; --n:{flinch.frames}; --ms:{flinch.ms}ms"><i style="background-image:url({url(flinch.sheet)}); animation-timing-function: steps({flinch.frames}, jump-none)"></i></span>
  {:else if move}
    <span class="sheet pixel hit" class:m={flip} style="{place(sheetArt(move), move.anchor, flip)}; --n:{move.frames}; --ms:{move.ms}ms"><i style="background-image:url({url(move.sheet)}); animation-timing-function: steps({move.frames}, jump-none)"></i></span>
  {:else if walk}
    <!-- frames 1..8 loop (frame 0 is the standing pose) -->
    <span class="sheet pixel walk" class:m={flip} style="{place(sheetArt(walk), walk.anchor, flip)}; --n:{walk.frames}; --ms:{walkMs}ms"><i style="background-image:url({url(walk.sheet)})"></i></span>
  {:else if loop}
    <!-- no death sheet in this pack: they stand, then burn out -->
    <span class="sheet pixel" class:taken={dying} class:m={flip} style="{place(sheetArt(loop), loop.anchor, flip)}; --n:{loop.frames}; --ms:{fire ? loop.ms : Math.round(loop.ms * 1.6)}ms"><i style="background-image:url({url(loop.sheet)})"></i></span>
  {:else if front}
    <img class="pixel front" src={front} alt="" style={place(128, C?.anchor?.portrait, false)} />
  {:else}
    <svg viewBox="0 0 16 24" class="pixel"><path d="M6 1h4v1h1v4h-1v1H6V6H5V2h1zM4 8h8v8h-1v7H9v-6H7v6H5v-7H4z" fill="hsl(var(--hue) 20% 26%)" /></svg>
  {/if}
  {#if fx}
    <!-- Gunfire: light at the muzzle, tracers out into the trees, brass ejected, smoke drifting up. -->
    <span class="gun" class:m={flip} aria-hidden="true" style={place(sheetArt(fire), fire?.anchor, flip)}>
      <i class="muzzle"></i>
      {#each [0, 1, 2, 3, 4] as k (k)}<i class="tracer" style="--k:{k}; --a:{(k % 3) - 1}"></i>{/each}
      {#if rich}
        {#each [0, 1, 2] as k (k)}<i class="brass" style="--k:{k}"></i>{/each}
        {#each [0, 1, 2] as k (k)}<i class="smoke" style="--k:{k}"></i>{/each}
      {/if}
    </span>
  {/if}
  {#if status === 'held'}<i class="net"></i>{/if}
  {#if dying}
    <i class="dot a"></i><i class="dot b"></i><i class="dot c"></i><i class="burst"></i>
    <!-- the hit: blood out of the back (away from the hunter in the trees), then a pool -->
    <span class="gore" aria-hidden="true">{#each GORE as g, i (i)}<i style="--dx:{g.dx}; --dy:{g.dy}; --s:{g.s}; animation-delay:{1 + g.d}s" class:dark={g.dark}></i>{/each}</span>
    <i class="pool"></i>
  {/if}
  <span class="shadow"></span>
  <span class="tag">{name}</span>
</div>

<style>
  .fig {
    position: relative;
    width: calc(var(--h) * 0.62);
    height: var(--h);
    display: grid;
    justify-items: center;
    align-items: end;
  }
  img, .sheet { position: absolute; z-index: 1; }
  /* Pixel art only stays true at whole-number sizes; shrunk in between, nearest-pixel scaling
     drops rows and squashes faces, so then it's drawn smooth instead. */
  .smooth img, .smooth .sheet { image-rendering: auto; }
  svg {
    position: relative;
    z-index: 1;
    height: 100%;
    width: auto;
    filter: drop-shadow(0 0 1px rgb(0 0 0 / 0.6));
    transform-origin: 50% 100%;
    animation: breathe 2.6s ease-in-out infinite;
  }
  /* Facing left: the sprites face right, so mirror them (and the gunfire with them). */
  .m { scale: -1 1; }
  img.front { transform-origin: 50% 100%; animation: breathe 2.6s ease-in-out infinite; }
  img { filter: drop-shadow(0 0 1px rgb(0 0 0 / 0.6)); }
  /* Sprite sheets are square frames in a row. The frame is a window; the strip inside it steps
     along by whole frames with a transform, which the compositor does on its own: no style
     or layout work per frame, however many are on screen. */
  .sheet { overflow: hidden; }
  .sheet i {
    position: absolute;
    left: 0;
    top: 0;
    width: calc(var(--n) * var(--S));
    height: var(--S);
    background-repeat: no-repeat;
    background-size: 100% 100%;
    image-rendering: inherit;
    animation: play calc(var(--n) * var(--ms)) steps(var(--n)) infinite;
  }
  @keyframes play { to { transform: translateX(-100%); } }
  /* Once, after the laser dots lock on, holding the last frame (the body on the ground), then gone. */
  .sheet.once i { animation: die calc(var(--n) * 80ms) 1.1s both; }
  .sheet.once { animation: gone 0.5s 2.3s steps(5) forwards; }
  .sheet.hit i { animation: die calc(var(--n) * 90ms) both; }
  @keyframes die { to { transform: translateX(calc(var(--S) - 100%)); } }
  /* The walk skips frame 0 (the standing pose), so the cycle never hitches. */
  .sheet.walk i { animation: stride calc((var(--n) - 1) * var(--ms)) steps(calc(var(--n) - 1)) infinite; }
  @keyframes stride { from { transform: translateX(calc(var(--S) * -1)); } to { transform: translateX(-100%); } }
  @keyframes breathe { 50% { transform: scaleY(1.015); } }
  .work img, .work svg { animation: recoil 0.18s steps(2) infinite; }
  @keyframes recoil { 50% { transform: translateX(-1px); } }
  /* ---------- gunfire ---------- */
  .gun { position: absolute; z-index: 2; pointer-events: none; }
  .gun i { position: absolute; left: calc(var(--mx) * 100%); top: calc(var(--my) * 100%); }
  .muzzle {
    width: 26px;
    height: 26px;
    margin: -13px 0 0 -10px;
    border-radius: 50%;
    background: radial-gradient(circle, #fff9d8 0 12%, #ffc94a 26%, rgb(255 140 40 / 0.55) 46%, transparent 70%);
    mix-blend-mode: screen;
    animation: muzzle 0.09s steps(2) infinite;
  }
  @keyframes muzzle { 50% { opacity: 0.2; transform: scale(0.6); } }
  .tracer {
    width: 34px;
    height: 1.5px;
    margin-top: -1px;
    background: linear-gradient(90deg, transparent, #ffd76a 40%, #fff6c8);
    box-shadow: 0 0 4px rgb(255 200 80 / 0.9);
    transform-origin: 0 50%;
    opacity: 0;
    animation: tracer 0.42s linear infinite;
    animation-delay: calc(var(--k) * 0.084s);
  }
  @keyframes tracer {
    0% { transform: rotate(calc(var(--a) * 2.5deg)) translateX(4px); opacity: 1; }
    100% { transform: rotate(calc(var(--a) * 2.5deg)) translateX(240px); opacity: 0; }
  }
  .brass {
    width: 2px;
    height: 1px;
    background: #e8b84a;
    animation: brass 0.7s cubic-bezier(0.3, 0.6, 0.6, 1) infinite;
    animation-delay: calc(var(--k) * 0.23s);
    margin-left: -14px;
  }
  @keyframes brass {
    0% { transform: translate(0, 0) rotate(0); opacity: 1; }
    40% { transform: translate(-10px, -14px) rotate(220deg); }
    100% { transform: translate(-18px, 40px) rotate(520deg); opacity: 0; }
  }
  .smoke {
    width: 10px;
    height: 10px;
    margin: -5px 0 0 -4px;
    border-radius: 50%;
    background: radial-gradient(circle, rgb(200 205 195 / 0.35), transparent 70%);
    animation: smoke 1.6s ease-out infinite;
    animation-delay: calc(var(--k) * 0.53s);
  }
  @keyframes smoke {
    0% { transform: translate(0, 0) scale(0.4); opacity: 0.8; }
    100% { transform: translate(-16px, -34px) scale(2.6); opacity: 0; }
  }
  /* Caught in the net: drained of colour and swaying. */
  .held img, .held .sheet { filter: saturate(0.2) brightness(0.7); }
  .held { transform-origin: 50% 0; animation: sway 2.4s ease-in-out infinite; }
  @keyframes sway { 50% { rotate: 2.5deg; } }
  .net {
    position: absolute;
    z-index: 2;
    inset: 4% 0 10%;
    background: repeating-linear-gradient(45deg, rgb(210 200 160 / 0.5) 0 1px, transparent 1px 6px), repeating-linear-gradient(-45deg, rgb(210 200 160 / 0.5) 0 1px, transparent 1px 6px);
    border-radius: 40%;
  }
  /* Waiting on you: their name plate lifts and falls, in the ask colour. */
  .ask .tag { animation: wave 0.9s ease-in-out infinite; }
  @keyframes wave { 50% { translate: 0 -3px; } }
  .shadow {
    position: absolute;
    bottom: -3px;
    width: 70%;
    height: 6px;
    border-radius: 50%;
    background: rgb(0 0 0 / 0.55);
  }
  .tag {
    position: absolute;
    z-index: 9;
    /* above the head: the undergrowth in front of the squad hides anything at their feet */
    top: -17px;
    padding: 2px 6px 3px;
    border-radius: 5px;
    background: rgb(5 8 6 / 0.7);
    font: 600 10.5px/1 var(--font-pixel);
    letter-spacing: 0.05em;
    color: #eceee6;
    white-space: nowrap;
  }
  .work .tag { color: var(--color-go); }
  .ask .tag { color: var(--color-ask); }
  .held .tag { color: #ff8b7f; }
  .dot {
    position: absolute;
    z-index: 3;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: #ff2a1a;
    box-shadow: 0 0 8px 2px rgb(255 42 26 / 0.85);
    top: 22%;
    /* they lock on, then go out with the shot: nothing is left hanging in the air */
    animation: lock 0.9s cubic-bezier(0.2, 0.8, 0.2, 1) forwards, out 0.25s 1.1s forwards;
  }
  @keyframes out { to { opacity: 0; } }
  .a { --x: -60px; --y: -40px; left: 46%; }
  .b { --x: 60px; --y: -30px; left: 54%; animation-delay: 0.08s, 1.1s; }
  .c { --x: 0px; --y: 50px; left: 50%; top: 28%; animation-delay: 0.16s, 1.1s; }
  @keyframes lock { from { transform: translate(var(--x), var(--y)); opacity: 0; } 60% { opacity: 1; } to { transform: none; opacity: 1; } }
  .burst {
    position: absolute;
    z-index: 4;
    top: 15%;
    width: 140%;
    aspect-ratio: 1;
    border-radius: 50%;
    background: radial-gradient(circle, #fff 0 8%, #8ff0ff 14%, rgb(80 200 255 / 0.5) 30%, transparent 62%);
    opacity: 0;
    animation: plasma 0.7s 1s ease-out forwards;
  }
  @keyframes plasma { 0% { opacity: 0; transform: scale(0.2); } 20% { opacity: 1; } 100% { opacity: 0; transform: scale(1.4); } }
  .gore { position: absolute; z-index: 3; left: 50%; top: 40%; }
  .gore i { position: absolute; width: calc(var(--s) * 1.5px); height: calc(var(--s) * 1.5px); background: #c8161a; opacity: 0; animation: spray 0.75s cubic-bezier(0.3, 0.5, 0.7, 1) both; }
  .gore i.dark { background: #7a0a0c; }
  @keyframes spray {
    0% { transform: translate(0, 0); opacity: 1; }
    45% { transform: translate(calc(var(--dx) * 0.8px), calc(var(--dy) * -0.8px)); opacity: 1; }
    100% { transform: translate(calc(var(--dx) * 1.1px), calc(var(--h) * 0.58)) scaleY(0.5); opacity: 0.85; }
  }
  .pool { position: absolute; z-index: 0; bottom: -4px; width: 110%; height: 7px; border-radius: 50%; background: radial-gradient(closest-side, #8f0d10 70%, #5c0709 92%, transparent); opacity: 0; transform: scaleX(0.1); animation: pool 1.6s 1.6s cubic-bezier(0.16, 1, 0.3, 1) forwards; }
  @keyframes pool { to { opacity: 0.9; transform: scaleX(1); } }
  .dying img, .dying svg, .sheet.taken { animation: gone 0.6s 1.15s steps(5) forwards; }
  @keyframes gone { to { opacity: 0; clip-path: inset(0 0 100% 0); filter: brightness(4) saturate(0); } }
  @media (prefers-reduced-motion: reduce) { img, svg, .sheet i, .gun i { animation: none !important; } }
</style>

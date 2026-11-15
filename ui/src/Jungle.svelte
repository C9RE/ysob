<script>
  // The squad's jungle, in layers: the far moonlit forest (the pack's scene), moonlight shafts,
  // drifting mist, a near layer of mossy trunks and vines drawn here in pixels, the hunter in the
  // trees, the squad (children), fireflies and falling leaves, then swaying foreground foliage.
  import { PK, url } from './lib/packdata.svelte.js';
  import { S } from './lib/store.svelte.js';
  import { scene, sky } from './lib/scene.svelte.js';
  import { sfx } from './lib/sound.js';

  /** @type {{ children?: import('svelte').Snippet, firing?: number, gap?: number }} */
  let { children, firing = 0, gap = 0.5 } = $props();

  const reduced = typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  const still = $derived(reduced || scene.effects === 'off');
  const light = $derived(scene.effects === 'light');

  /** @type {HTMLDivElement} */ let box;
  /** @type {HTMLCanvasElement} */ let mid;
  /** @type {HTMLCanvasElement} */ let top;
  /** @type {HTMLCanvasElement} */ let bottom;

  /** @param {number} a */
  function rng(a) {
    return () => {
      a |= 0;
      a = (a + 0x6d2b79f5) | 0;
      let t = Math.imul(a ^ (a >>> 15), 1 | a);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }
  /** @param {CanvasRenderingContext2D} c @param {number} x @param {number} y @param {number} a @param {number} len @param {number} w */
  function leaf(c, x, y, a, len, w) {
    const dx = Math.cos(a), dy = Math.sin(a);
    for (let i = 0; i <= len; i++) {
      const half = Math.sin((Math.PI * i) / len) * w;
      for (let k = -half; k <= half; k += 1) c.fillRect(Math.round(x + dx * i - dy * k), Math.round(y + dy * i + dx * k), 1, 1);
    }
  }

  const PX = 2; // canvas pixel size, close to the scene art's own scale

  /** Near trunks: slim, mossy, moonlit on one edge, at the sides and one behind the back row. */
  function drawMid() {
    const W = Math.ceil(box.clientWidth / PX) + 12, H = Math.ceil(box.clientHeight / PX);
    mid.width = W; mid.height = H;
    const c = /** @type {CanvasRenderingContext2D} */ (mid.getContext('2d'));
    const r = rng(77);
    c.clearRect(0, 0, W, H);
    [[0.04, 7], [0.63, 4], [0.97, 8]].forEach(([f, tw], i) => {
      const x0 = f * W;
      for (let y = 0; y < H; y++) {
        const bend = Math.sin(y * 0.04 + i * 2) * 1.2;
        const flare = y > H * 0.8 ? ((y - H * 0.8) / (H * 0.2)) ** 2 * 6 : 0;
        const l = Math.round(x0 + bend - tw / 2 - flare), w = Math.round(tw + flare * 2);
        c.fillStyle = '#10241f';
        c.fillRect(l, y, w, 1);
        c.fillStyle = '#2b5048';
        c.fillRect(l + (i === 2 ? 0 : w - 1), y, 1, 1); // moonlit edge toward the clearing
        if (r() < 0.22) { c.fillStyle = '#2a4a2c'; c.fillRect(l + Math.floor(r() * w), y, 1, 1); }
      }
    });
    // a few lianas and hanging vines, thin
    for (let k = 0; k < 3; k++) {
      const a = r() * W, bb = a + (40 + r() * 60) * (r() < 0.5 ? -1 : 1), sag = 8 + r() * 18, y0 = r() * H * 0.18;
      c.fillStyle = '#163029';
      for (let t = 0; t <= 1; t += 0.006) {
        const x = a + (bb - a) * t, y = y0 + Math.sin(Math.PI * t) * sag;
        c.fillRect(Math.round(x), Math.round(y), 1, 1);
        if (r() < 0.04) { c.fillStyle = r() < 0.4 ? '#2e5a3a' : '#1c3a2a'; leaf(c, x, y, Math.PI / 2 + (r() - 0.5) * 1.4, 3 + r() * 3, 1.2); c.fillStyle = '#163029'; }
      }
    }
    for (let k = 0; k < 4; k++) {
      let x = (k < 2 ? r() * 0.25 : 0.75 + r() * 0.25) * W; const len = H * (0.12 + r() * 0.3);
      c.fillStyle = '#163029';
      for (let y = 0; y < len; y++) { x += Math.sin(y * 0.2 + k) * 0.15; c.fillRect(Math.round(x), y, 1, 1); if (r() < 0.1) { c.fillStyle = '#244a33'; leaf(c, x, y, Math.PI / 2 + (r() < 0.5 ? -1 : 1) * 0.8, 3 + r() * 2, 1); c.fillStyle = '#163029'; } }
    }
  }

  /** Foreground: fern clumps in the bottom corners, and a few leaves hanging from the top corners. */
  function drawFront() {
    const W = Math.ceil(box.clientWidth / PX) + 14, H = Math.ceil(box.clientHeight / PX);
    for (const [cv, up] of /** @type {[HTMLCanvasElement, boolean][]} */ ([[top, true], [bottom, false]])) {
      cv.width = W; cv.height = H;
      const c = /** @type {CanvasRenderingContext2D} */ (cv.getContext('2d'));
      const r = rng(up ? 11 : 23);
      c.clearRect(0, 0, W, H);
      if (!up) c.setTransform(1, 0, 0, -1, 0, H);
      const fronds = up ? 6 : 14;
      for (let k = 0; k < fronds; k++) {
        const left = k % 2 === 0;
        const x = left ? r() * W * 0.18 : W - r() * W * 0.18;
        const len = (up ? 10 : 12) + r() * (up ? 16 : 18);
        const a = Math.PI / 2 + (left ? -1 : 1) * (0.25 + r() * 0.55);
        const dark = r() < 0.7;
        // a frond: a stem with leaflets on both sides, thinning to the tip
        for (let i = 0; i < len; i++) {
          const sx = x + Math.cos(a) * i + Math.sin(i * 0.15) * 1.5, sy = Math.sin(a) * i;
          c.fillStyle = dark ? '#071209' : '#0e2215';
          c.fillRect(Math.round(sx), Math.round(sy), 1, 1);
          if (i % 2 === 0 && i > 1) {
            const ll = 2 + (len - i) * 0.16;
            leaf(c, sx, sy, a - 1.15, ll, 0.8);
            leaf(c, sx, sy, a + 1.15, ll, 0.8);
            if (!dark && r() < 0.3) { c.fillStyle = '#1f4a2c'; c.fillRect(Math.round(sx), Math.round(sy), 1, 1); }
          }
        }
      }
      c.setTransform(1, 0, 0, 1, 0, 0);
    }
  }

  $effect(() => {
    const draw = () => { if (box?.clientWidth) { drawMid(); drawFront(); } };
    draw();
    const ro = new ResizeObserver(draw);
    ro.observe(box);
    return () => ro.disconnect();
  });

  // ---------- the hunter in the trees ----------
  // He never walks. He's just there, standing in the trees behind the squad where there's a
  // gap: first only as bent light, then his cloak fails for a split second, then he's gone.
  let hunt = $state(/** @type {null | { x: number, dir: number, phase: 'shimmer' | 'flash' | 'gone', key: number }} */ (null));
  let huntKey = 0;
  function sighting() {
    if (!PK.hunter.anims?.idle || S.mud || hunt) return;
    const key = ++huntKey;
    const x = 0.2 + Math.random() * 0.6;
    hunt = { x, dir: x > 0.5 ? -1 : 1, phase: 'shimmer', key };
    setTimeout(() => { if (hunt?.key === key) { hunt.phase = 'flash'; sfx.cloak(); } }, 2400);
    setTimeout(() => { if (hunt?.key === key) hunt.phase = 'gone'; }, 3300);
    setTimeout(() => { if (hunt?.key === key) hunt = null; }, 4600);
  }
  $effect(() => {
    if (still || scene.hunter === 'never') return;
    const gap = scene.hunter === 'often' ? [14, 28] : [40, 90];
    /** @type {ReturnType<typeof setTimeout>} */ let t;
    const next = () => { t = setTimeout(() => { sighting(); next(); }, (gap[0] + Math.random() * (gap[1] - gap[0])) * 1000); };
    t = setTimeout(() => { sighting(); next(); }, 9000);
    return () => clearTimeout(t);
  });
  // Seen from the settings page: "show me now".
  $effect(() => { if (scene.sightNow) { scene.sightNow = false; sighting(); } });

  // ---------- the sky: time of day and weather ----------
  /** The time of day from this machine's clock, unless the jungle is set to stay at night. */
  const phaseAt = (/** @type {number} */ h) => (h >= 5 && h < 8 ? 'dawn' : h >= 8 && h < 17 ? 'day' : h >= 17 && h < 20.5 ? 'dusk' : 'night');
  $effect(() => {
    if (scene.daylight === 'night') return void (sky.phase = 'night');
    const read = () => { const d = new Date(); sky.phase = phaseAt(d.getHours() + d.getMinutes() / 60); };
    read();
    const t = setInterval(read, 60_000);
    return () => clearInterval(t);
  });

  /** Weather comes and goes: a spell lasts a few minutes, then the next is picked by weight.
   *  Mist belongs to the dark and the dawn; storms are rare. */
  const SPELLS = /** @type {const} */ (['clear', 'mist', 'drizzle', 'rain', 'storm']);
  function nextWeather() {
    const misty = sky.phase === 'dawn' || sky.phase === 'night';
    const w = { clear: 44, mist: misty ? 16 : 3, drizzle: 16, rain: 14, storm: 8 };
    let n = Math.random() * Object.values(w).reduce((a, b) => a + b, 0);
    for (const k of SPELLS) if ((n -= w[k]) < 0) return k;
    return 'clear';
  }
  $effect(() => {
    if (scene.weather === 'clear' || still) return void (sky.weather = 'clear');
    sky.weather = nextWeather();
    /** @type {ReturnType<typeof setTimeout>} */ let t;
    const next = () => { t = setTimeout(() => { sky.weather = nextWeather(); next(); }, (150 + Math.random() * 270) * 1000); };
    next();
    return () => clearTimeout(t);
  });
  // "Bring a storm now", from Settings: a storm for a couple of minutes, then the weather goes on.
  $effect(() => {
    if (!scene.stormNow) return;
    scene.stormNow = false;
    if (still) return;
    sky.weather = 'storm';
  });

  const wet = $derived(!still && (sky.weather === 'drizzle' || sky.weather === 'rain' || sky.weather === 'storm'));
  /** @type {HTMLCanvasElement} */ let rain;
  // Rain in the scene's own chunky pixels: streaks that fall at a slant with the wind, and a
  // little splash where they hit the clearing. Drawn only while it rains and the window shows.
  $effect(() => {
    if (!wet || !rain || !box) return;
    const heavy = sky.weather === 'storm' ? 1 : sky.weather === 'rain' ? 0.6 : 0.22;
    const ctx = /** @type {CanvasRenderingContext2D} */ (rain.getContext('2d'));
    let W = 0, H = 0, ground = 0;
    /** @type {{x: number, y: number, v: number, l: number}[]} */ let drops = [];
    /** @type {{x: number, y: number, t: number}[]} */ let splash = [];
    const wind = sky.weather === 'storm' ? 0.42 : 0.18;
    const size = () => {
      W = rain.width = Math.ceil(box.clientWidth / PX) + 8;
      H = rain.height = Math.ceil(box.clientHeight / PX);
      ground = H - Math.round(30 / PX);
      const n = Math.round(W * (light ? 0.35 : 0.9) * heavy);
      drops = Array.from({ length: n }, () => ({ x: Math.random() * W, y: Math.random() * H, v: 2.2 + Math.random() * 2.2, l: 2 + Math.round(Math.random() * 3) }));
    };
    size();
    const ro = new ResizeObserver(size);
    ro.observe(box);
    let raf = 0;
    const frame = () => {
      ctx.clearRect(0, 0, W, H);
      for (const d of drops) {
        d.y += d.v;
        d.x += d.v * wind;
        if (d.y > ground - Math.random() * 6) {
          if (!light && Math.random() < 0.35) splash.push({ x: d.x, y: ground - Math.random() * 4, t: 0 });
          d.y = -d.l - Math.random() * 20;
          d.x = Math.random() * (W + 20) - 20;
        }
        ctx.fillStyle = d.v > 3.4 ? 'rgba(210,232,245,0.7)' : 'rgba(180,208,228,0.45)';
        for (let k = 0; k < d.l; k++) ctx.fillRect(Math.round(d.x - k * wind), Math.round(d.y - k), 1, 1);
      }
      ctx.fillStyle = 'rgba(200,225,240,0.5)';
      splash = splash.filter((p) => {
        p.t++;
        ctx.fillRect(Math.round(p.x - p.t), Math.round(p.y - (p.t > 1 ? 1 : 0)), 1, 1);
        ctx.fillRect(Math.round(p.x + p.t), Math.round(p.y - (p.t > 1 ? 1 : 0)), 1, 1);
        return p.t < 3;
      });
      raf = requestAnimationFrame(frame);
    };
    const watch = () => { cancelAnimationFrame(raf); if (!document.hidden) raf = requestAnimationFrame(frame); };
    watch();
    document.addEventListener('visibilitychange', watch);
    return () => { cancelAnimationFrame(raf); ro.disconnect(); document.removeEventListener('visibilitychange', watch); ctx.clearRect(0, 0, W, H); };
  });

  /** Lightning in a storm: a double flash across the jungle now and then, thunder after it. */
  let flash = $state(0);
  $effect(() => {
    if (sky.weather !== 'storm' || still || light) return;
    /** @type {ReturnType<typeof setTimeout>[]} */ const ts = [];
    const strike = () => {
      flash++;
      if (!S.mud) ts.push(setTimeout(() => sfx.thunder(), 500 + Math.random() * 1400));
      ts.push(setTimeout(strike, (7 + Math.random() * 16) * 1000));
    };
    ts.push(setTimeout(strike, 2500));
    return () => ts.forEach(clearTimeout);
  });

  const FIREFLIES = Array.from({ length: 14 }, (_, i) => ({ x: (i * 37) % 100, y: 20 + ((i * 53) % 60), d: (i % 7) * 0.9, s: 7 + (i % 5) * 2 }));
  const LEAVES = Array.from({ length: 5 }, (_, i) => ({ x: 10 + i * 19, d: i * 3.1, s: 9 + (i % 3) * 3 }));
</script>

<div class="jungle {sky.phase} w-{sky.weather}" class:still class:light class:thermal={S.thermal} class:firing={firing > 0} bind:this={box} style="--fire:{Math.min(1, firing / 3)}" aria-hidden="true">
  {#if PK.scenes.panel}
    <img class="far pixel" src={url(S.thermal && PK.scenes.thermal ? PK.scenes.thermal : PK.scenes.panel)} alt="" />
  {:else}
    <div class="far plain"></div>
  {/if}
  <div class="skyglow"></div>
  <div class="sun"></div>
  {#if !light}<div class="shafts"><i></i><i></i><i></i></div>{/if}
  <div class="mist back"></div>
  <canvas class="mid pixel" bind:this={mid}></canvas>

  {#if hunt && PK.hunter.anims?.idle}
    {@const idle = PK.hunter.anims.idle}
    {@const a = idle.anchor ?? [0.5, 0.85]}
    {#key hunt.key}
      <div class="hunter {hunt.phase} {hunt.dir < 0 ? 'left' : ''}" style="--x:{hunt.x * 100}%; --n:{idle.frames}; --idle:url({url(idle.sheet)}); --far:url({url(PK.scenes.panel ?? '')}); --cx:{hunt.dir < 0 ? 1 - a[0] : a[0]}; --fy:{a[1]}">
        <span class="wrap"><span class="cloak"></span></span>
        <span class="body pixel"></span>
        <span class="crackle"></span>
        <i class="eye a"></i><i class="eye b"></i>
      </div>
    {/key}
  {/if}

  <div class="firelight"></div>
  <div class="stage">{@render children?.()}</div>

  <canvas class="rain pixel" class:on={wet} bind:this={rain}></canvas>
  <div class="gloom"></div>
  {#key flash}{#if flash}<div class="bolt"></div>{/if}{/key}
  {#if !light && !still && (sky.phase === 'night' || sky.phase === 'dusk') && !wet}
    <div class="flies">{#each FIREFLIES as f, i (i)}<i style="left:{f.x}%; top:{f.y}%; animation-delay:-{f.d}s; animation-duration:{f.s}s"></i>{/each}</div>
    <div class="leaves">{#each LEAVES as l, i (i)}<i style="left:{l.x}%; animation-delay:-{l.d}s; animation-duration:{l.s}s"></i>{/each}</div>
  {/if}
  <div class="mist front"></div>
  <canvas class="front top pixel" bind:this={top}></canvas>
  <canvas class="front bottom pixel" bind:this={bottom}></canvas>
  <div class="shade"></div>
</div>

<!-- The cloak: the forest behind him, bent. -->
<svg width="0" height="0" style="position:absolute">
  <filter id="cloakwarp" x="-20%" y="-20%" width="140%" height="140%">
    <feTurbulence type="turbulence" baseFrequency="0.05 0.12" numOctaves="2" seed="3" result="n">
      <animate attributeName="seed" values="3;9;14;3" dur="0.6s" repeatCount="indefinite" />
    </feTurbulence>
    <feDisplacementMap in="SourceGraphic" in2="n" scale="7" xChannelSelector="R" yChannelSelector="G" />
  </filter>
</svg>

<style>
  .jungle {
    position: relative;
    height: 100%;
    overflow: hidden;
    background: #07100c;
    isolation: isolate;
    --ease: cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .far, .mid, .front, .shafts, .mist, .shade, .firelight, .stage, .flies, .leaves, .hunter { position: absolute; }
  .far {
    inset: -6% -6%;
    width: 112%;
    height: 112%;
    object-fit: cover;
    object-position: 50% 70%;
    filter: saturate(1.15) brightness(0.92);
  }
  .far.plain { background: radial-gradient(80% 60% at 60% 20%, #1d3a32, #07100c 70%); }

  /* Moonlight through the canopy: soft diagonal beams that breathe. */
  .shafts { inset: -10% 0 0; pointer-events: none; mix-blend-mode: screen; }
  .shafts i {
    position: absolute;
    top: -20%;
    width: 18%;
    height: 140%;
    background: linear-gradient(rgb(170 220 210 / 0.16), rgb(170 220 210 / 0.03) 70%, transparent);
    transform: rotate(18deg);
    filter: blur(6px);
    animation: beam 9s ease-in-out infinite;
  }
  .shafts i:nth-child(1) { left: 18%; }
  .shafts i:nth-child(2) { left: 46%; width: 10%; animation-delay: -3s; }
  .shafts i:nth-child(3) { left: 70%; width: 14%; animation-delay: -6s; }
  @keyframes beam { 50% { opacity: 0.45; } }

  .mist { left: -30%; width: 160%; pointer-events: none; }
  .mist.back { bottom: 18%; height: 34%; background: radial-gradient(50% 50% at 50% 50%, rgb(150 200 180 / 0.12), transparent 70%); animation: drift 28s linear infinite alternate; }
  .mist.front { bottom: -6%; height: 26%; background: radial-gradient(50% 60% at 50% 60%, rgb(160 200 185 / 0.1), transparent 70%); animation: drift 19s linear infinite alternate-reverse; z-index: 5; }
  @keyframes drift { from { transform: translateX(-8%); } to { transform: translateX(8%); } }

  .mid {
    inset: 0;
    width: calc(100% + 24px);
    height: 100%;
    left: -12px;
    opacity: 0.92;
  }
  .front {
    left: -14px;
    width: calc(100% + 28px);
    height: 100%;
    z-index: 6;
    pointer-events: none;
  }
  .front.top { top: 0; transform-origin: 50% 0; animation: sway 7s ease-in-out infinite; }
  .front.bottom { bottom: 0; transform-origin: 50% 100%; animation: sway 6s ease-in-out infinite reverse; }
  @keyframes sway { 50% { transform: skewX(var(--sway, 1.6deg)); } }

  /* Gunfire lights the jungle: a warm flicker across everything while anyone is shooting. */
  .firelight {
    inset: 0;
    z-index: 4;
    pointer-events: none;
    mix-blend-mode: screen;
    background: radial-gradient(70% 55% at 50% 78%, rgb(255 170 70 / 0.22), transparent 70%);
    opacity: 0;
  }
  .firing .firelight { animation: flick 0.6s ease-in-out infinite alternate; opacity: calc(0.35 + var(--fire) * 0.4); }
  @keyframes flick { 30% { opacity: calc(0.2 + var(--fire) * 0.3); } 60% { opacity: calc(0.4 + var(--fire) * 0.45); } 100% { opacity: calc(0.25 + var(--fire) * 0.35); } }

  .stage { inset: 0; z-index: 3; }

  /* Fireflies and the odd falling leaf. */
  .flies { inset: 0; z-index: 4; pointer-events: none; }
  .flies i { position: absolute; width: 2px; height: 2px; border-radius: 50%; background: #d8ff9a; box-shadow: 0 0 6px 2px rgb(200 255 120 / 0.55); animation: fly linear infinite; opacity: 0; }
  @keyframes fly {
    0% { transform: translate(0, 0); opacity: 0; }
    15% { opacity: 0.9; }
    40% { transform: translate(14px, -10px); opacity: 0.2; }
    60% { transform: translate(-6px, -18px); opacity: 0.85; }
    100% { transform: translate(10px, -30px); opacity: 0; }
  }
  .leaves { inset: 0; z-index: 4; pointer-events: none; }
  .leaves i { position: absolute; top: -8px; width: 5px; height: 3px; background: #2f5a35; border-radius: 0 3px 0 3px; animation: fall linear infinite; }
  @keyframes fall {
    0% { transform: translate(0, 0) rotate(0); opacity: 0; }
    10% { opacity: 0.9; }
    50% { transform: translate(18px, 130px) rotate(200deg); }
    100% { transform: translate(-6px, 260px) rotate(420deg); opacity: 0.6; }
  }

  .shade { inset: 0; z-index: 7; pointer-events: none; background: linear-gradient(transparent 88%, rgb(17 20 16 / 0.7)), radial-gradient(120% 95% at 50% 45%, transparent 50%, rgb(0 0 0 / 0.5)); }

  /* ---------- the hunter ---------- */
  /* Deep in the forest behind the squad: small for the distance, feet in the undergrowth
     (his measured standing point on the forest floor), above the squad's heads. */
  .hunter {
    --hs: 72px;
    z-index: 2;
    /* his lower legs fade into the undergrowth */
    -webkit-mask-image: linear-gradient(#000 calc(var(--fy) * 100% - 16px), transparent calc(var(--fy) * 100% - 2px));
    mask-image: linear-gradient(#000 calc(var(--fy) * 100% - 16px), transparent calc(var(--fy) * 100% - 2px));
    left: calc(var(--x) - var(--cx) * var(--hs));
    bottom: calc(118px - (1 - var(--fy)) * var(--hs));
    width: var(--hs);
    height: var(--hs);
    pointer-events: none;
  }
  .hunter > * { position: absolute; inset: 0; }
  .hunter.left > .wrap, .hunter.left > .body, .hunter.left > .crackle { scale: -1 1; }
  /* The warp and the edge of bent light go on a wrapper, so the mask doesn't clip them away. */
  .wrap { filter: url(#cloakwarp) drop-shadow(0 0 0.6px rgb(225 255 245 / 0.8)) drop-shadow(0 0 3px rgb(120 230 210 / 0.3)); opacity: 0; }
  /* Cloaked: the forest behind him, bent, inside his outline. He breathes; the light ripples. */
  .cloak {
    position: absolute;
    inset: 0;
    background: var(--far) 40% 60% / 520px auto no-repeat;
    -webkit-mask: var(--idle) 0 0 / calc(var(--n) * var(--hs)) var(--hs) no-repeat;
    mask: var(--idle) 0 0 / calc(var(--n) * var(--hs)) var(--hs) no-repeat;
    filter: brightness(1.28) contrast(1.2) saturate(1.1);
    animation: breathemask 1.6s steps(var(--n)) infinite;
  }
  @keyframes breathemask { to { -webkit-mask-position: calc(var(--n) * var(--hs) * -1) 0; mask-position: calc(var(--n) * var(--hs) * -1) 0; } }
  /* shimmer: fades in to a faint ripple, flickering; then holds while he decloaks; then melts */
  .shimmer .wrap { animation: shimmer 2.4s ease-out forwards; }
  @keyframes shimmer { 0% { opacity: 0; } 30% { opacity: 0.35; } 45% { opacity: 0.12; } 60% { opacity: 0.6; } 72% { opacity: 0.3; } 100% { opacity: 0.75; } }
  .flash .wrap { opacity: 0.4; }
  .gone .wrap { animation: melt 1.3s ease-in forwards; }
  @keyframes melt { 0% { opacity: 0.75; } 30% { opacity: 0.2; } 40% { opacity: 0.55; } 100% { opacity: 0; } }
  /* The cloak fails: he flickers into sight in electric blue, a split second at a time. */
  .body { background: var(--idle) 0 0 / calc(var(--n) * var(--hs)) var(--hs) no-repeat; opacity: 0; }
  .flash .body { animation: decloak 0.9s steps(1) forwards; }
  @keyframes decloak {
    0% { opacity: 0.9; filter: brightness(2.2) hue-rotate(150deg) saturate(2) drop-shadow(0 0 4px #6ff); clip-path: inset(0 0 55% 0); }
    10% { opacity: 0; }
    18% { opacity: 1; filter: brightness(1.4) drop-shadow(0 0 3px #7ff); clip-path: inset(30% 0 0 0); }
    26% { opacity: 1; filter: none; clip-path: none; }
    56% { opacity: 1; filter: none; }
    62% { opacity: 0.3; filter: brightness(2) hue-rotate(150deg) drop-shadow(0 0 5px #6ff); clip-path: inset(0 0 40% 0); }
    68% { opacity: 0.85; clip-path: inset(50% 0 0 0); }
    76% { opacity: 0; }
    100% { opacity: 0; }
  }
  .crackle { opacity: 0; }
  .flash .crackle {
    background:
      linear-gradient(115deg, transparent 46%, #bff 47%, transparent 49%) 20% 30% / 40% 30% no-repeat,
      linear-gradient(60deg, transparent 46%, #9ef 48%, transparent 50%) 60% 60% / 36% 34% no-repeat,
      linear-gradient(160deg, transparent 47%, #dff 48%, transparent 50%) 40% 10% / 30% 40% no-repeat;
    filter: drop-shadow(0 0 3px #5ef);
    animation: crack 0.9s steps(1) forwards;
  }
  @keyframes crack { 0% { opacity: 1; } 12% { opacity: 0; } 18% { opacity: 0.8; transform: scaleY(-1); } 26% { opacity: 0; } 62% { opacity: 1; } 70% { opacity: 0; } }
  .eye { position: absolute; inset: auto; top: 14%; width: 2px; height: 2px; background: #ffe36a; box-shadow: 0 0 5px 2px rgb(255 220 90 / 0.8); opacity: 0; }
  .eye.a { left: 46%; }
  .eye.b { left: 52%; }
  .flash .eye { animation: glint 0.9s steps(1) forwards; }
  @keyframes glint { 30% { opacity: 1; } 50% { opacity: 0; } 54% { opacity: 1; } 60% { opacity: 0; } }

  /* ---------- the sky ---------- */
  /* The pack's forest is painted by moonlight; the time of day relights it. Thermal vision has
     its own picture and is left alone. Changes ease over half a minute, like the real thing. */
  .far, .skyglow, .gloom, .shafts, .mist { transition: filter 30s ease, opacity 6s ease, background 30s ease; }
  .skyglow, .gloom, .bolt, .rain { position: absolute; inset: 0; pointer-events: none; }
  .skyglow { z-index: 1; mix-blend-mode: soft-light; opacity: 0; }
  .jungle:not(.thermal).dawn .far { filter: saturate(1.15) brightness(1.08) sepia(0.18) hue-rotate(-12deg); }
  .jungle:not(.thermal).dawn .skyglow { opacity: 1; background: linear-gradient(rgb(255 175 130 / 0.7), rgb(255 210 160 / 0.25) 45%, transparent 75%); }
  .jungle:not(.thermal).day .far { filter: saturate(1.2) brightness(1.32) sepia(0.22) hue-rotate(-2deg); }
  .jungle:not(.thermal).day .skyglow { opacity: 1; background: linear-gradient(rgb(255 240 190 / 0.75), rgb(255 235 180 / 0.2) 55%, transparent 80%); }
  .jungle:not(.thermal).dusk .far { filter: saturate(1.3) brightness(0.98) sepia(0.45) hue-rotate(-28deg); }
  .jungle:not(.thermal).dusk .skyglow { opacity: 1; background: linear-gradient(rgb(255 110 50 / 0.95), rgb(210 80 110 / 0.45) 50%, transparent 85%); }
  /* Where the moon hangs at night the sun burns by day, low and red at dawn and dusk. */
  .sun { position: absolute; z-index: 1; left: 46%; top: 9%; width: 34%; aspect-ratio: 1; translate: -50% -50%; border-radius: 50%; pointer-events: none; mix-blend-mode: screen; opacity: 0; transition: opacity 30s ease, background 30s ease, top 30s ease; }
  .jungle:not(.thermal).day .sun { opacity: 1; background: radial-gradient(circle, rgb(255 252 230) 0 12%, rgb(255 236 170 / 0.75) 22%, rgb(255 220 140 / 0.25) 45%, transparent 70%); }
  .jungle:not(.thermal).dawn .sun { opacity: 0.9; top: 14%; background: radial-gradient(circle, rgb(255 220 180) 0 10%, rgb(255 170 120 / 0.6) 24%, transparent 65%); }
  .jungle:not(.thermal).dusk .sun { opacity: 0.95; top: 15%; background: radial-gradient(circle, rgb(255 190 120) 0 10%, rgb(255 110 60 / 0.65) 26%, transparent 66%); }
  .w-rain .sun, .w-storm .sun { opacity: 0 !important; }
  /* By day the beams are sunlight, warm; by night, moonlight. */
  .jungle.day .shafts i, .jungle.dawn .shafts i { background: linear-gradient(rgb(255 235 180 / 0.2), rgb(255 230 170 / 0.04) 70%, transparent); }
  .jungle.dusk .shafts i { background: linear-gradient(rgb(255 170 110 / 0.18), rgb(255 150 100 / 0.03) 70%, transparent); }

  /* Weather: rain and cloud darken the jungle; mist thickens; the sun goes in. */
  .gloom { z-index: 5; background: rgb(8 16 22); opacity: 0; }
  .w-drizzle .gloom { opacity: 0.12; }
  .w-rain .gloom { opacity: 0.26; }
  .w-storm .gloom { opacity: 0.42; }
  .w-rain .shafts, .w-storm .shafts, .w-drizzle .shafts { opacity: 0; }
  .w-mist .mist.back { height: 70%; bottom: 8%; background: radial-gradient(55% 50% at 50% 50%, rgb(195 222 212 / 0.42), transparent 72%); }
  .w-mist .mist.front { height: 48%; background: radial-gradient(55% 60% at 50% 60%, rgb(200 225 215 / 0.36), transparent 72%); }
  .w-mist .gloom { background: rgb(175 200 192); opacity: 0.16; }
  .rain { z-index: 5; width: calc(100% + 16px); height: 100%; left: -8px; opacity: 0; transition: opacity 4s ease; }
  .rain.on { opacity: 1; }
  /* The wind gets up in a storm: the foliage sways harder and faster. */
  .w-storm .front.top { animation-duration: 2.4s; }
  .w-storm .front.bottom { animation-duration: 2s; }
  .w-storm .front { --sway: 4deg; }
  /* Lightning: a hard double flash that drains away. Gentle enough not to strobe. */
  .bolt { z-index: 8; background: rgb(225 235 255); opacity: 0; mix-blend-mode: screen; animation: bolt 1.1s ease-out forwards; }
  @keyframes bolt { 0% { opacity: 0.5; } 8% { opacity: 0.08; } 16% { opacity: 0.38; } 100% { opacity: 0; } }

  .still .front, .still .mist, .still .shafts i { animation: none; }
  .light .front { animation: none; }
</style>

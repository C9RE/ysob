<script>
  // In a browser: the same splash as the desktop shell's boot page, held until the engine state
  // and the theme pack are in (and at least a moment, so it never flashes), then it fades away.
  // In the desktop app the shell shows the handshake in a window of its own and keeps this one
  // hidden, so here it's only a plain veil: when the app is ready it tells the shell, the shell
  // brings the window in and marks the page `shown`, and the veil fades to reveal the app.
  import { S } from './lib/store.svelte.js';
  import { PK, url, t } from './lib/packdata.svelte.js';
  import { isDesktop, call, dragArea } from './lib/desktop.js';

  /** @type {{ packReady: boolean }} */
  let { packReady } = $props();
  let held = $state(true);
  let gone = $state(false);
  setTimeout(() => (held = false), 700);
  const done = $derived(!held && S.ready && packReady);
  // Desktop: is the window on screen yet? The shell marks the page `shown` when it is.
  let onScreen = $state(false);
  $effect(() => {
    if (!isDesktop) return;
    const root = document.documentElement;
    const look = () => root.classList.contains('shown') && (onScreen = true);
    look();
    const mo = new MutationObserver(look);
    mo.observe(root, { attributes: true, attributeFilter: ['class'] });
    return () => mo.disconnect();
  });
  $effect(() => {
    if (done && isDesktop) call('app_ready').catch(() => {});
  });
  // Browser: fade as soon as it's done. Desktop: fade once it's done and the window is on screen
  // (or a few seconds after done, if the shell never says).
  let late = $state(false);
  $effect(() => {
    if (!done || !isDesktop) return;
    const t = setTimeout(() => (late = true), 4000);
    return () => clearTimeout(t);
  });
  const out = $derived(done && (!isDesktop || onScreen || late));
  $effect(() => {
    if (out) setTimeout(() => (gone = true), 600);
  });
  // Desktop, window up but the app isn't ready (the engine is slow or failing): the full splash,
  // so the message is seen. Until then it's a plain veil in the app's own background.
  let slow = $state(false);
  $effect(() => {
    if (onScreen && !done) slow = true;
  });
  const veil = $derived(isDesktop && !slow);
</script>

{#if !gone}
  <div class="splash" class:veil class:out role="status" aria-label="Loading" inert={done} {...dragArea}>
    <div class="inner">
      <!-- The picture's box is always there, so the wordmark never moves when it arrives. -->
      <!-- The picture's box is always there and the dots stand in it when a pack has no
           picture, so the wordmark never moves. -->
      <div class="pic">{#if PK.handshake}<img class="shake" src={url(PK.handshake)} alt="" width="208" height="208" />{:else if PK.loaded}<div class="dots" aria-hidden="true"><i></i><i></i><i></i></div>{/if}</div>
      <h1 translate="no">YSOB</h1>
      <p class="tag">{t('tagline')}</p>
      <div class="bar" class:stuck={!!S.error && !S.ready}></div>
      <p class="msg">{S.error && !S.ready ? 'Can’t reach the engine. Trying again…' : t('loading')}</p>
    </div>
  </div>
{/if}

<style>
  .splash {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    place-items: center;
    background:
      radial-gradient(60% 50% at 50% 42%, rgb(40 90 70 / 0.35), transparent 70%),
      radial-gradient(120% 90% at 50% 100%, #0d1a12, #070b08 60%);
    transition: opacity 0.5s ease, filter 0.5s ease;
  }
  .splash.out { opacity: 0; filter: blur(4px); pointer-events: none; }
  /* Desktop: the app's own background over everything until the window is on screen. */
  .splash.veil { background: var(--color-bg); filter: none; transition: opacity 0.55s ease; }
  .veil .inner { visibility: hidden; }
  .inner { display: grid; justify-items: center; }
  @keyframes pulse { 50% { box-shadow: 0 0 26px 7px rgb(255 59 38 / 0.7), 0 0 2px #ff7b66; } }
  .pic { display: grid; place-items: center; width: 208px; height: 208px; }
  /* The arm-wrestle handshake, cut out; the arms run off the original crop, so the edges fade. */
  .shake {
    display: block;
    width: 208px;
    height: 208px;
    image-rendering: pixelated;
    -webkit-mask-image: linear-gradient(#000 60%, transparent 98%), linear-gradient(90deg, transparent, #000 20%, #000 80%, transparent);
    -webkit-mask-composite: source-in;
    mask-image: linear-gradient(#000 60%, transparent 98%), linear-gradient(90deg, transparent, #000 20%, #000 80%, transparent);
    mask-composite: intersect;
    filter: drop-shadow(0 10px 24px rgb(0 0 0 / 0.5));
    animation: grip 2.4s 0.6s ease-in-out infinite;
  }
  @keyframes grip { 50% { transform: translateY(-3px) rotate(-1deg); } }
  .dots { position: relative; width: 30px; height: 26px; margin-top: 2px; }
  .dots i { position: absolute; width: 7px; height: 7px; border-radius: 50%; background: #ff3b26; box-shadow: 0 0 8px 2px rgb(255 59 38 / 0.6); animation: pulse 1.6s ease-in-out infinite; }
  .dots i:nth-child(1) { left: 11px; top: 0; }
  .dots i:nth-child(2) { left: 0; top: 18px; animation-delay: 0.1s; }
  .dots i:nth-child(3) { left: 22px; top: 18px; animation-delay: 0.2s; }
  h1 { margin: 18px 0 0; font: 700 46px/1 var(--font-pixel); letter-spacing: 0.04em; text-shadow: 0 3px 0 #0b120d, 0 0 30px rgb(255 140 60 / 0.18); }
  .tag { margin: 10px 0 0; color: #9ba393; font-size: 13.5px; }
  .bar { position: relative; width: 220px; height: 3px; margin-top: 34px; border-radius: 2px; background: rgb(255 255 255 / 0.07); overflow: hidden; }
  .bar::after { content: ''; position: absolute; inset: 0; width: 40%; border-radius: 2px; background: linear-gradient(90deg, transparent, #7fd68f, transparent); animation: sweep 1.3s ease-in-out infinite; }
  .bar.stuck::after { background: linear-gradient(90deg, transparent, #f4b54a, transparent); }
  @keyframes sweep { from { transform: translateX(-100%); } to { transform: translateX(250%); } }
  .msg { min-height: 1.5em; margin: 14px 0 0; color: #8b9583; font-size: 12.5px; }
  @media (prefers-reduced-motion: reduce) { .shake, .dots i { animation: none; } .bar::after { animation: none; width: 100%; opacity: 0.5; } .splash { transition: none; } }
</style>

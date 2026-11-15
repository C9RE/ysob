<script>
  import { onMount } from 'svelte';
  import { S, start, setThermal, setMud, strike, dismissStrike, talkTo } from './lib/store.svelte.js';
  import { hasToken } from './lib/api.js';
  import { loadPack, PK } from './lib/packdata.svelte.js';
  import { scene, sky } from './lib/scene.svelte.js';
  import { sfx } from './lib/sound.js';
  import Sidebar from './Sidebar.svelte';
  import Chat from './Chat.svelte';
  import Squad from './Squad.svelte';
  import NewMission from './NewMission.svelte';
  import Hire from './Hire.svelte';
  import Settings from './Settings.svelte';
  import Strike from './Strike.svelte';
  import Chopper from './Chopper.svelte';
  import Toasts from './Toasts.svelte';
  import Palette from './Palette.svelte';
  import Splash from './Splash.svelte';
  import Arms from './Arms.svelte';
  import WindowBar from './WindowBar.svelte';
  import { dragArea } from './lib/desktop.js';
  import { initUpdates } from './lib/updates.svelte.js';

  onMount(() => {
    const mq = matchMedia('(max-width: 1020px)');
    const fit = () => {
      S.narrow = mq.matches;
      if (!mq.matches) S.squadPeek = false;
    };
    fit();
    mq.addEventListener('change', fit);
    loadPack();
    if (hasToken()) {
      start();
      initUpdates();
    }
    // The tray menu's actions arrive as an event from the shell (desktop/src-tauri main.rs).
    const menu = (/** @type {Event} */ e) => {
      const what = /** @type {CustomEvent} */ (e).detail;
      if (what === 'updates') {
        S.settingsAt = 'updates';
        S.modal = 'settings';
      } else if (what === 'needs') {
        // The first thing waiting on you: an ask, else a job stalled on a question.
        const n = S.needs[0];
        const crewId = n?.crew_id ?? S.tasks.find((x) => x.col === 'wait')?.crew_id;
        const c = S.crew.find((x) => x.id === crewId);
        if (c) {
          S.modal = null;
          if (S.mission !== c.island_id) S.mission = c.island_id;
          talkTo(c.id);
        }
      }
    };
    window.addEventListener('ysob:menu', menu);
    // ?debug exposes the big moments so they can be checked without a real failure.
    if (new URLSearchParams(location.search).has('debug')) {
      /** @type {any} */ (window).__ysob = { S, strike, setThermal, scene, sky, sfx, PK };
    }
  });

  /** Keyboard: Ctrl/Cmd+K go to, Ctrl/Cmd+N new mission, Ctrl/Cmd+B missions, Ctrl/Cmd+J squad
   *  panel, Ctrl/Cmd+, settings, T thermal, M mud, Esc waves a big moment away. */
  function keys(/** @type {KeyboardEvent} */ e) {
    if (!hasToken()) return;
    const mod = e.metaKey || e.ctrlKey;
    const typing = /** @type {HTMLElement} */ (e.target).closest('input, textarea, select, [contenteditable], .term');
    const k = e.key.toLowerCase();
    if (mod && !e.altKey && k === 'k') {
      e.preventDefault();
      S.modal = S.modal === 'palette' ? null : 'palette';
    } else if (mod && !e.altKey && k === 'n') {
      e.preventDefault();
      S.modal = 'mission';
    } else if (mod && !e.altKey && k === 'j') {
      e.preventDefault();
      if (S.narrow) S.squadPeek = !S.squadPeek;
      else S.squadOpen = !S.squadOpen;
    } else if (mod && !e.altKey && k === 'b') {
      e.preventDefault();
      S.sideOpen = !S.sideOpen;
    } else if (mod && !e.altKey && e.key === ',') {
      e.preventDefault();
      S.modal = S.modal === 'settings' ? null : 'settings';
    } else if (e.key === 'Escape' && S.squadPeek && !S.modal) {
      S.squadPeek = false;
    } else if (e.key === 'Escape' && S.strike && !S.modal) {
      dismissStrike();
    } else if (!typing && !mod && !e.altKey && !S.modal && !e.repeat) {
      if (k === 't') setThermal(!S.thermal);
      if (k === 'm') setMud(!S.mud);
      if (e.key === '?') {
        S.settingsAt = 'keys';
        S.modal = 'settings';
      }
    }
  }
</script>

<svelte:window onkeydown={keys} />

{#if !hasToken()}
  <main class="gate" {...dragArea}>
    <div class="mark"><Arms size={104} /></div>
    <h1>This page needs the engine’s link.</h1>
    <p>Open YSOB from the desktop app. Or, if you started the engine yourself with <code>ysob serve</code>, open the link it printed: it carries the sign-in token.</p>
  </main>
{:else}
  <div class="app" class:thermal={S.thermal} class:mud={S.mud} class:shake={S.strike?.kind === 'hit'} class:nosquad={!S.squadOpen || S.narrow} class:noside={!S.sideOpen} inert={S.modal === 'settings'}>
    {#if S.sideOpen}<Sidebar />{/if}
    <Chat />
    {#if S.squadOpen && !S.narrow}<Squad />{/if}
  </div>
{/if}

{#if hasToken() && S.narrow && S.squadPeek}
  <div class="drawer">
    <button type="button" class="scrim" aria-label="Close the squad" onclick={() => (S.squadPeek = false)}></button>
    <div class="panel"><Squad /></div>
  </div>
{/if}
{#if S.modal === 'mission'}<NewMission />{/if}
{#if S.modal === 'hire'}<Hire />{/if}
{#if S.modal === 'settings'}<Settings />{/if}
{#if S.modal === 'palette'}<Palette />{/if}
{#if hasToken()}<Splash packReady={PK.loaded} />{/if}
<WindowBar />
<Strike />
<Chopper />
<Toasts />

<style>
  .app {
    position: relative;
    display: grid;
    grid-template-columns: 264px minmax(0, 1fr) 352px;
    grid-template-rows: minmax(0, 1fr);
    height: 100vh;
    overflow: hidden;
    transition: filter 0.6s ease;
  }
  .app.nosquad { grid-template-columns: 264px minmax(0, 1fr); }
  .app.noside { grid-template-columns: minmax(0, 1fr) 352px; }
  .app.noside.nosquad { grid-template-columns: minmax(0, 1fr); }
  @media (max-width: 1180px) {
    .app { grid-template-columns: 232px minmax(0, 1fr) 312px; }
    .app.nosquad { grid-template-columns: 232px minmax(0, 1fr); }
    .app.noside { grid-template-columns: minmax(0, 1fr) 312px; }
  }
  /* A narrow window keeps the conversation and the missions; the squad opens as a drawer. */
  @media (max-width: 1020px) {
    .app, .app.nosquad { grid-template-columns: 220px minmax(0, 1fr); }
    .app.noside, .app.noside.nosquad { grid-template-columns: minmax(0, 1fr); }
  }
  .drawer { position: fixed; inset: 0; z-index: 40; }
  .drawer .scrim { position: absolute; inset: 0; width: 100%; border: 0; padding: 0; background: rgb(4 6 4 / 0.55); cursor: default; animation: fade 0.18s ease; }
  .drawer .panel { position: absolute; top: 0; right: 0; bottom: 0; width: min(352px, 88vw); box-shadow: -18px 0 50px rgb(0 0 0 / 0.5); animation: slide 0.22s cubic-bezier(0.2, 0.8, 0.2, 1); }
  @keyframes fade { from { opacity: 0; } }
  @keyframes slide { from { transform: translateX(100%); } }
  @media (prefers-reduced-motion: reduce) { .drawer .scrim, .drawer .panel { animation: none; } }
  /* Thermal vision: the whole world in heat. Faces and text stay readable. */
  .thermal { filter: url(#heat) contrast(1.05); }
  .mud { filter: sepia(0.55) saturate(0.6) brightness(0.86); }
  .shake { animation: shake 0.5s 0.65s steps(2) 2; }
  @keyframes shake { 25% { transform: translate(3px, -2px); } 50% { transform: translate(-3px, 2px); } 75% { transform: translate(2px, 1px); } }
  .gate { display: grid; place-content: center; justify-items: center; gap: 8px; height: 100vh; text-align: center; padding: 16px; }
  .gate h1 { margin: 10px 0 0; font-size: 22px; font-weight: 650; letter-spacing: -0.02em; }
  .gate p { margin: 0; max-width: 46ch; color: var(--color-mute); }
  .gate code { font: 12.5px var(--font-mono); padding: 1px 5px; border-radius: 5px; background: var(--color-raise); color: var(--color-ink); }
</style>

<!-- The thermal palette: luminance mapped through a heat ramp. -->
<svg width="0" height="0" style="position:absolute" aria-hidden="true">
  <filter id="heat" color-interpolation-filters="sRGB">
    <feColorMatrix type="matrix" values="0.33 0.33 0.33 0 0  0.33 0.33 0.33 0 0  0.33 0.33 0.33 0 0  0 0 0 1 0" />
    <feComponentTransfer>
      <feFuncR type="table" tableValues="0.04 0.25 0.75 1 1 1" />
      <feFuncG type="table" tableValues="0.02 0.02 0.12 0.45 0.8 1" />
      <feFuncB type="table" tableValues="0.18 0.45 0.35 0.08 0.15 0.85" />
    </feComponentTransfer>
  </filter>
</svg>

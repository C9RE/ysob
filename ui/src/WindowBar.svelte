<script>
  // Our own window buttons, top right, in place of the native title bar (the shell makes the
  // window frameless). The window is moved from the panels' top rows (`dragArea`). On Windows,
  // resting on maximise brings up Snap Layouts, the same grid the native button shows.
  import { ownFrame, isWindows, call } from './lib/desktop.js';

  let max = $state(false);
  const check = () => call('plugin:window|is_maximized').then((m) => (max = !!m)).catch(() => {});
  $effect(() => {
    if (!ownFrame) return;
    check();
    addEventListener('resize', check);
    return () => removeEventListener('resize', check);
  });

  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let snap;
  const rest = () => isWindows && (snap = setTimeout(() => call('snap_layouts').catch(() => {}), 450));
  const leave = () => clearTimeout(snap);
  const win = (/** @type {string} */ cmd) => () => {
    leave();
    call(`plugin:window|${cmd}`).catch(() => {});
  };
</script>

{#if ownFrame}
  <div class="wb">
    <button type="button" onclick={win('minimize')} aria-label="Minimise" title="Minimise">
      <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" /></svg>
    </button>
    <button type="button" onclick={win('toggle_maximize')} onpointerenter={rest} onpointerleave={leave} aria-label={max ? 'Restore' : 'Maximise'} title={max ? 'Restore' : 'Maximise'}>
      {#if max}
        <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M2.5 2.5V.5h7v7h-2M.5 2.5h7v7h-7z" /></svg>
      {:else}
        <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M.5.5h9v9h-9z" /></svg>
      {/if}
    </button>
    <button type="button" class="x" onclick={win('close')} aria-label="Close" title="Close">
      <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M.5.5l9 9M9.5.5l-9 9" /></svg>
    </button>
  </div>
{/if}

<style>
  .wb { position: fixed; top: 0; right: 0; z-index: 110; display: flex; }
  button {
    display: grid;
    place-items: center;
    width: 46px;
    height: 32px;
    color: rgb(236 238 230 / 0.82);
    transition: background-color 0.1s, color 0.1s;
  }
  button:hover { background: rgb(255 255 255 / 0.09); color: var(--color-ink); }
  button:active { background: rgb(255 255 255 / 0.05); }
  .x:hover { background: #c42b1c; color: #fff; }
  .x:active { background: #a52417; }
  /* Shadowed so the glyphs read over the jungle as well as the dark panels. */
  svg { width: 10px; height: 10px; fill: none; stroke: currentColor; stroke-width: 1; shape-rendering: crispEdges; filter: drop-shadow(0 1px 1.5px rgb(0 0 0 / 0.7)); }
  button:focus-visible { outline-offset: -2px; }
</style>

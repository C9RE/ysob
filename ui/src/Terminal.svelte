<script>
  // A squadmate who runs in a terminal (every engine but Claude Code): their real terminal,
  // live. The engine owns the process and keeps its scrollback; this is a window onto it.
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import '@xterm/xterm/css/xterm.css';
  import { api } from './lib/api.js';
  import { onTerminal } from './lib/store.svelte.js';

  /** @type {{ id: string, name: string }} */
  let { id, name } = $props();
  /** @type {HTMLDivElement} */ let box;
  let error = $state('');
  let ended = $state(false);
  /** Bumped by "Start again": the terminal is rebuilt and pty.open starts the program anew. */
  let runs = $state(0);

  const bytes = (/** @type {string} */ b64) => Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));

  $effect(() => {
    const crew = id;
    void runs;
    error = '';
    ended = false;
    const term = new Terminal({
      fontFamily: "'JetBrains Mono', ui-monospace, monospace",
      fontSize: 13,
      lineHeight: 1.25,
      cursorBlink: true,
      scrollback: 5000,
      theme: { background: '#0b0d0a', foreground: '#eceee6', cursor: '#ff8a1f', cursorAccent: '#0b0d0a', selectionBackground: 'rgba(255, 138, 31, 0.32)' }
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(box);
    const size = () => {
      try {
        fit.fit();
        api('pty.resize', { id: crew, cols: term.cols, rows: term.rows }).catch(() => {});
      } catch {
        /* not laid out yet */
      }
    };
    // Output that arrives while the scrollback loads is kept and written after it.
    /** @type {Uint8Array[] | null} */ let early = [];
    const stop = onTerminal(crew, (ev) => {
      if (ev.end) ended = true;
      else if (ev.data) early ? early.push(bytes(ev.data)) : term.write(bytes(ev.data));
    });
    api('pty.open', { id: crew }).then(
      (r) => {
        if (r.scrollback) term.write(bytes(r.scrollback));
        for (const chunk of early ?? []) term.write(chunk);
        early = null;
        size();
        term.focus();
      },
      (e) => (error = e.message)
    );
    const typed = term.onData((data) => api('pty.input', { id: crew, data }).catch(() => {}));
    const ro = new ResizeObserver(size);
    ro.observe(box);
    // the mono font may land after the first measure: measure again once it has
    document.fonts?.ready.then(size);
    return () => {
      stop();
      typed.dispose();
      ro.disconnect();
      term.dispose();
    };
  });
</script>

<div class="wrap">
  {#if error}
    <p class="note bad" role="alert">{name}’s terminal wouldn’t start: {error} <button type="button" class="again" onclick={() => runs++}>Try again</button></p>
  {:else if ended}
    <p class="note" role="status">{name}’s terminal closed. <button type="button" class="again" onclick={() => runs++}>Start again</button></p>
  {/if}
  <div class="term" bind:this={box} aria-label="{name}'s terminal"></div>
</div>

<style>
  .again { margin-left: 8px; padding: 2px 10px; border-radius: 8px; background: var(--color-raise); color: var(--color-ink); font: inherit; font-weight: 600; }
  .again:hover { background: var(--color-line); }
  .wrap { display: flex; flex-direction: column; flex: 1; min-height: 0; padding: 4px 8px 12px 20px; }
  .term { flex: 1; min-height: 0; }
  .term :global(.xterm) { height: 100%; }
  .term :global(.xterm-viewport) { scrollbar-width: thin; scrollbar-color: rgb(255 255 255 / 0.12) transparent; }
  .note { margin: 0 0 8px; font-size: 12.5px; color: var(--color-mute); }
  .note.bad { color: #ff8b7f; }
</style>

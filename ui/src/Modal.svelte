<script>
  import { tick } from 'svelte';
  /** A dialog over the app. Focus goes to the first field (or `[data-first]`), Tab stays
   *  inside, Esc and a click outside close it, and focus goes back where it was.
   *  @type {{ title: string, onclose: () => void, children: import('svelte').Snippet, width?: number }} */
  let { title, onclose, children, width = 520 } = $props();
  /** @type {HTMLDivElement | undefined} */ let card = $state();
  const id = $props.id();
  const FOCUSABLE = 'a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])';

  $effect(() => {
    const before = /** @type {HTMLElement | null} */ (document.activeElement);
    // after the content has settled (a list may pick its first choice as it mounts)
    tick().then(() => {
      const first = card?.querySelector('[data-first]') ?? card?.querySelector('input:not(:disabled), select:not(:disabled), textarea:not(:disabled)') ?? card?.querySelector('button:not(.x)');
      /** @type {HTMLElement | null | undefined} */ (first)?.focus();
    });
    return () => before?.isConnected && before.focus();
  });

  function keys(/** @type {KeyboardEvent} */ e) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    } else if (e.key === 'Tab' && card) {
      const els = /** @type {HTMLElement[]} */ ([...card.querySelectorAll(FOCUSABLE)]);
      if (!els.length) return;
      const [first, last] = [els[0], els[els.length - 1]];
      if (e.shiftKey && (document.activeElement === first || !card.contains(document.activeElement))) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && (document.activeElement === last || !card.contains(document.activeElement))) {
        e.preventDefault();
        first.focus();
      }
    }
  }
</script>

<svelte:window onkeydown={keys} />

<div class="back" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="card" role="dialog" aria-modal="true" aria-labelledby="{id}-title" bind:this={card} style="--w:{width}px">
    <header>
      <h2 id="{id}-title">{title}</h2>
      <button type="button" class="icon-btn x" onclick={onclose} aria-label="Close">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4.5 4.5l7 7M11.5 4.5l-7 7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
      </button>
    </header>
    {@render children()}
  </div>
</div>

<style>
  .back {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    padding: 16px;
    background: rgb(4 6 4 / 0.7);
    animation: fade 0.18s ease;
  }
  .card {
    width: min(var(--w), 100%);
    max-height: calc(100vh - 32px);
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 20px 22px 22px;
    border-radius: 18px;
    background: var(--color-panel);
    box-shadow: 0 0 0 1px var(--color-line), 0 24px 70px rgb(0 0 0 / 0.6);
    animation: rise 0.22s var(--ease);
  }
  header { display: flex; align-items: center; justify-content: space-between; margin: -4px -6px 12px 0; }
  h2 { margin: 0; font-size: 17px; font-weight: 650; letter-spacing: -0.01em; }
  @keyframes fade { from { opacity: 0; } }
  @keyframes rise { from { opacity: 0; transform: translateY(8px) scale(0.98); } }
</style>

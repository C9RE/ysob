<script>
  import { S, dismissToast } from './lib/store.svelte.js';
</script>

<div class="toasts" aria-live="polite">
  {#each S.toasts as t (t.id)}
    <div class="toast {t.tone}" class:act={!!t.act}>
      <i aria-hidden="true"></i>
      <span>{t.text}</span>
      {#if t.act}
        <button type="button" class="btn small" onclick={() => { t.act?.run(); dismissToast(t.id); }}>{t.act.label}</button>
      {/if}
    </div>
  {/each}
</div>

<style>
  /* Above the box you type in, so a note never sits on top of what you are writing. */
  .toasts { position: fixed; left: 50%; bottom: calc(112px + var(--dock-h, 0px)); z-index: 70; display: flex; flex-direction: column; align-items: center; gap: 8px; width: max-content; max-width: calc(100vw - 32px); transform: translateX(-50%); pointer-events: none; }
  .toast { display: flex; align-items: center; gap: 9px; max-width: min(560px, 100%); padding: 9px 14px; border-radius: 12px; background: #1d2219; box-shadow: 0 0 0 1px var(--color-line), 0 10px 30px rgb(0 0 0 / 0.5); font-size: 13px; animation: up 0.25s var(--ease); }
  .toast.act { padding-right: 8px; pointer-events: auto; }
  .toast span { min-width: 0; overflow-wrap: anywhere; }
  .toast i { flex: none; width: 7px; height: 7px; border-radius: 50%; background: var(--color-mute); }
  .toast.go i { background: var(--color-go); }
  .toast.hit i { background: var(--color-hit); }
  .toast.ask i { background: var(--color-ask); }
  .toast .btn { flex: none; background: var(--color-hover); }
  .toast .btn:hover { background: #2a3024; }
  @keyframes up { from { opacity: 0; transform: translateY(8px); } }
</style>

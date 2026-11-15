<script>
  // The debrief when a job is closed with proof: flies in over the conversation, says what
  // was proven and by whom, and leaves on its own. It never covers the squad or the composer.
  import { S } from './lib/store.svelte.js';
  import { t } from './lib/packdata.svelte.js';
  import Heli from './Heli.svelte';
</script>

{#if S.chopper}
  {#key S.chopper}
    <div class="chop" role="status">
      <Heli scale={0.5} />
      <div class="txt">
        <span class="k">{t('done.kicker')}</span>
        <b title={S.chopper.title}>{S.chopper.title}</b>
        <span class="p" title={S.chopper.proof}>{S.chopper.who}{S.chopper.proof ? ` · ${S.chopper.proof}` : ''}</span>
      </div>
      <button type="button" class="icon-btn" onclick={() => (S.chopper = null)} aria-label="Dismiss">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4.5 4.5l7 7M11.5 4.5l-7 7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
      </button>
    </div>
  {/key}
{/if}

<style>
  .chop {
    position: fixed;
    top: 62px;
    left: 50%;
    translate: -50% 0;
    z-index: 65;
    display: flex;
    align-items: center;
    gap: 14px;
    width: max-content;
    max-width: min(460px, calc(100vw - 32px));
    padding: 10px 10px 10px 14px;
    border-radius: 16px;
    background: linear-gradient(120deg, rgb(127 214 143 / 0.14), transparent 60%), #151a12;
    box-shadow: 0 0 0 1px var(--color-line), 0 18px 50px rgb(0 0 0 / 0.5);
    animation: fly 0.7s cubic-bezier(0.2, 0.9, 0.2, 1);
  }
  @keyframes fly { from { opacity: 0; transform: translate(120px, -30px) rotate(4deg); } }
  .txt { display: flex; flex-direction: column; min-width: 0; }
  .k { font: 600 11px/1.2 var(--font-pixel); letter-spacing: 0.08em; text-transform: uppercase; color: var(--color-go); }
  b { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .p { font-size: 12px; color: var(--color-mute); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>

<script>
  import { S, hire, squadOf } from './lib/store.svelte.js';
  import { LANE_WORDS } from './lib/words.js';
  import { PK } from './lib/packdata.svelte.js';
  import Modal from './Modal.svelte';
  import Avatar from './Avatar.svelte';

  const taken = $derived(new Set(squadOf(S.mission).map((c) => c.name.toLowerCase())));
  const open = $derived(PK.roster.filter((m) => !m.boss && !taken.has(m.name.toLowerCase())));
  const found = $derived(S.engines.filter((e) => e.found));
  /** @type {string | null} */ let pick = $state(null);
  let engine = $state('');
  let busy = $state(false);
  $effect(() => {
    if (!engine || !found.some((e) => e.id === engine)) engine = found.find((e) => e.id === 'claude')?.id ?? found[0]?.id ?? '';
  });
  $effect(() => {
    if (!pick || !open.some((m) => m.name === pick)) pick = open[0]?.name ?? null;
  });
  const chosen = $derived(open.find((m) => m.name === pick));

  async function go() {
    if (!pick || !engine || busy) return;
    busy = true;
    try {
      await hire(pick, engine);
      S.modal = null;
    } catch {
      /* shown as a toast */
    } finally {
      busy = false;
    }
  }
  /** Arrow keys move the choice, the way a radio group does. */
  function keys(/** @type {KeyboardEvent} */ e) {
    const step = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
    if (!step || !open.length) return;
    e.preventDefault();
    const i = open.findIndex((m) => m.name === pick);
    pick = open[(i + step + open.length) % open.length].name;
    requestAnimationFrame(() => /** @type {HTMLElement | null} */ (document.querySelector('.grid [aria-checked="true"]'))?.focus());
  }
</script>

<Modal title="Add to the squad" onclose={() => (S.modal = null)} width={660}>
  {#if open.length === 0}
    <p class="none">The whole squad is already on this mission.</p>
  {:else}
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="grid" role="radiogroup" aria-label="Squad member" onkeydown={keys}>
      {#each open as m (m.name)}
        <button type="button" role="radio" aria-checked={pick === m.name} tabindex={pick === m.name ? 0 : -1} data-first={pick === m.name ? '' : undefined} class:on={pick === m.name} onclick={() => (pick = m.name)} ondblclick={go}>
          <Avatar name={m.name} size={52} />
          <span class="n">{m.name}<span class="lane" class:full={m.lane === 'HOST'}>{LANE_WORDS[m.lane]}</span></span>
          <span class="j">{m.job}</span>
          <span class="d">{m.does}</span>
        </button>
      {/each}
    </div>
    {#if chosen?.lane === 'HOST'}
      <p class="warn">{chosen.name} gets full access: any command, outside the sandbox. Everyone else asks you before leaving their lane.</p>
    {/if}
    <div class="foot">
      {#if found.length}
        <label>
          <span>Runs on</span>
          <select class="field" bind:value={engine}>
            {#each S.engines as e (e.id)}
              <option value={e.id} disabled={!e.found}>{e.name}{!e.found ? ' (not installed)' : e.signed_in === 'none' || e.signed_in === 'key missing' ? ' (not signed in)' : ''}</option>
            {/each}
          </select>
        </label>
      {:else}
        <p class="none">No agent CLI found on this machine. See Settings, Agents.</p>
      {/if}
      <button type="button" class="btn primary" onclick={go} disabled={!pick || !engine || busy}>{busy ? 'Joining…' : `Add ${pick ?? ''}`}</button>
    </div>
  {/if}
</Modal>

<style>
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(196px, 1fr)); gap: 8px; }
  .grid button {
    display: grid;
    grid-template-columns: auto 1fr;
    grid-template-rows: auto auto 1fr;
    column-gap: 12px;
    align-items: start;
    padding: 12px;
    border-radius: 14px;
    background: var(--color-raise);
    text-align: left;
    box-shadow: inset 0 0 0 1.5px transparent;
    transition: background 0.15s var(--ease), box-shadow 0.15s var(--ease);
  }
  .grid button:hover { background: var(--color-hover); }
  .grid button.on { box-shadow: inset 0 0 0 1.5px rgb(255 138 31 / 0.85); background: var(--color-hover); }
  .grid :global(.av) { grid-row: 1 / 4; }
  .n { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; font-weight: 650; }
  .lane { flex: none; font-size: 11px; font-weight: 500; color: var(--color-faint); white-space: nowrap; }
  .lane.full { color: var(--color-ask); font-weight: 600; }
  .j { font-size: 12px; color: var(--color-heat); }
  .d { margin-top: 4px; font-size: 12px; line-height: 1.4; color: var(--color-mute); }
  .warn { margin: 12px 0 0; font-size: 12.5px; line-height: 1.5; color: var(--color-ask); }
  .foot { display: flex; align-items: end; justify-content: space-between; gap: 12px; margin-top: 16px; }
  label { display: flex; flex-direction: column; gap: 6px; }
  label span { font-size: 12.5px; font-weight: 600; color: var(--color-mute); }
  select { min-width: 200px; }
  .none { margin: 0; color: var(--color-mute); }
</style>

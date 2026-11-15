<script>
  import { S, startMission, run } from './lib/store.svelte.js';
  import { bossCard, t } from './lib/packdata.svelte.js';
  import { isDesktop, isWindows, pickFolder } from './lib/desktop.js';
  import Modal from './Modal.svelte';
  import Avatar from './Avatar.svelte';

  const boss = $derived(bossCard());
  let path = $state('');
  let name = $state('');
  let named = $state(false);
  let busy = $state(false);
  let err = $state('');
  const found = $derived(S.engines.filter((e) => e.found));
  let engine = $state('');
  $effect(() => {
    if (!engine || !found.some((e) => e.id === engine)) engine = found.find((e) => e.id === 'claude')?.id ?? found[0]?.id ?? '';
  });
  const picked = $derived(S.engines.find((e) => e.id === engine));
  const signedOut = $derived(picked?.signed_in === 'none' || picked?.signed_in === 'key missing');
  /** The mission takes the folder's name until you type your own. */
  $effect(() => {
    if (!named) name = path.trim().replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? '';
  });

  async function browse() {
    const got = await pickFolder();
    if (got) {
      path = got;
      err = '';
    }
  }
  let scanning = $state(false);
  async function rescan() {
    scanning = true;
    await run('engines.scan').catch(() => {});
    setTimeout(() => (scanning = false), 1500);
  }

  async function go(/** @type {SubmitEvent} */ e) {
    e.preventDefault();
    if (!path.trim() || !name.trim() || !engine || busy) return;
    busy = true;
    err = '';
    try {
      await startMission(name.trim(), path.trim(), engine);
      S.modal = null;
    } catch (x) {
      err = /** @type {Error} */ (x).message;
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="New mission" onclose={() => (S.modal = null)}>
  <form onsubmit={go}>
    <p class="lead"><Avatar name={boss.name} size={30} /> {t('mission.lead', { boss: boss.name })}</p>
    <label>
      <span>Project folder</span>
      <span class="pick">
        <input class="field" bind:value={path} oninput={() => (err = '')} placeholder={isWindows ? 'C:\\Users\\you\\code\\my-app' : '/home/you/code/my-app'} spellcheck="false" autocomplete="off" autocapitalize="off" required aria-invalid={!!err} aria-describedby={err ? 'nm-err' : undefined} />
        {#if isDesktop}<button type="button" class="btn" onclick={browse}>Browse…</button>{/if}
      </span>
    </label>
    <label>
      <span>Mission name</span>
      <input class="field" bind:value={name} oninput={() => (named = true)} placeholder="my-app" maxlength="40" autocomplete="off" required />
    </label>
    {#if found.length}
      <label>
        <span>{boss.name} runs on</span>
        <select class="field" bind:value={engine}>
          {#each S.engines as e (e.id)}
            <option value={e.id} disabled={!e.found}>{e.name}{!e.found ? ' (not installed)' : e.signed_in === 'none' || e.signed_in === 'key missing' ? ' (not signed in)' : ''}</option>
          {/each}
        </select>
        {#if signedOut}<small class="warn">{picked?.name} doesn’t look signed in. Run it once in a terminal and sign in, or the first message will fail.</small>{/if}
      </label>
    {:else}
      <p class="none">No agent CLI found on this machine. Install Claude Code (or Codex, Gemini CLI and others), sign in, then look again. <button type="button" class="btn small" onclick={rescan} disabled={scanning}>{scanning ? 'Looking…' : 'Look again'}</button></p>
    {/if}
    {#if err}<p class="err" id="nm-err" role="alert">{err}</p>{/if}
    <div class="row">
      <button type="button" class="btn" onclick={() => (S.modal = null)}>Cancel</button>
      <button class="btn primary" disabled={busy || !path.trim() || !name.trim() || !engine}>{busy ? t('mission.going') : t('mission.go')}</button>
    </div>
  </form>
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 14px; }
  .lead { display: flex; align-items: center; gap: 10px; margin: 0 0 4px; color: var(--color-mute); }
  label { display: flex; flex-direction: column; gap: 6px; }
  label > span:first-child { font-size: 12.5px; font-weight: 600; color: var(--color-mute); }
  .pick { display: flex; gap: 8px; }
  .pick .field { flex: 1; min-width: 0; font-family: var(--font-mono); font-size: 13px; }
  .pick .btn { height: 40px; }
  small.warn { font-size: 12px; line-height: 1.45; color: var(--color-ask); }
  .none { margin: 0; padding: 12px 14px; border-radius: 11px; background: var(--color-raise); font-size: 13px; line-height: 1.5; color: var(--color-ask); }
  .none .btn { margin-left: 4px; vertical-align: 1px; }
  .err { margin: 0; color: #ff8b7f; font-size: 13px; }
  .row { display: flex; justify-content: flex-end; gap: 8px; margin-top: 6px; }
</style>

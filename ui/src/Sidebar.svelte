<script>
  import { dragArea } from './lib/desktop.js';
  import { S, openMission, squadOf, money, removeMission, setThermal, setMud } from './lib/store.svelte.js';
  import { t } from './lib/packdata.svelte.js';
  import Arms from './Arms.svelte';
  import Avatar from './Avatar.svelte';
  import { U } from './lib/updates.svelte.js';

  const left = $derived(Math.max(0, S.settings.budget - S.spent));
  /** The budget only means something when an agent pays by API key: a subscription's turns
   *  cost nothing extra, so then there is no countdown to show. */
  const paying = $derived(S.spent > 0 || S.crew.some((c) => c.billing === 'api') || (S.detect?.engines ?? []).some((/** @type {any} */ e) => e.id === 'claude' && e.signed_in === 'api key'));
  const k = $derived(S.settings.budget > 0 ? Math.min(1, S.spent / S.settings.budget) : 0);
  /** One line under each mission: who needs you, who is working, or quiet. */
  const status = (/** @type {string} */ id) => {
    const squad = squadOf(id);
    const asking = squad.find((c) => S.needs.some((n) => n.crew_id === c.id && n.kind !== 'breaker'));
    if (asking) return `${asking.name} needs your OK`;
    const held = squad.find((c) => c.status === 'held' || S.needs.some((n) => n.crew_id === c.id && n.kind === 'breaker'));
    if (held) return t('held.short', { name: held.name });
    const busy = squad.filter((c) => c.status === 'working');
    if (busy.length === 1) return `${busy[0].name} is ${busy[0].label}`;
    if (busy.length > 1) return `${busy.length} working`;
    return squad.length ? `${squad.length} in the squad` : 'No squad yet';
  };
  /** Anything on this mission waiting on you, or caught. */
  const alert = (/** @type {string} */ id) => {
    const squad = squadOf(id);
    if (squad.some((c) => S.needs.some((n) => n.crew_id === c.id && n.kind !== 'breaker'))) return 'ask';
    if (squad.some((c) => c.status === 'held' || S.needs.some((n) => n.crew_id === c.id && n.kind === 'breaker'))) return 'hit';
    if (squad.some((c) => c.status === 'working')) return 'go';
    return '';
  };
  /** The mission being removed, while it asks "are you sure". @type {string | null} */
  let confirm = $state(null);
  let removing = $state(false);
  async function drop(/** @type {string} */ id) {
    removing = true;
    await removeMission(id).catch(() => {});
    removing = false;
    confirm = null;
  }
</script>

<nav class="side" aria-label="Missions">
  <div class="brand" {...dragArea}>
    <Arms size={36} />
    <span class="word" translate="no">YSOB</span>
  </div>

  <button type="button" class="new" onclick={() => (S.modal = 'mission')} title="New mission (Ctrl+N)">
    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
    New mission
  </button>

  <h2 class="label">Missions</h2>
  <ul>
    {#each S.missions as m (m.id)}
      {@const squad = squadOf(m.id)}
      {@const a = alert(m.id)}
      <li>
        {#if confirm === m.id}
          <div class="confirm">
            <span>Remove <b>{m.name}</b> and its squad? Your folder isn’t touched.</span>
            <span class="row">
              <button type="button" class="btn danger small" onclick={() => drop(m.id)} disabled={removing}>{removing ? 'Removing…' : 'Remove'}</button>
              <button type="button" class="btn small" onclick={() => (confirm = null)} disabled={removing}>Keep</button>
            </span>
          </div>
        {:else}
          <button type="button" class="mission" class:on={S.mission === m.id} aria-current={S.mission === m.id ? 'true' : undefined} onclick={() => openMission(m.id)}>
            <span class="txt"><span class="name">{m.name}</span><span class="st">{status(m.id)}</span></span>
            <span class="faces">
              {#each squad.slice(0, 4) as c (c.id)}
                <Avatar name={c.name} face={c.face} size={18} />
              {/each}
            </span>
            {#if a}<span class="pip {a}" role="img" aria-label={a === 'ask' ? 'Needs you' : a === 'hit' ? 'Someone is caught' : 'Working'}></span>{/if}
          </button>
          <button type="button" class="x" onclick={() => (confirm = m.id)} aria-label="Remove {m.name}" title="Remove this mission">
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4.5 4.5l7 7M11.5 4.5l-7 7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
          </button>
        {/if}
      </li>
    {:else}
      <li class="empty">No missions yet. A mission is one project folder and the squad working in it.</li>
    {/each}
  </ul>

  <footer>
    {#if U.available || U.phase}
      <!-- A new version stays in view until it's installed, not just for one toast. -->
      <button type="button" class="update" onclick={() => ((S.settingsAt = 'updates'), (S.modal = 'settings'))}>
        <i aria-hidden="true"></i>
        <span>{U.phase === 'downloading' ? `Downloading YSOB ${U.available?.version ?? ''}${U.total ? ` · ${Math.min(100, Math.round((U.done / U.total) * 100))}%` : '…'}` : U.phase === 'installing' ? 'Installing the update…' : `YSOB ${U.available?.version} is ready`}</span>
        {#if !U.phase}<b>See what’s new</b>{/if}
      </button>
    {/if}
    {#if paying}
    <div class="destruct" title="Today’s budget for agents on an API key. {S.settings.hard_stop ? 'At zero the whole squad stops.' : 'At zero you get a warning.'}">
      <div class="row">
        <span>{t('budget')}</span>
        <span class="num">{money(left)} left</span>
      </div>
      <div class="bar" role="progressbar" aria-label="Budget left today" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round((1 - k) * 100)}><span style="width:{(1 - k) * 100}%" class:low={k > 0.8}></span></div>
    </div>
    {/if}
    <div class="row tools">
      <div class="grp">
      <button type="button" class="icon-btn" onclick={() => (S.sound = !S.sound)} aria-pressed={S.sound} aria-label="Sound" title={S.sound ? 'Sound is on' : 'Sound is off'}>
        {#if S.sound}
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 6h2l3-3v10l-3-3H3z" fill="currentColor" /><path d="M10.5 5.5a3.5 3.5 0 0 1 0 5M12 4a5.5 5.5 0 0 1 0 8" stroke="currentColor" fill="none" stroke-width="1.5" stroke-linecap="round" /></svg>
        {:else}
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 6h2l3-3v10l-3-3H3z" fill="currentColor" /><path d="M11 6l3 4M14 6l-3 4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
        {/if}
      </button>
      <button type="button" class="icon-btn" class:on={S.thermal} onclick={() => setThermal(!S.thermal)} aria-pressed={S.thermal} aria-label="Thermal vision" title="Thermal vision (T)">
        <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="2.4" fill="currentColor" /><path d="M1.5 8s2.4-4.5 6.5-4.5S14.5 8 14.5 8 12.1 12.5 8 12.5 1.5 8 1.5 8z" stroke="currentColor" stroke-width="1.5" fill="none" /></svg>
      </button>
      <button type="button" class="icon-btn" class:on={S.mud} onclick={() => setMud(!S.mud)} aria-pressed={S.mud} aria-label="Mud mode" title="Mud mode: quiet, only what needs you (M)">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2 11c2-1.5 3.5-1.5 6 0s4 1.5 6 0M2 7.5c2-1.5 3.5-1.5 6 0s4 1.5 6 0" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" /></svg>
      </button>
      </div>
      {#if !S.link}<span class="link" role="status">Reconnecting…</span>{/if}
      <button type="button" class="icon-btn" onclick={() => (S.modal = 'settings')} title="Settings (Ctrl+,)" aria-label="Settings">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path fill="currentColor" fill-rule="evenodd" d="M6.9 1h2.2l.35 1.8c.5.15.95.35 1.37.62l1.53-1.03 1.56 1.56-1.03 1.53c.27.42.47.88.62 1.37L15 7.2v2.2l-1.8.35c-.15.5-.35.95-.62 1.37l1.03 1.53-1.56 1.56-1.53-1.03c-.42.27-.88.47-1.37.62L8.8 15.6H6.6l-.35-1.8c-.5-.15-.95-.35-1.37-.62l-1.53 1.03-1.56-1.56 1.03-1.53a5 5 0 0 1-.62-1.37L.4 9.4V7.2l1.8-.35c.15-.5.35-.95.62-1.37L1.8 3.95 3.36 2.4l1.53 1.03c.42-.27.88-.47 1.37-.62zM7.7 5.6a2.7 2.7 0 1 0 0 5.4 2.7 2.7 0 0 0 0-5.4z" transform="translate(.3 -.3)" /></svg>
      </button>
    </div>
  </footer>
</nav>

<style>
  .update { display: flex; align-items: center; gap: 8px; width: 100%; margin-bottom: 10px; padding: 8px 10px; border-radius: 9px; background: rgb(127 214 143 / 0.08); font-size: 12.5px; color: var(--color-ink); text-align: left; transition: background 0.15s var(--ease); }
  .update:hover { background: rgb(127 214 143 / 0.14); }
  .update i { flex: none; width: 6px; height: 6px; border-radius: 50%; background: var(--color-go); }
  .update span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .update b { flex: none; font-weight: 550; font-size: 12px; color: var(--color-go); }
  .txt { display: flex; flex-direction: column; min-width: 0; flex: 1; text-align: left; }
  .txt .st { font-size: 12px; color: var(--color-faint); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .side {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--color-panel);
    padding: 14px 10px 12px;
    gap: 6px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 8px 10px 4px;
  }
  .word {
    font: 600 20px/1 var(--font-pixel);
    letter-spacing: 0.06em;
  }
  .new {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 12px;
    border-radius: 10px;
    background: var(--color-raise);
    font-weight: 550;
    transition: background 0.15s var(--ease), scale 0.15s var(--ease);
  }
  .new:hover { background: var(--color-hover); }
  .new:active { scale: 0.98; }
  .new svg { width: 15px; height: 15px; }
  .label {
    margin: 14px 10px 4px;
    line-height: 1.55;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--color-faint);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  ul {
    flex: 1;
    overflow-y: auto;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li { position: relative; }
  .x {
    position: absolute;
    right: 6px;
    top: 50%;
    translate: 0 -50%;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: var(--color-hover);
    color: var(--color-mute);
    opacity: 0;
    transition: opacity 0.12s, color 0.12s;
  }
  li:hover .x, .x:focus-visible { opacity: 1; }
  .x:hover { color: #ff8b7f; }
  .x svg { width: 13px; height: 13px; }
  .confirm { display: grid; gap: 8px; padding: 10px; border-radius: 10px; background: var(--color-raise); font-size: 12.5px; line-height: 1.45; color: var(--color-mute); }
  .confirm b { color: var(--color-ink); font-weight: 600; overflow-wrap: anywhere; }
  .confirm .row { justify-content: flex-start; }
  .mission {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 44px;
    padding: 0 10px;
    border-radius: 10px;
    text-align: left;
    color: var(--color-mute);
    transition: background 0.12s, color 0.12s;
  }
  .mission:hover { background: var(--color-raise); color: var(--color-ink); }
  .mission.on { background: var(--color-hover); color: var(--color-ink); }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .faces { display: flex; }
  .faces :global(.av) { margin-left: -5px; box-shadow: 0 0 0 2px var(--color-panel); }
  .pip {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex: none;
  }
  .pip.go { background: var(--color-go); }
  .pip.ask { background: var(--color-ask); animation: pulse 1.4s infinite; }
  .pip.hit { background: var(--color-hit); animation: pulse 0.9s infinite; }
  @keyframes pulse { 50% { opacity: 0.35; } }
  .empty {
    padding: 8px 10px;
    color: var(--color-faint);
    font-size: 13px;
  }
  footer {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 10px 8px 0;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
    color: var(--color-mute);
  }
  .num { font-variant-numeric: tabular-nums; color: var(--color-ink); }
  .bar {
    height: 4px;
    margin-top: 6px;
    border-radius: 2px;
    background: var(--color-raise);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--color-go);
    transition: width 0.6s var(--ease);
  }
  .bar span.low { background: var(--color-hit); }
  .link { font-size: 12px; color: var(--color-ask); }
  .grp { display: flex; gap: 2px; }
  /* A mode that's on stays lit. */
  .icon-btn.on { background: rgb(255 138 31 / 0.16); color: var(--color-heat); }
  .icon-btn.on:hover { background: rgb(255 138 31 / 0.24); color: var(--color-heat); }
</style>

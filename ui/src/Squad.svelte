<script>
  import { dragArea } from './lib/desktop.js';
  import { S, squadOf, talkTo, remove, bossOf, money } from './lib/store.svelte.js';
  import { STATUS } from './lib/words.js';
  import { PK, member, slugOf, t } from './lib/packdata.svelte.js';
  import Avatar from './Avatar.svelte';
  import Figure from './Figure.svelte';
  import Jungle from './Jungle.svelte';
  import { scene, sky } from './lib/scene.svelte.js';

  const squad = $derived(squadOf(S.mission));
  const boss = $derived(bossOf(S.mission));
  const trophies = $derived(S.tasks.filter((x) => x.island_id === S.mission && x.col === 'done').length);
  const active = $derived(squad.filter((c) => c.status === 'working').length);
  const spent = $derived(squad.reduce((n, c) => n + (c.cost || 0), 0));
  /** @type {string | null} */ let confirm = $state(null);

  // A caught squadmate (the three-strikes hold) shows as caught, not as waiting on an answer.
  const asking = (/** @type {any} */ c) => S.needs.some((n) => n.crew_id === c.id && n.kind !== 'breaker');
  const tone = (/** @type {any} */ c) => (asking(c) ? 'ask' : (STATUS[c.status]?.tone ?? 'idle'));
  const word = (/** @type {any} */ c) => (asking(c) ? 'Needs you' : (STATUS[c.status]?.word ?? c.status));
  const doing = (/** @type {any} */ c) => (asking(c) ? 'Waiting for your answer' : t(/** @type {any} */ (`station.${c.station}`)) || c.label);

  // ---------- the squad moves ----------
  // Everyone keeps their own patch of the clearing, so nobody walks through anybody. Idle,
  // they stand side-on breathing; now and then they look the other way, and when there's
  // room (a small squad) they walk a few steps within their patch and come back. Working, they
  // face outwards and fire.
  const reduced = typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  /** @typedef {{ x: number, dir: number, pose: 'walk' | 'side', target: number, until: number }} Brain */
  /** @type {Record<string, Brain>} */
  let brain = $state({});
  let W = $state(320);
  /** A bigger squad stands a little smaller, so eight still fit the clearing. */
  const fig = $derived(squad.length > 6 ? 84 : squad.length > 4 ? 94 : 110);
  const speed = $derived(fig * 0.32);
  const rand = (/** @type {number} */ a, /** @type {number} */ b) => a + Math.random() * (b - a);
  const slot = (/** @type {number} */ i) => ((i + 1) / (squad.length + 1)) * W;
  /** How far from their spot someone may stroll: half the room to the next person, less a body. */
  const roam = $derived(Math.max(0, W / (squad.length + 1) / 2 - fig * 0.18));
  const busy = (/** @type {any} */ c) => c.status === 'working' || c.status === 'held' || asking(c) || S.dying.includes(c.id) || S.hurt.includes(c.id) || S.win.includes(c.id);
  /** Only a pack with a walk sheet has anyone walking; the rest just look about. */
  const canWalk = (/** @type {any} */ c) => !!PK.crew[slugOf(c)]?.anims?.walk;
  /** The widest gap between squadmates, as a fraction of the width: where the hunter can show. */
  const gap = $derived.by(() => {
    const xs = [0, ...squad.map((_, i) => slot(i)), W].sort((a, b) => a - b);
    let best = 0.5, room = 0;
    for (let i = 1; i < xs.length; i++) if (xs[i] - xs[i - 1] > room) { room = xs[i] - xs[i - 1]; best = (xs[i] + xs[i - 1]) / 2 / (W || 1); }
    return Math.min(0.85, Math.max(0.15, best));
  });

  /** Ease back to their spot, and stop exactly on it so a settled squad costs nothing. */
  function settle(/** @type {Brain} */ b, /** @type {number} */ home, /** @type {number} */ dt) {
    if (b.x === home) return;
    const d = home - b.x;
    b.x = Math.abs(d) < 0.5 ? home : b.x + d * Math.min(1, dt * 4);
  }

  /** Everyone is side-on (the way the art is drawn): turning round is a flip, on the spot. */
  function step(/** @type {number} */ dt, /** @type {number} */ now) {
    const still = !scene.wander || reduced || scene.effects === 'off';
    squad.forEach((c, i) => {
      const home = slot(i);
      let b = brain[c.id];
      // start facing the middle of the clearing, like a squad standing together
      if (!b) b = brain[c.id] = { x: home, dir: home < W / 2 ? 1 : -1, pose: 'side', target: home, until: now + rand(2000, 7000) };
      if (c.status === 'working') {
        // face outwards and shoot into the trees on their side
        const out = home < W / 2 ? -1 : 1;
        if (b.dir !== out) b.dir = out;
        if (b.pose !== 'side') b.pose = 'side';
        settle(b, home, dt);
        b.until = now + 3000;
        return;
      }
      if (busy(c) || still) {
        if (b.pose !== 'side') b.pose = 'side';
        settle(b, home, dt);
        return;
      }
      if (b.pose === 'walk') {
        const d = b.target - b.x;
        const stepPx = Math.sign(d) * speed * dt;
        if (Math.abs(d) <= Math.abs(stepPx)) {
          b.x = b.target;
          b.pose = 'side';
          b.until = now + rand(3000, 8000);
        } else b.x += stepPx;
        return;
      }
      if (now < b.until) return;
      // look the other way now and then, or (with room) stroll within their patch and back
      const away = Math.abs(b.x - home) > 4;
      // In a downpour nobody strolls about: they stay put and only look round.
      const wet = sky.weather === 'rain' || sky.weather === 'storm';
      if (canWalk(c) && roam > 14 && (away || (!wet && Math.random() < 0.5))) {
        b.target = away ? home : Math.max(home - roam, Math.min(home + roam, home + rand(roam * 0.5, roam) * (Math.random() < 0.5 ? -1 : 1)));
        b.dir = b.target > b.x ? 1 : -1;
        b.pose = 'walk';
      } else {
        b.dir = -b.dir;
        b.until = now + rand(3000, 9000);
      }
    });
    // people who left take their spot with them
    for (const id in brain) if (!squad.some((c) => c.id === id)) delete brain[id];
  }

  // The loop only runs while the window is showing; a hidden app does no work.
  $effect(() => {
    let raf = 0;
    let last = performance.now();
    const tick = (/** @type {number} */ now) => {
      step(Math.min(0.1, (now - last) / 1000), now);
      last = now;
      raf = requestAnimationFrame(tick);
    };
    const watch = () => {
      cancelAnimationFrame(raf);
      if (!document.hidden) {
        last = performance.now();
        raf = requestAnimationFrame(tick);
      }
    };
    watch();
    document.addEventListener('visibilitychange', watch);
    return () => {
      cancelAnimationFrame(raf);
      document.removeEventListener('visibilitychange', watch);
    };
  });
</script>

<aside class="squad" aria-label="Squad">
  <div class="scene" {...dragArea}>
    <Jungle firing={active} {gap}>
      <div class="stage" bind:clientWidth={W}>
        {#each squad as c, i (c.id)}
          {@const b = brain[c.id]}
          <div class="spot" class:back={squad.length > 4 && i % 2 === 1} style="left:{b?.x ?? slot(i)}px; z-index:{squad.length > 4 && i % 2 === 1 ? 1 : 2}">
            <Figure name={c.name} face={c.face} height={fig} status={asking(c) ? 'waiting' : c.status} dying={S.dying.includes(c.id)} hurt={S.hurt.includes(c.id)} win={S.win.includes(c.id)} pose={b?.pose ?? 'side'} dir={b?.dir ?? 1} walkMs={130} />
          </div>
        {/each}
      </div>
    </Jungle>
  </div>

  <div class="head">
    <h2>Squad</h2>
    {#if squad.length}
      <span class="tally" title="Working now, jobs closed with proof, and what this squad has spent on API keys">{#if active}<b class="go">{active} working</b>{/if}{#each [trophies ? `${trophies} done` : '', spent > 0 ? money(spent) : ''].filter(Boolean) as part, i (i)}{active || i ? ' · ' : ''}{part}{/each}</span>
    {/if}
    <button type="button" class="add" onclick={() => (S.modal = 'hire')} disabled={!S.mission}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
      Add
    </button>
  </div>

  <ul>
    {#each squad as c (c.id)}
      {@const dying = S.dying.includes(c.id)}
      <li class:dying>
        <button type="button" class="row" class:on={S.talk === c.id} aria-current={S.talk === c.id ? 'true' : undefined} onclick={() => talkTo(c.id)}>
          <Avatar name={c.name} face={c.face} size={38} status={asking(c) ? 'waiting' : c.status} {dying} />
          <span class="txt">
            <span class="n">{c.name}<em>{c.id === boss?.id ? t('boss.job') : (member(c.name)?.job ?? c.role)}</em></span>
            {#if dying || tone(c) !== 'idle'}<span class="s">{dying ? t('remove.dying') : doing(c)}</span>{/if}
          </span>
          {#if tone(c) !== 'idle' && !dying}<span class="chip {tone(c)}">{word(c)}</span>{/if}
        </button>
        {#if c.id !== boss?.id && !dying}
          {#if confirm === c.id}
            <div class="confirm">
              <span>{t('remove.ask', { name: c.name })} Their conversation goes with them.</span>
              <button type="button" class="btn danger small" onclick={() => { confirm = null; remove(c.id); }}>Remove</button>
              <button type="button" class="btn small" onclick={() => (confirm = null)}>Keep</button>
            </div>
          {:else}
            <button type="button" class="x" onclick={() => (confirm = c.id)} aria-label="Remove {c.name}" title="Remove {c.name}">
              <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4.5 4.5l7 7M11.5 4.5l-7 7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
            </button>
          {/if}
        {/if}
      </li>
    {:else}
      <li class="empty">{S.mission ? 'Nobody here yet. Add someone to the squad.' : 'The squad shows up here once a mission starts.'}</li>
    {/each}
  </ul>
</aside>

<style>
  .squad {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--color-panel);
    overflow: hidden;
  }
  .scene {
    position: relative;
    /* Grows with a tall window, so full screen isn't a short strip over an empty panel. */
    height: clamp(252px, 32vh, 400px);
    flex: none;
    overflow: hidden;
  }
  .stage { position: absolute; left: 0; right: 0; bottom: 34px; height: 100%; }
  /* Each member stands at their own spot along the clearing; a big squad stands in two depths. */
  .spot { position: absolute; bottom: 0; translate: -50% 0; }
  .spot.back { bottom: 20px; scale: 0.86; filter: brightness(0.8); }
  /* Names in the scene: on hover, and always for anyone busy or waiting on you. */
  .scene :global(.tag) { opacity: 0; transition: opacity 0.2s; }
  .scene:hover :global(.tag), .scene :global(.work .tag), .scene :global(.ask .tag), .scene :global(.held .tag) { opacity: 1; }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 10px 6px 16px;
  }
  .head h2 { margin: 0; font-size: 11.5px; font-weight: 600; color: var(--color-faint); letter-spacing: 0.04em; text-transform: uppercase; }
  .tally { flex: 1; min-width: 0; font-size: 12px; color: var(--color-faint); font-variant-numeric: tabular-nums; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tally .go { font-weight: 600; color: var(--color-go); }
  .add {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 9px;
    border-radius: 8px;
    margin-left: auto;
    color: var(--color-mute);
    font-size: 12.5px;
    font-weight: 550;
    transition: background 0.15s var(--ease), color 0.15s var(--ease);
  }
  .add:hover { background: var(--color-raise); color: var(--color-ink); }
  .add svg { width: 13px; height: 13px; }
  .add:disabled { opacity: 0.4; }

  ul { flex: 1; overflow-y: auto; margin: 0; padding: 0 8px 12px; list-style: none; }
  li { position: relative; }
  .row {
    display: flex;
    align-items: center;
    gap: 11px;
    width: 100%;
    padding: 8px;
    border-radius: 11px;
    text-align: left;
    transition: background 0.12s;
  }
  .row:hover { background: var(--color-raise); }
  .row.on { background: var(--color-hover); }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .n { font-weight: 600; font-size: 13.5px; }
  .n em { margin-left: 7px; font-style: normal; font-weight: 450; font-size: 12px; color: var(--color-faint); }
  .s { font-size: 12px; color: var(--color-mute); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* Status: a dot and a word, as everywhere else. */
  .chip {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 550;
    color: var(--color-mute);
  }
  .chip::before { content: ''; flex: none; width: 7px; height: 7px; border-radius: 50%; background: currentColor; }
  .chip.go { color: var(--color-go); }
  .chip.ask { color: var(--color-ask); }
  .chip.hit { color: #ff8b7f; }
  li:has(.x):hover .chip, li:has(.x):focus-within .chip { opacity: 0; }
  .x {
    position: absolute;
    right: 10px;
    top: 50%;
    transform: translateY(-50%);
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    color: var(--color-faint);
    opacity: 0;
    transition: opacity 0.12s;
  }
  li:hover .x, li:focus-within .x { opacity: 1; }
  .x:hover { background: rgb(255 75 58 / 0.14); color: #ff8b7f; }
  .x svg { width: 14px; height: 14px; }
  .confirm {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 4px 8px 10px 57px;
    font-size: 12.5px;
    color: var(--color-mute);
  }
  li.dying .row { background: rgb(255 75 58 / 0.08); }
  .empty { padding: 10px 12px; color: var(--color-faint); font-size: 13px; }
</style>

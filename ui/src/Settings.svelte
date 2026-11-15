<script>
  // Settings as a page of its own: a section list on the left, the settings on the right.
  import { untrack } from 'svelte';
  import { S, run, money, setThermal, setMud } from './lib/store.svelte.js';
  import { scene } from './lib/scene.svelte.js';
  import { t } from './lib/packdata.svelte.js';
  import { isMac, isDesktop, dragArea, ownFrame, openLink } from './lib/desktop.js';
  import { DISCORD, SITE } from './lib/links.js';
  import { U, check, install } from './lib/updates.svelte.js';

  const close = () => (S.modal = null);
  let budget = $state(S.settings.budget);
  /** Save a setting. A switch that flipped is put back if the engine didn't take it, so it
   *  never shows a state the app isn't in. @param {string} key @param {unknown} value @param {HTMLInputElement} [box] */
  const set = (key, value, box) =>
    run('settings.set', { key, value }).catch(() => {
      if (box) box.checked = !box.checked;
    });
  /** A budget is a positive number of dollars; anything else goes back to what it was. */
  function saveBudget() {
    const n = Number(budget);
    if (Number.isFinite(n) && n >= 1) set('budget', Math.round(n * 100) / 100);
    else budget = S.settings.budget;
  }

  /** @type {{ crew_id: string, raw: string, name: string, mission: string | null, what: string }[]} */
  let grants = $state([]);
  let loaded = $state(false);
  const load = () => run('grants.list').then((g) => { grants = g; loaded = true; }).catch(() => (loaded = true));
  load();
  const det = $derived(S.detect);
  let scanning = $state(false);
  async function rescan() {
    scanning = true;
    await run('engines.scan').catch(() => {});
    setTimeout(() => (scanning = false), 1500);
  }
  /** One plain status per engine. */
  const engineState = (/** @type {any} */ e) =>
    !e.found ? ['Not installed', 'off'] : e.signed_in === 'account' ? ['Signed in', 'go'] : e.signed_in === 'api key' ? ['API key', 'go'] : e.signed_in === 'key missing' ? ['Key missing', 'ask'] : e.signed_in === 'none' ? ['Not signed in', 'ask'] : ['Installed', 'go'];
  let copied = $state('');
  const copy = (/** @type {string} */ link) => navigator.clipboard?.writeText(link).then(() => { copied = link; setTimeout(() => (copied = ''), 1500); });
  const revoke = (/** @type {{ crew_id: string, raw: string }} */ g) => run('grants.revoke', { crew_id: g.crew_id, raw: g.raw }).catch(() => {}).finally(load);

  const SECTIONS = [
    ['general', 'General'],
    ['jungle', 'Jungle'],
    ['agents', 'Agents'],
    ['permissions', 'Permissions'],
    ['keys', 'Keyboard'],
    ['updates', 'Updates'],
    ['about', 'About']
  ];
  let at = $state('general');
  /** @type {HTMLDivElement} */ let body;
  /** @type {HTMLButtonElement} */ let back;
  function go(/** @type {string} */ id, smooth = true) {
    at = id;
    body.querySelector(`#set-${id}`)?.scrollIntoView({ behavior: smooth && !matchMedia('(prefers-reduced-motion: reduce)').matches ? 'smooth' : 'auto', block: 'start' });
  }
  /** The section list follows the scroll. */
  function spy() {
    const top = body.getBoundingClientRect().top + 80;
    let now = SECTIONS[0][0];
    for (const [id] of SECTIONS) {
      const el = body.querySelector(`#set-${id}`);
      if (el && el.getBoundingClientRect().top <= top) now = id;
    }
    at = now;
  }
  // Opens on the section it was sent to (the update toast), keeps focus inside, gives it back.
  $effect(() => {
    const before = /** @type {HTMLElement | null} */ (document.activeElement);
    // Focus for the keyboard, without a ring for someone who opened Settings with a click.
    back.focus({ focusVisible: false });
    // once, on opening: reading it here must not make this run again when it is cleared
    untrack(() => {
      if (S.settingsAt) {
        go(S.settingsAt, false);
        S.settingsAt = null;
      }
    });
    return () => before?.isConnected && before.focus();
  });
  const found = $derived(det ? det.engines.filter((/** @type {any} */ e) => e.found).length : 0);
  const mod = isMac ? '⌘' : 'Ctrl';
  const KEYS = [
    [`${mod} K`, 'Go to a mission or squadmate, or run an action'],
    [`${mod} N`, 'New mission'],
    [`${mod} B`, 'Show or hide the missions'],
    [`${mod} J`, 'Show or hide the squad'],
    [`${mod} ,`, 'Settings'],
    ['Enter', 'Send. Shift Enter starts a new line'],
    ['↑', 'In an empty message box: bring back what you last sent'],
    [`${mod} V`, 'Paste a picture into a message to Claude Code (or drop one on the box)'],
    ['@', 'At the start of a message: send it to one squadmate'],
    ['Y / N', 'Answer the first ask: allow once, or deny. Always takes a click'],
    ['T', 'Thermal vision'],
    ['M', 'Mud mode: quiet, only what needs you'],
    ['?', 'This list'],
    ['Esc', 'Close']
  ];
  const working = $derived(S.crew.filter((c) => c.status === 'working').length);
  const pct = $derived(U.total ? Math.min(100, Math.round((U.done / U.total) * 100)) : 0);
  const mb = (/** @type {number} */ n) => (n / 1048576).toFixed(1);
  const when = (/** @type {Date} */ d) => d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  /** The day the waiting version came out. The shell sends "2026-11-12 0:08:42.0 +00:00:00". */
  const released = $derived.by(() => {
    const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(U.available?.date ?? '');
    return m ? new Date(Date.UTC(+m[1], +m[2] - 1, +m[3], 12)).toLocaleDateString([], { day: 'numeric', month: 'long', year: 'numeric' }) : '';
  });
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && close()} />

<div class="page" class:framed={ownFrame} role="dialog" aria-modal="true" aria-label="Settings">
  {#if ownFrame}<div class="dragbar" {...dragArea}></div>{/if}
  <nav aria-label="Settings sections" {...dragArea}>
    <button type="button" class="back" bind:this={back} onclick={close}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M10 3.5L5.5 8l4.5 4.5" stroke="currentColor" stroke-width="1.6" fill="none" stroke-linecap="round" stroke-linejoin="round" /></svg>
      Back to the squad
    </button>
    <h1>Settings</h1>
    <ul>
      {#each SECTIONS as [id, label] (id)}
        <li><button type="button" class:on={at === id} aria-current={at === id ? 'true' : undefined} onclick={() => go(id)}>{label}{#if id === 'updates' && U.available}<i class="dot" role="img" aria-label="update ready"></i>{/if}</button></li>
      {/each}
    </ul>
  </nav>

  <div class="body" bind:this={body} onscroll={spy}>
    <section id="set-general" aria-labelledby="h-general">
      <h2 id="h-general">General</h2>
      <div class="row">
        <div><b>{t('budget')} budget</b><span>The most Claude Code agents on an API key may spend in a day. Spent today: {money(S.spent)}. A subscription (Claude Pro or Max) costs nothing extra and isn't counted. Terminal agents (Codex, Gemini CLI and the rest) don't report what they spend, so they aren't counted either.</span></div>
        <label class="money">
          <span aria-hidden="true">$</span>
          <input type="number" min="1" step="1" inputmode="decimal" bind:value={budget} onchange={saveBudget} aria-label="Daily budget in dollars" />
        </label>
      </div>
      <div class="row">
        <div><b>Hard stop</b><span>At zero, stand the whole squad down instead of only warning.</span></div>
        <input type="checkbox" role="switch" class="sw" checked={S.settings.hard_stop} onchange={(e) => set('hard_stop', e.currentTarget.checked, e.currentTarget)} aria-label="Hard stop" />
      </div>
      <div class="row">
        <div><b>Three strikes</b><span>The same failure three times and the hunter takes the shot: the agent stops until you say so.</span></div>
        <input type="checkbox" role="switch" class="sw" checked={S.settings.breaker} onchange={(e) => set('breaker', e.currentTarget.checked, e.currentTarget)} aria-label="Three strikes" />
      </div>
      <div class="row">
        <div><b>Sound</b><span>Squad radio, gunfire, the hunter. Mud mode keeps it quiet.</span></div>
        <input type="checkbox" role="switch" class="sw" bind:checked={S.sound} aria-label="Sound" />
      </div>
      <div class="row" class:dim={!S.sound}>
        <div><b>Volume</b><span class="num">{S.volume}%</span></div>
        <input type="range" min="0" max="100" step="5" bind:value={S.volume} disabled={!S.sound} aria-label="Volume" />
      </div>
      {#if isDesktop}
        <div class="row">
          <div><b>Desktop notifications</b><span>When someone needs you, a job stalls on a question or a job is done with proof, while YSOB is hidden or behind other windows.</span></div>
          <input type="checkbox" role="switch" class="sw" bind:checked={S.notify} aria-label="Desktop notifications" />
        </div>
      {/if}
    </section>

    <section id="set-jungle" aria-labelledby="h-jungle">
      <h2 id="h-jungle">Jungle</h2>
      <p class="lead">How alive the squad’s corner of the screen is.</p>
      <div class="row">
        <div><b>Squad moves about</b><span>Idle squadmates look round, walk a few steps and come back.</span></div>
        <input type="checkbox" role="switch" class="sw" bind:checked={scene.wander} aria-label="Squad moves about" />
      </div>
      <div class="row">
        <div><b>Hunter sightings</b><span>Now and then something moves in the trees.</span></div>
        <div class="seg" role="radiogroup" aria-label="Hunter sightings">
          {#each [['often', 'Often'], ['sometimes', 'Sometimes'], ['never', 'Never']] as [v, l] (v)}
            <button type="button" role="radio" aria-checked={scene.hunter === v} class:on={scene.hunter === v} onclick={() => (scene.hunter = /** @type {any} */ (v))}>{l}</button>
          {/each}
        </div>
      </div>
      <div class="row">
        <div><b>Weather</b><span>Clear spells, mist, rain and the odd storm with lightning, changing every few minutes.</span></div>
        <div class="seg" role="radiogroup" aria-label="Weather">
          {#each [['live', 'Changes'], ['clear', 'Always clear']] as [v, l] (v)}
            <button type="button" role="radio" aria-checked={scene.weather === v} class:on={scene.weather === v} onclick={() => (scene.weather = /** @type {any} */ (v))}>{l}</button>
          {/each}
        </div>
      </div>
      <div class="row">
        <div><b>Time of day</b><span>Dawn, day, dusk and night, from this computer’s clock.</span></div>
        <div class="seg" role="radiogroup" aria-label="Time of day">
          {#each [['clock', 'My clock'], ['night', 'Always night']] as [v, l] (v)}
            <button type="button" role="radio" aria-checked={scene.daylight === v} class:on={scene.daylight === v} onclick={() => (scene.daylight = /** @type {any} */ (v))}>{l}</button>
          {/each}
        </div>
      </div>
      <div class="row">
        <div><b>Effects</b><span>Full: light through the trees, fireflies, rain and lightning, smoke and brass. Light: just the essentials. Off: a still picture, no weather.</span></div>
        <div class="seg" role="radiogroup" aria-label="Effects">
          {#each [['full', 'Full'], ['light', 'Light'], ['off', 'Off']] as [v, l] (v)}
            <button type="button" role="radio" aria-checked={scene.effects === v} class:on={scene.effects === v} onclick={() => (scene.effects = /** @type {any} */ (v))}>{l}</button>
          {/each}
        </div>
      </div>
      <div class="row">
        <div><b>Thermal vision</b><span>See the world in heat. Also on T.</span></div>
        <input type="checkbox" role="switch" class="sw" checked={S.thermal} onchange={(e) => setThermal(e.currentTarget.checked)} aria-label="Thermal vision" />
      </div>
      <div class="row">
        <div><b>Mud mode</b><span>Quiet: no hunter, no big moments, no sound but the radio when someone needs you. Also on M.</span></div>
        <input type="checkbox" role="switch" class="sw" checked={S.mud} onchange={(e) => setMud(e.currentTarget.checked)} aria-label="Mud mode" />
      </div>
      <button type="button" class="btn ghost" onclick={() => { scene.sightNow = true; S.squadOpen = true; if (S.narrow) S.squadPeek = true; close(); }} disabled={scene.effects === 'off' || S.mud}>Watch the trees now</button>
      <button type="button" class="btn ghost" onclick={() => { scene.stormNow = true; S.squadOpen = true; if (S.narrow) S.squadPeek = true; close(); }} disabled={scene.effects === 'off'}>Bring a storm</button>
    </section>

    <section id="set-agents" aria-labelledby="h-agents">
      <div class="h2row">
        <h2 id="h-agents">Agents</h2>
        <button type="button" class="btn" onclick={rescan} disabled={scanning}>{scanning ? 'Looking…' : 'Look again'}</button>
      </div>
      <p class="lead">
        {#if det}{found} of {det.engines.length} agent CLIs found on this machine.{/if}
        Claude Code talks in the chat with every tool call checked. The others run in their own terminal inside the app{S.settings.sandbox_ok ? ', and all of them run in the sandbox' : ''}.
      </p>
      {#if !det}
        <p class="none">Looking for agent CLIs…</p>
      {:else}
        <ul class="list">
          {#each det.engines.filter((e) => e.found) as e (e.id)}
            {@const st = engineState(e)}
            <li>
              <div>
                <b>{e.name}{#if e.version}<span class="ver">{e.version}</span>{/if}</b>
                <span>{e.chat ? 'Chat, every tool call checked' : 'Terminal'}</span>
              </div>
              <span class="pill {st[1]}">{st[0]}</span>
            </li>
          {/each}
        </ul>
        {@const missing = det.engines.filter((e) => !e.found)}
        {#if missing.length}
          <h3>Not installed</h3>
          <p class="sub">Install one, sign in, then Look again.</p>
          <ul class="list">
            {#each missing as e (e.id)}
              <li class="missing">
                <div><b>{e.name}</b><span>{e.site?.replace('https://', '') ?? 'Not installed'}</span></div>
                {#if e.site}<button type="button" class="btn small" onclick={() => copy(e.site)}>{copied === e.site ? 'Copied' : 'Copy link'}</button>{/if}
              </li>
            {/each}
          </ul>
        {/if}
        <h3>API keys</h3>
        <p class="sub">From the environment YSOB started in. Only whether a key is set is shown, never the key.</p>
        <div class="keys">
          {#each det.keys as k (k.provider)}
            <span class="key" class:on={k.set} title="{k.vars.join(' or ')}: {k.set ? 'set' : 'not set'}">{k.provider}<span class="sr">{k.set ? ', set' : ', not set'}</span></span>
          {/each}
        </div>
        <h3>Local models</h3>
        <ul class="list">
          {#each det.local as l (l.name)}
            <li>
              <div><b>{l.name}</b><span>{l.up ? `${l.models.length} model${l.models.length === 1 ? '' : 's'}${l.models.length ? ': ' + l.models.slice(0, 3).join(', ') : ''}` : `Not running on port ${l.port}`}</span></div>
              <span class="pill {l.up ? 'go' : 'off'}">{l.up ? 'Running' : 'Off'}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section id="set-permissions" aria-labelledby="h-permissions">
      <h2 id="h-permissions">Permissions</h2>
      <div class="row">
        <div>
          <b>Sandbox</b>
          {#if S.settings.sandbox_ok}
            <span>Look and edit agents see only their mission folder and their own sign-in: not your home folder, keys or other projects. Full-access agents run outside it. Applies from each agent’s next start.</span>
          {:else}
            <span class="warn">Not available here: {S.settings.sandbox_why}. The guard still checks every tool call, but an agent’s own code can reach what you can.</span>
          {/if}
        </div>
        <input type="checkbox" role="switch" class="sw" checked={S.settings.sandbox && S.settings.sandbox_ok} disabled={!S.settings.sandbox_ok} onchange={(e) => set('sandbox', e.currentTarget.checked ? 'on' : 'off', e.currentTarget)} aria-label="Sandbox" />
      </div>
      <h3>Standing permissions</h3>
      <p class="sub">Everything you said Always to. Take one back and the agent has to ask again next time.</p>
      {#if !loaded}
        <p class="none">Loading…</p>
      {:else if grants.length === 0}
        <p class="none">None. Every ask still comes to you.</p>
      {:else}
        <ul class="list">
          {#each grants as g (g.crew_id + g.raw)}
            <li>
              <div><b class="mono">{g.what}</b><span>{g.name}{g.mission ? ` on ${g.mission}` : ''}</span></div>
              <button type="button" class="btn small danger" onclick={() => revoke(g)}>Take back</button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section id="set-keys" aria-labelledby="h-keys">
      <h2 id="h-keys">Keyboard</h2>
      <dl class="kbd">
        {#each KEYS as [k, what] (k)}
          <div><dt>{#each k.split(' ') as part, i (i)}{#if part === '/'}<span class="or">or</span>{:else}<kbd>{part}</kbd>{/if}{/each}</dt><dd>{what}</dd></div>
        {/each}
      </dl>
    </section>

    <section id="set-updates" aria-labelledby="h-updates">
      <h2 id="h-updates">Updates</h2>
      {#if !U.desktop}
        <p class="lead">Updates come with the desktop app. This is the engine’s page in a browser{S.settings.version ? `, engine version ${S.settings.version}` : ''}.</p>
      {:else if !U.enabled}
        <p class="lead">You’re on version {U.version || '…'}. This build doesn’t update itself: it was built from source without an update address. Get a newer one the same way you got this one.</p>
      {:else}
        <div class="row" aria-live="polite">
          <div>
            {#if U.phase === 'installing'}
              <b>Installing version {U.available?.version}</b>
              <span>YSOB will close and open again on the new version.</span>
            {:else if U.phase === 'downloading'}
              <b>Downloading version {U.available?.version}</b>
              <span class="num">{U.total ? `${mb(U.done)} of ${mb(U.total)} MB` : 'Starting…'}</span>
            {:else if U.available}
              <b>Version {U.available.version} is ready</b>
              <span>You’re on {U.version}.{released ? ` Released ${released}.` : ''}{working ? ` ${working} agent${working === 1 ? ' is' : 's are'} working and will be stopped.` : ''}</span>
            {:else if U.checking}
              <b>You’re on version {U.version || '…'}</b>
              <span>Checking for a newer one…</span>
            {:else if U.checked && !U.error}
              <b>You’re on version {U.version}</b>
              <span>Up to date. Checked at {when(U.checked)}.</span>
            {:else}
              <b>You’re on version {U.version || '…'}</b>
              <span>YSOB checks for a newer one when it starts.</span>
            {/if}
          </div>
          {#if U.available && !U.phase}
            <button type="button" class="btn go" onclick={install}>Install and restart</button>
          {:else if !U.phase}
            <button type="button" class="btn" onclick={() => check()} disabled={U.checking}>{U.checking ? 'Checking…' : 'Check now'}</button>
          {/if}
        </div>
        {#if U.phase}
          <div class="progress" class:busy={U.phase === 'installing' || !U.total} role="progressbar" aria-label={U.phase === 'installing' ? 'Installing' : 'Downloading'} aria-valuemin={0} aria-valuemax={100} aria-valuenow={U.phase === 'installing' || !U.total ? undefined : pct}>
            <i style="width:{U.phase === 'installing' ? 100 : U.total ? pct : 0}%"></i>
          </div>
        {/if}
        {#if U.available?.notes && !U.phase}
          <div class="notes"><b>What’s new in {U.available.version}</b><p>{U.available.notes}</p></div>
        {/if}
        {#if U.error}
          <p class="sub warn" role="alert">{U.error}</p>
        {/if}
      {/if}
    </section>

    <section id="set-about" aria-labelledby="h-about">
      <h2 id="h-about">About</h2>
      <p class="lead">YSOB runs your AI coding agents as a squad, on your machine. No account, no telemetry: your code only goes to the agents you hire.</p>
      <p class="sub">{U.version ? `Version ${U.version}. ` : S.settings.version ? `Engine version ${S.settings.version}. ` : ''}Free and open source, under the MIT licence.</p>
      <p class="sub">A fan-made toy for people who code with AI agents. Not affiliated with, or endorsed by, anyone who made the film.</p>
      <div class="about-links">
        <a class="btn" href={DISCORD} target="_blank" rel="noopener" onclick={(e) => openLink(DISCORD) && e.preventDefault()}>Join the squad on Discord</a>
        <a class="btn ghostlink" href={SITE} target="_blank" rel="noopener" onclick={(e) => openLink(SITE) && e.preventDefault()}>yousonofabitch.wtf</a>
      </div>
      <p class="sub">Help, ideas, bugs and every new version are on the Discord.</p>
    </section>
  </div>
</div>

<style>
  .page {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    grid-template-columns: 248px minmax(0, 1fr);
    background: var(--color-bg);
    animation: in 0.22s cubic-bezier(0.2, 0.8, 0.2, 1) both;
  }
  @keyframes in { from { opacity: 0; transform: translateY(6px); } }
  /* Desktop: a strip along the top moves the window, and the content starts below our window buttons. */
  .dragbar { position: absolute; top: 0; left: 248px; right: 138px; height: 32px; z-index: 1; }
  .framed .body { padding-top: 40px; }
  nav { display: flex; flex-direction: column; gap: 4px; padding: 20px 14px; background: var(--color-panel); }
  .back { display: flex; align-items: center; gap: 6px; height: 34px; padding: 0 10px; border-radius: 9px; color: var(--color-mute); font-size: 13px; align-self: flex-start; }
  .back:hover { background: var(--color-raise); color: var(--color-ink); }
  .back svg { width: 15px; height: 15px; }
  h1 { margin: 22px 10px 12px; font: 700 22px/1 var(--font-pixel); letter-spacing: 0.02em; }
  nav ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  nav li button { display: flex; align-items: center; justify-content: space-between; width: 100%; height: 36px; padding: 0 12px; border-radius: 9px; text-align: left; color: var(--color-mute); font-size: 13.5px; font-weight: 500; transition: background 0.15s var(--ease), color 0.15s var(--ease); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--color-go); }
  nav li button:hover { background: var(--color-raise); color: var(--color-ink); }
  nav li button.on { background: var(--color-hover); color: var(--color-ink); }

  .body { overflow-y: auto; overscroll-behavior: contain; padding: 28px clamp(24px, 5vw, 64px) 0; }
  section { max-width: 760px; margin-inline: auto; padding-top: 12px; margin-bottom: 44px; scroll-margin-top: 16px; }
  /* The last section can scroll to the top like the rest, so the list on the left stays true. */
  section:last-child { min-height: calc(100vh - 60px); margin-bottom: 0; }
  h2 { margin: 0 0 6px; font-size: 18px; font-weight: 650; letter-spacing: -0.01em; }
  .h2row { display: flex; align-items: center; justify-content: space-between; }
  h3 { margin: 26px 0 4px; font-size: 13.5px; font-weight: 600; }
  .lead { margin: 0 0 10px; color: var(--color-mute); font-size: 13.5px; max-width: 60ch; }
  .sub, .none { margin: 0 0 10px; color: var(--color-mute); font-size: 12.5px; max-width: 64ch; }

  .row { display: flex; align-items: center; justify-content: space-between; gap: 24px; padding: 15px 0; }
  .row + .row { border-top: 1px solid var(--color-line); }
  .row b { display: block; font-weight: 600; font-size: 14px; }
  .row span { display: block; margin-top: 2px; font-size: 12.5px; line-height: 1.5; color: var(--color-mute); max-width: 52ch; }
  .row .warn { color: var(--color-ask); }
  .num { font-variant-numeric: tabular-nums; }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  .row.dim { opacity: 0.6; }

  .seg { flex: none; display: flex; padding: 3px; border-radius: 10px; background: var(--color-raise); }
  .seg button { height: 28px; padding: 0 12px; border-radius: 8px; font-size: 12.5px; font-weight: 550; color: var(--color-mute); }
  .seg button:hover { color: var(--color-ink); }
  .seg button.on { background: var(--color-hover); color: var(--color-ink); box-shadow: 0 1px 2px rgb(0 0 0 / 0.4); }

  .btn.ghost { margin-top: 10px; }
  .about-links { display: flex; flex-wrap: wrap; gap: 8px; margin: 14px 0 8px; }
  .about-links .btn { text-decoration: none; }
  .ghostlink { background: transparent; box-shadow: inset 0 0 0 1px var(--color-line); color: var(--color-mute); }
  .btn.ghost + .btn.ghost { margin-left: 8px; }
  .btn.small.danger { background: var(--color-raise); color: var(--color-ink); }
  .btn.small.danger:hover { background: rgb(255 75 58 / 0.16); color: #ff8b7f; }
  .notes { margin-top: 6px; padding: 12px 14px; border-radius: 11px; background: var(--color-panel); }
  .notes b { font-size: 12.5px; font-weight: 600; }
  .notes p { margin: 4px 0 0; color: var(--color-mute); font-size: 13px; white-space: pre-wrap; }
  .progress { position: relative; height: 6px; margin: 2px 0 14px; border-radius: 3px; background: var(--color-raise); overflow: hidden; }
  .progress i { position: absolute; inset: 0 auto 0 0; border-radius: 3px; background: var(--color-go); transition: width 0.25s ease; }
  /* No total yet, or installing: a band that travels, so it never looks stuck. */
  .progress.busy i { width: 30% !important; animation: busy 1.1s ease-in-out infinite alternate; }
  @keyframes busy { from { transform: translateX(0); } to { transform: translateX(234%); } }
  .sub.warn { margin-top: 10px; color: #ff8b7f; overflow-wrap: anywhere; }

  /* Lists read like the rest of the page: divider rows, no boxes. */
  .list { list-style: none; margin: 4px 0 0; padding: 0; display: flex; flex-direction: column; }
  .list li { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 12px 0; }
  .list li + li { border-top: 1px solid var(--color-line); }
  .list b { display: block; font-weight: 600; font-size: 13.5px; }
  .list b.mono { font: 500 12.5px/1.4 var(--font-mono); overflow-wrap: anywhere; }
  .list span { font-size: 12px; color: var(--color-mute); }
  .list li.missing b { color: var(--color-mute); font-weight: 500; }
  .ver { margin-left: 8px; font-size: 11.5px; color: var(--color-faint); font-weight: 400; }
  /* Status is a dot and a word, the same everywhere in the app. */
  .pill { flex: none; display: inline-flex; align-items: center; gap: 7px; font-size: 12.5px; font-weight: 550; color: var(--color-mute); }
  .pill::before { content: ''; width: 7px; height: 7px; border-radius: 50%; background: currentColor; opacity: 0.9; }
  .pill.go { color: var(--color-go); }
  .pill.ask { color: var(--color-ask); }
  .keys { display: flex; flex-wrap: wrap; gap: 6px; }
  .key { padding: 6px 11px; border-radius: 9px; font-size: 12.5px; background: var(--color-panel); color: var(--color-faint); }
  .key.on { color: var(--color-ink); background: rgb(127 214 143 / 0.1); }
  .key.on::before { content: ''; display: inline-block; width: 6px; height: 6px; margin-right: 7px; border-radius: 50%; background: var(--color-go); vertical-align: 1px; }

  .money { display: flex; flex: none; align-items: center; gap: 4px; padding: 0 12px; height: 36px; border-radius: 10px; background: var(--color-raise); transition: box-shadow 0.15s var(--ease); }
  .money:focus-within { box-shadow: inset 0 0 0 1.5px rgb(255 138 31 / 0.85); }
  .money span { margin: 0; color: var(--color-mute); }
  .money input { width: 64px; border: 0; background: transparent; color: var(--color-ink); font: inherit; font-variant-numeric: tabular-nums; outline: none; }
  input[type='range'] { flex: none; width: 160px; accent-color: var(--color-go); }
  .sw { appearance: none; flex: none; width: 40px; height: 24px; margin: 0; border-radius: 12px; background: #2c3327; position: relative; transition: background 0.2s; cursor: pointer; }
  .sw::after { content: ''; position: absolute; left: 3px; top: 3px; width: 18px; height: 18px; border-radius: 50%; background: var(--color-mute); transition: transform 0.2s cubic-bezier(0.2, 0.8, 0.2, 1), background 0.2s; }
  .sw:checked { background: rgb(127 214 143 / 0.35); }
  .sw:checked::after { transform: translateX(16px); background: var(--color-go); }
  .sw:disabled { opacity: 0.4; cursor: default; }

  .kbd { margin: 8px 0 0; display: grid; gap: 2px; }
  .kbd div { display: flex; align-items: center; gap: 16px; padding: 8px 0; }
  .kbd dt { flex: none; width: 110px; display: flex; gap: 4px; align-items: center; }
  .kbd dd { margin: 0; color: var(--color-mute); font-size: 13px; }
  kbd { min-width: 22px; padding: 2px 7px; background: var(--color-raise); font-size: 11.5px; color: var(--color-ink); }
  .or { font-size: 11px; color: var(--color-faint); }

  @media (max-width: 760px) {
    .page { grid-template-columns: 1fr; grid-template-rows: auto 1fr; }
    nav { padding: 12px; }
    nav ul { flex-direction: row; overflow-x: auto; }
    nav li button { white-space: nowrap; }
    h1 { display: none; }
  }
</style>

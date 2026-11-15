<script>
  // Ctrl+K: go to a mission or a squadmate, or run one of the app's actions, by typing.
  import { S, openMission, talkTo, missionOf, crewOf, setThermal, setMud, conversationMarkdown, toast } from './lib/store.svelte.js';
  import { copyText } from './lib/clock.js';
  import { DISCORD } from './lib/links.js';
  import { member } from './lib/packdata.svelte.js';
  import { isMac, openLink } from './lib/desktop.js';
  import Avatar from './Avatar.svelte';

  let q = $state('');
  let at = $state(0);
  /** @type {HTMLInputElement | undefined} */ let input = $state();
  /** @type {HTMLUListElement | undefined} */ let list = $state();
  const mod = isMac ? '⌘' : 'Ctrl';
  $effect(() => {
    const before = /** @type {HTMLElement | null} */ (document.activeElement);
    input?.focus();
    return () => before?.isConnected && before.focus();
  });

  /** Missions, every squadmate and the actions, filtered by what you type. */
  const items = $derived.by(() => {
    const act = (/** @type {string} */ key, /** @type {string} */ label, /** @type {string} */ sub, /** @type {() => void} */ go) => ({ key, kind: 'Action', label, sub, go, crew: null });
    const all = [
      ...S.missions.map((m) => ({ key: 'm' + m.id, kind: 'Mission', label: m.name, sub: m.path, go: () => openMission(m.id), crew: null })),
      ...S.crew.map((c) => ({ key: 'c' + c.id, kind: missionOf(c.island_id)?.name ?? '', label: c.name, sub: member(c.name)?.job ?? c.role, go: () => { S.mission = c.island_id; talkTo(c.id); }, crew: c })),
      act('new', 'New mission', `${mod} N`, () => (S.modal = 'mission')),
      ...(S.mission ? [act('hire', 'Add to the squad', '', () => (S.modal = 'hire'))] : []),
      act('settings', 'Settings', `${mod} ,`, () => (S.modal = 'settings')),
      act('thermal', S.thermal ? 'Thermal vision off' : 'Thermal vision on', 'T', () => setThermal(!S.thermal)),
      act('mud', S.mud ? 'Mud mode off' : 'Mud mode on', 'M', () => setMud(!S.mud)),
      act('sound', S.sound ? 'Sound off' : 'Sound on', '', () => (S.sound = !S.sound)),
      ...(S.talk && S.msgs[S.talk]?.length ? [act('copy-md', `Copy the conversation with ${crewOf(S.talk)?.name ?? 'them'} as Markdown`, '', async () => ((await copyText(conversationMarkdown(S.talk ?? ''))) ? toast('Conversation copied as Markdown', 'go') : toast('Couldn’t copy', 'hit')))] : []),
      act('keys', 'Keyboard shortcuts', '?', () => ((S.settingsAt = 'keys'), (S.modal = 'settings'))),
      act('discord', 'Join the YSOB Discord', 'Help, ideas, bugs, new versions', () => { if (!openLink(DISCORD)) window.open(DISCORD, '_blank', 'noopener'); })
    ];
    const words = q.trim().toLowerCase().split(/\s+/).filter(Boolean);
    return words.length ? all.filter((i) => words.every((w) => `${i.label} ${i.kind} ${i.sub}`.toLowerCase().includes(w))) : all;
  });
  $effect(() => {
    q;
    at = 0;
  });
  $effect(() => {
    at;
    list?.querySelector('.on')?.scrollIntoView({ block: 'nearest' });
  });
  function pick(/** @type {number} */ i) {
    const it = items[i];
    if (!it) return;
    S.modal = null;
    it.go();
  }
  function key(/** @type {KeyboardEvent} */ e) {
    if (e.key === 'ArrowDown') { e.preventDefault(); at = Math.min(items.length - 1, at + 1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); at = Math.max(0, at - 1); }
    else if (e.key === 'Enter') { e.preventDefault(); pick(at); }
    else if (e.key === 'Escape' || e.key === 'Tab') { e.preventDefault(); S.modal = null; }
  }
</script>

<div class="back" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && (S.modal = null)}>
  <div class="pal" role="dialog" aria-modal="true" aria-label="Go to">
    <input bind:this={input} bind:value={q} onkeydown={key} placeholder="Go to a mission or squadmate, or do something…" role="combobox" aria-expanded="true" aria-controls="pal-list" aria-activedescendant={items[at] ? `pal-${items[at].key}` : undefined} aria-label="Go to" autocomplete="off" spellcheck="false" />
    <ul role="listbox" id="pal-list" bind:this={list}>
      {#each items as it, i (it.key)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li role="option" id="pal-{it.key}" aria-selected={i === at} class:on={i === at} onpointermove={() => (at = i)} onclick={() => pick(i)}>
          {#if it.crew}<Avatar name={it.crew.name} face={it.crew.face} size={24} status={it.crew.status} />{:else}<span class="ic" aria-hidden="true">{#if it.kind === 'Mission'}<svg viewBox="0 0 16 16"><path d="M2 5.5V12a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V6.5a1 1 0 0 0-1-1H8L6.5 4H3a1 1 0 0 0-1 1z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" /></svg>{:else}<svg viewBox="0 0 16 16"><path d="M6 3.5 10.5 8 6 12.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>{/if}</span>{/if}
          <span class="l">{it.label}</span>
          <span class="s">{it.kind === 'Action' ? '' : it.sub}</span>
          <span class="k">{it.kind === 'Action' ? it.sub : it.kind}</span>
        </li>
      {:else}
        <li class="none">Nothing matches “{q.trim()}”.</li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .back { position: fixed; inset: 0; z-index: 60; display: grid; justify-items: center; align-items: start; padding: 14vh 16px 16px; background: rgb(4 6 4 / 0.6); animation: fade 0.15s ease; }
  .pal { width: min(580px, 100%); border-radius: 16px; background: var(--color-panel); box-shadow: 0 0 0 1px var(--color-line), 0 24px 70px rgb(0 0 0 / 0.6); overflow: hidden; animation: rise 0.18s var(--ease); }
  input { width: 100%; height: 52px; padding: 0 18px; border: 0; outline: 0; background: var(--color-raise); color: var(--color-ink); font: inherit; font-size: 15px; }
  input::placeholder { color: var(--color-faint); }
  ul { list-style: none; margin: 0; padding: 6px; max-height: 50vh; overflow-y: auto; overscroll-behavior: contain; }
  li { display: flex; align-items: center; gap: 10px; height: 40px; padding: 0 10px; border-radius: 10px; cursor: pointer; }
  li.on { background: var(--color-hover); }
  .ic { display: grid; place-items: center; width: 24px; color: var(--color-faint); }
  .ic svg { width: 15px; height: 15px; }
  .l { font-weight: 600; white-space: nowrap; }
  .s { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12.5px; color: var(--color-faint); }
  .k { flex: none; font-size: 11.5px; color: var(--color-mute); }
  .none { height: auto; padding: 14px; color: var(--color-faint); cursor: default; }
  @keyframes rise { from { opacity: 0; transform: translateY(-6px); } }
  @keyframes fade { from { opacity: 0; } }
</style>

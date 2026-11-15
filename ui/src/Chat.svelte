<script>
  import { tick } from 'svelte';
  import { S, crewOf, missionOf, squadOf, talkTo, say, answer, bossOf, run, toast, engineName, money, enqueue, unqueue, queueUp, sendQueued, TESTY, claims } from './lib/store.svelte.js';
  import { formatDuration, ticking, copyText as copy } from './lib/clock.js';
  import { LANE_WORDS } from './lib/words.js';
  import { member, t, bossCard } from './lib/packdata.svelte.js';
  import Arms from './Arms.svelte';
  import Avatar from './Avatar.svelte';
  import { md } from './lib/md.js';
  import { openLink, ownFrame, dragArea } from './lib/desktop.js';

  const mission = $derived(missionOf(S.mission));
  const who = $derived(crewOf(S.talk));
  const isBoss = $derived(!!who && who.id === bossOf(S.mission)?.id);
  /** Claude Code talks in the chat; every other engine runs in its own terminal. */
  const inTerminal = $derived(!!who && who.engine !== 'claude');
  const msgs = $derived((S.talk && S.msgs[S.talk]) || []);
  const squad = $derived(squadOf(S.mission));
  /** Asks on this mission: this conversation's first, then everyone else's. */
  const mine = $derived(S.needs.filter((n) => n.crew_id === S.talk));
  const others = $derived(S.needs.filter((n) => n.crew_id !== S.talk && squad.some((c) => c.id === n.crew_id)));
  /** Asks and stalled jobs on other missions: one line each, so nothing waits unseen. Y and N
   *  never reach these; they take a click. */
  const away = $derived(S.needs.filter((n) => !squad.some((c) => c.id === n.crew_id)));
  const awayJobs = $derived(S.tasks.filter((x) => x.col === 'wait' && x.island_id !== S.mission));
  /** Everything else waiting: one row each, and past the first they fold into "N more", so the
   *  conversation keeps most of the screen. */
  const extra = $derived([...others.map((n) => ({ key: `o${n.id}`, n, away: false })), ...away.map((n) => ({ key: `a${n.id}`, n, away: true })), ...awayJobs.map((w) => ({ key: `j${w.id}`, w, away: true }))]);
  let allExtra = $state(false);
  const shownExtra = $derived(allExtra || extra.length <= 2 ? extra : extra.slice(0, 1));
  function goTo(/** @type {string | null | undefined} */ crewId) {
    const c = crewOf(crewId ?? null);
    if (!c) return;
    S.mission = c.island_id;
    talkTo(c.id);
  }
  /** Commands that can't be taken back or leave this machine. Y never approves one of these:
   *  it takes a click on the card that shows exactly what it is. */
  const RISKY = /\b(git\s+push|push\s+--force|reset\s+--hard|rm\s+-|rm\s+\S|rmdir|del\s|remove-item|rd\s+\/s|drop\s+(table|database)|truncate|deploy|publish|npm\s+publish|wrangler|kubectl|terraform\s+(apply|destroy)|sudo|chmod|chown|mkfs|dd\s+if=|curl[^|]*\|\s*(sh|bash)|format\s)/i;
  const risky = (/** @type {any} */ n) => n.kind !== 'breaker' && RISKY.test(n.title ?? '');
  /** The ask the Y and N keys answer: the first one on screen that is a yes-or-no, and not risky. */
  const keyed = $derived([...mine, ...others].find((x) => x.kind !== 'breaker' && !risky(x)));
  /** Follow new messages only while the reader is at the bottom. */
  let pinned = $state(true);

  let text = $state('');
  let sending = $state(false);
  /** Pictures waiting to go with the next message. @type {{type: string, data: string, url: string, name: string}[]} */
  let pics = $state([]);
  let dragging = $state(false);
  /** Read a picture for Claude: big ones are scaled to 2000 px on the long side as JPEG, so a
   *  phone photo fits the 5 MB the API takes. */
  async function addPics(/** @type {Iterable<File>} */ files) {
    for (const f of files) {
      if (!f.type.startsWith('image/')) continue;
      if (pics.length >= 5) {
        toast('Five pictures at most in one message.', 'ask');
        break;
      }
      try {
        pics = [...pics, await readPic(f)];
      } catch {
        toast(`Couldn’t read ${f.name || 'that picture'}.`, 'hit');
      }
    }
  }
  async function readPic(/** @type {File} */ f) {
    const OK = ['image/png', 'image/jpeg', 'image/gif', 'image/webp'];
    const url = URL.createObjectURL(f);
    let blob = /** @type {Blob} */ (f);
    let type = f.type;
    const img = new Image();
    img.src = url;
    await img.decode();
    const big = Math.max(img.naturalWidth, img.naturalHeight);
    if (!OK.includes(type) || f.size > 3_500_000 || big > 2000) {
      const k = Math.min(1, 2000 / big);
      const cv = document.createElement('canvas');
      cv.width = Math.round(img.naturalWidth * k);
      cv.height = Math.round(img.naturalHeight * k);
      cv.getContext('2d')?.drawImage(img, 0, 0, cv.width, cv.height);
      blob = await new Promise((r, x) => cv.toBlob((b) => (b ? r(b) : x(new Error('encode'))), 'image/jpeg', 0.85));
      type = 'image/jpeg';
    }
    /** @type {string} */
    const data = await new Promise((r, x) => {
      const fr = new FileReader();
      fr.onload = () => r(String(fr.result).split(',')[1] ?? '');
      fr.onerror = x;
      fr.readAsDataURL(blob);
    });
    return { type, data, url, name: f.name || 'pasted picture' };
  }
  function dropPic(/** @type {number} */ i) {
    URL.revokeObjectURL(pics[i].url);
    pics = pics.filter((_, j) => j !== i);
  }
  function paste(/** @type {ClipboardEvent} */ e) {
    const files = [...(e.clipboardData?.files ?? [])].filter((f) => f.type.startsWith('image/'));
    if (!files.length) return;
    e.preventDefault();
    addPics(files);
  }
  function drop(/** @type {DragEvent} */ e) {
    dragging = false;
    const files = [...(e.dataTransfer?.files ?? [])];
    if (!files.some((f) => f.type.startsWith('image/'))) return;
    e.preventDefault();
    addPics(files);
  }
  /** @type {HTMLInputElement | undefined} */ let picker = $state();
  /** @type {HTMLElement | undefined} */ let box = $state();
  /** @type {HTMLTextAreaElement | undefined} */ let ta = $state();
  /** @type {Set<number>} */ let open = $state(new Set());

  /** Consecutive tool calls fold into one activity block. */
  const blocks = $derived.by(() => {
    /** @type {{kind: string, items: any[], key: number}[]} */
    const out = [];
    for (const [i, m] of msgs.entries()) {
      const last = out.at(-1);
      if (m.kind === 'tool' && !(m.verb || m.name)) continue;
      // Claude Code says a failure twice, as its reply and as the error: show it once, as the error.
      if (m.kind === 'text') {
        const next = msgs.slice(i + 1).find((x) => x.kind !== 'turn');
        if (next?.kind === 'error' && next.text?.trim() === m.text?.trim()) continue;
      }
      if (m.kind === 'tool' && last?.kind === 'tool') last.items.push(m);
      else out.push({ kind: m.kind, items: [m], key: m.id ?? out.length });
    }
    return out;
  });

  const bottom = () => box && (box.scrollTop = box.scrollHeight);
  $effect(() => {
    S.talk;
    pinned = true;
  });
  $effect(() => {
    msgs.length;
    S.talk;
    if (pinned) tick().then(bottom);
  });
  /** Content can grow without a new message (a tool line resolves, a long reply renders):
   *  keep following it while the reader is at the bottom. */
  $effect(() => {
    const col = box?.firstElementChild;
    if (!col) return;
    const ro = new ResizeObserver(() => {
      if (box) viewH = box.clientHeight;
      if (pinned) bottom();
    });
    ro.observe(col);
    if (box) ro.observe(box);
    return () => ro.disconnect();
  });
  /** The view's own height changes when an ask docks under it; that is not the reader
   *  scrolling away, so the pin holds and the view follows. */
  let viewH = 0;
  function scrolled() {
    if (!box) return;
    if (box.clientHeight !== viewH) {
      viewH = box.clientHeight;
      if (pinned) return void bottom();
    }
    pinned = box.scrollHeight - box.scrollTop - box.clientHeight < 80;
  }
  /** Y allows once, N denies: the first ask on screen. "Always" is a standing grant, so it
   *  takes a click on the card that says what it covers. Held keys don't repeat, and typing
   *  in any field never answers anything. */
  function hotkeys(/** @type {KeyboardEvent} */ e) {
    if (e.repeat || e.metaKey || e.ctrlKey || e.altKey || S.modal || !keyed) return;
    if (/** @type {HTMLElement} */ (e.target).closest('input, textarea, select, [contenteditable], .term')) return;
    const k = e.key.toLowerCase();
    if (k !== 'y' && k !== 'n') return;
    // An ask that has only just taken first place can't be answered by a key yet: a second
    // tap meant for the last one, or a key already on its way down, must not land on it.
    if (performance.now() - keyedSince < 600) return;
    answer(keyed.id, k === 'y' ? 'grant' : 'deny');
  }
  let keyedSince = 0;
  $effect(() => {
    keyed?.id;
    keyedSince = performance.now();
  });
  /** Jobs waiting on the commander: a squadmate's question, or a job that stalled. */
  const waiting = $derived(S.tasks.filter((x) => x.col === 'wait' && x.island_id === S.mission));
  /** @type {Record<number, string>} */ let replies = $state({});
  async function reply(/** @type {number} */ id) {
    const text = (replies[id] ?? '').trim();
    if (!text) return;
    await run('task.answer', { id, text }).then(() => (replies[id] = ''), () => {});
  }
  /** Known message headers from the engine; anything else is shown as written. */
  function header(/** @type {string} */ text = '') {
    let m = /^\[(Report|Order) from ([^\]]+)\]\s*/.exec(text);
    if (m) return { tag: m[1], who: m[2], rest: text.slice(m[0].length) };
    m = /^\[Job #(\d+) from the (?:board|map room)\]\s*/.exec(text);
    if (m) return { tag: `Job #${m[1]}`, who: '', rest: text.slice(m[0].length) };
    m = /^\[Commander's answer on job #(\d+): [^\]\n]*\]\s*/.exec(text);
    if (m) return { tag: `Your answer · #${m[1]}`, who: '', rest: text.slice(m[0].length) };
    m = /^\[Commander\]\s*/.exec(text);
    if (m) return { tag: 'From you', who: '', rest: text.slice(m[0].length) };
    return null;
  }
  /** A job as the agent got it ends with the engine's how-to lines; people don't need those. */
  const jobText = (/** @type {string} */ s) => s.split('\n').filter((l) => !/^(If you can't finish it without the commander|When it's done, run:|Carry on\. When it's done)/.test(l.trim())).join('\n').trim();

  const esc = (/** @type {string} */ s) => s.replace(/[&<>"']/g, (ch) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[ch] ?? ch);
  /** A small, safe subset of markdown: code blocks, inline code, bold, lists and line breaks. */
  /** Returns whole blocks: paragraphs, and code blocks beside them (a <pre> may not sit in a <p>). */
  function fmt(/** @type {string} */ s = '') {
    // Split on fenced code first: the odd pieces are code, kept exactly as written.
    return esc(s)
      .split(/```[a-z]*\n?([\s\S]*?)```/g)
      .map((part, i) => {
        if (i % 2) return `<pre>${part.replace(/^\n+|\s+$/g, '')}</pre>`;
        const text = part.replace(/^\n+|\n+$/g, '');
        if (!text) return '';
        const inline = text
          .replace(/`([^`\n]+)`/g, '<code>$1</code>')
          .replace(/\*\*([^*\n]+)\*\*/g, '<b>$1</b>')
          .replace(/^[-*] (.+)$/gm, '<span class="li">$1</span>')
          .replace(/\n{2,}/g, '</p><p>')
          .replace(/\n/g, '<br>');
        return `<p>${inline}</p>`;
      })
      .join('');
  }
  /** "Scout wants to run `x`" without the name the card already shows. */
  const wants = (/** @type {any} */ n, /** @type {string} */ name) => (n.title.startsWith(`${name} wants to `) ? n.title.slice(name.length + 10) : n.title);

  /** "@Ma" at the start of a message offers the squadmates it could mean. */
  const atWho = $derived.by(() => {
    const m = /^@([\w'-]*)$/.exec(text);
    if (!m) return [];
    const q = m[1].toLowerCase();
    return squad.filter((c) => c.id !== S.talk && c.name.toLowerCase().startsWith(q)).slice(0, 5);
  });
  function pickAt(/** @type {any} */ c) {
    text = `@${c.name} `;
    ta?.focus();
  }

  async function send() {
    const msg = text.trim();
    if ((!msg && !pics.length) || sending) return;
    sending = true;
    text = '';
    const sent = pics;
    const images = sent.map(({ type, data }) => ({ type, data }));
    pics = [];
    resize();
    // "@Doc do this" goes straight to Doc without leaving this conversation.
    const at = /^@([\w'-]+)[\s,:]+([\s\S]+)/.exec(msg);
    const mate = at ? squad.find((c) => c.name.toLowerCase() === at[1].toLowerCase()) : null;
    let ok;
    if (working && who && !images.length && !(mate && mate.id !== S.talk)) {
      // Mid-turn, a follow-up waits its turn where you can still change it (lib/store drain).
      enqueue(who.id, msg);
      sending = false;
      await tick();
      resize();
      ta?.focus();
      return;
    }
    if (mate && mate.id !== S.talk) {
      ok = await run('crew.say', { id: mate.id, text: at?.[2] ?? '', ...(images.length ? { images } : {}) }).then(() => true, () => false);
      if (ok) toast(`Sent to ${mate.name}`, 'go');
    } else {
      pinned = true;
      ok = await say(msg, images).catch(() => false);
    }
    if (!ok) {
      text = msg;
      pics = sent;
    } else sent.forEach((p) => URL.revokeObjectURL(p.url));
    sending = false;
    await tick();
    resize();
    ta?.focus();
  }
  function key(/** @type {KeyboardEvent} */ e) {
    if (e.isComposing) return;
    if (atWho.length && (e.key === 'Tab' || e.key === 'Enter')) {
      e.preventDefault();
      pickAt(atWho[0]);
    } else if (e.key === 'ArrowUp' && !text && lastMine) {
      // Up in an empty box brings back what you last sent, to fix and send again.
      e.preventDefault();
      text = lastMine;
      tick().then(() => {
        resize();
        ta?.setSelectionRange(text.length, text.length);
      });
    } else if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }
  function resize() {
    if (!ta) return;
    ta.style.height = 'auto';
    ta.style.height = Math.min(ta.scrollHeight, 220) + 'px';
  }
  /** Edit steps showing their change. */
  /** @type {Set<number>} */ let diffs = $state(new Set());
  const toggleDiff = (/** @type {number} */ id) => {
    const n = new Set(diffs);
    n.has(id) ? n.delete(id) : n.add(id);
    diffs = n;
  };
  const toggle = (/** @type {number} */ k) => {
    const n = new Set(open);
    n.has(k) ? n.delete(k) : n.add(k);
    open = n;
  };
  /** The reply being typed right now, streamed from the engine. */
  const live = $derived((S.talk && S.stream[S.talk]) || '');
  $effect(() => {
    live;
    if (pinned) tick().then(bottom);
  });
  const working = $derived(who?.status === 'working');
  /** Follow-ups waiting for this agent. */
  const queued = $derived((who && S.queue[who.id]) || []);
  const paused = $derived(!!(who && S.queuePaused[who.id]));
  function editQueued(/** @type {{id: number, text: string}} */ q) {
    if (!who) return;
    unqueue(who.id, q.id);
    text = text ? `${text}\n${q.text}` : q.text;
    tick().then(() => {
      resize();
      ta?.focus();
    });
  }
  const held = $derived(who?.status === 'held');
  /** How long the current turn has run, ticking once a second while there is one. */
  let now = $state(Date.now() / 1000);
  $effect(() => {
    if (!who?.since) return;
    now = Date.now() / 1000;
    const timer = setInterval(() => (now = Date.now() / 1000), 1000);
    return () => clearInterval(timer);
  });
  const elapsed = $derived(who?.since ? ticking(now - who.since) : '');
  /** What proves a turn: the last test or check it ran, and whether its reply claims "done" over
   *  a failed one. Read from the steps since the message that started the turn. */
  function proof(/** @type {any} */ turn) {
    const i = msgs.indexOf(turn);
    /** @type {{tests: boolean | undefined, lie: boolean}} */
    const out = { tests: undefined, lie: false };
    let said = '';
    for (let j = i - 1; j >= 0; j--) {
      const m = msgs[j];
      if (m.kind === 'me' || m.kind === 'job' || m.kind === 'turn') break;
      if (m.kind === 'text' && !said) said = m.text ?? '';
      if (out.tests === undefined && m.kind === 'tool' && m.name === 'Bash' && m.ok !== undefined && TESTY.test(m.arg ?? '')) out.tests = m.ok;
    }
    out.lie = out.tests === false && claims(said);
    return out;
  }
  /** The last thing in the conversation that isn't a turn's summary line. */
  const lastBlock = $derived(blocks.findLast((x) => x.kind !== 'turn'));
  /** What you last sent here, for Up in an empty box and for trying a failed turn again. */
  const lastMine = $derived(msgs.findLast((m) => m.kind === 'me')?.text ?? '');
  /** The agent's latest plan: only that one is drawn as a checklist. */
  const planId = $derived(msgs.findLast((m) => m.kind === 'tool' && m.todos?.length)?.id);
  /** How full the agent's context is (Claude Code only, once it has said). */
  const ctx = $derived(who?.engine === 'claude' && who.ctx > 0 ? Math.round(who.ctx * 100) : 0);
  /** Turn the conversation's last failed turn into a fresh try. */
  async function retry() {
    if (!lastMine || sending) return;
    pinned = true;
    sending = true;
    await say(lastMine).catch(() => false);
    sending = false;
  }
  function editLast() {
    text = lastMine;
    tick().then(() => {
      resize();
      ta?.focus();
    });
  }
  /** Messages that came in while the reader was scrolled up, for the Latest button. */
  let seenUpTo = $state(0);
  $effect(() => {
    if (pinned) seenUpTo = msgs.length;
  });
  const unseen = $derived(pinned ? 0 : msgs.slice(seenUpTo).filter((m) => m.kind === 'text' || m.kind === 'bottle' || m.kind === 'job' || m.kind === 'error').length);
  /** Older Windows missions were saved with the `\\?\` prefix. */
  const plainPath = (/** @type {string} */ p) => p.replace(/^\\\\\?\\/, '');
  /** The end of the path is the part that tells missions apart: the last two folders. */
  const shortPath = (/** @type {string} */ p) => {
    const parts = plainPath(p).split(/[\\/]/).filter(Boolean);
    return parts.length > 2 ? '…/' + parts.slice(-2).join('/') : plainPath(p);
  };
  /** The live reply with the cursor at the end of the text, not on a line of its own. */
  const CARET = '<span class="caret" aria-hidden="true"></span>';
  const liveHtml = $derived(live ? md(live).replace(/((?:<\/[a-z0-9]+>)+)$/, CARET + '$1') || CARET : '');

  async function copyText(/** @type {string} */ s) {
    (await copy(s)) ? toast('Copied', 'go') : toast('Couldn’t copy', 'hit');
  }
  /** Clicks inside rendered replies: a link opens in the system's browser (the desktop app
   *  has no tabs of its own), a code block's button copies it. */
  function proseClick(/** @type {MouseEvent} */ e) {
    const a = /** @type {HTMLAnchorElement | null} */ (/** @type {HTMLElement} */ (e.target).closest('.prose a[href]'));
    if (a && openLink(a.href)) return e.preventDefault();
    const b = /** @type {HTMLElement} */ (e.target).closest('[data-copy]');
    if (!b) return;
    const code = b.closest('.code')?.querySelector('code')?.textContent ?? '';
    copy(code).then((ok) => {
      b.textContent = ok ? 'Copied' : 'Couldn’t copy';
      setTimeout(() => (b.textContent = 'Copy'), 1400);
    });
  }
  const clock = (/** @type {number} */ ts) => new Date(ts * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  /** Stop the reply in progress: the agent is stopped and can be talked to again straight away. */
  async function stopIt() {
    if (!who) return;
    await run('crew.stop', { id: who.id }).catch(() => {});
    S.stream[who.id] = '';
    if (queued.length) S.queuePaused[who.id] = true;
  }
  /** Left and right arrows move between the squad's tabs. */
  function tabKeys(/** @type {KeyboardEvent} */ e) {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    const i = squad.findIndex((c) => c.id === S.talk);
    const next = squad[(i + (e.key === 'ArrowRight' ? 1 : -1) + squad.length) % squad.length];
    if (!next) return;
    e.preventDefault();
    talkTo(next.id);
    tick().then(() => /** @type {HTMLElement | null} */ (document.querySelector('.tabs [aria-selected="true"]'))?.focus());
  }

  const SUGGEST = [
    'Recon this codebase and tell me what looks risky',
    'Find the failing tests and fix them',
    'Plan this feature and split it across the squad: '
  ];

  // ---------- first run ----------
  const boss = $derived(bossCard());
  const found = $derived((S.detect?.engines ?? S.engines).filter((/** @type {any} */ e) => e.found));
  const ready = $derived(found.filter((/** @type {any} */ e) => e.signed_in !== 'none' && e.signed_in !== 'key missing'));
  let scanning = $state(false);
  async function rescan() {
    scanning = true;
    await run('engines.scan').catch(() => {});
    setTimeout(() => (scanning = false), 1500);
  }
  // Toasts sit above the asks dock, never on it (Toasts.svelte reads --dock-h).
  let dockH = $state(0);
  $effect(() => {
    const h = waiting.length || mine.length || others.length || away.length || awayJobs.length ? dockH : 0;
    document.documentElement.style.setProperty('--dock-h', `${h ? h + 10 : 0}px`);
  });
</script>

<svelte:window onkeydown={hotkeys} />

<main class="chat" aria-label="Conversation">
  <header class:framed={ownFrame && (!S.squadOpen || S.narrow)} {...dragArea}>
    <button type="button" class="icon-btn" onclick={() => (S.sideOpen = !S.sideOpen)} aria-label={S.sideOpen ? 'Hide missions' : 'Show missions'} title="{S.sideOpen ? 'Hide' : 'Show'} missions (Ctrl+B)" aria-pressed={S.sideOpen}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="1.75" y="2.75" width="12.5" height="10.5" rx="2" fill="none" stroke="currentColor" stroke-width="1.5" /><path d="M6 3v10" stroke="currentColor" stroke-width="1.5" /></svg>
    </button>
    {#if mission}
      <div class="title">
        <h1 title={mission.name}>{mission.name}</h1>
        <span class="path" title={plainPath(mission.path)}>{shortPath(mission.path)}</span>
      </div>
      <!-- svelte-ignore a11y_interactive_supports_focus -->
      <div class="tabs" class:compact={squad.length > 5} role="tablist" aria-label="Talk to" onkeydown={tabKeys}>
        {#each squad as c (c.id)}
          {@const asking = S.needs.some((n) => n.crew_id === c.id)}
          <button type="button" role="tab" aria-selected={S.talk === c.id} tabindex={S.talk === c.id ? 0 : -1} class:on={S.talk === c.id} onclick={() => talkTo(c.id)} title="{c.name}: {asking ? 'needs you' : t(/** @type {any} */ (`station.${c.station}`)) || c.label}">
            <Avatar name={c.name} face={c.face} size={22} status={c.status} />
            <span>{c.name}</span>
            {#if asking}<i class="pip wants" role="img" aria-label="needs you"></i>{:else if c.status === 'working'}<i class="pip busy" role="img" aria-label="working"></i>{/if}
          </button>
        {/each}
      </div>
    {:else}
      <div class="title"></div>
    {/if}
    {#if !S.squadOpen || S.narrow}
      <button type="button" class="icon-btn" onclick={() => (S.narrow ? (S.squadPeek = true) : (S.squadOpen = true))} aria-label="Show the squad" title="Show the squad (Ctrl+J)">
        <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="1.75" y="2.75" width="12.5" height="10.5" rx="2" fill="none" stroke="currentColor" stroke-width="1.5" /><path d="M10 3v10" stroke="currentColor" stroke-width="1.5" /></svg>
      </button>
    {/if}
  </header>

  {#if who && inTerminal}
    <!-- The terminal is only loaded for squadmates who use one. -->
    <p class="term-note">{who.name} runs {engineName(who.engine)} in its own terminal. Type straight into it; orders from the squad arrive as typed lines.</p>
    {#await import('./Terminal.svelte') then { default: Terminal }}
      <Terminal id={who.id} name={who.name} />
    {/await}
  {:else}
    <div class="view">
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="scroll" bind:this={box} onscroll={scrolled} onclick={proseClick}>
        <div class="col">
          {#if !mission}
            <div class="hero">
              <div class="mark"><Arms size={104} /></div>
              <h2>{t('welcome.title')}</h2>
              <p>{t('welcome.body', { boss: boss.name })}</p>
              <button type="button" class="btn primary big" onclick={() => (S.modal = 'mission')} disabled={!!S.detect && !found.length}>Start a mission</button>
              <dl class="check">
                <div>
                  <dt>Agents</dt>
                  {#if !S.detect}
                    <dd>Looking for the agent CLIs on this machine…</dd>
                  {:else if !found.length}
                    <dd class="warn">
                      None found. YSOB drives the agent CLIs you already use, so it needs one: install Claude Code (or Codex, Gemini CLI and others), sign in, then look again.
                      <span class="row">
                        <button type="button" class="btn small" onclick={rescan} disabled={scanning}>{scanning ? 'Looking…' : 'Look again'}</button>
                        <button type="button" class="btn small" onclick={() => copyText('https://claude.com/claude-code')}>Copy the Claude Code link</button>
                      </span>
                    </dd>
                  {:else}
                    <dd>
                      {found.slice(0, 3).map((/** @type {any} */ e) => e.name).join(', ')}{found.length > 3 ? ` and ${found.length - 3} more` : ''} found{ready.length < found.length ? `, ${ready.length} signed in` : ''}.
                      <button type="button" class="link" onclick={() => (S.modal = 'settings')}>See all</button>
                    </dd>
                  {/if}
                </div>
                <div>
                  <dt>Safety</dt>
                  {#if S.settings.sandbox_ok}
                    <dd>Every tool call is checked, and agents run in a sandbox that shows them the mission folder and nothing else of yours.</dd>
                  {:else}
                    <dd>Every tool call is checked, and anything outside an agent’s lane asks you first. There is no sandbox on this system yet, so an agent’s own code can reach what you can.</dd>
                  {/if}
                </div>
              </dl>
            </div>
          {:else if !who}
            <div class="hero">
              <h2>Nobody is on this mission.</h2>
              <p>A mission needs a squad to work it.</p>
              <button type="button" class="btn primary big" onclick={() => (S.modal = 'hire')}>Add to the squad</button>
            </div>
          {:else}
            {#if msgs.length === 0}
              <div class="intro">
                <Avatar name={who.name} face={who.face} size={64} />
                <h2>{isBoss ? t('boss.ask') : `Talk to ${who.name}`}</h2>
                <p>{isBoss ? t('boss.hint', { boss: who.name }) : (member(who.name)?.does ?? '')}</p>
                {#if isBoss}
                  <div class="chips">
                    {#each SUGGEST as s (s)}
                      <button type="button" onclick={() => { text = s; ta?.focus(); tick().then(resize); }}>{s}</button>
                    {/each}
                  </div>
                {/if}
              </div>
            {/if}

            {#each blocks as b (b.key)}
              {@const m = b.items[0]}
              {#if b.kind === 'me'}
                <div class="me">
                  {#if m.images}<div class="pics-sent"><svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true"><rect x="2" y="3" width="12" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.4" /><path d="m3 12 3.5-4 2.5 3 1.5-1.5L13 12" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" /></svg>{m.images} picture{m.images === 1 ? '' : 's'}</div>{/if}
                  {#if m.text}<div class="bubble">{@html fmt(m.text)}</div>{/if}
                  <div class="acts-row mine">
                    {#if m.ts}<time>{clock(m.ts)}</time>{/if}
                    <button type="button" class="ico" onclick={() => copyText(m.text)} title="Copy" aria-label="Copy message">
                      <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M5 5V2.5h8.5V11H11M2.5 5h8.5v8.5H2.5z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" /></svg>
                    </button>
                  </div>
                </div>
              {:else if b.kind === 'text'}
                <div class="them">
                  <Avatar name={who.name} face={who.face} size={28} />
                  <div class="body">
                    <div class="who">{who.name}</div>
                    <div class="prose">{@html md(m.text)}</div>
                    <div class="acts-row">
                      <button type="button" class="ico" onclick={() => copyText(m.text)} title="Copy" aria-label="Copy reply">
                        <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M5 5V2.5h8.5V11H11M2.5 5h8.5v8.5H2.5z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" /></svg>
                      </button>
                      {#if m.ts}<time>{clock(m.ts)}</time>{/if}
                    </div>
                  </div>
                </div>
              {:else if b.kind === 'tool'}
                {@const shown = open.has(b.key) ? b.items : b.items.slice(-3)}
                {@const plan = b.items.find((x) => x.id === planId)}
                <div class="acts">
                  {#if b.items.length > 3}
                    <button type="button" class="more" onclick={() => toggle(b.key)} aria-expanded={open.has(b.key)}>{open.has(b.key) ? 'Show fewer steps' : `${b.items.length - 3} earlier step${b.items.length - 3 === 1 ? '' : 's'}`}</button>
                  {/if}
                  {#each shown as step, i (step.id ?? i)}
                    {@const bad = step.ok === false || step.blocked}
                    {@const hasDiff = !!(step.diff?.del?.length || step.diff?.add?.length)}
                    <div class="act" class:bad class:run={step.ok === undefined && !step.blocked}>
                      <span class="st" aria-hidden="true"></span>
                      <span class="verb">{step.blocked ? 'Stopped' : (step.verb ?? step.name)}</span>
                      <span class="arg" title={step.arg ?? ''}>{step.arg ?? ''}</span>
                      {#if hasDiff}
                        <button type="button" class="see" onclick={() => toggleDiff(step.id)} aria-expanded={diffs.has(step.id)}>{diffs.has(step.id) ? 'Hide change' : 'Show change'}</button>
                      {/if}
                    </div>
                    {#if hasDiff && diffs.has(step.id)}
                      <pre class="diff" aria-label="The change">{#each step.diff.del as l, j (j)}<span class="del">- {l}</span>{/each}{#each step.diff.add as l, j (j)}<span class="add">+ {l}</span>{/each}</pre>
                    {/if}
                    {#if bad && step.res}<div class="why">{step.res}</div>{/if}
                  {/each}
                  {#if plan}
                    <ul class="plan" aria-label="{who.name}'s plan">
                      {#each plan.todos as item, i (i)}
                        <li class={item.st}><span class="tick-box" aria-hidden="true"></span><span>{item.text}</span>{#if item.st === 'in_progress'}<span class="sr-only"> (in progress)</span>{:else if item.st === 'completed'}<span class="sr-only"> (done)</span>{/if}</li>
                      {/each}
                    </ul>
                  {/if}
                </div>
              {:else if b.kind === 'bottle_out'}
                <div class="order">
                  <span class="tag">Order</span>
                  <span class="to">to {m.to}</span>
                  <div class="prose">{@html fmt(m.text)}</div>
                </div>
              {:else if b.kind === 'bottle' || b.kind === 'job'}
                {@const head = header(m.text)}
                <div class="order in" class:report={head?.tag === 'Report'}>
                  <span class="tag">{head ? head.tag : b.kind === 'job' ? 'New job' : 'Message'}</span>
                  {#if head?.who}<span class="to">{head.who}</span>{/if}
                  <div class="prose">{@html fmt(jobText(head ? head.rest : m.text))}</div>
                </div>
              {:else if b.kind === 'breaker'}
                <div class="caught">
                  <b>{t('held.card')}</b>
                  <div class="prose"><p>{@html fmt(m.text)}</p></div>
                  {#if m.quote}<div class="quote">From the mission log: {m.quote}</div>{/if}
                </div>
              {:else if b.kind === 'error'}
                <div class="err" role="alert">
                  <span>{m.text}</span>
                  {#if /authenticat|oauth|\b401\b|not (logged|signed) in|log ?in again/i.test(m.text ?? '')}
                    <span class="hint-line">{engineName(who.engine)} isn’t signed in on this machine. Open a terminal, run <code>{who.engine === 'claude' ? 'claude' : who.engine}</code> and sign in, then Try again.</span>
                  {/if}
                  {#if b === lastBlock && !working && lastMine}
                    <span class="err-acts">
                      <button type="button" class="btn small" onclick={retry} disabled={sending || held || !S.link}>Try again</button>
                      <button type="button" class="btn small quiet" onclick={editLast}>Edit and resend</button>
                    </span>
                  {/if}
                </div>
              {:else if b.kind === 'turn'}
                {@const pf = proof(m)}
                <div class="turn">
                  {#if pf.tests !== undefined}<span class="proof" class:bad={!pf.tests}>{pf.tests ? 'Tests passed' : 'Tests failed'}</span>{/if}
                  <span>{formatDuration(m.ms)}</span>
                  {#if m.tools}<span>{m.tools} step{m.tools === 1 ? '' : 's'}{#if m.failed}, <b>{m.failed} failed</b>{/if}</span>{/if}
                  {#if m.cost > 0 && !m.plan}<span>{money(m.cost)}</span>{/if}
                </div>
                {#if pf.lie}<p class="lie">{who.name} says it’s done, but the last test run in this turn failed.</p>{/if}
              {:else if b.kind === 'bell'}
                <div class="note"><span>{@html fmt(m.text)}</span>{#if m.answer}<b class:no={/denied|stopped/i.test(m.answer)}>{m.answer}</b>{/if}</div>
              {/if}
            {/each}

            {#if live}
              <div class="them streaming" aria-live="polite" aria-atomic="false">
                <Avatar name={who.name} face={who.face} size={28} />
                <div class="body"><div class="who">{who.name}{#if elapsed}<span class="tick">{elapsed}</span>{/if}</div><div class="prose">{@html liveHtml}</div></div>
              </div>
            {:else if working}
              <div class="typing" role="status"><Avatar name={who.name} face={who.face} size={20} /><span>{t(/** @type {any} */ (`station.${who.station}`)) || who.label}</span><i></i><i></i><i></i>{#if elapsed}<span class="tick">{elapsed}</span>{/if}</div>
            {/if}
          {/if}
        </div>
      </div>
      {#if !pinned && msgs.length}
        <button type="button" class="jump" onclick={() => { pinned = true; bottom(); }}>
          <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"><path d="M8 3v10M3.5 8.5 8 13l4.5-4.5" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round" stroke-linejoin="round" /></svg>
          {unseen ? `${unseen} new` : 'Latest'}
        </button>
      {/if}
    </div>
  {/if}

  <!-- Everything waiting on you sits here, above the box you type in: never scrolled away. -->
  {#if waiting.length || mine.length || others.length || away.length || awayJobs.length}
    <div class="dock" role="region" aria-label="Waiting on you" bind:offsetHeight={dockH}>
      {#each waiting as w (w.id)}
        {@const c = crewOf(w.crew_id)}
        <form class="ask wait" onsubmit={(e) => { e.preventDefault(); reply(w.id); }}>
          <Avatar name={c?.name ?? '?'} face={c?.face} size={24} status="waiting" />
          <span class="aw" title="{w.title}: {w.ask ?? ''}"><b>#{w.id} {w.title}</b> <span class="q">{w.ask ?? 'Waiting on you'}</span></span>
          <input class="field" bind:value={replies[w.id]} placeholder="Answer…" aria-label="Answer job {w.id}" autocomplete="off" />
          <button class="btn primary small" disabled={!(replies[w.id] ?? '').trim()}>Send</button>
        </form>
      {/each}

      {#each mine as n (n.id)}
        {#if who}
          <div class="need" class:hit={n.kind === 'breaker'} role="group" aria-label="{who.name} needs an answer">
            <div class="nh">
              <span class="radio" aria-hidden="true"></span>
              <b>{n.kind === 'breaker' ? t('held.title', { name: who.name }) : `${who.name} needs your OK`}</b>
              {#if n.detail && n.kind !== 'breaker'}<span class="lane">{n.detail}</span>{/if}
            </div>
            {#if n.kind === 'breaker'}
              <p class="what">{@html fmt(n.detail ?? '')}</p>
              <div class="nb">
                <button type="button" class="btn primary" onclick={() => answer(n.id, 'release')}>{t('held.release')}</button>
                <button type="button" class="btn" onclick={() => answer(n.id, 'stop')}>Stop the job</button>
              </div>
            {:else}
              <p class="what">Wants to {@html fmt(wants(n, who.name))}</p>
              {#if risky(n)}<p class="careful">This can’t be undone, or it reaches beyond this machine. Check it, then click: Y won’t answer this one.</p>{/if}
              <div class="nb">
                <button type="button" class="btn primary" onclick={() => answer(n.id, 'grant')}>Allow once{#if keyed?.id === n.id} <kbd>Y</kbd>{/if}</button>
                <button type="button" class="btn danger" onclick={() => answer(n.id, 'deny')}>Deny{#if keyed?.id === n.id} <kbd>N</kbd>{/if}</button>
                <span class="always">
                  <button type="button" class="btn quiet" onclick={() => answer(n.id, 'always')}>Always allow</button>
                  <span class="scope">{@html fmt(n.always ? `${n.always}, for ${who.name}, without asking again.` : `This, for ${who.name}, without asking again.`)}</span>
                </span>
              </div>
            {/if}
          </div>
        {/if}
      {/each}

      {#each shownExtra as x (x.key)}
        {#if x.w}
          {@const w = x.w}
          {@const c = crewOf(w.crew_id)}
          <div class="ask">
            <Avatar name={c?.name ?? '?'} face={c?.face} size={24} status="waiting" />
            <span class="aw" title="{w.title}: {w.ask ?? ''}"><span class="where">{missionOf(w.island_id)?.name ?? 'Another mission'}</span><b>#{w.id} {w.title}</b> <span class="q">{w.ask ?? 'Waiting on you'}</span></span>
            <button type="button" class="btn small" onclick={() => (w.crew_id ? goTo(w.crew_id) : (S.mission = w.island_id))}>Open</button>
          </div>
        {:else}
          {@const n = x.n}
          {@const c = crewOf(n.crew_id)}
          <div class="ask" class:hit={n.kind === 'breaker'}>
            <Avatar name={c?.name ?? '?'} face={c?.face} size={24} status="waiting" />
            <span class="aw" title={n.title}>
              {#if x.away}<span class="where">{missionOf(c?.island_id ?? null)?.name ?? 'Another mission'}</span>{/if}
              {#if n.kind === 'breaker'}<b>{t('held.short', { name: c?.name ?? '' })}</b>{:else}<b>{c?.name}</b> wants to <span class="at">{@html fmt(wants(n, c?.name ?? ''))}</span>{/if}
            </span>
            <button type="button" class="btn small" onclick={() => (x.away ? goTo(n.crew_id) : c && talkTo(c.id))}>Open</button>
            <!-- Another mission's ask is answered there, where its folder and lane are in view. -->
            {#if !x.away && n.kind !== 'breaker' && !risky(n)}
              <button type="button" class="btn primary small" onclick={() => answer(n.id, 'grant')}>Allow once{#if keyed?.id === n.id} <kbd>Y</kbd>{/if}</button>
              <button type="button" class="btn danger small" onclick={() => answer(n.id, 'deny')}>Deny{#if keyed?.id === n.id} <kbd>N</kbd>{/if}</button>
            {/if}
          </div>
        {/if}
      {/each}
      {#if extra.length > 2}
        <button type="button" class="more-wait" onclick={() => (allExtra = !allExtra)} aria-expanded={allExtra}>{allExtra ? 'Show fewer' : `${extra.length - 1} more waiting on you`}</button>
      {/if}
    </div>
  {/if}

  {#if who && !inTerminal}
    <div class="composer">
      {#if ctx >= 85}
        <p class="full" role="status">{who.name}’s context is {ctx}% full. Claude Code will squash the oldest turns soon.</p>
      {/if}
      {#if atWho.length}
        <div class="at-list" role="listbox" aria-label="Squadmates">
          {#each atWho as c, i (c.id)}
            <button type="button" role="option" aria-selected={i === 0} class:on={i === 0} onclick={() => pickAt(c)}><Avatar name={c.name} face={c.face} size={20} />{c.name}<span>{member(c.name)?.job ?? c.role}</span></button>
          {/each}
        </div>
      {/if}
      {#if queued.length}
        <div class="queue" role="region" aria-label="Waiting to send to {who.name}">
          <div class="qh">
            <span>{queued.length} waiting to send{paused ? ' · paused' : working ? ` · after ${who.name}’s turn` : ''}</span>
            <button type="button" class="qbtn" onclick={() => { if (!who) return; S.queuePaused[who.id] = !paused; if (paused && !working && who.status === 'idle') sendQueued(who.id, queued[0].id); }}>{paused ? 'Resume' : 'Pause'}</button>
          </div>
          {#each queued as q, i (q.id)}
            <div class="qrow">
              <span class="qdot" aria-hidden="true"></span>
              <span class="qt" title={q.text}>{q.text}</span>
              {#if i > 0}<button type="button" class="qbtn" onclick={() => who && queueUp(who.id, q.id)} aria-label="Move up" title="Move up"><svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"><path d="M8 13V3M4 7l4-4 4 4" stroke="currentColor" stroke-width="1.6" fill="none" stroke-linecap="round" stroke-linejoin="round" /></svg></button>{/if}
              <button type="button" class="qbtn" onclick={() => who && sendQueued(who.id, q.id)}>Send now</button>
              <button type="button" class="qbtn" onclick={() => editQueued(q)}>Edit</button>
              <button type="button" class="qbtn" onclick={() => who && unqueue(who.id, q.id)} aria-label="Remove" title="Remove"><svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg></button>
            </div>
          {/each}
        </div>
      {/if}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="box" class:off={held || !S.link} class:dragging ondragover={(e) => { if (e.dataTransfer?.types.includes('Files')) { e.preventDefault(); dragging = true; } }} ondragleave={() => (dragging = false)} ondrop={drop}>
        {#if pics.length}
          <div class="thumbs">
            {#each pics as p, i (p.url)}
              <span class="thumb"><img src={p.url} alt={p.name} /><button type="button" onclick={() => dropPic(i)} aria-label="Remove {p.name}" title="Remove">×</button></span>
            {/each}
          </div>
        {/if}
        <textarea
          onpaste={paste}
          bind:this={ta}
          bind:value={text}
          oninput={resize}
          onkeydown={key}
          rows="1"
          disabled={held || !S.link}
          placeholder={!S.link ? 'Reconnecting to the engine…' : working ? `${who.name} is working. Anything you send waits its turn…` : held ? `${t('held.title', { name: who.name })}. Answer above first.` : isBoss ? t('boss.placeholder', { boss: who.name }) : `Message ${who.name}…`}
          aria-label="Message {who.name}"
        ></textarea>
        <div class="bar">
          <span class="hint">{engineName(who.engine)} · {LANE_WORDS[who.lane] ?? who.lane}{#if ctx}{' · '}<span class="gauge" class:hot={ctx >= 85} title="How much of the model’s context window the last request used">Context {ctx}%</span>{/if}{#if squad.length > 1 && isBoss}<span class="tip">{' · '}@name sends to one squadmate</span>{/if}</span>
          <input bind:this={picker} type="file" accept="image/png,image/jpeg,image/gif,image/webp" multiple hidden onchange={(e) => { addPics(e.currentTarget.files ?? []); e.currentTarget.value = ''; }} />
          <button type="button" class="ico attach" onclick={() => picker?.click()} disabled={held || !S.link} aria-label="Add a picture" title="Add a picture (or paste or drop one)">
            <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><path d="M13.5 7.5 8 13a3.5 3.5 0 0 1-5-5l6-6a2.3 2.3 0 0 1 3.3 3.3L6.5 11.2a1.1 1.1 0 0 1-1.6-1.6L10 4.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" /></svg>
          </button>
          {#if working && !text.trim() && !pics.length}
            <button type="button" class="send stop" onclick={stopIt} aria-label="Stop {who.name}" title="Stop {who.name}">
              <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="4" y="4" width="8" height="8" rx="1.5" fill="currentColor" /></svg>
            </button>
          {:else}
            <button type="button" class="send" onclick={send} disabled={(!text.trim() && !pics.length) || sending || held || !S.link} aria-label={working ? 'Queue' : 'Send'} title={working ? 'Queue it for after this turn (Enter)' : 'Send (Enter)'}>
              <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 13V3M3.5 7.5 8 3l4.5 4.5" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round" stroke-linejoin="round" /></svg>
            </button>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</main>

<style>
  .chat {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    min-height: 0;
    background: var(--color-bg);
    /* The conversation, the asks and the box you type in share one width that grows with the
       window: 760 px in a normal window, up to 1200 px full screen. */
    container-type: inline-size;
    --col: clamp(760px, 76cqi, 1200px);
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 56px;
    padding: 10px 16px 10px 14px;
  }
  /* With the squad panel shut, the window buttons are in this corner (WindowBar.svelte). */
  header.framed { padding-right: 150px; }
  .title { min-width: 110px; flex: 1; }
  h1 { margin: 0; font-size: 15px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-wrap: nowrap; }
  .path {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: 11.5px var(--font-mono);
    color: var(--color-faint);
  }
  .tabs { display: flex; gap: 4px; overflow-x: auto; min-width: 0; padding: 3px; scrollbar-width: none; }
  /* A big squad: faces only, the name just for the one you're talking to (it's on hover too). */
  .tabs.compact button:not(.on) span { display: none; }
  .tabs.compact button:not(.on) { padding: 0 5px; }
  /* Tight on space: names only for the one you're talking to. */
  @media (max-width: 1320px) {
    .tabs button:not(.on) span { display: none; }
    .tabs button:not(.on) { padding: 0 5px; }
  }
  .tabs button {
    position: relative;
    display: flex;
    flex: none;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 10px 0 5px;
    border-radius: 9px;
    font-size: 13px;
    color: var(--color-mute);
    white-space: nowrap;
    transition: background 0.15s var(--ease), color 0.15s var(--ease);
  }
  .tabs button:hover { background: var(--color-panel); color: var(--color-ink); }
  .tabs button.on { background: var(--color-raise); color: var(--color-ink); }
  .pip { flex: none; width: 6px; height: 6px; border-radius: 50%; }
  .pip.busy { background: var(--color-go); }
  .pip.wants { background: var(--color-ask); }

  .term-note { margin: 0; padding: 0 24px 8px 22px; font-size: 12.5px; color: var(--color-faint); }
  .view { position: relative; display: flex; flex: 1; min-height: 0; }
  .scroll { flex: 1; min-width: 0; overflow-y: auto; }
  .col {
    display: flex;
    flex-direction: column;
    gap: 18px;
    max-width: var(--col);
    margin: 0 auto;
    padding: 24px 24px 32px;
  }

  .hero, .intro {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 10px;
    padding-top: 10vh;
  }
  .intro { padding-top: 8vh; }
  .hero h2, .intro h2 { margin: 6px 0 0; font-size: 26px; font-weight: 650; letter-spacing: -0.02em; }
  .hero p, .intro p { margin: 0 0 10px; max-width: 44ch; color: var(--color-mute); }
  .btn.big { height: 40px; padding: 0 18px; font-size: 14px; }
  .mark { margin-bottom: 4px; }
  /* What a new user needs to know before the first mission: what it will run, and how safe. */
  .check { display: grid; gap: 14px; width: 100%; max-width: 520px; margin: 30px 0 0; text-align: left; }
  .check div { display: grid; grid-template-columns: 64px minmax(0, 1fr); gap: 14px; }
  .check dt { font-size: 12.5px; font-weight: 600; color: var(--color-faint); padding-top: 1px; }
  .check dd { margin: 0; font-size: 13px; line-height: 1.55; color: var(--color-mute); }
  .check dd.warn { color: var(--color-ask); }
  .check .row { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 10px; }
  .link { color: var(--color-ink); text-decoration: underline; text-underline-offset: 3px; text-decoration-color: var(--color-faint); }
  .link:hover { text-decoration-color: var(--color-ink); }
  .chips { display: flex; flex-direction: column; gap: 8px; width: 100%; max-width: 460px; }
  .chips button {
    padding: 11px 14px;
    border-radius: 12px;
    background: var(--color-panel);
    text-align: left;
    color: var(--color-mute);
    transition: background 0.15s var(--ease), color 0.15s var(--ease);
  }
  .chips button:hover { background: var(--color-raise); color: var(--color-ink); }

  .me { display: flex; flex-direction: column; align-items: flex-end; }
  .bubble {
    max-width: min(82%, 720px);
    padding: 10px 14px;
    border-radius: 18px 18px 6px 18px;
    background: var(--color-raise);
    overflow-wrap: anywhere;
  }
  .bubble :global(p) { margin: 0 0 0.5em; }
  .bubble :global(p:last-child) { margin: 0; }
  .bubble :global(code) { font: 12.5px var(--font-mono); padding: 1px 5px; border-radius: 5px; background: var(--color-hover); }
  .bubble :global(pre) { margin: 6px 0; padding: 10px 12px; border-radius: 9px; background: var(--color-bg); font: 12.5px/1.5 var(--font-mono); white-space: pre-wrap; }
  .them { display: flex; gap: 12px; }
  .body { min-width: 0; flex: 1; }
  .who { font-size: 12.5px; font-weight: 600; color: var(--color-mute); margin-bottom: 2px; }
  .prose :global(p) { margin: 0 0 0.6em; overflow-wrap: anywhere; }
  .prose :global(p:last-child) { margin-bottom: 0; }
  .prose :global(code), .what :global(code), .note :global(code), .at :global(code), .scope :global(code) { font: 12.5px var(--font-mono); padding: 1px 5px; border-radius: 5px; background: var(--color-raise); overflow-wrap: anywhere; }
  .prose :global(pre) {
    margin: 8px 0;
    padding: 12px 14px;
    border-radius: 10px;
    background: var(--color-panel);
    font: 12.5px/1.5 var(--font-mono);
    overflow-x: auto;
    white-space: pre-wrap;
  }
  /* Replies read like a chat app's: comfortable measure, real headings, lists, tables, code.
     In a wide window code, tables and steps use the width; running text stops at a length
     that still reads. */
  .prose { font-size: 14.5px; line-height: 1.65; }
  .prose > :global(:is(p, ul, ol, h2, h3, h4, blockquote)) { max-width: 92ch; }
  .prose :global(h2), .prose :global(h3), .prose :global(h4) { margin: 1.1em 0 0.45em; font-weight: 650; line-height: 1.3; letter-spacing: -0.01em; }
  .prose :global(h2) { font-size: 1.2em; }
  .prose :global(h3) { font-size: 1.08em; }
  .prose :global(h4) { font-size: 1em; color: var(--color-mute); }
  .prose :global(:is(h2, h3, h4):first-child) { margin-top: 0; }
  .prose :global(ul), .prose :global(ol) { margin: 0.2em 0 0.7em; padding-left: 1.4em; }
  .prose :global(ul) { list-style: disc; }
  .prose :global(ol) { list-style: decimal; }
  .prose :global(li) { margin: 0.2em 0; padding-left: 0.2em; }
  .prose :global(li.sub) { margin-left: 1.2em; }
  .prose :global(li::marker) { color: var(--color-faint); font-variant-numeric: tabular-nums; }
  .prose :global(ul.tasks) { list-style: none; padding-left: 0.2em; }
  .prose :global(li.task) { display: flex; gap: 9px; align-items: baseline; }
  .prose :global(li.task .box) { flex: none; width: 14px; height: 14px; border-radius: 4px; box-shadow: inset 0 0 0 1.5px var(--color-faint); transform: translateY(2px); }
  .prose :global(li.task.done .box) { background: var(--color-go); box-shadow: none; }
  .prose :global(li.task.done) { color: var(--color-mute); text-decoration: line-through; text-decoration-color: var(--color-faint); }
  .prose :global(strong) { font-weight: 650; color: var(--color-ink); }
  .prose :global(a) { color: var(--color-go); text-decoration: underline; text-underline-offset: 3px; text-decoration-thickness: 1px; overflow-wrap: anywhere; }
  .prose :global(blockquote) { margin: 0.4em 0 0.8em; padding: 8px 12px; border-radius: 8px; background: var(--color-panel); color: var(--color-mute); }
  .prose :global(hr) { border: 0; height: 1px; background: var(--color-line); margin: 1em 0; }
  .prose :global(.table) { overflow-x: auto; margin: 0.4em 0 0.9em; border-radius: 10px; background: var(--color-panel); }
  .prose :global(table) { border-collapse: collapse; font-size: 13.5px; min-width: 100%; font-variant-numeric: tabular-nums; }
  .prose :global(th), .prose :global(td) { padding: 7px 12px; text-align: left; vertical-align: top; }
  .prose :global(th) { font-weight: 600; color: var(--color-mute); background: var(--color-raise); }
  .prose :global(tr + tr td) { border-top: 1px solid var(--color-line); }
  .prose :global(.code) { margin: 0.4em 0 0.9em; border-radius: 10px; overflow: hidden; background: #0d0f0c; box-shadow: inset 0 0 0 1px var(--color-line); }
  .prose :global(.code-h) { display: flex; align-items: center; justify-content: space-between; padding: 5px 6px 5px 12px; background: var(--color-raise); font: 11.5px var(--font-mono); color: var(--color-faint); }
  .prose :global(.code pre) { margin: 0; padding: 12px 14px; background: transparent; border-radius: 0; overflow-x: auto; white-space: pre; }
  .prose :global(.code code) { padding: 0; background: none; font-size: 12.5px; line-height: 1.6; overflow-wrap: normal; }
  .prose :global(.copy) { border: 0; border-radius: 6px; padding: 4px 9px; background: transparent; color: var(--color-mute); font: 500 11.5px var(--font-sans); cursor: pointer; transition: background 0.15s var(--ease), color 0.15s var(--ease); }
  .prose :global(.copy:hover) { background: var(--color-hover); color: var(--color-ink); }
  .prose :global(.t-k) { color: #c792ea; }
  .prose :global(.t-s) { color: #a5d6a7; }
  .prose :global(.t-n) { color: #f4b54a; }
  .prose :global(.t-c) { color: #8b9583; font-style: italic; }
  /* Under each message: copy and the time, shown on hover (always on touch screens). */
  .acts-row { display: flex; align-items: center; gap: 6px; margin-top: 4px; min-height: 28px; opacity: 0; transition: opacity 0.15s; }
  /* The hover row (copy, time) sits in the gap to the next message rather than adding to it. */
  .me, .them:not(.streaming) { margin-bottom: -14px; }
  .them:hover .acts-row, .me:hover .acts-row, .acts-row:focus-within { opacity: 1; }
  @media (hover: none) { .acts-row { opacity: 1; } }
  .acts-row.mine { justify-content: flex-end; }
  .acts-row time { font-size: 11.5px; color: var(--color-faint); font-variant-numeric: tabular-nums; }
  .ico { display: grid; place-items: center; width: 28px; height: 28px; border: 0; border-radius: 7px; background: transparent; color: var(--color-faint); cursor: pointer; transition: background 0.15s var(--ease), color 0.15s var(--ease); }
  .ico:hover { background: var(--color-hover); color: var(--color-ink); }
  .prose :global(.caret) { display: inline-block; width: 7px; height: 1.05em; margin-left: 2px; vertical-align: -0.15em; border-radius: 1px; background: var(--color-go); animation: caret 1s steps(2) infinite; }
  @keyframes caret { 50% { opacity: 0; } }
  .prose :global(.li) { display: block; padding-left: 14px; position: relative; }
  .prose :global(.li)::before { content: ''; position: absolute; left: 3px; top: 0.65em; width: 4px; height: 4px; border-radius: 50%; background: var(--color-faint); }

  .acts { display: flex; flex-direction: column; gap: 2px; margin-left: 40px; }
  .act {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    font-size: 12.5px;
    color: var(--color-mute);
  }
  .st { width: 6px; height: 6px; border-radius: 50%; background: var(--color-go); flex: none; }
  .act.run .st { background: var(--color-heat); animation: blink 1s infinite; }
  .act.bad .st { background: var(--color-hit); }
  .act.bad .verb { color: #ff8b7f; }
  @keyframes blink { 50% { opacity: 0.3; } }
  .verb { color: var(--color-ink); font-weight: 500; white-space: nowrap; }
  .arg { font: 12px var(--font-mono); color: var(--color-faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .why { margin: 0 0 3px 14px; font: 12px/1.5 var(--font-mono); color: #d98a80; overflow-wrap: anywhere; }
  .more { align-self: flex-start; font-size: 12px; color: var(--color-faint); padding: 2px 0; }
  .more:hover { color: var(--color-ink); }

  .order {
    margin-left: 40px;
    padding: 12px 14px;
    border-radius: 12px;
    background: var(--color-panel);
  }
  /* Small labels in the text face: the pixel face only reads at display sizes. */
  .tag { font: 650 11px/1 var(--font-sans); letter-spacing: 0.08em; text-transform: uppercase; color: var(--color-heat); }
  .order.in .tag { color: var(--color-mute); }
  .order.report .tag { color: var(--color-go); }
  .to { margin-left: 6px; font-weight: 600; font-size: 13px; }
  .order .prose { margin-top: 6px; color: var(--color-mute); }

  .caught {
    padding: 14px 16px;
    border-radius: 14px;
    background: rgb(255 75 58 / 0.08);
  }
  .caught b { color: #ff8b7f; }
  .quote { margin-top: 8px; font-size: 12.5px; color: var(--color-mute); }
  .err { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 12px; margin-left: 40px; font-size: 13px; color: #ff8b7f; overflow-wrap: anywhere; }
  .err-acts { display: flex; gap: 6px; }
  .hint-line { flex-basis: 100%; font-size: 12.5px; color: var(--color-mute); }
  .hint-line code { font: 12px var(--font-mono); padding: 1px 5px; border-radius: 5px; background: var(--color-raise); color: var(--color-ink); }
  /* The line that closes a turn: time, steps, cost. Quiet, like a timestamp. */
  .turn { display: flex; flex-wrap: wrap; gap: 4px 0; margin-left: 40px; font-size: 12px; color: var(--color-faint); font-variant-numeric: tabular-nums; }
  .turn span + span::before { content: '·'; margin: 0 7px; }
  .turn b { font-weight: 500; color: #d98a80; }
  .tick { margin-left: 8px; font: 500 11.5px var(--font-mono); color: var(--color-faint); font-variant-numeric: tabular-nums; }
  /* The agent's own plan, as it reported it. Only "completed" is crossed out. */
  .plan { display: flex; flex-direction: column; gap: 3px; margin: 6px 0 2px 14px; padding: 0; list-style: none; font-size: 12.5px; color: var(--color-mute); }
  .plan li { display: flex; gap: 8px; align-items: baseline; }
  .tick-box { flex: none; width: 11px; height: 11px; border-radius: 3px; box-shadow: inset 0 0 0 1.5px var(--color-faint); transform: translateY(1px); }
  .plan li.in_progress { color: var(--color-ink); }
  .plan li.in_progress .tick-box { box-shadow: inset 0 0 0 1.5px var(--color-heat); background: rgb(255 138 31 / 0.25); }
  .plan li.completed { text-decoration: line-through; text-decoration-color: var(--color-faint); color: var(--color-faint); }
  .plan li.completed .tick-box { background: var(--color-go); box-shadow: none; }
  .gauge.hot { color: var(--color-ask); }
  .proof { color: var(--color-go); font-weight: 550; }
  .proof.bad { color: #ff8b7f; }
  .lie { margin: -10px 0 0 40px; font-size: 12.5px; color: #ff8b7f; }
  .careful { margin: 6px 0 0; font-size: 12.5px; color: var(--color-ask); }
  .more-wait { padding: 9px 12px; text-align: left; font-size: 12.5px; color: var(--color-mute); }
  .more-wait:hover { color: var(--color-ink); background: var(--color-raise); }
  .see { margin-left: 2px; padding: 1px 6px; border-radius: 5px; font-size: 11.5px; color: var(--color-faint); flex: none; }
  .see:hover { background: var(--color-hover); color: var(--color-ink); }
  .diff { margin: 2px 0 6px 14px; padding: 8px 10px; border-radius: 8px; background: #0d0f0c; box-shadow: inset 0 0 0 1px var(--color-line); font: 12px/1.55 var(--font-mono); overflow-x: auto; white-space: pre; }
  .diff span { display: block; }
  .diff .del { color: #e8968b; background: rgb(255 75 58 / 0.08); }
  .diff .add { color: #9fdcaa; background: rgb(127 214 143 / 0.08); }
  /* An ask on one line: the formatter's paragraph flows inline here. */
  .aw :global(p) { display: inline; margin: 0; }
  .where { margin-right: 8px; font: 650 11px var(--font-sans); letter-spacing: 0.06em; text-transform: uppercase; color: var(--color-faint); }
  .pics-sent { display: flex; align-items: center; gap: 6px; margin-bottom: 4px; font-size: 12px; color: var(--color-mute); }
  .box.dragging { box-shadow: 0 0 0 1.5px var(--color-go), 0 12px 40px rgb(0 0 0 / 0.4); }
  .thumbs { display: flex; flex-wrap: wrap; gap: 8px; margin: 0 0 10px; }
  .thumb { position: relative; width: 56px; height: 56px; border-radius: 9px; overflow: hidden; box-shadow: inset 0 0 0 1px var(--color-line); }
  .thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .thumb button { position: absolute; top: 3px; right: 3px; width: 18px; height: 18px; border-radius: 50%; background: rgb(0 0 0 / 0.7); color: #fff; font-size: 13px; line-height: 1; display: grid; place-items: center; }
  .attach { margin-left: auto; }
  /* Follow-ups waiting their turn: one quiet list on the box's surface, divider rows. */
  .queue { max-height: 18vh; overflow-y: auto; overscroll-behavior: contain; max-width: var(--col); margin: 0 auto 8px; padding: 6px 6px 4px 14px; border-radius: 14px; background: var(--color-panel); font-size: 13px; }
  .qh { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 2px 0 6px; font-size: 12px; color: var(--color-faint); }
  .qrow { display: flex; align-items: center; gap: 4px; min-height: 32px; border-top: 1px solid var(--color-line); }
  .qdot { flex: none; width: 6px; height: 6px; margin-right: 6px; border-radius: 50%; background: var(--color-faint); }
  .qt { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--color-ink); }
  .qbtn { flex: none; height: 26px; padding: 0 8px; border-radius: 7px; font-size: 12px; color: var(--color-mute); }
  .qbtn svg { display: block; }
  .qbtn:hover { background: var(--color-hover); color: var(--color-ink); }
  .full { max-width: var(--col); margin: 0 auto 8px; padding: 0 4px; font-size: 12.5px; color: var(--color-ask); }
  .note { display: flex; flex-wrap: wrap; gap: 4px 10px; margin-left: 40px; font-size: 12.5px; color: var(--color-faint); overflow-wrap: anywhere; }
  .note b { font-weight: 600; color: var(--color-go); }
  .note b.no { color: #ff8b7f; }

  .typing { display: flex; align-items: center; gap: 8px; font-size: 12.5px; color: var(--color-faint); }
  .typing i { width: 4px; height: 4px; border-radius: 50%; background: var(--color-faint); animation: typing 1.2s infinite; }
  .typing i:nth-of-type(2) { animation-delay: 0.15s; }
  .typing i:nth-of-type(3) { animation-delay: 0.3s; }
  @keyframes typing { 30% { transform: translateY(-3px); opacity: 1; } 0%, 60%, 100% { opacity: 0.4; } }

  .jump {
    position: absolute;
    left: 50%;
    bottom: 12px;
    translate: -50% 0;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 13px 0 10px;
    border-radius: 999px;
    background: var(--color-hover);
    box-shadow: 0 8px 20px rgb(0 0 0 / 0.45);
    font-size: 12.5px;
    font-weight: 550;
    color: var(--color-ink);
    animation: rise 0.2s var(--ease);
  }
  .jump:hover { background: #2a3024; }

  /* ---------- waiting on you ---------- */
  /* Everything waiting on you is one surface: divider rows, a dot and a soft tint say what kind. */
  .dock { display: flex; flex-direction: column; max-width: var(--col); width: calc(100% - 48px); margin: 0 auto 10px; max-height: 30vh; overflow-y: auto; overscroll-behavior: contain; border-radius: 14px; background: var(--color-panel); }
  .dock > :global(* + *) { border-top: 1px solid var(--color-line); }
  .need {
    padding: 14px 16px;
    background: rgb(244 181 74 / 0.07);
    animation: rise 0.25s var(--ease);
  }
  .need.hit { background: rgb(255 75 58 / 0.08); }
  .nh { display: flex; align-items: baseline; flex-wrap: wrap; gap: 4px 10px; }
  .nh b { font-weight: 600; }
  .lane { font-size: 12px; color: var(--color-faint); }
  .radio { align-self: center; width: 8px; height: 8px; border-radius: 50%; background: var(--color-ask); animation: blink 1.2s infinite; }
  .need.hit .radio { background: var(--color-hit); }
  .what { margin: 8px 0 0; max-height: 9.5em; overflow-y: auto; font-size: 14px; line-height: 1.6; color: var(--color-ink); overflow-wrap: anywhere; }
  .what :global(code) { background: rgb(0 0 0 / 0.35); }
  .what :global(p) { display: inline; margin: 0; }
  .nb { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-top: 12px; }
  .always { display: flex; align-items: center; gap: 8px; min-width: 0; margin-left: auto; }
  .scope { max-width: 32ch; font-size: 11.5px; line-height: 1.45; color: var(--color-faint); }
  .scope :global(code) { font-size: 11px; background: rgb(0 0 0 / 0.3); }
  .btn.quiet { background: transparent; color: var(--color-mute); box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.1); }
  .btn.quiet:hover { background: var(--color-raise); color: var(--color-ink); }
  .ask {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 12px;
    animation: rise 0.25s var(--ease);
  }
  .ask.hit { background: rgb(255 75 58 / 0.06); }
  @keyframes rise { from { opacity: 0; transform: translateY(6px); } }
  .aw { flex: 1; min-width: 0; font-size: 13px; color: var(--color-mute); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .aw b { color: var(--color-ink); font-weight: 600; }
  .q { color: var(--color-mute); }
  .ask .field { width: 220px; height: 30px; padding: 0 10px; border-radius: 9px; font-size: 13px; }
  .need kbd, .ask kbd { margin-left: 2px; background: rgb(0 0 0 / 0.18); box-shadow: none; }

  /* ---------- the box you type in ---------- */
  .composer { position: relative; padding: 0 24px 20px; }
  .at-list { position: absolute; left: 50%; bottom: calc(100% + 6px); translate: -50% 0; width: min(var(--col), calc(100% - 48px)); display: flex; flex-direction: column; padding: 6px; border-radius: 14px; background: var(--color-raise); box-shadow: 0 12px 40px rgb(0 0 0 / 0.5); z-index: 3; }
  .at-list button { display: flex; align-items: center; gap: 9px; height: 34px; padding: 0 8px; border-radius: 9px; font-size: 13.5px; font-weight: 550; text-align: left; }
  .at-list button span { font-size: 12px; font-weight: 400; color: var(--color-faint); }
  .at-list button.on, .at-list button:hover { background: var(--color-hover); }
  .box {
    max-width: var(--col);
    margin: 0 auto;
    padding: 12px 12px 10px 16px;
    border-radius: 20px;
    background: var(--color-panel);
    box-shadow: 0 0 0 1px var(--color-line), 0 12px 40px rgb(0 0 0 / 0.35);
    transition: box-shadow 0.2s var(--ease);
  }
  .box:focus-within { box-shadow: 0 0 0 1.5px rgb(255 138 31 / 0.55), 0 12px 40px rgb(0 0 0 / 0.4); }
  .box.off { opacity: 0.6; }
  textarea {
    display: block;
    width: 100%;
    min-height: 24px;
    resize: none;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--color-ink);
    font: inherit;
    font-size: 15px;
  }
  textarea::placeholder { color: var(--color-faint); }
  .bar { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-top: 8px; }
  .hint { min-width: 0; font-size: 12px; color: var(--color-faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  @media (max-width: 1180px) { .tip { display: none; } }
  .send {
    display: grid;
    flex: none;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 50%;
    background: var(--color-ink);
    color: #0b0d0a;
    transition: opacity 0.15s, scale 0.15s var(--ease);
  }
  .send:disabled { opacity: 0.25; }
  .send:not(:disabled):hover { scale: 1.06; }
  .send:not(:disabled):active { scale: 0.96; }
  .send svg { width: 16px; height: 16px; }
</style>

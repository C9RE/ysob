import { api, connect } from './api.js';
import { BOSS_ROLE } from './words.js';
import { bossCard, member, t } from './packdata.svelte.js';
import { sfx, setEnabled, setVolume } from './sound.js';
import { notify, trayNeeds } from './desktop.js';

/** What this window remembers between runs (per machine, in localStorage). */
const PREFS = 'ysob.ui';
function prefs() {
  try {
    return JSON.parse(localStorage.getItem(PREFS) ?? '{}') ?? {};
  } catch {
    return {};
  }
}
const kept = prefs();
/** Follow-ups typed while an agent works, per crew id, kept between runs on this machine. */
const QUEUE = 'ysob.queue';
function keptQueue() {
  try {
    const q = JSON.parse(localStorage.getItem(QUEUE) ?? '{}');
    return q && typeof q === 'object' ? q : {};
  } catch {
    return {};
  }
}
const reduced = () => typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;

/** Everything the app shows. The engine owns the truth; this is its mirror. */
export const S = $state({
  ready: false,
  link: true,
  error: '',
  settings: { ship: '', budget: 18, breaker: true, hard_stop: true, quote_log: true, sandbox: true, sandbox_ok: false, sandbox_why: '', version: '' },
  /** @type {any[]} */ missions: [],
  /** @type {any[]} */ crew: [],
  /** @type {any[]} */ tasks: [],
  /** @type {any[]} */ needs: [],
  /** @type {any[]} */ engines: [],
  spent: 0,
  /** Selected mission id. @type {string | null} */ mission: null,
  /** Whose conversation is open (crew id). Defaults to the mission's squad leader. @type {string | null} */ talk: null,
  /** @type {Record<string, any[]>} */ msgs: {},
  /** The missions sidebar (Ctrl+B hides it). */ sideOpen: kept.sideOpen !== false,
  /** What this machine can run: agent CLIs, API keys, local models (engine detect.rs). @type {any} */ detect: null,
  /** The reply each agent is typing right now (crew id -> text so far). @type {Record<string, string>} */ stream: {},
  /** Squad members being taken by the hunter right now (crew ids). @type {string[]} */ dying: [],
  /** Squad members flinching from a failed tool call right now (crew ids). @type {string[]} */ hurt: [],
  /** Squad members doing their move after a proven job (crew ids). @type {string[]} */ win: [],
  /** The big moment on screen. @type {null | {line: string, sub: string, kind: string, who?: string}} */ strike: null,
  /** @type {null | 'mission' | 'hire' | 'settings' | 'palette'} */ modal: null,
  /** The Settings section to open on (once). @type {string | null} */ settingsAt: null,
  squadOpen: kept.squadOpen !== false,
  /** A narrow window (1020 px and under): no room for the squad panel beside the chat, so it
   *  opens as a drawer over it instead (squadPeek). */ narrow: false,
  /** The squad drawer is out (narrow windows only). */ squadPeek: false,
  sound: kept.sound === true,
  /** A desktop notification when someone needs you and the window isn't in front. */ notify: kept.notify !== false,
  /** Master volume, 0 to 100. */ volume: Number.isFinite(kept.volume) ? Math.max(0, Math.min(100, kept.volume)) : 70,
  thermal: false,
  mud: kept.mud === true,
  /** The debrief for a job just closed with proof. @type {null | {title: string, proof: string, who: string}} */ chopper: null,
  /** @type {{id: number, text: string, tone: string, act?: {label: string, run: () => void}}[]} */ toasts: [],
  /** Follow-ups waiting for an agent to be free (crew id -> list). They go one at a time, oldest
   *  first, when its turn ends. @type {Record<string, {id: number, text: string}[]>} */ queue: keptQueue(),
  /** Queues held back: Stop pauses an agent's queue until you resume it. @type {Record<string, boolean>} */ queuePaused: {}
});

$effect.root(() => {
  $effect(() => {
    const { sideOpen, squadOpen, sound, volume, mud, notify } = S;
    try {
      localStorage.setItem(PREFS, JSON.stringify({ sideOpen, squadOpen, sound, volume, mud, notify }));
    } catch {
      /* private window or blocked storage: these last for this session only */
    }
  });
  $effect(() => {
    const q = Object.fromEntries(Object.entries(S.queue).filter(([, list]) => list.length));
    try {
      localStorage.setItem(QUEUE, JSON.stringify(q));
    } catch {
      /* as above */
    }
  });
  $effect(() => setEnabled(S.sound));
  $effect(() => setVolume(S.volume / 100));
  // The tray says how much is waiting on you: asks, and jobs stalled on a question.
  $effect(() => trayNeeds(S.needs.length + S.tasks.filter((x) => x.col === 'wait').length));
});

/** Tell the desktop, if it's wanted. The shell stays quiet while the window is in front. */
const ping = (/** @type {string} */ title, /** @type {string} */ body) => S.ready && S.notify && notify(title, body);

let toastId = 0;
/**
 * A short note at the bottom of the window. With an action it stays longer.
 * @param {string} text @param {string} [tone] @param {{label: string, run: () => void}} [act]
 */
export function toast(text, tone = 'info', act) {
  const id = ++toastId;
  S.toasts = [...S.toasts.slice(-2), { id, text, tone, act }];
  setTimeout(() => dismissToast(id), act ? 12000 : 4200);
}
export const dismissToast = (/** @type {number} */ id) => (S.toasts = S.toasts.filter((x) => x.id !== id));

/** Run a command. Failures are shown, never swallowed. */
export async function run(/** @type {string} */ cmd, /** @type {Record<string, unknown>} */ args = {}) {
  try {
    return await api(cmd, args);
  } catch (e) {
    toast(/** @type {Error} */ (e).message, 'hit');
    throw e;
  }
}

export const missionOf = (/** @type {string | null} */ id) => S.missions.find((m) => m.id === id) ?? null;
export const crewOf = (/** @type {string | null} */ id) => S.crew.find((c) => c.id === id) ?? null;
export const squadOf = (/** @type {string | null} */ missionId) => S.crew.filter((c) => c.island_id === missionId);
export const bossOf = (/** @type {string | null} */ missionId) => squadOf(missionId).find((c) => c.role === BOSS_ROLE) ?? null;
/** The name an engine goes by ("Claude Code"), from what the machine scan found. */
export const engineName = (/** @type {string} */ id) => S.engines.find((e) => e.id === id)?.name ?? id;

/** Open a mission and talk to its squad leader. */
export function openMission(/** @type {string} */ id) {
  S.mission = id;
  const boss = bossOf(id);
  talkTo(boss?.id ?? squadOf(id)[0]?.id ?? null);
}

export function talkTo(/** @type {string | null} */ id) {
  S.talk = id;
  if (id && !S.msgs[id]) loadMsgs(id).catch(() => {});
}

/** Load a conversation. Messages that arrive while it loads are kept and merged in. */
export async function loadMsgs(/** @type {string} */ id) {
  if (!S.msgs[id]) S.msgs[id] = [];
  const got = await api('messages', { crew_id: id });
  const seen = new Set(got.map((/** @type {any} */ m) => m.id));
  S.msgs[id] = [...got, ...(S.msgs[id] ?? []).filter((m) => m.id == null || !seen.has(m.id))];
}

export async function load() {
  try {
    const st = await api('state');
    Object.assign(S, { settings: st.settings, missions: st.islands, crew: st.crew, tasks: st.tasks, needs: st.needs, engines: st.engines, detect: st.detect, spent: st.spent, error: '' });
    if (!missionOf(S.mission)) S.mission = S.missions[0]?.id ?? null;
    if (!crewOf(S.talk)) S.talk = null;
    if (S.mission && !S.talk) openMission(S.mission);
    // After a reconnect every conversation already open may have missed messages: refresh them all.
    await Promise.all(Object.keys(S.msgs).filter((id) => crewOf(id)).map((id) => loadMsgs(id).catch(() => {})));
    S.ready = true;
    cutShort();
  } catch (e) {
    S.error = /** @type {Error} */ (e).message;
    // The engine may still be starting (or restarting): keep knocking until it answers.
    if (!S.ready) setTimeout(load, 1500);
  }
}

/** Jobs the engine stopping cut short (a crash and restart, or YSOB closed mid-job) come back
 *  waiting on you, never re-sent by themselves (hub.rs). Say so once, in the app and on the
 *  desktop, since nobody watched them change. Each job is told once per session. */
function cutShort() {
  const KEY = 'ysob.cutShort';
  let seen = [];
  try {
    seen = JSON.parse(sessionStorage.getItem(KEY) ?? '[]');
  } catch {
    /* blocked storage: they may be told again after a reload */
  }
  const fresh = S.tasks.filter((x) => x.col === 'wait' && /^The engine stopped while /.test(x.ask ?? '') && !seen.includes(x.id));
  if (!fresh.length) return;
  try {
    sessionStorage.setItem(KEY, JSON.stringify([...seen, ...fresh.map((x) => x.id)]));
  } catch {
    /* as above */
  }
  const n = fresh.length;
  const text = `The engine stopped while ${n === 1 ? 'a job was' : `${n} jobs were`} underway. ${n === 1 ? 'It’s' : 'They’re'} waiting on you to send ${n === 1 ? 'it' : 'them'} back out.`;
  const first = fresh[0];
  toast(text, 'ask', { label: 'Show', run: () => { S.mission = first.island_id; if (first.crew_id) talkTo(first.crew_id); } });
  ping(n === 1 ? `Job waiting: ${first.title}` : `${n} jobs waiting on you`, text);
}

/** Start a mission: a folder, a name, and the squad leader to run it. */
export async function startMission(/** @type {string} */ name, /** @type {string} */ path, /** @type {string} */ engine) {
  const boss = bossCard();
  // No toast on failure: the form that called this shows the reason next to the field.
  const { id } = await api('island.add', { name, path });
  try {
    await api('crew.hire', { name: boss.name, engine, island_id: id, lane: 'WRITE', role: BOSS_ROLE, face: boss.slug });
  } catch (e) {
    // A mission nobody leads is no use: take it back so the list stays honest.
    await api('island.remove', { id }).catch(() => {});
    throw e;
  }
  await load();
  openMission(id);
  strike(t('strike.line'), t('strike.join', { name: boss.name, mission: name }), 'join', boss.name);
}

/** Take a mission off the list, with its squad. The folder itself is never touched. */
export async function removeMission(/** @type {string} */ id) {
  for (const c of squadOf(id)) await run('crew.fire', { id: c.id });
  await run('island.remove', { id });
  await load();
}

/** Bring a squad member onto the current mission. */
export async function hire(/** @type {string} */ name, /** @type {string} */ engine) {
  if (!S.mission) return;
  const m = member(name);
  await run('crew.hire', { name, engine, island_id: S.mission, lane: m?.lane ?? 'READ', role: m?.job ?? 'Squad', face: m?.slug ?? 'temp' });
  if (!S.mud) sfx.join();
}

/** A failed tool call: the squad member takes a hit and recovers. */
function flinch(/** @type {string} */ id) {
  if (S.hurt.includes(id)) return;
  S.hurt = [...S.hurt, id];
  setTimeout(() => (S.hurt = S.hurt.filter((x) => x !== id)), 900);
}

/** The hunter takes a squad member: the kill plays first, then the engine lets them go. */
export async function remove(/** @type {string} */ id) {
  const c = crewOf(id);
  if (!c || S.dying.includes(id)) return;
  S.dying = [...S.dying, id];
  if (!S.mud) sfx.kill();
  // Laser lock, the death sheet, a beat on the ground, then gone. No show in mud mode or
  // with reduced motion: they just go.
  await new Promise((r) => setTimeout(r, S.mud || reduced() ? 300 : 2800));
  try {
    await run('crew.fire', { id });
    toast(t('remove.done', { name: c.name }), 'hit');
  } finally {
    S.dying = S.dying.filter((x) => x !== id);
  }
}

/** @type {ReturnType<typeof setTimeout> | undefined} */
let strikeTimer;
/** The big on-screen moment. Mud mode and reduced motion keep it quiet: a toast instead. */
export function strike(/** @type {string} */ line, /** @type {string} */ sub, /** @type {'join'|'hit'|'lie'} */ kind = 'hit', who = '') {
  if (S.mud || reduced()) return toast(`${line} ${sub}`, kind === 'join' ? 'go' : 'hit');
  S.strike = { line, sub, kind, who };
  if (kind === 'hit') sfx.strike();
  else if (kind === 'lie') sfx.lie();
  else sfx.join();
  clearTimeout(strikeTimer);
  strikeTimer = setTimeout(() => (S.strike = null), 2800);
}
/** Esc or a click waves the big moment away. */
export function dismissStrike() {
  clearTimeout(strikeTimer);
  S.strike = null;
}

export function setThermal(/** @type {boolean} */ on) {
  S.thermal = on;
  if (!S.mud) sfx.thermal(on);
  else sfx.thermal(false);
}
export function setMud(/** @type {boolean} */ on) {
  S.mud = on;
  // Mud is quiet: the thermal hum goes too.
  if (on) sfx.thermal(false);
  else if (S.thermal) sfx.thermal(true);
}

/** Words an agent uses when it claims the job is finished, and words that take the claim back. */
const CLAIMS = /\b(done|fixed|complete[d]?|all (tests )?pass(ing)?|works now|finished|shipped)\b/i;
const HEDGE = /\b(not|isn't|aren't|haven't|hasn't|yet|still|won't|can't|until|once|before|unless)\b|n't\b/i;
/** A real test or check run, not any line that happens to contain the word "test". */
export const TESTY = /(^|[;&|]\s*)(bun (run )?test|cargo (test|check|clippy)|npm (run )?test|pnpm (run )?test|yarn test|pytest|go test|vitest|jest|bunx? (svelte-check|tsc|vitest|playwright test)|svelte-check|tsc\b)/;
/** A sentence makes the claim only if nothing in it hedges. */
export const claims = (/** @type {string} */ text) => text.split(/(?<=[.!?\n])\s+/).some((s) => CLAIMS.test(s) && !HEDGE.test(s));

/**
 * The lie detector: a turn that ends with a "done" claim while the last test run in that
 * turn failed. Only for conversations loaded in this window.
 */
function liar(/** @type {string} */ crewId) {
  const list = S.msgs[crewId];
  if (!list?.length) return false;
  let i = list.length - 1;
  while (i >= 0 && list[i].kind !== 'text') i--;
  if (i < 0 || !claims(list[i].text ?? '')) return false;
  for (let j = i - 1; j >= 0 && list[j].kind !== 'me' && list[j].kind !== 'job'; j--) {
    const m = list[j];
    if (m.kind === 'tool' && m.name === 'Bash' && TESTY.test(m.arg ?? '')) return m.ok === false;
  }
  return false;
}

/** Say something to whoever is open, with any pictures pasted in ({type, data} in base64). */
export async function say(/** @type {string} */ text, /** @type {{type: string, data: string}[]} */ images = []) {
  const c = crewOf(S.talk);
  if (!c || (!text.trim() && !images.length)) return false;
  await run('crew.say', { id: c.id, text: text.trim(), ...(images.length ? { images } : {}) });
  return true;
}

// ---------- follow-ups ----------
let queueId = Date.now();
/** Hold a follow-up until the agent's turn is over. */
export function enqueue(/** @type {string} */ crewId, /** @type {string} */ text) {
  S.queue[crewId] = [...(S.queue[crewId] ?? []), { id: ++queueId, text }];
}
export function unqueue(/** @type {string} */ crewId, /** @type {number} */ id) {
  S.queue[crewId] = (S.queue[crewId] ?? []).filter((x) => x.id !== id);
}
export function queueUp(/** @type {string} */ crewId, /** @type {number} */ id) {
  const list = [...(S.queue[crewId] ?? [])];
  const i = list.findIndex((x) => x.id === id);
  if (i > 0) [list[i - 1], list[i]] = [list[i], list[i - 1]];
  S.queue[crewId] = list;
}
/** Send one follow-up now, ahead of the rest. It goes back in place if the engine says no. */
export async function sendQueued(/** @type {string} */ crewId, /** @type {number} */ id) {
  const item = S.queue[crewId]?.find((x) => x.id === id);
  if (!item) return;
  unqueue(crewId, id);
  try {
    await run('crew.say', { id: crewId, text: item.text });
  } catch {
    S.queue[crewId] = [item, ...(S.queue[crewId] ?? [])];
  }
}
/** An agent is free: the oldest follow-up goes, unless its queue is paused. */
function drain(/** @type {string} */ crewId) {
  const next = S.queue[crewId]?.[0];
  if (next && !S.queuePaused[crewId] && S.link) sendQueued(crewId, next.id);
}

/** One conversation as Markdown, for keeping or sharing: who said what, the steps, the asks. */
export function conversationMarkdown(/** @type {string} */ crewId) {
  const c = crewOf(crewId);
  const mission = missionOf(c?.island_id ?? null);
  const when = (/** @type {number} */ ts) => (ts ? new Date(ts * 1000).toLocaleString() : '');
  const lines = [`# ${c?.name ?? 'Conversation'}${mission ? ` on ${mission.name}` : ''}`, '', `Saved from YSOB, ${new Date().toLocaleString()}.`, ''];
  for (const m of S.msgs[crewId] ?? []) {
    if (m.kind === 'me') lines.push('', `## You${m.ts ? ` · ${when(m.ts)}` : ''}`, '', m.text ?? '', ...(m.images ? ['', `_(${m.images} picture${m.images === 1 ? '' : 's'})_`] : []), '');
    else if (m.kind === 'text') lines.push('', `## ${c?.name ?? 'Agent'}${m.ts ? ` · ${when(m.ts)}` : ''}`, '', m.text ?? '', '');
    else if (m.kind === 'tool' && (m.verb || m.name)) lines.push(`- ${m.blocked ? 'Stopped' : (m.verb ?? m.name)} \`${(m.arg ?? '').replace(/`/g, "'")}\`${m.ok === false ? ' (failed)' : ''}`);
    else if (m.kind === 'turn') lines.push('', `_${Math.round((m.ms ?? 0) / 1000)} s · ${m.tools ?? 0} steps${m.cost > 0 ? ` · ${money(m.cost)}` : ''}_`, '');
    else if (m.kind === 'error') lines.push(`> Error: ${m.text}`, '');
    else if (m.kind === 'bottle' || m.kind === 'bottle_out' || m.kind === 'job' || m.kind === 'bell' || m.kind === 'breaker') lines.push(`> ${(m.text ?? '').replace(/\n/g, '\n> ')}${m.answer ? ` (${m.answer})` : ''}`, '');
  }
  return lines.join('\n').replace(/\n{3,}/g, '\n\n').trim() + '\n';
}

/** A finished test run: a clean burst when it passes, the hunter's clicks when it fails. */
function shot(/** @type {any} */ m) {
  if (S.mud || m?.kind !== 'tool' || m.name !== 'Bash' || m.ok === undefined || !TESTY.test(m.arg ?? '')) return;
  m.ok ? sfx.pass() : sfx.clicks();
}

/** Asks being answered right now, so a double click or a held key can't answer twice. */
const answering = new Set();
export async function answer(/** @type {number} */ id, /** @type {string} */ a) {
  if (answering.has(id)) return;
  answering.add(id);
  try {
    await run('need.answer', { id, answer: a });
  } catch {
    /* shown as a toast */
  } finally {
    answering.delete(id);
  }
}

/** The day's budget: a warning at four fifths, the alarm when it is gone. Once each per day. */
let warned = '';
function budgetWatch() {
  const { budget, hard_stop } = S.settings;
  if (!(budget > 0)) return;
  const day = new Date().toDateString();
  const level = S.spent >= budget ? 'out' : S.spent >= budget * 0.8 ? 'low' : '';
  if (!level || warned === day + level || (level === 'low' && warned === day + 'out')) return;
  warned = day + level;
  const raise = { label: 'Raise it', run: () => (S.modal = 'settings') };
  if (level === 'out') {
    if (!S.mud) sfx.destruct();
    toast(hard_stop ? `${t('budget')}: today’s ${money(budget)} is spent. The squad is stood down.` : `${t('budget')}: today’s ${money(budget)} is spent.`, 'hit', raise);
  } else toast(`${t('budget')}: ${money(Math.max(0, budget - S.spent))} left today.`, 'ask', raise);
}

/** Terminal output for squadmates who run in a terminal (crew id -> listener). */
const terminals = new Map();
/** Listen to one squadmate's terminal. Returns the way to stop listening. */
export function onTerminal(/** @type {string} */ id, /** @type {(ev: {data?: string, end?: boolean}) => void} */ fn) {
  terminals.set(id, fn);
  return () => terminals.get(id) === fn && terminals.delete(id);
}

export function start() {
  load();
  connect(
    (ev) => {
      const d = ev.data;
      switch (ev.type) {
        case 'crew': {
          const i = S.crew.findIndex((c) => c.id === d.id);
          const before = i >= 0 ? S.crew[i] : null;
          if (i >= 0) S.crew[i] = d;
          else S.crew.push(d);
          if (before && before.status !== 'held' && d.status === 'held') strike(t('strike.line'), t('strike.held', { name: d.name }), 'hit');
          if (before && before.status === 'working' && d.status !== 'working' && liar(d.id)) strike(t('lie.line'), t('lie.sub', { name: d.name }), 'lie');
          if (before && before.status !== 'idle' && d.status === 'idle') drain(d.id);
          if (!S.talk && d.island_id === S.mission) talkTo(d.id);
          break;
        }
        case 'crew_gone':
          S.crew = S.crew.filter((c) => c.id !== d.id);
          delete S.queue[d.id];
          delete S.msgs[d.id];
          delete S.stream[d.id];
          if (S.talk === d.id) openMission(S.mission ?? '');
          break;
        case 'detect':
          S.detect = d;
          S.engines = d.engines;
          break;
        case 'stream':
          S.stream[d.id] = d.text;
          break;
        case 'msg':
          if (S.msgs[d.crew_id]) S.msgs[d.crew_id].push(d.msg);
          break;
        case 'msg_update': {
          shot(d.msg);
          if (d.msg.kind === 'tool' && d.msg.ok === false) flinch(d.crew_id);
          const list = S.msgs[d.crew_id];
          const i = list ? list.findIndex((m) => m.id === d.msg.id) : -1;
          if (i >= 0) list[i] = d.msg;
          break;
        }
        case 'needs': {
          const fresh = d.filter((/** @type {any} */ n) => !S.needs.some((o) => o.id === n.id));
          if (fresh.length && !S.mud) sfx.needs();
          for (const n of fresh) {
            const who = crewOf(n.crew_id)?.name ?? 'A squad member';
            ping(n.kind === 'breaker' ? t('held.title', { name: who }) : `${who} needs your OK`, n.title);
          }
          S.needs = d;
          break;
        }
        case 'tasks': {
          for (const task of d) {
            const was = S.tasks.find((o) => o.id === task.id);
            if (was && was.col !== 'wait' && task.col === 'wait') {
              const who = S.crew.find((c) => c.id === task.crew_id)?.name ?? 'The squad';
              ping(`${who} is waiting on you`, `#${task.id} ${task.title}: ${task.ask ?? 'needs an answer'}`);
            }
            if (was && was.col !== 'done' && task.col === 'done') ping(`Done with proof: ${task.title}`, task.proof ?? '');
            if (was && was.col !== 'done' && task.col === 'done') {
              const closer = S.crew.find((c) => c.id === (task.crew_id ?? was.crew_id));
              const who = closer?.name ?? task.by ?? 'The squad';
              if (closer && !S.win.includes(closer.id)) {
                S.win = [...S.win, closer.id];
                setTimeout(() => (S.win = S.win.filter((x) => x !== closer.id)), 1000);
              }
              S.chopper = { title: task.title, proof: task.proof ?? '', who };
              if (!S.mud) sfx.done();
              setTimeout(() => S.chopper?.title === task.title && (S.chopper = null), 6000);
            }
          }
          S.tasks = d;
          break;
        }
        case 'islands': S.missions = d; break;
        case 'settings': S.settings = d; budgetWatch(); break;
        case 'spent': S.spent = d; budgetWatch(); break;
        case 'toast': toast(d.text); break;
        case 'pty': terminals.get(d.id)?.({ data: d.data }); break;
        case 'pty_end': terminals.get(d.id)?.({ end: true }); break;
        case 'resync': load(); break;
      }
    },
    (up) => {
      if (up && !S.link) load();
      S.link = up;
    }
  );
}

/** Money the way people read it: cents, or a third digit under ten cents. */
export const money = (/** @type {number} */ n) => '$' + (n > 0 && n < 0.1 ? n.toFixed(3) : n.toFixed(2));

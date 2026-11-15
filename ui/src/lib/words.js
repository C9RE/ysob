// The app's theme words: who the squad are and what things are called. A theme pack can
// replace any of them (pack.json: `roster` and `words`); these are the built-in ones, used
// when no pack is installed or a pack leaves something out. Pure: no state, no DOM.

/**
 * @typedef {{name: string, slug: string, job: string, does: string, lane: 'READ'|'WRITE'|'HOST', hue: number, boss?: boolean}} Member
 */

/** The built-in squad. The first one leads every mission. @type {Member[]} */
export const DEFAULT_ROSTER = [
  { name: 'Chief', slug: 'chief', job: 'Squad leader', does: 'Plans the mission and gives the orders', lane: 'WRITE', hue: 32, boss: true },
  { name: 'Scout', slug: 'scout', job: 'Recon', does: 'Reads the code and finds things first', lane: 'READ', hue: 150 },
  { name: 'Gunner', slug: 'gunner', job: 'Builder', does: 'Builds features and runs the commands', lane: 'WRITE', hue: 8 },
  { name: 'Watch', slug: 'watch', job: 'Reviewer', does: 'Reviews changes and runs the tests', lane: 'WRITE', hue: 205 },
  { name: 'Sapper', slug: 'sapper', job: 'Demolitions', does: 'Risky work: migrations, deploys, big refactors', lane: 'HOST', hue: 48 },
  { name: 'Radio', slug: 'radio', job: 'Research', does: 'Research, docs and keeping you posted', lane: 'READ', hue: 265 },
  { name: 'Suit', slug: 'suit', job: 'Second opinion', does: 'A second opinion from another engine', lane: 'READ', hue: 330 },
  { name: 'Ranger', slug: 'ranger', job: 'Tester', does: 'Checks the work really works', lane: 'WRITE', hue: 95 }
];

/** Every string a pack may replace. `{name}`, `{boss}` and `{mission}` are filled in. */
export const DEFAULT_WORDS = /** @type {const} */ ({
  tagline: 'Your AI agents are the squad. The bugs are the hunter.',
  loading: 'Dropping in…',
  'welcome.title': 'Welcome to the jungle.',
  'welcome.body': 'Point the squad at a project folder. {boss} runs the mission and hands out the work.',
  'mission.lead': '{boss} leads every mission. Point them at a project folder.',
  'mission.go': 'Drop in',
  'mission.going': 'Dropping in…',
  'boss.job': 'The boss',
  'boss.ask': 'What’s the mission?',
  'boss.hint': 'Tell {boss} what you need. They’ll do it themselves or split it across the squad.',
  'boss.placeholder': 'Give {boss} the mission…',
  'strike.line': 'YOU SON OF A BITCH!',
  'strike.join': '{name} is on the ground at {mission}',
  'strike.held': '{name} is caught in the net',
  'lie.line': 'LIAR!',
  'lie.sub': '{name} says it’s done. The tests say otherwise.',
  'done.kicker': 'Job proven',
  'held.title': '{name} is caught in the net',
  'held.card': 'Caught by the hunter.',
  'held.release': 'Let them carry on',
  'held.short': '{name} is caught',
  'remove.ask': 'Remove {name} from the squad?',
  'remove.dying': 'Being hunted…',
  'remove.done': '{name} was taken.',
  budget: 'Self-destruct',
  'station.helm': 'Giving orders',
  'station.chart': 'Tracking',
  'station.work': 'Setting traps',
  'station.hatch': 'At the armoury',
  'station.cannons': 'On the firing range',
  'station.bow': 'Up the lookout',
  'station.cabin': 'On the radio',
  'station.brig': 'Caught in the net',
  'station.rest': 'At the campfire'
});

/** @typedef {keyof typeof DEFAULT_WORDS} WordKey */

const LANES = ['READ', 'WRITE', 'HOST'];
/** The engine's rule for a squadmate's name: letters, numbers, spaces, hyphens, apostrophes. */
const NAME = /^[\p{L}\p{N} '-]{1,24}$/u;
const SLUG = /^[a-z0-9_-]{1,24}$/;
const text = (/** @type {unknown} */ v, /** @type {number} */ max) => (typeof v === 'string' && v.trim() && v.trim().length <= max ? v.trim() : null);

/**
 * A pack's roster, checked. Entries the engine would refuse are dropped; there is always
 * exactly one boss (the first marked, else the first). Null when nothing usable is left.
 * @param {unknown} raw
 * @returns {Member[] | null}
 */
export function cleanRoster(raw) {
  if (!Array.isArray(raw)) return null;
  /** @type {Member[]} */
  const out = [];
  for (const e of raw.slice(0, 16)) {
    if (!e || typeof e !== 'object') continue;
    const name = text(e.name, 24);
    if (!name || !NAME.test(name) || out.some((m) => m.name.toLowerCase() === name.toLowerCase())) continue;
    out.push({
      name,
      slug: typeof e.slug === 'string' && SLUG.test(e.slug) ? e.slug : 'temp',
      job: text(e.job, 32) ?? 'Squad',
      does: text(e.does, 90) ?? '',
      lane: LANES.includes(e.lane) ? e.lane : 'READ',
      hue: Number.isFinite(e.hue) ? ((Math.round(e.hue) % 360) + 360) % 360 : 120,
      boss: e.boss === true
    });
  }
  if (!out.length) return null;
  const lead = out.find((m) => m.boss) ?? out[0];
  for (const m of out) m.boss = m === lead;
  return out;
}

/**
 * A pack's words, checked: known keys only, short plain strings only.
 * @param {unknown} raw
 * @returns {Partial<Record<WordKey, string>>}
 */
export function cleanWords(raw) {
  /** @type {Partial<Record<WordKey, string>>} */
  const out = {};
  if (!raw || typeof raw !== 'object') return out;
  for (const k of /** @type {WordKey[]} */ (Object.keys(DEFAULT_WORDS))) {
    const v = text(/** @type {Record<string, unknown>} */ (raw)[k], 160);
    if (v) out[k] = v;
  }
  return out;
}

/**
 * The words for `key`: the pack's if it has them, else the built-in ones, with `{name}` style
 * blanks filled from `vars`. A blank with nothing to fill it is left out.
 * @param {Partial<Record<WordKey, string>>} words
 * @param {WordKey} key
 * @param {Record<string, string | undefined>} [vars]
 */
export function say(words, key, vars = {}) {
  return (words[key] ?? DEFAULT_WORDS[key] ?? '').replace(/\{(\w+)\}/g, (_, v) => vars[v] ?? '').replace(/\s{2,}/g, ' ').trim();
}

/** Engine status to a short word and a colour token. */
export const STATUS = /** @type {Record<string, {word: string, tone: string}>} */ ({
  working: { word: 'Working', tone: 'go' },
  idle: { word: 'Ready', tone: 'idle' },
  waiting: { word: 'Needs you', tone: 'ask' },
  held: { word: 'Caught', tone: 'hit' },
  off: { word: 'Off', tone: 'idle' }
});

/** The engine's role string for the squad leader (an id, never shown). */
export const BOSS_ROLE = 'Captain';

/** What an agent may do, in the words the app uses for its lane. */
export const LANE_WORDS = /** @type {Record<string, string>} */ ({ READ: 'Look only', WRITE: 'Can edit', HOST: 'Full access' });

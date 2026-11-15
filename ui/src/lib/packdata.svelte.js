// The theme pack in use: its pictures and sounds (pack.json) and its words (lib/words.js).
// The installed pack lives in /packs/jungle; with none there, the default pack that ships with
// the app is used. Every consumer falls back gracefully when an entry is missing.
import { setClips } from './sound.js';
import { DEFAULT_ROSTER, cleanRoster, cleanWords, say } from './words.js';

/** Where a pack can be, in the order they are tried. */
const SLOTS = ['/packs/jungle', '/packs/default'];

export const PK = $state({
  /** The folder the pack in use was loaded from. */
  base: SLOTS[0],
  /** @type {Record<string, {portrait?: string, anchor?: Record<string, number[]>, anims?: Record<string, {sheet: string, frames: number, w: number, h: number, ms: number, muzzle?: [number, number], anchor?: [number, number]}>}>} */
  crew: {},
  /** @type {{portrait?: string, anims?: Record<string, any>}} */
  hunter: {},
  /** @type {Record<string, string>} */
  sounds: {},
  /** @type {Record<string, string>} */
  scenes: {},
  /** The splash picture, if the pack has one. @type {string | null} */
  handshake: null,
  /** Who the squad are. @type {import('./words.js').Member[]} */
  roster: DEFAULT_ROSTER,
  /** The pack's own words for things. @type {Partial<Record<import('./words.js').WordKey, string>>} */
  words: {},
  /** True once a pack was read (or none was found): the splash waits for it. */
  loaded: false
});

/** @param {string} path a file inside the pack */
export const url = (path) => `${PK.base}/${path}`;

/** The app's words for `key`, in the pack's voice. @param {import('./words.js').WordKey} key @param {Record<string, string | undefined>} [vars] */
export const t = (key, vars) => say(PK.words, key, vars);

/** The roster card for a name, if it is one of the pack's squad. @param {string} [name] */
export const member = (name = '') => PK.roster.find((m) => m.name.toLowerCase() === name.toLowerCase());
/** The squad leader's card. */
export const bossCard = () => PK.roster.find((m) => m.boss) ?? PK.roster[0];
/** The pack's art key for a squadmate: the one saved when they were hired, else by name. @param {{name: string, face?: string}} c */
export const slugOf = (c) => (c.face && c.face !== 'temp' ? c.face : (member(c.name)?.slug ?? ''));

export async function loadPack() {
  for (const base of SLOTS) {
    try {
      const r = await fetch(`${base}/pack.json`, { cache: 'no-cache' });
      if (!r.ok) continue;
      const p = await r.json();
      Object.assign(PK, {
        base,
        crew: p.crew ?? {},
        hunter: p.hunter ?? {},
        sounds: p.sounds ?? {},
        scenes: p.scenes ?? {},
        handshake: typeof p.handshake === 'string' ? p.handshake : null,
        roster: cleanRoster(p.roster) ?? DEFAULT_ROSTER,
        words: cleanWords(p.words)
      });
      setClips(PK.sounds, base);
      // warm the browser cache for the scene and the squad's first poses, so nothing pops in
      const urls = [PK.scenes.panel, PK.handshake, ...Object.values(PK.crew).flatMap((c) => [c.portrait, c.anims?.idle?.sheet])].filter(Boolean);
      await Promise.all(urls.map((u) => new Promise((res) => { const im = new Image(); im.onload = im.onerror = res; im.src = url(/** @type {string} */ (u)); })));
      break;
    } catch {
      /* not a pack: try the next place */
    }
  }
  // No pack at all (a stripped build): silhouettes, the built-in squad and synth sounds.
  PK.loaded = true;
}

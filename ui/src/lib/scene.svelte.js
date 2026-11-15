// How alive the squad's jungle is. Per machine, kept in localStorage (a convenience, so a
// missing or blocked store just means the defaults).
const KEY = 'ysob.scene';

/** @typedef {{ wander: boolean, hunter: 'often' | 'sometimes' | 'never', effects: 'full' | 'light' | 'off', weather: 'live' | 'clear', daylight: 'clock' | 'night', sightNow: boolean, stormNow: boolean }} Scene */

/** @type {Scene} */
const DEFAULTS = { wander: true, hunter: 'sometimes', effects: 'full', weather: 'live', daylight: 'clock', sightNow: false, stormNow: false };

function load() {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? '{}');
    return { ...DEFAULTS, ...v, sightNow: false, stormNow: false };
  } catch {
    return { ...DEFAULTS };
  }
}

export const scene = $state(/** @type {Scene} */ (load()));

/** What the sky is doing right now (not kept): the time of day and the weather, which the
 *  squad reads too (nobody strolls about in a downpour).
 *  @type {{ phase: 'dawn' | 'day' | 'dusk' | 'night', weather: 'clear' | 'mist' | 'drizzle' | 'rain' | 'storm' }} */
export const sky = $state({ phase: 'night', weather: 'clear' });

$effect.root(() => {
  $effect(() => {
    const { wander, hunter, effects, weather, daylight } = scene;
    try {
      localStorage.setItem(KEY, JSON.stringify({ wander, hunter, effects, weather, daylight }));
    } catch {
      /* private window or blocked storage: settings last for this session only */
    }
  });
});

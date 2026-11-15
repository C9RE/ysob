// Sounds for squad events. Pack clips play when the pack has them; everything else is
// synthesised here, so the app always sounds right even with an empty pack.

/** @type {Record<string, string>} event -> pack file */
let clips = {};
/** The folder those files are in. */
let base = '';
let enabled = false;
let volume = 0.7;
/** @type {AudioContext | null} */ let ctx = null;
/** The synth's master bus: a gentle compressor, plus a reverb send for the big moments. */
/** @type {AudioNode | null} */ let bus = null;
/** @type {AudioNode | null} */ let verb = null;
function wire() {
  if (!ctx || bus) return;
  const comp = ctx.createDynamicsCompressor();
  comp.threshold.value = -16;
  comp.ratio.value = 4;
  comp.connect(ctx.destination);
  bus = comp;
  // A jungle-sized room: two seconds of decaying noise as the impulse.
  const len = Math.floor(ctx.sampleRate * 2.2);
  const ir = ctx.createBuffer(2, len, ctx.sampleRate);
  for (let c = 0; c < 2; c++) {
    const d = ir.getChannelData(c);
    for (let i = 0; i < len; i++) d[i] = (Math.random() * 2 - 1) * Math.pow(1 - i / len, 3.2);
  }
  const conv = ctx.createConvolver();
  conv.buffer = ir;
  const wet = ctx.createGain();
  wet.gain.value = 0.45;
  conv.connect(wet).connect(comp);
  verb = conv;
}
/** Where a synth voice goes: the bus, and the reverb too when asked. */
function out(/** @type {AudioNode} */ node, /** @type {boolean} */ wet = false) {
  wire();
  node.connect(/** @type {AudioNode} */ (bus));
  if (wet && verb) node.connect(verb);
}
/** @type {Map<string, HTMLAudioElement>} */ const playing = new Map();

/** @param {Record<string, string>} map @param {string} folder */
export function setClips(map, folder) {
  clips = map || {};
  base = folder;
}
/** Sound on or off. Off stops whatever is playing. Browsers hold audio until the first click
 *  or key press, so turning it on before then just arms it. */
export function setEnabled(/** @type {boolean} */ on) {
  enabled = on;
  if (on && !ctx && typeof AudioContext !== 'undefined') {
    ctx = new AudioContext();
    if (ctx.state === 'suspended') {
      const wake = () => ctx?.resume().catch(() => {});
      addEventListener('pointerdown', wake, { once: true });
      addEventListener('keydown', wake, { once: true });
    }
  }
  wire();
  if (!on) {
    for (const a of playing.values()) a.pause();
    playing.clear();
  }
}
/** Master volume, 0 to 1. Clips already playing follow it. */
export function setVolume(/** @type {number} */ v) {
  volume = Math.max(0, Math.min(1, v));
  for (const a of playing.values()) a.volume = Math.min(1, volume * Number(a.dataset.gain ?? 1));
}

/** Play a pack clip. Resolves when it ends (or at once if missing/off). */
function clip(/** @type {string} */ name, /** @type {{loop?: boolean, gain?: number}} */ o = {}) {
  const file = clips[name];
  if (!enabled || !file) return Promise.resolve(false);
  const a = new Audio(`${base}/${file}`);
  a.dataset.gain = String(o.gain ?? 1);
  a.volume = volume * (o.gain ?? 1);
  a.loop = !!o.loop;
  playing.set(name, a);
  return new Promise((res) => {
    a.onended = () => {
      playing.delete(name);
      res(true);
    };
    a.onerror = () => res(false);
    a.play().catch(() => res(false));
  });
}
export const stop = (/** @type {string} */ name) => {
  playing.get(name)?.pause();
  playing.delete(name);
};

// ---------- synthesised effects ----------
function tone(/** @type {{f: number, f2?: number, t?: number, type?: OscillatorType, g?: number, at?: number, wet?: boolean}} */ o) {
  if (!enabled || !ctx) return;
  const t0 = ctx.currentTime + (o.at ?? 0);
  const osc = ctx.createOscillator();
  const g = ctx.createGain();
  osc.type = o.type ?? 'sine';
  osc.frequency.setValueAtTime(o.f, t0);
  if (o.f2) osc.frequency.exponentialRampToValueAtTime(o.f2, t0 + (o.t ?? 0.2));
  g.gain.setValueAtTime(0.0001, t0);
  g.gain.exponentialRampToValueAtTime((o.g ?? 0.2) * volume, t0 + 0.01);
  g.gain.exponentialRampToValueAtTime(0.0001, t0 + (o.t ?? 0.2));
  osc.connect(g);
  out(g, o.wet);
  osc.start(t0);
  osc.stop(t0 + (o.t ?? 0.2) + 0.02);
}
function noise(/** @type {{t?: number, g?: number, at?: number, hp?: number, bp?: number, q?: number, wet?: boolean, pow?: number}} */ o = {}) {
  if (!enabled || !ctx) return;
  const len = Math.floor(ctx.sampleRate * (o.t ?? 0.3));
  const buf = ctx.createBuffer(1, len, ctx.sampleRate);
  const d = buf.getChannelData(0);
  for (let i = 0; i < len; i++) d[i] = (Math.random() * 2 - 1) * Math.pow(1 - i / len, o.pow ?? 1);
  const src = ctx.createBufferSource();
  src.buffer = buf;
  const f = ctx.createBiquadFilter();
  f.type = o.bp ? 'bandpass' : 'highpass';
  f.frequency.value = o.bp ?? o.hp ?? 1800;
  if (o.q) f.Q.value = o.q;
  const g = ctx.createGain();
  g.gain.value = (o.g ?? 0.12) * volume;
  src.connect(f).connect(g);
  out(g, o.wet);
  src.start(ctx.currentTime + (o.at ?? 0));
}

const synth = {
  laser: () => {
    tone({ f: 1800, f2: 2600, t: 0.5, type: 'sawtooth', g: 0.05 });
    tone({ f: 3200, t: 0.08, type: 'square', g: 0.04, at: 0.55 });
  },
  cloak: () => {
    noise({ t: 0.7, g: 0.08, hp: 3000 });
    tone({ f: 600, f2: 2400, t: 0.6, type: 'triangle', g: 0.05 });
  },
  clicks: () => {
    for (let i = 0; i < 5; i++) tone({ f: 900 + Math.random() * 400, t: 0.03, type: 'square', g: 0.08, at: i * 0.07 });
  },
  radio: () => {
    noise({ t: 0.12, g: 0.06, hp: 1200 });
    tone({ f: 1320, t: 0.09, type: 'square', g: 0.05, at: 0.12 });
    tone({ f: 1760, t: 0.09, type: 'square', g: 0.05, at: 0.24 });
  },
  /** The targeting laser locking on: a thin rising whine that settles into a pulsing lock tone. */
  lock: () => {
    tone({ f: 1400, f2: 3100, t: 0.75, type: 'sawtooth', g: 0.035 });
    tone({ f: 1405, f2: 3120, t: 0.75, type: 'sine', g: 0.05 });
    for (let i = 0; i < 3; i++) tone({ f: 3400, t: 0.05, type: 'square', g: 0.035, at: 0.62 + i * 0.09 });
  },
  /** The shoulder plasma caster: a short charge, an electric crack, a heavy discharge with a sub
   *  boom, then crackle and the jungle's echo. */
  plasma: () => {
    // charge
    tone({ f: 380, f2: 2200, t: 0.16, type: 'sawtooth', g: 0.05 });
    // crack: bright filtered noise, very short
    noise({ t: 0.09, g: 0.5, bp: 3200, q: 0.8, at: 0.15, pow: 2, wet: true });
    // the bolt: a screaming downward sweep, two detuned voices
    tone({ f: 2600, f2: 140, t: 0.42, type: 'sawtooth', g: 0.16, at: 0.15, wet: true });
    tone({ f: 2450, f2: 120, t: 0.46, type: 'square', g: 0.07, at: 0.16, wet: true });
    // body and sub boom
    noise({ t: 0.6, g: 0.32, bp: 700, q: 0.6, at: 0.17, pow: 2.5, wet: true });
    tone({ f: 95, f2: 32, t: 0.7, type: 'sine', g: 0.5, at: 0.17 });
    // crackle tail
    for (let i = 0; i < 9; i++) noise({ t: 0.025, g: 0.12 + Math.random() * 0.1, bp: 2500 + Math.random() * 3500, q: 3, at: 0.35 + i * 0.05 + Math.random() * 0.04, wet: true });
  },
  destruct: () => {
    for (let i = 0; i < 6; i++) tone({ f: 1400, t: 0.06, type: 'square', g: 0.05, at: i * (0.32 - i * 0.04) });
  },
  pass: () => {
    tone({ f: 660, t: 0.08, g: 0.07 });
    tone({ f: 990, t: 0.12, g: 0.07, at: 0.08 });
  },
  pop: () => tone({ f: 520, f2: 780, t: 0.08, type: 'triangle', g: 0.05 }),
  /** Thunder a little way off: a low rumble that rolls and fades. */
  thunder: () => {
    noise({ t: 2.6, g: 0.22, bp: 90, q: 0.7, pow: 1.6, wet: true });
    noise({ t: 1.4, g: 0.12, bp: 220, q: 0.9, at: 0.12, pow: 2.2, wet: true });
    tone({ f: 55, f2: 32, t: 1.8, type: 'sine', g: 0.18, at: 0.05 });
  }
};

/** Clip if the pack has it, otherwise the synth version. */
async function either(/** @type {string} */ name, /** @type {keyof typeof synth} */ fallback, /** @type {{gain?: number}} */ o = {}) {
  if (!(await clip(name, o))) synth[fallback]?.();
}

/** Squad events, as sound. */
export const sfx = {
  /** New squad member or a new mission: the handshake. */
  join: () => clip('join').then((ok) => ok || synth.pop()),
  /** The hunter locks on (three strikes). */
  strike: async () => {
    synth.laser();
    await new Promise((r) => setTimeout(r, 650));
    if (!(await clip('strike'))) await clip('roar', { gain: 0.6 });
  },
  /** An agent claimed done with failing tests. */
  lie: async () => {
    if (!(await clip('lie'))) synth.clicks();
    await clip('strike');
  },
  /** A squad member removed: the hunter takes them. Timed to the picture (Figure.svelte): the
   *  clicking starts as the three dots come in, they lock at 0.9 s, the bolt lands at 1 s, then
   *  the hunter snarls over the body. No laugh. */
  kill: async () => {
    const purr = clip('purr', { gain: 0.7 });
    if (!clips.purr) synth.clicks();
    synth.lock();
    await new Promise((r) => setTimeout(r, 850));
    stop('purr');
    void purr;
    synth.plasma();
    await new Promise((r) => setTimeout(r, 700));
    await clip('snarl', { gain: 0.75 });
  },
  needs: () => synth.radio(),
  cloak: () => either('cloak', 'cloak', { gain: 0.6 }),
  clicks: () => either('clicks', 'clicks'),
  pass: () => either('pass', 'pass', { gain: 0.35 }),
  /** A job closed with proof. */
  done: async () => {
    if (!clips.done && !clips.done_line) return synth.pass();
    clip('done', { gain: 0.5 });
    await clip('done_line');
  },
  /** Today's budget is spent. */
  destruct: () => either('destruct', 'destruct'),
  thunder: () => synth.thunder(),
  thermal: (/** @type {boolean} */ on) => (on ? clip('thermal', { loop: true, gain: 0.35 }) : stop('thermal'))
};

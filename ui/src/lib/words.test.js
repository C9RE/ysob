import { expect, test } from 'bun:test';
import { DEFAULT_ROSTER, DEFAULT_WORDS, cleanRoster, cleanWords, say } from './words.js';

test('built-in words are used when a pack leaves them out', () => {
  expect(say({}, 'lie.line')).toBe('LIAR!');
  expect(say({ 'lie.line': 'BUSTED!' }, 'lie.line')).toBe('BUSTED!');
  expect(say({}, 'strike.join', { name: 'Chief', mission: 'shop' })).toBe('Chief is on the ground at shop');
  // a blank nobody filled leaves no hole in the sentence
  expect(say({ 'strike.join': '{name} is in {place} now' }, 'strike.join', { name: 'Ada' })).toBe('Ada is in now');
});

test('a pack can only replace words the app knows, with short plain strings', () => {
  const w = cleanWords({ 'lie.line': '  VANCE!  ', 'made.up': 'x', tagline: 42, loading: 'x'.repeat(500), 'done.kicker': '' });
  expect(w).toEqual({ 'lie.line': 'VANCE!' });
  expect(cleanWords(null)).toEqual({});
  expect(cleanWords('nope')).toEqual({});
});

test('a pack roster is checked the way the engine checks names, and always has one boss', () => {
  const r = cleanRoster([
    { name: 'Hank', slug: 'hank', job: 'Tracker', does: 'Reads the code', lane: 'READ', hue: 510 },
    { name: 'Rex', slug: 'rex', job: 'The boss', does: 'Plans', lane: 'WRITE', boss: true },
    { name: 'hank', job: 'Twin' },
    { name: 'Bad <b>name</b>' },
    { name: 'Odd', slug: '../x', lane: 'ROOT' },
    'junk'
  ]);
  expect(r?.map((m) => m.name)).toEqual(['Hank', 'Rex', 'Odd']);
  expect(r?.filter((m) => m.boss).map((m) => m.name)).toEqual(['Rex']);
  expect(r?.[0].hue).toBe(150);
  expect(r?.[2]).toMatchObject({ slug: 'temp', lane: 'READ', job: 'Squad' });
  // nobody marked: the first one leads
  expect(cleanRoster([{ name: 'A' }, { name: 'B' }])?.map((m) => m.boss)).toEqual([true, false]);
  expect(cleanRoster([])).toBeNull();
  expect(cleanRoster({})).toBeNull();
});

test('the built-in squad is valid by its own rules', () => {
  expect(cleanRoster(DEFAULT_ROSTER)).toEqual(DEFAULT_ROSTER.map((m) => ({ ...m, boss: !!m.boss })));
  expect(Object.values(DEFAULT_WORDS).every((v) => typeof v === 'string' && v.length <= 160)).toBe(true);
});

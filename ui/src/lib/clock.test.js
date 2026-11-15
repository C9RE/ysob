import { expect, test } from 'bun:test';
import { formatDuration, ticking, copyText } from './clock.js';

test('a turn reads in plain units', () => {
  expect(formatDuration(420)).toBe('420 ms');
  expect(formatDuration(8_400)).toBe('8.4 sec');
  expect(formatDuration(42_000)).toBe('42 sec');
  expect(formatDuration(95_000)).toBe('1 min 35 sec');
  expect(formatDuration(120_000)).toBe('2 min');
  expect(formatDuration(3_900_000)).toBe('1 hour 5 min');
  expect(formatDuration(7_200_000)).toBe('2 hours');
  expect(formatDuration(-1)).toBe('');
  expect(formatDuration(Number.NaN)).toBe('');
});

test('the live clock counts like a stopwatch', () => {
  expect(ticking(7)).toBe('0:07');
  expect(ticking(95)).toBe('1:35');
  expect(ticking(3729)).toBe('1:02:09');
  expect(ticking(-3)).toBe('0:00');
});

test('a copy with no clipboard at all says it failed', async () => {
  expect(await copyText('x')).toBe(false);
});

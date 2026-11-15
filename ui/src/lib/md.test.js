import { test, expect } from 'bun:test';
import { md } from './md.js';
test('blocks', () => {
  const h = md('## Plan\n\n1. Hank maps\n2. Moose builds\n\n- a\n- b\n\n> careful\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n```js\nconst x = "hi"; // note\n```\nDone **now**.');
  expect(h).toContain('<h3>Plan</h3>');
  expect(h).toContain('<ol><li>Hank maps</li><li>Moose builds</li></ol>');
  expect(h).toContain('<ul><li>a</li><li>b</li></ul>');
  expect(h).toContain('<blockquote>careful</blockquote>');
  expect(h).toContain('<th>a</th>');
  expect(h).toContain('<span class="t-k">const</span>');
  expect(h).toContain('class="t-c">// note');
  expect(h).toContain('<span>js</span><button type="button" class="copy" data-copy>Copy</button>');
  expect(h).toContain('<strong>now</strong>');
});
test('safe', () => {
  const h = md('<img src=x onerror=alert(1)> [x](javascript:alert(1)) <script>bad()</script> `<b>`');
  expect(h).not.toContain('<img');
  expect(h).not.toContain('<script');
  expect(h).not.toContain('href="javascript');
  expect(h).toContain('&lt;img');
  expect(h).toContain('<code>&lt;b&gt;</code>');
});
test('links and an unclosed fence mid-stream', () => {
  expect(md('see https://docs.rs/serde.')).toContain('<a href="https://docs.rs/serde" target="_blank" rel="noreferrer noopener">https://docs.rs/serde</a>.');
  expect(md('```rust\nfn main() {')).toContain('<span class="t-k">fn</span>');
});

test('task lists', () => {
  const h = md('- [ ] Hank maps\n- [x] Moose builds');
  expect(h).toContain('<ul class="tasks"><li class="task"><span class="box" aria-hidden="true"></span>Hank maps</li><li class="task done">');
});

test('the review attacks stay inert', () => {
  // Independent review H2: a link pattern inside a link address.
  const h = md("see [docs](https://x.dev/(https://y/onmouseover=location='javascript:alert%28document.domain%29'//)");
  expect(h).not.toMatch(/<a[^>]*\son\w+=/i);
  expect(h).not.toContain('onmouseover="');
  // Second review: emphasis inside an address must not inject tags into the attribute.
  const d = md('https://example.com/_foo_/bar and [x](https://e.com/*a*/~~b~~)');
  for (const m of d.matchAll(/href="([^"]*)"/g)) expect(m[1]).not.toMatch(/[<>]/);
  expect(d).toContain('href="https://example.com/_foo_/bar"');
});

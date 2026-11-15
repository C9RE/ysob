// Markdown for agent replies, the way chat apps show it: headings, lists, quotes, tables, links,
// and code blocks with a language label, light highlighting and a copy button.
// Safe by construction: everything is escaped first, links must be http(s), no raw HTML gets through.

const esc = (/** @type {string} */ s) => s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c] ?? c);

const KEYWORDS = new Set(
  ('const let var function return if else for while do switch case break continue new class extends import from export default async await try catch finally throw typeof instanceof in of ' +
    'fn pub mut impl struct enum trait use mod match self Self where loop move ref static type as crate super dyn ' +
    'def lambda pass yield with elif None True False and or not is ' +
    'true false null undefined void this package interface func go defer chan map range select').split(' ')
);

/** Light syntax colouring on already-escaped code: comments, strings, numbers, keywords. */
function highlight(/** @type {string} */ code) {
  return code.replace(
    /(\/\/[^\n]*|#(?![\da-f]{3,6}\b)[^\n]*|\/\*[\s\S]*?\*\/)|(&quot;(?:[^&]|&(?!quot;))*?&quot;|&#39;(?:[^&]|&(?!#39;))*?&#39;|`[^`]*`)|\b(\d+(?:\.\d+)?)\b|\b([A-Za-z_]\w*)\b/g,
    (m, com, str, num, word) => {
      if (com) return `<span class="t-c">${com}</span>`;
      if (str) return `<span class="t-s">${str}</span>`;
      if (num) return `<span class="t-n">${num}</span>`;
      if (word && KEYWORDS.has(word)) return `<span class="t-k">${word}</span>`;
      return m;
    }
  );
}

/** Inline marks on escaped text. Code spans and links are swapped for placeholders before any
 *  other mark runs, so nothing can match inside a link's address or a code span. */
function inline(/** @type {string} */ s) {
  /** @type {string[]} */
  const codes = [];
  /** @type {string[]} */
  const links = [];
  const a = (/** @type {string} */ href, /** @type {string} */ text) => `\u0001${links.push(`<a href="${href}" target="_blank" rel="noreferrer noopener">${text}</a>`) - 1}\u0001`;
  const marks = (/** @type {string} */ t) =>
    t
      .replace(/\*\*([^*\n]+)\*\*/g, '<strong>$1</strong>')
      .replace(/(^|[^*\w])\*([^*\n]+)\*(?!\w)/g, '$1<em>$2</em>')
      .replace(/(^|[^_\w])_([^_\n]+)_(?!\w)/g, '$1<em>$2</em>')
      .replace(/~~([^~\n]+)~~/g, '<del>$1</del>');
  s = s.replace(/`([^`\n]+)`/g, (_, c) => `\u0000${codes.push(c) - 1}\u0000`);
  // [text](https://...): the address may not hold a quote, bracket, space or placeholder.
  s = s.replace(/\[([^\]\n]+)\]\((https?:\/\/[^\s)"'<>\u0000\u0001]+)\)/g, (_, text, href) => a(href, marks(text)));
  s = s.replace(/(^|[\s(])(https?:\/\/[^\s<>"'()\u0000\u0001]*[^\s<>"'().,;:!?\u0000\u0001])/g, (_, pre, href) => pre + a(href, href));
  s = marks(s);
  s = s.replace(/\u0001(\d+)\u0001/g, (_, i) => links[Number(i)]);
  return s.replace(/\u0000(\d+)\u0000/g, (_, i) => `<code>${codes[Number(i)]}</code>`);
}

const cells = (/** @type {string} */ row) => row.trim().replace(/^\||\|$/g, '').split('|').map((c) => c.trim());

/** Render markdown to safe HTML. */
export function md(/** @type {string} */ src = '') {
  const lines = esc(src.replace(/\r\n?/g, '\n')).split('\n');
  /** @type {string[]} */
  const out = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    // Fenced code (an unclosed fence, mid-stream, runs to the end).
    const fence = /^\s*```\s*([\w+#.-]*)\s*$/.exec(line);
    if (fence) {
      const lang = fence[1].toLowerCase();
      const body = [];
      i++;
      while (i < lines.length && !/^\s*```\s*$/.test(lines[i])) body.push(lines[i++]);
      i++;
      out.push(
        `<div class="code"><div class="code-h"><span>${lang || 'code'}</span><button type="button" class="copy" data-copy>Copy</button></div><pre><code>${highlight(body.join('\n'))}</code></pre></div>`
      );
      continue;
    }
    if (!line.trim()) {
      i++;
      continue;
    }
    const h = /^(#{1,4})\s+(.+)$/.exec(line);
    if (h) {
      const n = Math.min(4, h[1].length + 1);
      out.push(`<h${n}>${inline(h[2])}</h${n}>`);
      i++;
      continue;
    }
    if (/^\s*(-{3,}|\*{3,}|_{3,})\s*$/.test(line)) {
      out.push('<hr>');
      i++;
      continue;
    }
    if (/^&gt;\s?/.test(line)) {
      const q = [];
      while (i < lines.length && /^&gt;\s?/.test(lines[i])) q.push(lines[i++].replace(/^&gt;\s?/, ''));
      out.push(`<blockquote>${inline(q.join('<br>'))}</blockquote>`);
      continue;
    }
    // Table: a header row, then a |---|---| row.
    if (/^\s*\|.*\|\s*$/.test(line) && /^\s*\|?\s*:?-{2,}/.test(lines[i + 1] ?? '')) {
      const head = cells(line);
      i += 2;
      const rows = [];
      while (i < lines.length && /^\s*\|.*\|\s*$/.test(lines[i])) rows.push(cells(lines[i++]));
      out.push(
        `<div class="table"><table><thead><tr>${head.map((c) => `<th>${inline(c)}</th>`).join('')}</tr></thead><tbody>${rows
          .map((r) => `<tr>${r.map((c) => `<td>${inline(c)}</td>`).join('')}</tr>`)
          .join('')}</tbody></table></div>`
      );
      continue;
    }
    const li = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/.exec(line);
    if (li) {
      const ordered = /\d/.test(li[2]);
      const items = [];
      while (i < lines.length) {
        const m = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/.exec(lines[i]);
        if (m && /\d/.test(m[2]) === ordered) {
          items.push({ depth: m[1].length >= 2 ? 1 : 0, text: m[3] });
          i++;
        } else if (lines[i].trim() && /^\s{2,}\S/.test(lines[i]) && items.length) {
          items[items.length - 1].text += ' ' + lines[i++].trim();
        } else break;
      }
      const tag = ordered ? 'ol' : 'ul';
      // Task lists: "- [ ] todo" and "- [x] done" get a box.
      const task = items.some((it) => /^\[[ xX]\]\s/.test(it.text));
      const item = (/** @type {{depth: number, text: string}} */ it) => {
        const m = /^\[([ xX])\]\s+(.*)$/.exec(it.text);
        const cls = [it.depth ? 'sub' : '', m ? (m[1] === ' ' ? 'task' : 'task done') : ''].filter(Boolean).join(' ');
        return `<li${cls ? ` class="${cls}"` : ''}>${m ? `<span class="box" aria-hidden="true"></span>` : ''}${inline(m ? m[2] : it.text)}</li>`;
      };
      out.push(`<${tag}${task ? ' class="tasks"' : ''}>${items.map(item).join('')}</${tag}>`);
      continue;
    }
    // Paragraph: runs until a blank line or another block starts.
    const para = [];
    while (
      i < lines.length &&
      lines[i].trim() &&
      !/^\s*```/.test(lines[i]) &&
      !/^#{1,4}\s/.test(lines[i]) &&
      !/^&gt;/.test(lines[i]) &&
      !/^(\s*)([-*+]|\d+[.)])\s+/.test(lines[i]) &&
      !/^\s*\|.*\|\s*$/.test(lines[i])
    )
      para.push(lines[i++]);
    out.push(`<p>${inline(para.join('<br>'))}</p>`);
  }
  return out.join('');
}

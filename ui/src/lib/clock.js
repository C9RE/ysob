// Time and clipboard helpers for the chat. formatDuration and copyText are adapted from
// Autohand Squad (https://github.com/autohandai/squad, src/lib/duration.js and
// src/lib/copy-text.js), under its MIT licence:
//
//   Copyright (c) 2026 Autohand AI Ltd.
//
//   Permission is hereby granted, free of charge, to any person obtaining a copy of this
//   software and associated documentation files (the "Software"), to deal in the Software
//   without restriction, including without limitation the rights to use, copy, modify, merge,
//   publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons
//   to whom the Software is furnished to do so, subject to the following conditions:
//
//   The above copyright notice and this permission notice shall be included in all copies or
//   substantial portions of the Software.
//
//   THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
//   INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR
//   PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE
//   FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
//   OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
//   DEALINGS IN THE SOFTWARE.

/** Elapsed time the way people say it: "8.4 sec", "1 min 35 sec", "1 hour 5 min". */
export function formatDuration(/** @type {number} */ ms) {
  const value = Number(ms);
  if (!Number.isFinite(value) || value < 0) return '';
  if (value < 1000) return `${Math.max(1, Math.round(value))} ms`;
  if (value < 10000) return `${(value / 1000).toFixed(1)} sec`;
  const total = Math.round(value / 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  if (hours) return `${hours} hour${hours === 1 ? '' : 's'}${minutes ? ` ${minutes} min` : ''}`;
  if (minutes) return `${minutes} min${seconds ? ` ${seconds} sec` : ''}`;
  return `${seconds} sec`;
}

/** A live count-up that ticks once a second: "0:07", "1:35", "1:02:09". */
export function ticking(/** @type {number} */ secs) {
  const s = Math.max(0, Math.floor(secs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const pad = (/** @type {number} */ n) => String(n).padStart(2, '0');
  return h ? `${h}:${pad(m)}:${pad(s % 60)}` : `${m}:${pad(s % 60)}`;
}

/** Copy text and say whether it worked. Where the clipboard API is missing, fall back to a
 *  hidden selection, so a Copy button never claims a copy that didn't happen. */
export async function copyText(/** @type {string} */ text) {
  const value = String(text ?? '');
  try {
    if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value);
      return true;
    }
    if (typeof document === 'undefined') return false;
    const area = document.createElement('textarea');
    area.value = value;
    area.setAttribute('readonly', '');
    area.style.position = 'fixed';
    area.style.opacity = '0';
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand('copy');
    area.remove();
    return ok;
  } catch {
    return false;
  }
}

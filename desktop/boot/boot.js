// The handshake is pack art; without a pack there's no picture, just the wordmark.
const shake = document.querySelector('.shake');
shake?.addEventListener('error', () => shake.classList.add('missing'));
// Says what's happening while the engine starts; stops if the shell reports an error.
const lines = ['Waking the squad…', 'Checking the perimeter…', 'Loading weapons…', 'Listening to the trees…'];
const msg = document.getElementById('msg');
let i = 0;
const t = setInterval(() => {
  if (msg.textContent.startsWith('The engine')) { document.body.classList.add('err'); clearInterval(t); return; }
  msg.textContent = lines[++i % lines.length];
}, 1400);
new MutationObserver(() => { if (msg.textContent.startsWith('The engine')) { document.body.classList.add('err'); clearInterval(t); } }).observe(msg, { childList: true, characterData: true, subtree: true });
// When the engine won't start: try once more, or read the log. Both are the shell's own
// commands, allowed for this page only (capabilities/default.json).
const ipc = window.__TAURI_INTERNALS__;
const retry = document.getElementById('retry');
retry?.addEventListener('click', () => {
  retry.disabled = true;
  document.body.classList.remove('err');
  msg.textContent = 'Starting the engine again…';
  ipc?.invoke('engine_retry').catch(() => {});
  setTimeout(() => (retry.disabled = false), 3000);
});
document.getElementById('log')?.addEventListener('click', () => {
  ipc?.invoke('open_log').catch((e) => { msg.textContent = `The engine log couldn't be opened: ${e}`; });
});

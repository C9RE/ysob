// The desktop shell, when the app runs inside it (desktop/src-tauri). The shell lets this
// page call a handful of its own commands and nothing else of Tauri's
// (capabilities/app.json). In a plain browser there is no shell and every call is a no-op.

/** @type {any} */
const ipc = typeof window !== 'undefined' ? /** @type {any} */ (window).__TAURI_INTERNALS__ : null;

export const isDesktop = !!ipc;

/** Call one of the shell's commands. Rejects in a browser. @param {string} cmd @param {Record<string, unknown>} [args] */
export const call = (cmd, args = {}) => (ipc ? ipc.invoke(cmd, args) : Promise.reject(new Error('Not the desktop app.')));

/** The system's folder picker. The chosen path, or null if it was cancelled (or in a browser). */
export async function pickFolder() {
  if (!ipc) return null;
  try {
    return /** @type {string | null} */ (await call('pick_folder'));
  } catch {
    return null;
  }
}

/** Open a web link in the system's browser. Returns false in a plain browser, where the link's
 *  own new-tab behaviour does the job. @param {string} href */
export function openLink(href) {
  if (!ipc || !/^https?:\/\//.test(href)) return false;
  call('open_link', { url: href }).catch(() => {});
  return true;
}

/** Ask the shell for a desktop notification. It shows only while the window is hidden, minimised
 *  or behind other windows, and flashes the taskbar button too. @param {string} title @param {string} body */
export function notify(title, body) {
  if (ipc) call('notify', { title, body }).catch(() => {});
}

/** How many things are waiting on you, for the tray's menu and tooltip. @param {number} count */
export function trayNeeds(count) {
  if (ipc) call('tray_needs', { count }).catch(() => {});
}

/** True on Apple keyboards, where the shortcuts use Cmd. */
export const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform ?? '');
/** True on Windows, for example paths. */
export const isWindows = typeof navigator !== 'undefined' && /Win/.test(navigator.platform ?? '');

/** The desktop app draws its own top bar (WindowBar.svelte) everywhere but macOS, where the
 *  native one stays. The shell makes the window frameless to match (desktop/src-tauri). */
export const ownFrame = isDesktop && !isMac;
/** Mark an element as a handle for moving the window: Tauri's own drag script picks up the
 *  attribute (buttons and other controls inside still work), and a double-click maximises. */
export const dragArea = ownFrame ? { 'data-tauri-drag-region': 'deep' } : {};

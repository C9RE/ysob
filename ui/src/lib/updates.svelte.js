// App updates, through the desktop shell (desktop/src-tauri/src/update.rs). In a plain browser
// there is no shell, so `desktop` stays false and nothing here does anything.
import { S, toast } from './store.svelte.js';
import { call, isDesktop } from './desktop.js';

export const U = $state({
  desktop: isDesktop,
  /** False for a build that wasn't given an update address (built from source). */
  enabled: false,
  version: '',
  /** @type {null | { version: string, notes?: string, date?: string }} */
  available: null,
  checking: false,
  checked: /** @type {Date | null} */ (null),
  error: '',
  /** '' | 'downloading' | 'installing' */
  phase: '',
  done: 0,
  /** @type {number | null} */ total: null
});

/** What went wrong, in words someone can act on. The shell's own text goes last, in brackets. */
function plain(/** @type {unknown} */ e, /** @type {string} */ doing) {
  const raw = String(/** @type {any} */ (e)?.message ?? e);
  if (/already up to date/i.test(raw)) return 'Already up to date.';
  if (/signature|verif|minisign|pubkey/i.test(raw)) return `The update didn’t pass its signature check, so it was not installed. Try again later; if it keeps happening, get the installer from the website. (${raw})`;
  if (/network|dns|connect|timed? ?out|request|status|sending|resolve|404|50\d/i.test(raw)) return `Couldn’t reach the update server while ${doing}. Check your connection and try again. (${raw})`;
  return `Something went wrong while ${doing}. Try again; if it keeps happening, get the installer from the website. (${raw})`;
}

export async function initUpdates() {
  if (!isDesktop) return;
  try {
    const v = await call('app_version');
    U.version = v.version;
    U.enabled = v.updates;
    // An install that failed last time restarted the engine (and this page): say why now.
    if (v.failed) {
      U.error = plain(v.failed, 'installing the update');
      toast('The update didn’t install. YSOB is still on the version you had.', 'hit', { label: 'Details', run: () => ((S.settingsAt = 'updates'), (S.modal = 'settings')) });
    }
  } catch {
    return;
  }
  if (!U.enabled) return;
  // A quiet look a few seconds after start, then a few times a day while the app stays open.
  // It only speaks up when there's something new.
  setTimeout(() => check(true), 4000);
  setInterval(() => check(true), 6 * 3600 * 1000);
}

export async function check(quiet = false) {
  if (!isDesktop || !U.enabled || U.checking || U.phase) return;
  U.checking = true;
  if (!quiet) U.error = '';
  try {
    const had = U.available?.version;
    U.available = await call('update_check');
    U.checked = new Date();
    // A quiet check leaves a standing error alone (the reason an install failed, say).
    if (!quiet) U.error = '';
    if (U.available && quiet && U.available.version !== had) {
      toast(`YSOB ${U.available.version} is ready to install.`, 'go', { label: 'See what’s new', run: () => ((S.settingsAt = 'updates'), (S.modal = 'settings')) });
    }
  } catch (e) {
    if (!quiet) U.error = plain(e, 'checking for updates');
  } finally {
    U.checking = false;
  }
}

export async function install() {
  if (!isDesktop || !U.available || U.phase) return;
  U.error = '';
  U.phase = 'downloading';
  U.done = 0;
  U.total = null;
  const poll = setInterval(async () => {
    try {
      const p = await call('update_progress');
      U.done = p.done;
      U.total = p.total;
      if (p.phase) U.phase = p.phase;
    } catch {
      /* the app is restarting */
    }
  }, 200);
  try {
    await call('update_install'); // the app restarts on the new version
  } catch (e) {
    U.error = plain(e, U.phase === 'installing' ? 'installing the update' : 'downloading the update');
    U.phase = '';
  } finally {
    clearInterval(poll);
  }
}

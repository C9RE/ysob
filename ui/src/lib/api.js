// The app's line to the engine: HTTP for commands, a WebSocket for live events.

let token = '';
try {
  const m = location.hash.match(/token=([0-9a-f]{32,})/);
  if (m) {
    token = m[1];
    sessionStorage.setItem('ysob-token', token);
    history.replaceState(null, '', location.pathname + location.search);
  } else token = sessionStorage.getItem('ysob-token') || '';
} catch {
  /* storage can be blocked; the token then lives for this page only */
}

export const hasToken = () => token.length >= 32;

/**
 * @param {string} cmd
 * @param {Record<string, unknown>} [args]
 */
export async function api(cmd, args = {}) {
  const r = await fetch(`/api/${cmd}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', authorization: `Bearer ${token}` },
    body: JSON.stringify(args)
  });
  const body = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(body.error || (r.status === 401 ? 'Not signed in to the engine. Open the link it printed.' : 'The engine did not answer.'));
  return body;
}

/**
 * @param {(ev: {type: string, data: any}) => void} onEvent
 * @param {(up: boolean) => void} onLink
 */
export function connect(onEvent, onLink) {
  let wait = 500;
  const open = () => {
    const ws = new WebSocket(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws?token=${token}`);
    ws.onopen = () => {
      wait = 500;
      onLink(true);
    };
    ws.onmessage = (m) => {
      try {
        onEvent(JSON.parse(m.data));
      } catch {
        /* ignore a malformed frame */
      }
    };
    ws.onclose = () => {
      onLink(false);
      setTimeout(open, wait);
      wait = Math.min(wait * 2, 8000);
    };
  };
  open();
}

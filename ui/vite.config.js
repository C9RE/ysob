import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { cpSync, createReadStream, existsSync, statSync } from 'node:fs';
import { extname, join, normalize, resolve } from 'node:path';

const DEFAULT_PACK = resolve(import.meta.dirname, '../packs/default');
const TYPES = { '.json': 'application/json', '.png': 'image/png', '.mp3': 'audio/mpeg', '.ico': 'image/x-icon' };

/** The default theme pack (packs/default, committed) is served at /packs/default and copied
 *  into the build, so the app always has a squad to show. An installed pack in
 *  public/packs/jungle (never committed) is picked up by Vite as usual and wins. */
function defaultPack() {
  return {
    name: 'ysob-default-pack',
    configureServer(server) {
      server.middlewares.use('/packs/default', (req, res, next) => {
        const file = normalize(join(DEFAULT_PACK, decodeURIComponent((req.url ?? '').split('?')[0])));
        if (!file.startsWith(DEFAULT_PACK) || !existsSync(file) || !statSync(file).isFile()) return next();
        res.setHeader('content-type', TYPES[extname(file)] ?? 'application/octet-stream');
        createReadStream(file).pipe(res);
      });
    },
    closeBundle() {
      if (existsSync(DEFAULT_PACK)) cpSync(DEFAULT_PACK, resolve(import.meta.dirname, 'dist/packs/default'), { recursive: true });
    }
  };
}

export default defineConfig({
  plugins: [tailwindcss(), svelte(), defaultPack()],
  server: { port: 5199, proxy: { '/api': 'http://127.0.0.1:7799', '/ws': { target: 'ws://127.0.0.1:7799', ws: true } } },
  build: { target: 'es2022' }
});

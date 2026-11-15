// Run before `tauri build` / `tauri dev`: builds the UI and the `ysob` engine, then puts the
// engine where Tauri's externalBin wants it (binaries/ysob-<target triple>[.exe]).
import { $ } from "bun";
import { copyFileSync, cpSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";

const here = import.meta.dir;
const root = resolve(here, "..");
const ui = join(root, "ui");

// YSOB_UI_DIST=<folder> bundles an already-built UI instead of building ui/ here.
if (process.env.YSOB_UI_DIST) {
  rmSync(join(ui, "dist"), { recursive: true, force: true });
  cpSync(resolve(process.env.YSOB_UI_DIST), join(ui, "dist"), { recursive: true });
} else {
  if (!existsSync(join(ui, "public/packs/jungle/pack.json"))) {
    console.warn("prep: no theme pack in ui/public/packs/jungle, so the build carries the default pack only (README, Theme packs)");
  }
  await $`bun install`.cwd(ui);
  await $`bun run build`.cwd(ui);
}

// The splash's pixel font, from the UI's own font package (not committed here).
{
  const font = join(ui, "node_modules/@fontsource/pixelify-sans/files/pixelify-sans-latin-700-normal.woff2");
  if (existsSync(font)) copyFileSync(font, join(here, "boot", "pixel.woff2"));
  // The splash picture: the installed theme pack's (pack art: not committed), else the default
  // pack's. A pack with none leaves the splash as the three dots and the wordmark.
  const boot = join(here, "boot", "handshake.png");
  const shake = [join(ui, "public/packs/jungle/icon/handshake.png"), join(root, "packs/default/icon/handshake.png")].find((f) => existsSync(f));
  if (existsSync(join(ui, "public/packs/jungle/pack.json")) && !existsSync(join(ui, "public/packs/jungle/icon/handshake.png"))) rmSync(boot, { force: true });
  else if (shake) copyFileSync(shake, boot);
}

// The icon comes from the theme pack when there is one (packs are not committed), else the default
// pack's own, so a fresh clone builds too.
const icons = join(here, "src-tauri", "icons");
if (!existsSync(join(icons, "icon.ico"))) {
  const pack = join(ui, "public/packs/jungle/icon/icon_1024.png");
  const src = existsSync(pack) ? pack : join(root, "packs/default/icon/icon_1024.png");
  if (!existsSync(src)) throw new Error(`prep: no icon at ${pack} or in packs/default/icon`);
  await $`bunx tauri icon ${src} -o ${icons}`.cwd(here);
  rmSync(join(icons, "android"), { recursive: true, force: true });
  rmSync(join(icons, "ios"), { recursive: true, force: true });
}

const triple = (await $`rustc -vV`.text()).match(/^host: (\S+)/m)[1];
await $`cargo build --release -p ysob-engine`.cwd(root);
const exe = process.platform === "win32" ? ".exe" : "";
const bins = join(here, "src-tauri", "binaries");
mkdirSync(bins, { recursive: true });
copyFileSync(join(root, "target", "release", `ysob${exe}`), join(bins, `ysob-${triple}${exe}`));
console.log(`prep: ui/dist built, engine copied to binaries/ysob-${triple}${exe}`);

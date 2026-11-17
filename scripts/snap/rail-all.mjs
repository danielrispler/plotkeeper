// `npm run snap:rail -- <folder>`: renders every rail fixture (scripts/snap/rail-fixtures.mjs) at 1728, 1100 and 864 px wide
// into the folder, one PNG each, named <fixture>-<width>.png. At 864 px the rail is an overlay, so it is also rendered
// open: <fixture>-864-open.png. Run `npm run build` first. Stops at the first run that fails.
import { spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { railStates } from "./rail-fixtures.mjs";

const [folder] = process.argv.slice(2);
if (!folder) {
  console.error("usage: npm run snap:rail -- <folder>");
  process.exit(2);
}
mkdirSync(folder, { recursive: true });
const snap = join(dirname(fileURLToPath(import.meta.url)), "snap.mjs");
const sizes = [
  ["1728", "1728x1117", ""],
  ["1100", "1100x800", ""],
  ["864", "864x800", ""],
  ["864-open", "864x800", "rail-open"],
];
for (const name of Object.keys(railStates)) {
  for (const [label, size, body] of sizes) {
    const out = resolve(folder, `${name}-${label}.png`);
    const run = spawnSync(process.execPath, [snap, name, out], { stdio: "inherit", env: { ...process.env, SNAP_SIZE: size, SNAP_BODY: body } });
    if (run.status !== 0) process.exit(run.status ?? 1);
  }
}

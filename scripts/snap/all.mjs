// `npm run snap:all`: renders every fixture (scripts/snap/fixtures.ts) once into a temp folder, as CI does after
// `npm run check` (#144), and stops at the first that fails: Chrome's exit, a `snap:fail` line or an uncaught error in the page.
// Run `npm run build` first. `npm run check` leaves it out, so agents' checks stay fast.
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { fixtures } from "./fixtures.ts";

// The sample fixture whose script fails on purpose (rail-fixtures.ts).
const failsOnPurpose = new Set(["railIdleFail"]);

const snap = join(dirname(fileURLToPath(import.meta.url)), "snap.mjs");
const folder = mkdtempSync(join(tmpdir(), "snap-all-"));
for (const name of Object.keys(fixtures).filter((name) => !failsOnPurpose.has(name))) {
  const run = spawnSync(process.execPath, [snap, name, join(folder, `${name}.png`)], { stdio: "inherit" });
  if (run.status !== 0) {
    console.error(`snap:all: ${name} failed`);
    process.exit(run.status ?? 1);
  }
}
rmSync(folder, { recursive: true, force: true });
console.log(`snap:all: ${Object.keys(fixtures).length - failsOnPurpose.size} fixtures rendered`);

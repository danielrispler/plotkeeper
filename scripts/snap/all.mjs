// `npm run snap:all`: renders every fixture (scripts/snap/fixtures.ts) once into a temp folder, as CI does after
// `npm run build` (#144), and fails if any fails: Chrome's exit, a `snap:fail` line or an uncaught error in the page.
// Fixtures render one per core at a time, each with its own Chrome; once one fails, no new one starts.
// Run `npm run build` first. `npm run check` leaves it out, so agents' checks stay fast.
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { availableParallelism, tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { fixtures } from "./fixtures.ts";

// The sample fixture whose script fails on purpose (rail-fixtures.ts).
const failsOnPurpose = new Set(["railIdleFail"]);

// Home, the waiting room (#238), is drawn at 1400x900, the rest at snap's default size.
const sizeOf = (name) => (name === "home" || name === "homeClearToast" ? "1400x900" : undefined);

const snap = join(dirname(fileURLToPath(import.meta.url)), "snap.mjs");
const folder = mkdtempSync(join(tmpdir(), "snap-all-"));
const queue = Object.keys(fixtures).filter((name) => !failsOnPurpose.has(name));
const total = queue.length;
let failure;

// A fixture's output is printed whole once it finishes, so parallel renders don't interleave.
const render = (name) =>
  new Promise((done) => {
    const child = spawn(process.execPath, [snap, name, join(folder, `${name}.png`)], { stdio: ["ignore", "pipe", "pipe"], env: { ...process.env, ...(sizeOf(name) && { SNAP_SIZE: sizeOf(name) }) } });
    let output = "";
    child.stdout.on("data", (chunk) => (output += chunk));
    child.stderr.on("data", (chunk) => (output += chunk));
    child.on("close", (status) => {
      process.stdout.write(output);
      if (status !== 0) {
        console.error(`snap:all: ${name} failed`);
        failure ??= status ?? 1;
      }
      done();
    });
  });

const worker = async () => {
  while (queue.length && failure === undefined) await render(queue.shift());
};
await Promise.all(Array.from({ length: availableParallelism() }, worker));
rmSync(folder, { recursive: true, force: true });
if (failure !== undefined) process.exit(failure);
console.log(`snap:all: ${total} fixtures rendered`);

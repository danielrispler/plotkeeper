// `npm run snap:all`: renders every fixture (scripts/snap/fixtures.ts) once into a temp folder, as CI does after
// `npm run build` (#144), and fails if any fails: Chrome's exit, a `snap:fail` line or an uncaught error in the page.
// Fixtures render one per core at a time, each with its own Chrome; once one fails, no new one starts.
// Run `npm run build` first. `npm run check` leaves it out, so agents' checks stay fast.
// `npm run snap:all -- --shard=<i>/<n>` renders only the i-th of n shares (shard.mjs), as CI's two snap jobs do (#432).
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { availableParallelism, tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { fixtures } from "./fixtures.ts";
import { shard } from "./shard.mjs";

// The sample fixture whose script fails on purpose (rail-fixtures.ts).
const failsOnPurpose = new Set(["railIdleFail"]);

// The map fixtures and the writer section are drawn at 1900x1180, the rest at snap's default size.
// A fixture named `…Narrow` is drawn at 1000x800, below the 1007 px where the map keeps one overlay open (#385); the map's and the
// writer section's, in the Settings panel over the map (#387), at 1900x1180. A fixture named `…Short` is drawn at 1300x700, a short window.
const sizeOf = (name) => (name.endsWith("Narrow") ? "1000x800" : name.endsWith("Short") ? "1300x700" : name.startsWith("map") || name.startsWith("writers") ? "1900x1180" : undefined);

const shardFlag = process.argv.slice(2).find((arg) => arg.startsWith("--shard="))?.slice("--shard=".length);
let queue;
try {
  queue = shard(Object.keys(fixtures).filter((name) => !failsOnPurpose.has(name)), shardFlag);
} catch (why) {
  console.error(why.message);
  process.exit(2);
}

const snap = join(dirname(fileURLToPath(import.meta.url)), "snap.mjs");
const folder = mkdtempSync(join(tmpdir(), "snap-all-"));
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
console.log(`snap:all: ${total} fixtures rendered${shardFlag ? ` (shard ${shardFlag})` : ""}`);

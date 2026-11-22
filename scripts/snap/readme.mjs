// `npm run snap:readme`: draws the README's four pictures into docs/studio/assets/readme/, dark, from fixtures, with headless
// Chrome: hero.png (the whole page, on the `home` fixture) and three crops of the rail
// (turn.png, quiz.png, where-we-were.png). A crop's fixture script logs the box to keep; a fixture whose target is missing logs
// `snap:fail`, which stops this run. Run `npm run build` first. Cropping uses macOS `sips`.
import { spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const folder = resolve(here, "../../docs/studio/assets/readme");
const scale = 2;
const pictures = [
  { out: "hero.png", fixture: "home", size: "1400x800", scale: 1, crop: false },
  { out: "turn.png", fixture: "readmeTurn", size: "1400x1000", scale, crop: true },
  { out: "quiz.png", fixture: "readmeQuiz", size: "1400x1000", scale, crop: true },
  { out: "where-we-were.png", fixture: "readmeWhere", size: "1400x1000", scale, crop: true },
];

mkdirSync(folder, { recursive: true });
for (const { out, fixture, size, scale: density, crop: wantsCrop } of pictures) {
  const file = join(folder, out);
  const run = spawnSync(process.execPath, [join(here, "snap.mjs"), fixture, file], {
    encoding: "utf8", env: { ...process.env, SNAP_SIZE: size, SNAP_SCALE: String(density) },
  });
  process.stdout.write(run.stdout ?? "");
  if (run.status !== 0) {
    process.stderr.write(run.stderr ?? "");
    console.error(`snap:readme: ${fixture} failed`);
    process.exit(run.status ?? 1);
  }
  const crop = /^snap:crop (\d+) (\d+) (\d+) (\d+)$/m.exec(run.stdout ?? "");
  if (wantsCrop && !crop) {
    console.error(`snap:readme: ${fixture} logged no crop box`);
    process.exit(1);
  }
  if (crop) {
    const [x, y, width, height] = crop.slice(1).map((n) => Number(n) * density);
    const cut = spawnSync("sips", ["--cropToHeightWidth", String(height), String(width), "--cropOffset", String(y), String(x), file], { encoding: "utf8" });
    if (cut.status !== 0) {
      console.error(cut.stderr);
      process.exit(cut.status ?? 1);
    }
  }
}

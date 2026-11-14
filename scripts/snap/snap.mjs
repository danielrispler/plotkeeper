// `npm run snap -- <fixture> <out.png>`: renders the studio UI from dist/ against a stub Tauri API
// fed by a fixture (scripts/snap/fixtures.mjs) to a PNG, with headless Chrome. Run `npm run build` first.
// Chrome never opens a window, so it takes no focus. Set CHROME to a Chrome binary to override the default.
import { spawn } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { fixtures } from "./fixtures.mjs";
import { snapPage } from "./page.mjs";

const [name, out] = process.argv.slice(2);
if (!fixtures[name] || !out) {
  console.error(`usage: npm run snap -- <${Object.keys(fixtures).join("|")}> <out.png>`);
  process.exit(2);
}
const dist = resolve(dirname(fileURLToPath(import.meta.url)), "../../dist");
const chrome = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

// A copy of dist/ with the stubbed index, so the stub never reaches dist.
const site = mkdtempSync(join(tmpdir(), "snap-site-"));
const profile = mkdtempSync(join(tmpdir(), "snap-chrome-"));
cpSync(dist, site, { recursive: true });
writeFileSync(join(site, "index.html"), snapPage(readFileSync(join(dist, "index.html"), "utf8"), fixtures[name]()));

const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css" };
const server = createServer((req, res) => {
  try {
    const file = join(site, new URL(req.url, "http://x").pathname.replace(/^\/$/, "/index.html"));
    if (!file.startsWith(site)) throw new Error("outside");
    res.setHeader("content-type", types[extname(file)] ?? "application/octet-stream");
    res.end(readFileSync(file));
  } catch {
    res.statusCode = 404;
    res.end();
  }
});
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const chromeArgs = [
  "--headless=new", "--enable-logging=stderr", "--v=0", "--disable-gpu", `--user-data-dir=${profile}`, "--window-size=1900,900",
  "--virtual-time-budget=5000", `--screenshot=${resolve(out)}`, `http://127.0.0.1:${server.address().port}/index.html`,
];
const run = await new Promise((done) => {
  const child = spawn(chrome, chromeArgs, { stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  child.stderr.on("data", (chunk) => (stderr += chunk));
  child.on("close", (status) => done({ status, stderr }));
});
server.close();
rmSync(site, { recursive: true, force: true });
rmSync(profile, { recursive: true, force: true });
const log = (run.stderr ?? "").split("\n").filter((line) => /CONSOLE/.test(line));
if (log.length) console.log(log.join("\n"));
if (run.status !== 0) process.exit(run.status ?? 1);
console.log(`wrote ${out}`);

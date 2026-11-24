// A fake of dsh-visual's verify.mjs for the micro-world tests (#399), so no test needs Daniel's checkout, Chrome or the network:
// node verify.mjs <page.html>, exit 0 = pass, 1 = fail, with `FAIL` lines as the real one prints. A test copies this folder into a temp
// one with `playwright-core/` at `node_modules/playwright-core/`, and names it as `microWorlds.dshVisual`. It imports `chromium` as the
// real one does, asks a context for the page's own `file:` URL and every `fetch("…")` URL the page holds, and fails on a request that
// was aborted, and on a page that holds `VERIFY-FAIL`.
import { chromium } from "playwright-core";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const file = resolve(process.argv[2]);
const html = readFileSync(file, "utf8");
const browser = await chromium.launch({ channel: "chrome" });
const context = await browser.newContext({ viewport: { width: 390, height: 900 } });
const fails = [];
const urls = [pathToFileURL(file).href, ...[...html.matchAll(/fetch\("([^"]+)"\)/g)].map((m) => m[1])];
for (const url of urls) {
  const answer = await context.request(url);
  if (answer === "aborted") fails.push(`FAIL no page or request errors @390/light: ["request failed: ${url} (net::ERR_BLOCKED_BY_CLIENT)"]`);
}
if (html.includes("VERIFY-FAIL")) fails.push('FAIL no horizontal overflow @390/light: {"culprits":["div.wide right=2000"]}');
console.log(`launched with: ${JSON.stringify(browser.args)}`);
for (const line of fails) console.log(line);
console.log(`${fails.length ? "FAIL" : "PASS"} ${urls.length + 1} checks`);
process.exitCode = fails.length ? 1 : 0;

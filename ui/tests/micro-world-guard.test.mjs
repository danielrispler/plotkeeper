// The host check's network guard for micro-worlds (spec #378, ticket #399): plotkeeper runs dsh-visual's verify.mjs with
// `node --import server/src/micro_world_guard.mjs`, which makes Playwright's Chrome abort every request that is not a local file, so
// model-written script never reaches the network. The fake dsh-visual (`server/tests/fake-dsh-visual/`) stands in for Chrome; with
// PLOTKEEPER_DSH_VISUAL naming a real dsh-visual `scripts/` folder, the last test runs the real verify.mjs in Chrome.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "../..");
const guard = join(root, "server/src/micro_world_guard.mjs");

// A temp dsh-visual `scripts/` folder from the fake, with its playwright-core where Node finds it.
function fakeScripts() {
  const dir = mkdtempSync(join(tmpdir(), "pk-guard-"));
  const fake = join(root, "server/tests/fake-dsh-visual");
  for (const name of ["build.py", "verify.mjs"]) cpSync(join(fake, name), join(dir, name));
  cpSync(join(fake, "playwright-core"), join(dir, "node_modules/playwright-core"), { recursive: true });
  return dir;
}

const verify = (scripts, page) => spawnSync(process.execPath, ["--import", guard, join(scripts, "verify.mjs"), page], { encoding: "utf8", timeout: 120_000 });

test("a page that fetches a URL fails the host check: the guard aborts the request", () => {
  const scripts = fakeScripts();
  const page = join(scripts, "world.html");
  writeFileSync(page, `<!doctype html><script>fetch("https://example.com/steal")</script>`);
  const ran = verify(scripts, page);
  assert.equal(ran.status, 1, ran.stdout + ran.stderr);
  assert.match(ran.stdout, /FAIL no page or request errors .*request failed: https:\/\/example\.com\/steal/);
});

test("a page with only its own file passes, and Chrome is sent through a proxy that leads nowhere", () => {
  const scripts = fakeScripts();
  const page = join(scripts, "world.html");
  writeFileSync(page, `<!doctype html><p>ok</p>`);
  const ran = verify(scripts, page);
  assert.equal(ran.status, 0, ran.stdout + ran.stderr);
  assert.match(ran.stdout, /--proxy-server=http:\/\/127\.0\.0\.1:9/);
  assert.match(ran.stdout, /--proxy-bypass-list=<-loopback>/);
});

test("without playwright-core where verify.mjs finds it, the check fails rather than run unguarded", () => {
  const scripts = mkdtempSync(join(tmpdir(), "pk-guard-"));
  cpSync(join(root, "server/tests/fake-dsh-visual/verify.mjs"), join(scripts, "verify.mjs"));
  const page = join(scripts, "world.html");
  writeFileSync(page, `<!doctype html><p>ok</p>`);
  assert.notEqual(verify(scripts, page).status, 0);
});

const real = process.env.PLOTKEEPER_DSH_VISUAL;
// Blocked by the guard's route, not by a network that is down: without the guard, the same page passes (tried 2026-11-24).
test("the real verify.mjs in Chrome fails a page whose script fetches a URL", { skip: !real && "PLOTKEEPER_DSH_VISUAL names no dsh-visual scripts folder" }, () => {
  assert.ok(existsSync(join(real, "verify.mjs")), `${real} holds no verify.mjs`);
  const dir = mkdtempSync(join(tmpdir(), "pk-guard-real-"));
  const page = join(dir, "world.html");
  const head = `<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width"><style>:root{color-scheme:light dark}body{background:#fff;color:#111}@media (prefers-color-scheme: dark){body{background:#111;color:#eee}}</style>`;
  writeFileSync(page, `${head}<p>ok</p><script>fetch("https://example.com/steal").catch(() => {})</script>`);
  const ran = verify(real, page);
  assert.equal(ran.status, 1, ran.stdout + ran.stderr);
  const report = JSON.parse(readFileSync(join(dir, "world.verify.json"), "utf8"));
  const errors = report.checks.filter((c) => c.name === "no page or request errors" && !c.ok).flatMap((c) => c.detail);
  assert.ok(errors.some((e) => /request failed: https:\/\/example\.com\/steal \(net::ERR_BLOCKED_BY_CLIENT/.test(e)), JSON.stringify(errors));
});

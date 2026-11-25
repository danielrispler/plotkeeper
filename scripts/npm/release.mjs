// The release script (danielrispler/plotkeeper#512). Two pure functions, each also a subcommand:
//   node scripts/npm/release.mjs bump <patch|minor>   rewrites npm/package.json, server/Cargo.toml and Cargo.lock
//   node scripts/npm/release.mjs notes <X.Y.Z> [file] prints (or writes) that version's CHANGELOG.md section
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const FILES = { npmManifest: "npm/package.json", crateManifest: "server/Cargo.toml", lockfile: "Cargo.lock" };

const escapeRegExp = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

function next(version, level) {
  const [major, minor, patch] = version.split(".").map(Number);
  if (level === "patch") return `${major}.${minor}.${patch + 1}`;
  if (level === "minor") return `${major}.${minor + 1}.0`;
  throw new Error(`bump takes patch or minor, not ${JSON.stringify(level)}`);
}

// Replaces the one match of `pattern` (its first group kept, the old version swapped), or fails naming `what`.
function swap(text, pattern, version, what) {
  if (!pattern.test(text)) throw new Error(`${what} has no version ${version}`);
  return text.replace(pattern, (_match, head) => `${head}${version}`);
}

// Takes the three files' texts and `patch` or `minor`; returns their texts at the next version. The version is the npm
// package's; the crate's manifest and the lockfile's `studio-server` entry must hold the same one.
export function bump(texts, level) {
  const old = JSON.parse(texts.npmManifest).version;
  if (!/^\d+\.\d+\.\d+$/.test(old)) throw new Error(`npm/package.json's version ${old} is not X.Y.Z`);
  const version = next(old, level);
  const v = escapeRegExp(old);
  return {
    npmManifest: swap(texts.npmManifest, new RegExp(`^(  "version": ")${v}(?=",?$)`, "m"), version, FILES.npmManifest),
    crateManifest: swap(
      texts.crateManifest,
      new RegExp(`^(\\[package\\]\\n(?:(?!\\[)[^\\n]*\\n)*?version = ")${v}(?="$)`, "m"),
      version,
      FILES.crateManifest,
    ),
    lockfile: swap(
      texts.lockfile,
      new RegExp(`^(name = "studio-server"\\nversion = ")${v}(?="$)`, "m"),
      version,
      FILES.lockfile,
    ),
  };
}

// The body of the changelog's `## X.Y.Z` section, up to the next `## ` heading, without its blank edges.
export function notesFor(changelog, version) {
  const lines = changelog.split("\n");
  const start = lines.findIndex((line) => line.trimEnd() === `## ${version}`);
  if (start === -1) throw new Error(`CHANGELOG.md has no section for ${version}`);
  const rest = lines.slice(start + 1);
  const end = rest.findIndex((line) => line.startsWith("## "));
  const body = (end === -1 ? rest : rest.slice(0, end)).join("\n").trim();
  if (body === "") throw new Error(`CHANGELOG.md's section for ${version} is empty`);
  return `${body}\n`;
}

function main([command, arg, out]) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
  const read = (path) => readFileSync(resolve(root, path), "utf8");
  if (command === "bump") {
    const texts = Object.fromEntries(Object.entries(FILES).map(([key, path]) => [key, read(path)]));
    const bumped = bump(texts, arg);
    for (const [key, path] of Object.entries(FILES)) writeFileSync(resolve(root, path), bumped[key]);
    console.log(JSON.parse(bumped.npmManifest).version);
  } else if (command === "notes") {
    const notes = notesFor(read("CHANGELOG.md"), arg);
    if (out) writeFileSync(out, notes);
    else process.stdout.write(notes);
  } else {
    throw new Error("usage: release.mjs bump <patch|minor> | notes <X.Y.Z> [file]");
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(`release: ${error.message}`);
    process.exit(1);
  }
}

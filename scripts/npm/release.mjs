// The release script (danielrispler/plotkeeper#512). Each exported function is also a subcommand:
//   node scripts/npm/release.mjs bump <patch|minor>   rewrites npm/package.json, server/Cargo.toml and Cargo.lock
//   node scripts/npm/release.mjs notes <X.Y.Z> [file] prints (or writes) that version's CHANGELOG.md section
//   node scripts/npm/release.mjs guard                fails unless HEAD's commit changed npm/package.json's version
//   node scripts/npm/release.mjs platform <platform> <dir>
//                                                     writes the platform package's package.json and README.md into
//                                                     <dir>: darwin, linux-x64 or linux-arm64 (#706)
//   node scripts/npm/release.mjs tag <X.Y.Z> <commit> [--dry-run]
//                                                     tags vX.Y.Z at the commit on origin and creates its GitHub
//                                                     Release from CHANGELOG.md; --dry-run prints the changes instead
// `guard` and `tag` run git and gh in the current directory's repository.
import { spawnSync } from "node:child_process";
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

// Replaces the one match of `pattern` (its first group kept, the old version swapped), or fails naming `what` and `old`.
function swap(text, pattern, old, version, what) {
  if (!pattern.test(text)) throw new Error(`${what} has no version ${old}`);
  return text.replace(pattern, (_match, head) => `${head}${version}`);
}

// Takes the three files' texts and `patch` or `minor`; returns their texts at the next version. The version is the npm
// package's; the crate's manifest, the lockfile's `studio-server` entry and each platform package the npm package asks
// for (its `optionalDependencies`, #706) must hold the same one.
export function bump(texts, level) {
  const manifest = JSON.parse(texts.npmManifest);
  const old = manifest.version;
  if (!/^\d+\.\d+\.\d+$/.test(old)) throw new Error(`npm/package.json's version ${old} is not X.Y.Z`);
  const version = next(old, level);
  const v = escapeRegExp(old);
  let npmManifest = swap(texts.npmManifest, new RegExp(`^(  "version": ")${v}(?=",?$)`, "m"), old, version, FILES.npmManifest);
  for (const [name, wanted] of Object.entries(manifest.optionalDependencies ?? {})) {
    if (wanted !== old) throw new Error(`${FILES.npmManifest} asks for ${name} ${wanted}, not its version ${old}`);
    const line = new RegExp(`^(    "${escapeRegExp(name)}": ")${v}(?=",?$)`, "m");
    npmManifest = swap(npmManifest, line, old, version, `${FILES.npmManifest}'s ${name}`);
  }
  return {
    npmManifest,
    crateManifest: swap(
      texts.crateManifest,
      new RegExp(`^(\\[package\\]\\n(?:(?!\\[)[^\\n]*\\n)*?version = ")${v}(?="$)`, "m"),
      old,
      version,
      FILES.crateManifest,
    ),
    lockfile: swap(
      texts.lockfile,
      new RegExp(`^(name = "studio-server"\\nversion = ")${v}(?="$)`, "m"),
      old,
      version,
      FILES.lockfile,
    ),
  };
}

// The platform packages, one optional dependency of the npm package each, marked with the `os` and `cpu` npm installs
// it on; `platform` is `process.platform` and, for Linux, `process.arch`, as the launcher (`npm/launcher.js`) names them.
const PLATFORMS = {
  darwin: { label: "macOS (Apple silicon and Intel)", os: ["darwin"], cpu: ["arm64", "x64"] },
  "linux-x64": { label: "Linux on x64", os: ["linux"], cpu: ["x64"] },
  "linux-arm64": { label: "Linux on arm64", os: ["linux"], cpu: ["arm64"] },
};

// The package.json of `platform`'s package, from the npm package's: its version and links, the binaries in `bin/`, and
// no command, since the npm package's launcher is the one on the PATH.
export function platformManifest(launcherManifest, platform) {
  const marks = PLATFORMS[platform];
  if (marks === undefined) throw new Error(`no platform package for ${platform}`);
  const { version, license, author, homepage, bugs, repository } = JSON.parse(launcherManifest);
  const manifest = {
    name: `plotkeeper-${platform}`,
    description: `plotkeeper's binaries for ${marks.label}. Install plotkeeper, which uses this.`,
    version,
    license,
    author,
    homepage,
    bugs,
    repository,
    files: ["bin/"],
    os: marks.os,
    cpu: marks.cpu,
  };
  return `${JSON.stringify(manifest, null, 2)}\n`;
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

// The release commit guard: a publish runs only on the Release PR's commit, the one that changed the npm package's
// version. It takes npm/package.json's text at HEAD's parent and at HEAD.
export function checkReleaseCommit(parentManifest, headManifest) {
  if (JSON.parse(parentManifest).version === JSON.parse(headManifest).version) {
    throw new Error("main moved past the Release PR, run /release again");
  }
}

// The commit origin's `tag` points at, or null when origin has no such tag. An annotated tag lists its commit on a
// second line, `refs/tags/<tag>^{}`, which wins over the tag object's own line.
function taggedCommit(run, tag) {
  const { status, stdout, stderr } = run("git", ["ls-remote", "--tags", "origin", `refs/tags/${tag}`, `refs/tags/${tag}^{}`]);
  if (status !== 0) throw new Error(`git ls-remote origin failed for ${tag}: ${stderr}`);
  const refs = new Map(stdout.split("\n").filter(Boolean).map((line) => line.split("\t").reverse()));
  return refs.get(`refs/tags/${tag}^{}`) ?? refs.get(`refs/tags/${tag}`) ?? null;
}

// Tags `v<version>` at `commit` on origin and creates its GitHub Release from the changelog's section. Idempotent: a
// tag already at `commit` is kept, one at another commit fails, and an existing Release is left as it is. `run(command,
// args, input)` runs git or gh and returns `{ status, stdout, stderr }`.
export function tagAndRelease({ version, commit, changelog, run }) {
  const notes = notesFor(changelog, version);
  const tag = `v${version}`;
  const tagged = taggedCommit(run, tag);
  if (tagged === null) run("git", ["push", "origin", `${commit}:refs/tags/${tag}`]);
  else if (tagged !== commit) throw new Error(`${tag} already points at ${tagged}, not ${commit}`);
  const view = run("gh", ["release", "view", tag]);
  if (view.status === 0) return;
  if (!/release not found/.test(view.stderr ?? "")) throw new Error(`gh release view ${tag} failed: ${view.stderr}`);
  run("gh", ["release", "create", tag, "--verify-tag", "--title", tag, "--notes-file", "-"], notes);
}

// Runs git or gh. A read returns its result; a change must succeed, and with `dryRun` it is printed instead of run.
function runner(dryRun) {
  return (command, args, input) => {
    const change = !(["ls-remote", "rev-parse", "show"].includes(args[0]) || args[1] === "view");
    if (change && dryRun) {
      console.log(`would run: ${[command, ...args].join(" ")}${input === undefined ? "" : `, with notes:\n${input}`}`);
      return { status: 0, stdout: "", stderr: "" };
    }
    const result = spawnSync(command, args, { input, encoding: "utf8" });
    if (result.error) throw result.error;
    if (change && result.status !== 0) throw new Error(`${command} ${args.join(" ")} failed: ${result.stderr}`);
    return result;
  };
}

function git(args) {
  const { status, stdout, stderr } = runner(false)("git", args);
  if (status !== 0) throw new Error(`git ${args.join(" ")} failed: ${stderr}`);
  return stdout;
}

/** @param {string[]} argv the command, its argument, the output folder, then flags */
function main([command, arg, out, ...flags]) {
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
  } else if (command === "guard") {
    // A checkout at depth 1 has no HEAD^, so this fails there rather than pass.
    checkReleaseCommit(git(["show", "HEAD^:npm/package.json"]), git(["show", "HEAD:npm/package.json"]));
  } else if (command === "platform" && arg && out) {
    const text = platformManifest(read(FILES.npmManifest), arg);
    const manifest = JSON.parse(text);
    writeFileSync(resolve(out, "package.json"), text);
    writeFileSync(resolve(out, "README.md"), `# ${manifest.name}\n\n${manifest.description}\n\n    npx plotkeeper\n`);
  } else if (command === "tag" && arg && out && flags.every((flag) => flag === "--dry-run")) {
    const commit = git(["rev-parse", "--verify", `${out}^{commit}`]).trim();
    tagAndRelease({ version: arg, commit, changelog: read("CHANGELOG.md"), run: runner(flags.length > 0) });
  } else {
    throw new Error(
      "usage: release.mjs bump <patch|minor> | notes <X.Y.Z> [file] | guard | platform <platform> <dir> |" +
        " tag <X.Y.Z> <commit> [--dry-run]",
    );
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

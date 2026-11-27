// The release script (danielrispler/plotkeeper#512): `bump` writes one version into the npm package's manifest, the
// server crate's manifest and the lockfile's `studio-server` entry, `notesFor` reads a version's changelog section,
// `checkReleaseCommit` is the publish guard on HEAD's commit, `tagAndRelease` tags a commit and creates its Release, and
// `platformManifest` writes a platform package's manifest (#706).
// Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { bump, checkReleaseCommit, notesFor, platformManifest, tagAndRelease } from "../../scripts/npm/release.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

const npmManifest = `{
  "name": "plotkeeper",
  "version": "0.1.2",
  "description": "x",
  "optionalDependencies": {
    "plotkeeper-darwin": "0.1.2",
    "plotkeeper-linux-x64": "0.1.2"
  }
}
`;
const crateManifest = `[package]
name = "studio-server"
edition = "2024"
version = "0.1.2"

[dependencies]
other = { version = "0.1.2" }
`;
// "other" and "zeta" share the old version string and must come through byte for byte.
const lockfile = `version = 4

[[package]]
name = "other"
version = "0.1.2"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "studio-server"
version = "0.1.2"
dependencies = [
 "other",
]

[[package]]
name = "zeta"
version = "0.1.2"
`;
const texts = { npmManifest, crateManifest, lockfile };

const expected = (version) => ({
  npmManifest: `{
  "name": "plotkeeper",
  "version": "${version}",
  "description": "x",
  "optionalDependencies": {
    "plotkeeper-darwin": "${version}",
    "plotkeeper-linux-x64": "${version}"
  }
}
`,
  crateManifest: `[package]
name = "studio-server"
edition = "2024"
version = "${version}"

[dependencies]
other = { version = "0.1.2" }
`,
  lockfile: `version = 4

[[package]]
name = "other"
version = "0.1.2"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "studio-server"
version = "${version}"
dependencies = [
 "other",
]

[[package]]
name = "zeta"
version = "0.1.2"
`,
});

test("bump patch takes 0.1.2 to 0.1.3 in all three texts and leaves other lockfile entries unchanged", () => {
  assert.deepEqual(bump(texts, "patch"), expected("0.1.3"));
});

test("bump minor takes 0.1.2 to 0.2.0 in all three texts", () => {
  assert.deepEqual(bump(texts, "minor"), expected("0.2.0"));
});

test("bump rejects major and unknown levels", () => {
  for (const level of ["major", "premajor", "", undefined]) {
    assert.throws(() => bump(texts, level), /patch or minor/);
  }
});

test("bump names the old version a file lacks", () => {
  const stale = { ...texts, crateManifest: crateManifest.replace('version = "0.1.2"\n\n', 'version = "0.1.1"\n\n') };
  assert.throws(() => bump(stale, "patch"), /^Error: server\/Cargo\.toml has no version 0\.1\.2$/);
});

test("bump names a platform package the launcher asks for at another version", () => {
  const stale = { ...texts, npmManifest: npmManifest.replace('"plotkeeper-linux-x64": "0.1.2"', '"plotkeeper-linux-x64": "0.1.1"') };
  assert.throws(
    () => bump(stale, "patch"),
    /^Error: npm\/package\.json asks for plotkeeper-linux-x64 0\.1\.1, not its version 0\.1\.2$/,
  );
});

test("the npm package asks for each platform package at its own version", () => {
  const { version, optionalDependencies } = JSON.parse(readFileSync(resolve(root, "npm/package.json"), "utf8"));
  assert.deepEqual(optionalDependencies, {
    "plotkeeper-darwin": version,
    "plotkeeper-linux-x64": version,
    "plotkeeper-linux-arm64": version,
  });
});

const launcherManifest = JSON.stringify({
  name: "plotkeeper",
  version: "0.1.2",
  description: "x",
  license: "MIT",
  author: "A",
  homepage: "https://h",
  bugs: { url: "https://b" },
  repository: { type: "git", url: "git+https://r" },
  keywords: ["k"],
  bin: { plotkeeper: "launcher.js" },
  optionalDependencies: { "plotkeeper-darwin": "0.1.2" },
});

test("platformManifest marks each platform package with its os and cpu, at the launcher's version, with no command", () => {
  const common = {
    version: "0.1.2",
    license: "MIT",
    author: "A",
    homepage: "https://h",
    bugs: { url: "https://b" },
    repository: { type: "git", url: "git+https://r" },
    files: ["bin/"],
  };
  assert.deepEqual(JSON.parse(platformManifest(launcherManifest, "darwin")), {
    name: "plotkeeper-darwin",
    description: "plotkeeper's binaries for macOS (Apple silicon and Intel). Install plotkeeper, which uses this.",
    ...common,
    os: ["darwin"],
    cpu: ["arm64", "x64"],
  });
  assert.deepEqual(JSON.parse(platformManifest(launcherManifest, "linux-x64")), {
    name: "plotkeeper-linux-x64",
    description: "plotkeeper's binaries for Linux on x64. Install plotkeeper, which uses this.",
    ...common,
    os: ["linux"],
    cpu: ["x64"],
  });
  assert.deepEqual(JSON.parse(platformManifest(launcherManifest, "linux-arm64")), {
    name: "plotkeeper-linux-arm64",
    description: "plotkeeper's binaries for Linux on arm64. Install plotkeeper, which uses this.",
    ...common,
    os: ["linux"],
    cpu: ["arm64"],
  });
});

test("platformManifest refuses a platform with no package", () => {
  assert.throws(() => platformManifest(launcherManifest, "win32-x64"), /^Error: no platform package for win32-x64$/);
});

const changelog = `# Changelog

## 0.2.0

### Added

- A thing. (#9)

## 0.1.10

### Fixed

- Ten. (#8)

## 0.1.1

### Fixed

- One. (#7)
`;

test("notesFor returns a version's section body and stops at the next heading", () => {
  assert.equal(notesFor(changelog, "0.2.0"), "### Added\n\n- A thing. (#9)\n");
  assert.equal(notesFor(changelog, "0.1.1"), "### Fixed\n\n- One. (#7)\n");
});

test("notesFor fails on a missing version, including a prefix of one present", () => {
  assert.throws(() => notesFor(changelog, "0.1.3"), /no section for 0\.1\.3/);
  assert.throws(() => notesFor(changelog, "0.1"), /no section for 0\.1$/);
});

// A note line: one sentence ending with its PR number, or with all the numbers of the PRs that share it.
const noteLine = /^- .+\(#\d+(, #\d+)*\)$/m;

test("a note line ends with one PR number or several", () => {
  assert.match("- A fix. (#7)", noteLine);
  assert.match("- One change. (#468, #470)", noteLine);
  assert.doesNotMatch("- No number.", noteLine);
});

test("CHANGELOG.md has notes for the npm package's current version", () => {
  const { version } = JSON.parse(readFileSync(resolve(root, "npm/package.json"), "utf8"));
  const notes = notesFor(readFileSync(resolve(root, "CHANGELOG.md"), "utf8"), version);
  assert.match(notes, noteLine, `CHANGELOG.md's ${version} section has no line ending with (#N) or (#N, #M)`);
});

const manifestAt = (version) => npmManifest.replace("0.1.2", version);

test("checkReleaseCommit passes when HEAD changed the npm package's version and fails when it didn't", () => {
  assert.doesNotThrow(() => checkReleaseCommit(manifestAt("0.1.2"), manifestAt("0.1.3")));
  assert.throws(
    () => checkReleaseCommit(manifestAt("0.1.3"), manifestAt("0.1.3").replace('"x"', '"y"')),
    /^Error: main moved past the Release PR, run \/release again$/,
  );
});

const commit = "a".repeat(40);
const other = "b".repeat(40);

// A stand-in for git and gh: `tag` is the commit origin's v0.2.0 tag points at (none when absent), `release` whether
// v0.2.0's GitHub Release exists, or "error" when gh cannot tell (signed out, offline). It answers the reads and records every command that would change something.
function fakeRun({ tag, release }) {
  const changes = [];
  const run = (command, args, input) => {
    const line = [command, ...args].join(" ");
    if (line.startsWith("git ls-remote ")) return { status: 0, stdout: tag ? `${tag}\trefs/tags/v0.2.0\n` : "" };
    if (line === "gh release view v0.2.0") {
      if (release === "error") return { status: 1, stdout: "", stderr: "HTTP 401: Bad credentials\n" };
      return release ? { status: 0, stdout: "" } : { status: 1, stdout: "", stderr: "release not found\n" };
    }
    changes.push(input === undefined ? line : `${line} <<< ${input}`);
    return { status: 0, stdout: "" };
  };
  return { run, changes };
}

const release = (fake, version = "0.2.0") => tagAndRelease({ version, commit, changelog, run: fake.run });

test("tagAndRelease tags an untagged version at the commit and creates its Release from the changelog", () => {
  const fake = fakeRun({ tag: null, release: false });
  release(fake);
  assert.deepEqual(fake.changes, [
    `git push origin ${commit}:refs/tags/v0.2.0`,
    "gh release create v0.2.0 --verify-tag --title v0.2.0 --notes-file - <<< ### Added\n\n- A thing. (#9)\n",
  ]);
});

test("tagAndRelease fails on a tag at another commit and changes nothing", () => {
  const fake = fakeRun({ tag: other, release: false });
  assert.throws(() => release(fake), new RegExp(`v0\\.2\\.0 already points at ${other}, not ${commit}`));
  assert.deepEqual(fake.changes, []);
});

test("tagAndRelease leaves an existing Release as it is", () => {
  const fake = fakeRun({ tag: commit, release: true });
  release(fake);
  assert.deepEqual(fake.changes, []);
});

test("tagAndRelease fails when gh cannot tell whether the Release exists, and creates nothing", () => {
  const fake = fakeRun({ tag: commit, release: "error" });
  assert.throws(() => release(fake), /gh release view v0\.2\.0 failed: HTTP 401/);
  assert.deepEqual(fake.changes, []);
});

test("tagAndRelease fails on a version with no changelog section before touching anything", () => {
  const fake = fakeRun({ tag: null, release: false });
  assert.throws(() => release(fake, "0.3.0"), /no section for 0\.3\.0/);
  assert.deepEqual(fake.changes, []);
});

test("tagAndRelease leaves a tag already at the commit and still creates a missing Release", () => {
  const fake = fakeRun({ tag: commit, release: false });
  release(fake);
  assert.deepEqual(fake.changes, [
    "gh release create v0.2.0 --verify-tag --title v0.2.0 --notes-file - <<< ### Added\n\n- A thing. (#9)\n",
  ]);
});

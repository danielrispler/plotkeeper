// The release script (danielrispler/plotkeeper#512): `bump` writes one version into the npm package's manifest, the
// server crate's manifest and the lockfile's `studio-server` entry, and `notesFor` reads a version's changelog section.
// Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { bump, notesFor } from "../../scripts/npm/release.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

const npmManifest = `{
  "name": "plotkeeper",
  "version": "0.1.2",
  "description": "x"
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
  "description": "x"
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

test("CHANGELOG.md has notes for the npm package's current version", () => {
  const { version } = JSON.parse(readFileSync(resolve(root, "npm/package.json"), "utf8"));
  const notes = notesFor(readFileSync(resolve(root, "CHANGELOG.md"), "utf8"), version);
  assert.match(notes, /^- .+\(#\d+\)$/m, `CHANGELOG.md's ${version} section has no line ending with (#N)`);
});

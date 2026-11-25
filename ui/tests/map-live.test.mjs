// The live link's rules (spec danielrispler/plotkeeper#549, ticket #559): per-path placement, the 7-day cut, the footprint, the marked rule and
// the spotlit session, run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { footprint, footprintNote, marked, placeGroups, sessionHere, spotlit } from "../../dist/map-live.js";

const day = 24 * 60 * 60 * 1000;
const now = 100 * day;
const file = (path, agent = null) => ({ root: "/w/p", path, from: null, added: 1, removed: 0, agent });
// A turn of `paths` (a path, or [path, agent] for a Worker's) that ended `ago` ms before now; `open` leaves it unended, aged from its start.
const turn = (paths, ago = 0, open = false) => ({
  started_at: now - ago - 1000, ended_at: open ? null : now - ago, prompt: "", id: null, message: null, checks: [], listed: [],
  files: paths === null ? null : { files: paths.map((p) => (Array.isArray(p) ? file(...p) : file(p))), merged: 0 },
});
const worker = (id, extra = {}) => ({ id, parent: "session", kind: "worker", name: `worker ${id}`, status: "running", ...extra });
const session = (id, state, extra = {}) => ({
  id, state, name: `session ${id}`, cwd: "/w/p", shell: false, status_only: false, asker: null, agents: [], turns: [], last_step: null,
  capabilities: { turn_trees: true }, identity: { name: `identity ${id}` }, unseen: false, ...extra,
});

test("map_place is asked one group per distinct path of every turn within the cut, and a Running session's in-flight edit", () => {
  const sessions = [
    session("a", "running", { turns: [turn(["core/x.rs"], 2 * day), turn(["core/x.rs", ["ui/w.ts", "w1"]])], last_step: { tool: "Edit", detail: "/w/p/hook/h.rs" } }),
    session("b", "ended", { turns: [turn(["old/gone.rs"], 9 * day), turn(["server/s.rs"], 6 * day)] }),
    // A Needs you session's last edit is not in flight; a Bash step names no file.
    session("c", "needs_you", { turns: [turn(null)], last_step: { tool: "Write", detail: "/w/p/stale.rs" } }),
    session("d", "running", { turns: [turn([])], last_step: { tool: "Bash", detail: "ls" } }),
  ];
  assert.deepEqual(placeGroups(sessions, now), {
    "core/x.rs": ["core/x.rs"], "ui/w.ts": ["ui/w.ts"], "hook/h.rs": ["hook/h.rs"], "server/s.rs": ["server/s.rs"],
  });
});

// The placement as `map_place` answers per path: core and ui are parts, docs is ignored and stray.txt is owned by no part.
const placed = { "core/x.rs": ["core"], "core/y.rs": ["core"], "ui/w.ts": ["window"], "server/s.rs": ["server"], "hook/h.rs": ["hook"], "docs/a.md": [], "stray.txt": [], "old/gone.rs": ["old"] };

test("a footprint is firm on the latest turn's parts and faint on the other parts within the cut, Workers' files included", () => {
  const s = session("a", "your_turn", { agents: [worker("w1", { status: "completed" })], turns: [
    turn(["old/gone.rs"], 8 * day),
    turn(["server/s.rs", "core/y.rs"], 3 * day),
    turn(["core/x.rs", ["ui/w.ts", "w1"], "docs/a.md", "stray.txt"]),
  ] });
  assert.deepEqual(footprint(s, placed, now), { firm: ["core", "window"], faint: ["server"] });
});

test("the cut counts a turn from its end, or from its start if it never ended, whatever files it still holds", () => {
  const s = (turns) => session("a", "ended", { turns });
  // Ended 7 days less a minute ago: in. Ended 7 days and a minute ago: out, though its files are still there.
  assert.deepEqual(footprint(s([turn(["core/x.rs"], 7 * day - 60e3)]), placed, now), { firm: ["core"], faint: [] });
  assert.deepEqual(footprint(s([turn(["core/x.rs"], 7 * day + 60e3)]), placed, now), { firm: [], faint: [] });
  // A turn that never ended (its session was killed) ages from its start: started 8 days ago, out; a day ago, in.
  assert.deepEqual(footprint(s([turn(["core/x.rs"], 8 * day, true)]), placed, now), { firm: [], faint: [] });
  assert.deepEqual(footprint(s([turn(["core/x.rs"], day, true)]), placed, now), { firm: ["core"], faint: [] });
});

test("a Running session's in-flight edit is on its latest turn before the turn's files are diffed", () => {
  const s = session("a", "running", { turns: [turn(["server/s.rs"], day), turn(null, 0, true)], last_step: { tool: "MultiEdit", detail: "/w/p/hook/h.rs" } });
  assert.deepEqual(footprint(s, placed, now), { firm: ["hook"], faint: ["server"] });
  // Once it stops, its last edit no longer counts.
  assert.deepEqual(footprint({ ...s, state: "needs_you" }, placed, now), { firm: [], faint: ["server"] });
});

test("Running and Needs you sessions are marked, and Your turn or Idle ones while unseen; shells, Ended and other projects' never", () => {
  const here = (cwd) => cwd === "/w/p";
  const cases = [
    [session("r", "running"), true], [session("n", "needs_you"), true],
    [session("y", "your_turn"), false], [session("yu", "your_turn", { unseen: true }), true],
    [session("i", "idle"), false], [session("iu", "idle", { unseen: true }), true],
    [session("e", "ended", { unseen: true }), false], [session("sh", "running", { shell: true }), false],
    [session("away", "running", { cwd: "/elsewhere" }), false],
  ];
  assert.deepEqual(cases.map(([s]) => [s.id, marked(s, here)]), cases.map(([s, want]) => [s.id, want]));
});

test("the spotlit session is the hovered one, else the open one; none while a brief plays or a past commit shows", () => {
  const at = (extra) => spotlit({ hovered: null, open: null, brief: false, past: false, shown: ["a", "b"], ...extra });
  assert.equal(at({ hovered: "a", open: "b" }), "a");
  assert.equal(at({ open: "b" }), "b");
  assert.equal(at({}), null);
  assert.equal(at({ hovered: "a", open: "b", brief: true }), null);
  assert.equal(at({ hovered: "a", open: "b", past: true }), null);
  // A hovered session that left the shown set (another project's, or gone) gives the open one back.
  assert.equal(at({ hovered: "gone", open: "b" }), "b");
  assert.equal(at({ hovered: "gone" }), null);
});

test("a session that touched no part within the cut says so; one whose placement is pending says nothing yet", () => {
  const note = (turns, given = placed) => footprintNote(session("a", "ended", { turns }), given, now);
  assert.equal(note([]), "Touched no part in the last 7 days");
  assert.equal(note([turn(["core/x.rs"], 9 * day), turn(["docs/a.md", "stray.txt"], day)]), "Touched no part in the last 7 days");
  assert.equal(note([turn(["core/x.rs"], day)]), null);
  assert.equal(note([turn(["new/file.rs"], day)]), null);
});

// "This session here" (#561): the open session's turns within the cut that changed a part, newest first, and whether it has older turns.
test("the sheet lists the open session's turns within the cut that changed the part, newest first, each by its number", () => {
  const s = session("a", "your_turn", { agents: [worker("w1", { status: "completed" })], turns: [
    turn(["core/x.rs"], 9 * day),
    turn(["core/y.rs", "server/s.rs"], 3 * day),
    turn(["server/s.rs"], 2 * day),
    turn(null, day),
    turn([["core/x.rs", "w1"], "docs/a.md"]),
  ] });
  const here = (part) => { const found = sessionHere(s, part, placed, now); return { rows: found.rows.map((row) => row.number), older: found.older }; };
  assert.deepEqual(here("core"), { rows: [5, 2], older: true });
  assert.deepEqual(here("server"), { rows: [3, 2], older: true });
  assert.deepEqual(here("old"), { rows: [], older: true });
});

test("the sheet's footer shows only when the open session has turns older than the cut", () => {
  const s = (turns) => sessionHere(session("a", "ended", { turns }), "core", placed, now).older;
  assert.equal(s([]), false);
  assert.equal(s([turn(["server/s.rs"], 6 * day), turn(["core/x.rs"])]), false);
  // An older turn counts whatever it changed, and a turn that never ended ages from its start.
  assert.equal(s([turn(["docs/a.md"], 8 * day), turn(["core/x.rs"])]), true);
  assert.equal(s([turn(null, 8 * day, true)]), true);
});

// The live link's rules (spec danielrispler/plotkeeper#549, ticket #559): per-path placement, the 7-day cut, the footprint, the marked rule and
// the spotlit session, run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { footprint, footprintNote, homePart, spotlightView, marked, notPlacedLine, placeGroups, sessionBadges, sessionHere, spotlit, turnLight, oldTurn, OLD_TURN } from "../../dist/map-live.js";

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

test("the sheet's rows are pending while map_place has not answered for a path of the session's turns within the cut", () => {
  const s = session("a", "ended", { turns: [turn(["old/new.rs"], 9 * day), turn(["core/x.rs"], 2 * day), turn(["new/n.rs"])] });
  assert.equal(sessionHere(s, "server", placed, now).pending, true, "new/n.rs is not answered yet");
  assert.equal(sessionHere(s, "server", { ...placed, "new/n.rs": [] }, now).pending, false, "a path past the cut is never waited for");
});

test("the sheet's footer shows only when the open session has turns older than the cut", () => {
  const s = (turns) => sessionHere(session("a", "ended", { turns }), "core", placed, now).older;
  assert.equal(s([]), false);
  assert.equal(s([turn(["server/s.rs"], 6 * day), turn(["core/x.rs"])]), false);
  // An older turn counts whatever it changed, and a turn that never ended ages from its start.
  assert.equal(s([turn(["docs/a.md"], 8 * day), turn(["core/x.rs"])]), true);
  assert.equal(s([turn(null, 8 * day, true)]), true);
});

test("a turn lights the parts it changed, Workers' files included; nothing past the cut, without files or while a brief plays", () => {
  const recent = turn(["core/x.rs", ["ui/w.ts", "w1"], "core/y.rs", "stray.txt"], day);
  assert.deepEqual(turnLight(recent, placed, now, false), ["core", "window"]);
  assert.deepEqual(turnLight(recent, placed, now, true), []);
  // Older than the cut, its files still held: nothing lights.
  assert.deepEqual(turnLight(turn(["core/x.rs"], 8 * day), placed, now, false), []);
  assert.deepEqual(turnLight(turn([], day), placed, now, false), []);
  assert.deepEqual(turnLight(turn(null, 0, true), placed, now, false), []);
});

test("a turn past the cut says its files aren't shown, only for an agent that records turn files", () => {
  assert.equal(OLD_TURN, "Files older than 7 days aren't shown on the map");
  assert.equal(oldTurn(turn(["core/x.rs"], 8 * day), true, now), true);
  assert.equal(oldTurn(turn(null, 8 * day, true), true, now), true);
  assert.equal(oldTurn(turn(["core/x.rs"], 6 * day), true, now), false);
  // An agent without turn trees never records files: no line blames the cut.
  assert.equal(oldTurn(turn(null, 8 * day), false, now), false);
});

// Session badges (#562): one badge per part, counting the marked sessions whose latest turn touches it.
const here = () => true;
const counts = (byPart) => Object.fromEntries(Object.entries(byPart).map(([part, badge]) => [part, badge.rows.map((row) => row.session.id)]));

test("a badge counts a session once with its Workers, and a Worker-only part counts the Worker's session", () => {
  const s = session("a", "running", { agents: [worker("w1"), worker("w2")], turns: [
    turn(["hook/h.rs"], 2 * day),
    turn(["core/x.rs", ["core/y.rs", "w1"], ["ui/w.ts", "w2"]]),
  ] });
  const { byPart } = sessionBadges([s], placed, now, here);
  assert.deepEqual(counts(byPart), { core: ["a"], window: ["a"] });
  assert.deepEqual(byPart.core.rows[0].workers.map((w) => w.id), ["w1"]);
  assert.deepEqual(byPart.window.rows[0].workers.map((w) => w.id), ["w2"]);
});

test("a badge counts the session's own files and its running Workers', never a finished Worker's; the footprint keeps every Worker's", () => {
  const s = session("a", "running", { agents: [worker("w1"), worker("w2", { status: "completed" })], turns: [
    turn(["core/x.rs", ["ui/w.ts", "w1"], ["server/s.rs", "w2"]]),
  ] });
  assert.deepEqual(counts(sessionBadges([s], placed, now, here).byPart), { core: ["a"], window: ["a"] });
  assert.deepEqual(spotlightView({}, s, placed, now, []).chips.sort(), ["core", "window"], "the chip follows the badge");
  assert.deepEqual(footprint(s, placed, now).firm.sort(), ["core", "server", "window"]);
});

test("Running, Needs you and Unseen sessions are counted; a seen Your turn or Idle, a shell, an Ended and another project's are not", () => {
  const on = () => turn(["core/x.rs"]);
  const sessions = [
    session("run", "running", { turns: [on()] }),
    session("ask", "needs_you", { turns: [on()] }),
    session("new", "your_turn", { unseen: true, turns: [on()] }),
    session("rest", "idle", { unseen: true, turns: [on()] }),
    session("seen", "your_turn", { turns: [on()] }),
    session("calm", "idle", { turns: [on()] }),
    session("shell", "running", { shell: true, turns: [on()] }),
    session("done", "ended", { unseen: true, turns: [on()] }),
    session("away", "running", { cwd: "/elsewhere", turns: [on()] }),
  ];
  const { byPart } = sessionBadges(sessions, placed, now, (cwd) => cwd === "/w/p");
  // Needs you first, then the order given (the session list's).
  assert.deepEqual(counts(byPart), { core: ["ask", "run", "new", "rest"] });
});

test("a badge carries the accent when any of its sessions needs you, a Worker's ask included, on a Worker-only part too", () => {
  const asking = session("a", "needs_you", { agents: [worker("w1")], asker: { agent: "w1", since: 1 }, turns: [turn([["ui/w.ts", "w1"]])] });
  const calm = session("b", "running", { turns: [turn(["core/x.rs", "ui/w.ts"])] });
  const { byPart } = sessionBadges([calm, asking], placed, now, here);
  assert.equal(byPart.window.accent, true);
  assert.equal(byPart.core.accent, false);
  assert.deepEqual(counts(byPart), { core: ["b"], window: ["a", "b"] });
});

test("the not placed rows are the marked sessions, Unseen included, whose latest turn touches no part, one row each", () => {
  const sessions = [
    session("none", "running", { turns: [turn([])] }),
    session("stray", "needs_you", { agents: [worker("w1")], turns: [turn(["stray.txt", ["docs/a.md", "w1"]])] }),
    session("unseen", "idle", { unseen: true, turns: [turn(["docs/a.md"])] }),
    session("codex", "running", { status_only: true, capabilities: { turn_trees: false } }),
    // An earlier turn's part does not place the session.
    session("before", "your_turn", { unseen: true, turns: [turn(["core/x.rs"], day), turn([])] }),
    session("placed", "running", { turns: [turn(["core/x.rs"])] }),
    session("seen", "idle", { turns: [turn([])] }),
  ];
  const { byPart, notPlaced } = sessionBadges(sessions, placed, now, here);
  assert.deepEqual(notPlaced.map((s) => s.id), ["stray", "none", "unseen", "codex", "before"]);
  assert.deepEqual(counts(byPart), { core: ["placed"] });
});

// The home part (#562): where the spotlit session's line sits.
test("the home part is a Running session's in-flight edit's part, else the part with most changed files, a tie to reading order", () => {
  const order = ["window", "server", "core", "hook"];
  const turns = [turn(["core/x.rs", "core/y.rs", "server/s.rs"])];
  // Running with an edit in flight in a part: that part, though core holds more files.
  assert.equal(homePart(session("a", "running", { turns, last_step: { tool: "Edit", detail: "/w/p/hook/h.rs" } }), placed, now, order), "hook");
  // An in-flight file no part owns falls through to most files.
  assert.equal(homePart(session("a", "running", { turns, last_step: { tool: "Write", detail: "/w/p/stray.txt" } }), placed, now, order), "core");
  // A stale last edit of a session that is not Running falls through too.
  assert.equal(homePart(session("a", "needs_you", { turns, last_step: { tool: "Edit", detail: "/w/p/hook/h.rs" } }), placed, now, order), "core");
  // A tie goes to the map's reading order; a Worker's files count.
  const tie = [turn(["core/x.rs", ["ui/w.ts", "w1"], "server/s.rs"])];
  assert.equal(homePart(session("a", "running", { turns: tie }), placed, now, order), "window");
  assert.equal(homePart(session("a", "running", { turns: tie }), placed, now, ["server", "core", "window"]), "server");
  // No part: no home.
  assert.equal(homePart(session("a", "running", { turns: [turn(["stray.txt"])] }), placed, now, order), null);
});

test("a spotlit session's chip replaces the badge on its latest turn's parts; the other badges dim, a Needs-you one stays full", () => {
  const spot = session("a", "idle", { turns: [turn(["server/s.rs"], 2 * day), turn(["core/x.rs", "core/y.rs", "ui/w.ts"])] });
  const others = [
    session("b", "running", { turns: [turn(["core/x.rs", "hook/h.rs"])] }),
    session("c", "needs_you", { turns: [turn(["server/s.rs"])] }),
  ];
  const { byPart } = sessionBadges(others, placed, now, here);
  const order = ["window", "server", "core", "hook"];
  const shown = spotlightView(byPart, spot, placed, now, order);
  assert.deepEqual(shown.chips.sort(), ["core", "window"]);
  assert.equal(shown.home, "core");
  // Core's badge gives way to the chip; hook dims; server needs you and stays full.
  assert.deepEqual(shown.dimmed, ["hook"]);
  // With no spotlit session nothing changes.
  assert.deepEqual(spotlightView(byPart, null, placed, now, order), { chips: [], home: null, dimmed: [] });
});

test("a spotlit session with no part in its latest turn has no chip and no home, and still dims the badges", () => {
  const spot = session("a", "ended", { turns: [turn(["core/x.rs"], 3 * day), turn([])] });
  const { byPart } = sessionBadges([session("b", "running", { turns: [turn(["hook/h.rs"])] })], placed, now, here);
  assert.deepEqual(spotlightView(byPart, spot, placed, now, []), { chips: [], home: null, dimmed: ["hook"] });
});

test("the not placed line counts the sessions no part holds, and says how many need you (#472)", () => {
  const of = (...states) => states.map((state) => ({ state }));
  assert.deepEqual(notPlacedLine(of("running")), { count: "1 not placed", needsYou: null });
  assert.deepEqual(notPlacedLine(of("running", "needs_you", "needs_you")), { count: "3 not placed", needsYou: "2 need you" });
  assert.deepEqual(notPlacedLine(of("needs_you")), { count: "1 not placed", needsYou: "1 needs you" });
});

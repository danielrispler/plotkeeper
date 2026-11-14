// Studio state for `npm run snap`: each fixture maps a Tauri command to what it returns.
// A fixture is a function so that its times are relative to the moment it renders.

const session = (id, name, state, extra = {}) => {
  const now = Date.now();
  return {
    id, cwd: `/w/plotkeeper/.claude/worktrees/${id}`, folder_name: id, terminal: null, status_only: false, name, prompt_name: null,
    resume_id: id, permission_mode: null, started_mode: null, cleared: [], asks_before_closing: false, landed: false, last_step: null,
    started_at: now - 3600e3, last_event_at: now - 60e3, turn_span: null, step_span: null, state, checks: [], agents: [], asker: null,
    prompts: [], turns: [], questions: [], returned: null,
    transcript: { intention: null, steps_ago: 0, responses: [], unavailable: null, context: null, branch: null, model: null, effort: null },
    identity: { issue: null, branch: null, pr: null, worktree: "", name },
    work: { kind: null, phases: [], agent_ms: 0, estimate: { range: null, depends_on_you: false } },
    unseen: false, ...extra,
  };
};

const subagent = (id, name) => ({
  id, parent: "session", kind: "worker", name, asked: null, agent_type: null, status: "running",
  started_at: Date.now() - 600e3, ended_at: null, last_step: null, step_span: null,
});

const common = {
  settings_files: { settings: null, keymap: null },
  cards: { brief: null, recent: [], older: [], decisions: [] },
  workspaces: { main: { path: "/w/plotkeeper", branch: "main" }, tasks: [], hidden: 0 },
  terminal_connect: { repo: "/w/plotkeeper", outdated: false },
  terminal_lost: [],
  revival: [],
};

export const fixtures = {
  // The list with a nested row (a session with subagents), the selected row and a landed one.
  list() {
    const sessions = [
      session("s1", "#56 · Row controls", "running", { agents: [subagent("a1", "Review the diff"), subagent("a2", "Write tests")] }),
      session("s2", "#57 · Name from the prompt", "needs_you"),
      session("s3", "#54 · Subagent terminal", "your_turn", { landed: true }),
      session("s4", "#55 · Delete default", "your_turn", { unseen: true }),
    ];
    return { ...common, sessions, selected: { session: sessions[0], agent: null } };
  },
  // Home: nothing selected, with Needs you, Unseen, Landed and the project row.
  home() {
    const sessions = [
      session("s1", "#56 · Row controls", "running"),
      session("s2", "#57 · Name from the prompt", "needs_you"),
      session("s4", "#55 · Delete default", "your_turn", { unseen: true }),
      session("s3", "#54 · Subagent terminal", "idle", { landed: true }),
      session("s5", "#48 · Agent deadline", "ended", { landed: true }),
    ];
    return { ...common, sessions, selected: null };
  },
};

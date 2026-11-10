// The skills browser (#528): a palette action (#565, decision 7) that takes the rail's place; Esc
// returns. Skills are grouped by source, Daniel's own, the project's, then each plugin's, and each
// opens to its `SKILL.md`, rendered. "Edit" opens a user or project skill through the opener (#557);
// plugin skills are read-only.

import { element, invoke, pressable, text } from "./dom.js";
import { rendered } from "./markdown.js";
import { studio } from "./state.js";

type Skill = { name: string; source: string; path: string; text: string; editable: boolean };

const titles: Record<string, string> = { user: "Your skills", project: "This project" };

// The front matter's `description`, and the body after it.
function parts(md: string): { description: string; body: string } {
  const front = /^---\n([\s\S]*?)\n---\n?/.exec(md);
  const description = front && /^description:\s*["']?(.*?)["']?\s*$/m.exec(front[1])?.[1];
  return { description: description ?? "", body: front ? md.slice(front[0].length) : md };
}

function row(skill: Skill): HTMLElement {
  const { description, body } = parts(skill.text);
  const fold = document.createElement("details");
  fold.className = "skill";
  const summary = text("summary", "");
  summary.append(text("span", skill.name, "name"));
  if (description) summary.append(text("span", description, "l2"));
  fold.append(summary);
  // Rendered on first open, so a hundred skills cost nothing until read.
  fold.addEventListener("toggle", () => {
    if (!fold.open || fold.childElementCount > 1) return;
    if (skill.editable) {
      const editor = studio.files.settings.editor;
      const edit = () => void invoke("open_file", { path: skill.path, line: 1, column: 1, cwd: null, editor }).catch((e) => console.warn(e));
      fold.append(pressable(text("button", "Edit", "btn small"), false, edit));
    }
    fold.append(rendered("div", body, "", skill.path.replace(/\/[^/]*$/, "")));
  });
  return fold;
}

// Shows the browser in the rail's place for the selected session's folder, else the configured
// repository's.
export async function showSkills() {
  const view = element("skills");
  try {
    const skills = await invoke<Skill[]>("skills", { project: studio.selection?.session.cwd ?? null });
    const sources = [...new Set(skills.map((skill) => skill.source))];
    view.replaceChildren(
      ...(skills.length ? [] : [text("p", "No skills found.", "empty")]),
      ...sources.flatMap((source) => [
        text("h1", titles[source] ?? source.replace(/^plugin /, "Plugin: ")),
        ...skills.filter((skill) => skill.source === source).map(row),
      ]),
    );
  } catch (e) {
    view.replaceChildren(text("p", `No skills: ${e}`, "empty"));
  }
  document.body.classList.add("skills-open", "rail-open");
}

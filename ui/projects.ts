// The studio's projects (#93): the main-checkout paths in `projects.json`, in order. Read once per
// run, then replaced by `setProjects` after an add, remove or reorder (#98). The sidebar (#97) shows one group per project.

import { invoke } from "./dom.js";

let loaded: Promise<string[]> | null = null;
// The list once read, for the code that draws without waiting.
let known: string[] = [];
// Why the last read failed, null after one succeeds: the read that then succeeds tells the listeners, as an add does.
let failure: string | null = null;

export const projects = (): Promise<string[]> => {
  if (!loaded) {
    const reading: Promise<string[]> = invoke<string[]>("projects").then(
      (paths) => {
        known = paths;
        if (failure !== null) {
          failure = null;
          listeners.forEach((listener) => listener());
        }
        return paths;
      },
      (error) => {
        failure = String(error);
        if (loaded === reading) loaded = null;
        throw error;
      },
    );
    loaded = reading;
  }
  return loaded;
};

const listeners: (() => void)[] = [];
// Runs `listener` each time the projects change.
export const onProjects = (listener: () => void) => void listeners.push(listener);

// The projects after Daniel added, removed or reordered one: what the commands return.
export function setProjects(paths: string[]) {
  known = paths;
  loaded = Promise.resolve(paths);
  listeners.forEach((listener) => listener());
}

// Why the projects could not be read, shown in the sidebar until a read succeeds.
export const projectsError = (): string | null => failure;

// The projects as last read: empty until `projects()` has resolved.
export const knownProjects = (): string[] => known;

// The first project, "" when there is none: where a view with no project of its own (the palette's
// Fetch, a branch switch with no workspace shown) works.
export const mainProject = async (): Promise<string> => (await projects())[0] ?? "";

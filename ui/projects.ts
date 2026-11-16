// The studio's projects (#93): the main-checkout paths in `projects.json`, in order. Read once per
// run, then replaced by `setProjects` after an add, remove or reorder (#98). The sidebar (#97) shows one group per project.

import { invoke } from "./dom.js";

let loaded: Promise<string[]> | null = null;
// The list once read, for the code that draws without waiting.
let known: string[] = [];
// Whether the last read failed: the read that then succeeds tells the listeners, as an add does.
let failed = false;

export const projects = (): Promise<string[]> => {
  if (!loaded) {
    const reading: Promise<string[]> = invoke<string[]>("projects").then(
      (paths) => {
        known = paths;
        if (failed) {
          failed = false;
          listeners.forEach((listener) => listener());
        }
        return paths;
      },
      (error) => {
        failed = true;
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

// The projects as last read: empty until `projects()` has resolved.
export const knownProjects = (): string[] => known;

// The first project, "" when there is none: where a view with no project of its own (the palette's
// Fetch, a branch switch with no workspace shown) works.
export const mainProject = async (): Promise<string> => (await projects())[0] ?? "";

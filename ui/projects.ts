// The studio's projects (#93): the main-checkout paths in `projects.json`, in order. Read once per
// run; the add and remove flows (#98) will update it. The sidebar (#97) shows one group per project.

import { invoke } from "./dom.js";

let loaded: Promise<string[]> | null = null;
// The list once read, for the code that draws without waiting.
let known: string[] = [];

export const projects = (): Promise<string[]> => (loaded ??= invoke<string[]>("projects").then((paths) => (known = paths)));

// The projects as last read: empty until `projects()` has resolved.
export const knownProjects = (): string[] => known;

// The first project, "" when there is none: where a view with no project of its own (the palette's
// Fetch, a branch switch with no workspace shown) works.
export const mainProject = async (): Promise<string> => (await projects())[0] ?? "";

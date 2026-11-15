// The studio's projects (#93): the main-checkout paths in `projects.json`, in order. Read once per
// run; the add and remove flows (#98) will update it. Until the sidebar (#97) shows every project,
// Workspaces, the picker and Delete work on the first.

import { invoke } from "./dom.js";

let loaded: Promise<string[]> | null = null;

export const projects = (): Promise<string[]> => (loaded ??= invoke<string[]>("projects"));

// The project the window's single-project views work on, "" when there is none.
export const mainProject = async (): Promise<string> => (await projects())[0] ?? "";

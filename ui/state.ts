// The session list and the selection, as the shell last sent them.

import type { Selection, Session } from "./types.js";

export const studio = { listed: [] as Session[], selection: null as Selection | null };

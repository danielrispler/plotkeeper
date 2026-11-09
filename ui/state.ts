// The session list and the selection, as the shell last sent them, and the settings files in effect.

import { initial } from "./settings.js";
import type { Selection, Session } from "./types.js";

export const studio = { listed: [] as Session[], selection: null as Selection | null, files: initial };

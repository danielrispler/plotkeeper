// The notice line's error lines (danielrispler/plotkeeper#191). An `error` line of the Studio log shows here, from the app
// through the log plugin's `Webview` target and from the UI's own helper (`logLine` in dom.ts), once per distinct message
// until Daniel dismisses it. Settings errors and the Claude Code version notice share the line and keep their rules.

// The plugin's `LogLevel` serializes to a number; `Error` is 5.
const ERROR = 5;

// The error lines this module added, so a dismiss removes them and nothing else.
let errors: string[] = [];

const line = () => document.getElementById("notice")!;
const linesOf = (text: string | null) => (text ?? "").split("\n").filter(Boolean);

export function showError(message: string) {
  const notice = line();
  const lines = linesOf(notice.textContent);
  if (lines.includes(message)) return;
  errors.push(message);
  notice.textContent = [...lines, message].join("\n");
  notice.hidden = false;
}

// A record of the `Webview` target. The Rust side filters it to `Error`; the page checks the level too.
export function onLogRecord(record: { level: number; message: string }) {
  if (record.level === ERROR) showError(record.message);
}

export function dismissNotice() {
  const notice = line();
  notice.textContent = linesOf(notice.textContent).filter((l) => !errors.includes(l)).join("\n");
  errors = [];
  notice.hidden = true;
}

// `settings.json` and `keymap.json` errors, which replace the earlier ones; the other lines stay beside them.
export function setSettingsErrors(settingsErrors: string[]) {
  const notice = line();
  const others = linesOf(notice.textContent).filter((l) => !/^(settings|keymap)\.json: /.test(l));
  notice.textContent = [...others, ...settingsErrors].join("\n");
  if (settingsErrors.length) notice.hidden = false;
  else if (!others.length) notice.hidden = true;
}

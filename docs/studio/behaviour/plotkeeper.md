# Starting plotkeeper

`plotkeeper` is started by hand in any terminal (danielrispler/plotkeeper#325, spec #319). It replays the event log, writes cards, follows the log as the hook records, and serves the learning layer as a local page, then opens the browser on it. The hook records whether it runs or not; a later start catches up.

## The page and its address

- It listens on 127.0.0.1 only, on port 47800, or the next free port after it.
- Each start has a random token. The browser opens `http://127.0.0.1:<port>/?token=<token>`; the server answers with a cookie (`plotkeeper`, HttpOnly, SameSite=Strict) and sends the browser on to `/`, so the token leaves the address bar. Every other request needs the cookie.
- A request whose Host is not `127.0.0.1:<port>` or `localhost:<port>`, or whose Origin, when it has one, is not one of those, is refused with 403, as is a request with no cookie or a wrong token.
- The page is the built `dist/`, embedded in the program when it is built (a build without `dist/` serves nothing).

## Commands and events

- A command is `POST /api/<command>` with its arguments as JSON, in the camel case the page writes. The reply is the command's JSON reply. A refused command answers 500 with its reason as a JSON string, a missing or mistyped argument 400, a command the page does not have 404. The commands are the learning-layer and session-view ones, answered by the function of the same name in the `server` crate; terminals, projects, branches, skills and usage are not served.
- `GET /events` is one server-sent event stream: `event: <name>` and `data: <json>`, the events the window used to receive (`sessions`, `cards`, `selected`, `notice`, `writer-notice`, `map-landed`, `retests`, and the rest), and a comment every 15 seconds.
- The page tells the server whether its tab is visible and focused (`page_focus`); the read loop treats that as the window's focus. With no tab listening, nothing is focused.
- `POST /api/log` puts the page's log line in plotkeeper's log (stderr).

## One plotkeeper per data folder

The first start holds a lock on `plotkeeper.lock` in the data folder and writes its port and token to `plotkeeper.running.json` there (mode 0600). A second start finds the lock held, opens the browser on the first one's address and exits. Ctrl+C (or SIGTERM) stops the server, removes that file and frees the lock; the stores write by temp file and rename, so a stop never leaves one half written.

At start it applies the card writer `settings.json` names (`summaries.writer`, `claude` by default), so cards are written with it before any page opens. A start writes cards for the turns that ended in the catch-up window (`catchUp.hours`, 2 by default, and `catchUp.max`, 10) and the writer's failures reach the page as `writer-notice`; see [cards](cards.md).

`PLOTKEEPER_NO_BROWSER=1` leaves the browser closed; the address is printed either way (`plotkeeper: <url>`).

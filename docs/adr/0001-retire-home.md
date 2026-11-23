# Home is retired; the Map view is the page's base

plotkeeper used to open on Home, the waiting room in the centre column (#53 for the page Daniel landed on after a delete, #226 for the waiting room), and the Map view was a layer opened over the page by a key or the palette. By 2026-11 Home had stopped earning its place: its Needs you and Unseen parts repeated the session list and the top bar's chips, and its last part was only a list of projects with a fog mark, since plotkeeper starts no sessions (#319). Meanwhile the map, the learning layer's main feature, was one step away and easy never to open. So we retired Home outright, with no setting to bring it back, and made the Map view the page's base: a top bar, the map filling the rest, the session list at the right, and everything else (a session's story, History, a diff, Settings) in one left panel over the map (spec #380, decided in the grilling of 2026-11-23).

## Considered Options

- **Map as start page only.** Open on the map but keep Home and the centre column one click away. Rejected: it keeps two front doors, and every later feature would still have to decide whether it also goes on Home.
- **Map as the page's base** (chosen). The map is always under everything; panels float over it and closing them returns to the bare map. Home's useful parts move to the session list first: a saved project list so a project keeps its header with no sessions (#381), Home's quiet dot on the project header (#386), second lines on needs-you, running and unseen rows and "Clear landed" at the list's end (#382).
- **Map only.** The map with no session list or panels. Rejected: Daniel still needs to see and answer his agents, and a session's story needs a place to open.

## Consequences

- Reversing this means rebuilding a centre column and a view nothing now draws; the list and the panel took over what Home did.
- The design was checked in a throwaway prototype Daniel approved (`local-evidence/2026-11-23-map-first-grilling/prototype/`, on his machine). Its calls stand unless he overturns one: no large project title on the map (the name is in the top bar); a monochrome diff, added lines bright and removed lines faint and struck through, to keep the colour rule; with the panel and the part's sheet both open the view only pans, so one part may be slightly clipped; agent logos small and muted; the legend hidden at narrow widths while the panel is open.
- The Map view's stop rule could no longer count days the map was open, since it is always open: it counts engaged days instead (#384), and Home's mark-seen event went. Old log lines that name it still read and count for nothing.
- `studio::GoHome` merged into `studio::ShowMap`, "back to the map"; a keymap that still names it keeps working (#386).

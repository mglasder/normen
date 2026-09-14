# Missing [core] seeds without writing; TUI never writes conf

Existing conf files have no `[core]` section. A new `normen` process uses the shipped seed until the user edits that section. Overlay add/remove and MENU `d` change only the in-memory CORE for this session. Ctrl-q writes that list into the workspace JSON; Ctrl-c discards it. The next bare `normen` always opens the conf (or seed) default. Writing `[core]` from the TUI would mix a session working set with the user's default list.

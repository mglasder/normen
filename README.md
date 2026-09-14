# normen

Vim-style CLI to search and read German law from
[gesetze-im-internet.de](https://www.gesetze-im-internet.de/).

Currently the shipped CORE default is: **BGB**, **GG**, **VwGO**, **VwVfG**, **BVerfGG**,
**GOBT**, **GOBR**, **PartG**, **VereinsG**, **VersammlG**, **BauGB**,
**BauNVO**, **VwZG**, **VwVG**, **StGB**, **ZPO**, **StPO**, **HGB**,
**EGBGB**. Edit `[core] order` in the conf file to change the default list
for a new session. In the TUI, add or remove laws from the Bundesrecht pane
(`Ctrl-n` then `a`); those changes last only for this session and are kept
when you quit with **Ctrl-q**.

## Features

- Full-text reader for the bundled statutes, always opened in a new tab
- MENU CORE list with `/` filter and `i` shortcut jump
- Bundesrecht pane to search all gesetze-im-internet.de laws and add them to CORE
- Vim motion in a law: line (`j`/`k`), paragraph (`J`/`K`), norm (`h`/`l`)
- Citation jump (PARA) and in-law `/` search with highlighted hits
- tmux-style tabs (`Ctrl-n` then `n`/`p`/`x`/`m`/`0`–`9`)
- Workspaces you save explicitly; never auto-resumed
- Remappable keys and colors in the config file

## Files

| Path | Purpose |
| --- | --- |
| `~/.normen/sessions.json` | Persisted workspaces (tabs, citations, session CORE, active tab, MRU) |
| `~/.normen/*.xml` | Downloaded law cache |
| `~/.normen/bundesrecht.json` | Cached Bundesrecht index (Teilliste A–Z) |
| `~/.config/normen/normen.conf` | Keybindings, `[core] order`, and `[theme]` colors (created on first run) |

A new conf file includes a commented `[theme]` block and a commented `[core]`
example. Missing `[core]` uses the shipped CORE for every new `normen`.
Pane add/remove and MENU `d` do not write the conf: **Ctrl-q** stores the
session CORE in `sessions.json`; **Ctrl-c** discards it. Edit `[core] order`
to change the default. Later upgrades never merge new shipped laws into that
list. Uncomment `[theme]` and set tokens as quoted `#rrggbb`
(`primary = "#9ce5c0"`). Names are case-insensitive. Invalid or omitted tokens
keep the compiled defaults. Restart to apply keymap/theme. Existing conf files
are never patched automatically.

Bare `normen` always starts a **new unsaved** workspace on MENU, with the
conf/seed CORE. Use `normen attach` for the last saved one, or
`normen attach <id>`. `normen list` / `normen rm` manage the JSON file.
**Ctrl-q** then `y` writes a row (MENU-only quit writes nothing unless CORE
changed this session). **Ctrl-c** then `y` quits without saving and deletes
that session id if any. **Esc** cancels confirm overlays. `--refresh`
re-downloads cached laws (print and TUI paths).

## Usage

`normen --help` is the discovery text for the print CLI (JSON shapes, `/` search, exit codes).

### Print CLI (JSON on stdout)

Law arguments print **pretty JSON** to stdout and never start the TUI.
There is no `--json` flag and no `Lade …` progress line on this path;
errors go to stderr with exit code 1 (load/parse) or 2 (unknown law or
citation).

```bash
normen laws                 # CORE: { "query", "laws": [{ shortcut, slug, title }] }
normen laws bürger          # filter by shortcut, slug, or title substring
normen laws --sort alpha    # shortcut A–Z
normen laws --sort rev      # shortcut Z–A
normen laws --sort priority # conf / shipped order (default)

normen BGB                  # outline: { "law", "norms": [{ citation, title }] }
normen BGB 433              # get one norm: { "law", "citation", "title", "text" }
normen BGB /kauf            # in-law search: { "law", "query", "limit", "total", "hits" }
normen --limit 20 BGB /kauf # cap hits (default 10; ignored on outline/get)
normen --all BGB /kauf      # all hits; "limit" is null
normen --refresh BGB        # re-download before outline/get/search
```

The second argument is either a citation (`433`, `31a`) or a `/` search
needle. A bare word like `Kaufvertrag` is rejected as “not a citation”.

### TUI

```bash
normen                 # new unsaved workspace (MENU)
normen attach          # last saved workspace (error if none)
normen attach 3        # workspace id 3
```

### Workspace management

```bash
normen list            # id and open tabs
normen rm 3
normen rm --all
```

### Development

```bash
cargo run              # TUI (same as bare normen)
cargo run -- BGB       # print outline JSON
cargo test
```

Opening a law shows the full text and **always creates a new tab**. The same
law can be open twice; each tab has its own position. The command line sits
above the text; a tmux-style tab bar at the top always starts with
`0:MENU`, then open laws (`0:MENU  1:BGB*  2:GG-`: `*` current, `-` last).
The status bar at the bottom shows the mode (`NORMAL`, `SEARCH`, `PARA`) on
the left and the current norm over the last numbered norm on the right
(`35a:2385`). `Ctrl-n` then `m` or `0` returns to MENU; `Ctrl-n` then
`x` closes the current law tab (MENU cannot be closed). Closing the last
law tab returns to MENU. Opening a law again starts at the beginning.

### Normal mode

| Key | Action |
| --- | --- |
| `j` `k` | Scroll down / up (line) |
| `J` `K` | Next / previous paragraph |
| `h` `l` | Previous / next norm |
| `g` `G` | Top / bottom |
| `0`–`9` | Para mode (type a number) |
| `/` | Search mode |
| `?` | Key bindings |

### Tabs

Press `Ctrl-n`, then the command key. The status bar shows `PREFIX` until
you press the suffix. `0`–`9` still enter PARA mode.

| Key | Action |
| --- | --- |
| `Ctrl-n` `n` | Next tab (wraps, includes MENU) |
| `Ctrl-n` `p` | Previous tab |
| `Ctrl-n` `0` / `m` | Jump to MENU |
| `Ctrl-n` `1`–`9` | Jump to that law tab |
| `Ctrl-n` `a` | Bundesrecht pane (add/remove CORE) |
| `Ctrl-n` `x` | Close current law tab (no confirm) |
| `Ctrl-q` | Save workspace and quit (`y` confirm, `Esc` cancel) |
| `Ctrl-c` | Kill session and quit (`y` confirm, `Esc` cancel; deletes the saved row if any) |

### Search mode (`/`)

On MENU, `/` filters CORE by shortcut, slug, or title (`gg`,
`bürger`, `grund`). `j` and `k` are part of the query until `Enter`; then
they move the filtered list. `Enter` again opens the highlighted law.
`/` continues editing. `Esc` restores the full CORE list. `d` removes the
highlighted law from CORE (`y` confirm; open tabs stay). That change stays
in this session until **Ctrl-q**; it does not edit the conf.

`Ctrl-n` then `a` opens the Bundesrecht pane on the entire catalog. Full
titles wrap onto following rows. `/` searches by shortcut, slug, or title;
the query stays in the pane. `hjkl` move. Space marks a law to add (or to
remove if it is already on CORE). Enter applies every mark and closes; Esc
discards marks. The pane does not open a law — add it, then open from MENU.

In a law tab, `/` filters paragraphs (fuzzy list with preview) the same
way: type, `Enter` to move with `j` `k`, `Enter` again to jump. `Esc`
returns to normal mode.

### Para mode (type a number)

Type a citation (`433`, `31a`) and press `Enter` to jump. `Esc` returns to
normal mode.

On the law picker, `i` still focuses the shortcut field (`BGB`, `gg`, …)
for an exact jump within CORE. `/` searches CORE.

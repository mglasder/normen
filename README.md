# normen

Vim-style CLI to search and read German law from
[gesetze-im-internet.de](https://www.gesetze-im-internet.de/).

Currently included: **BGB**, **GG**, **VwGO**, **VwVfG**, **BVerfGG**,
**GOBT**, **GOBR**, **PartG**, **VereinsG**, **VersammlG**, **BauGB**,
**BauNVO**, **VwZG**, **VwVG**, **StGB**, **ZPO**, **StPO**, **HGB**,
**EGBGB**.

## Features

- Full-text reader for the bundled statutes, always opened in a new tab
- MENU catalog with `/` filter and `i` shortcut jump
- Vim motion in a law: line (`j`/`k`), paragraph (`J`/`K`), norm (`h`/`l`)
- Citation jump (PARA) and in-law `/` search with highlighted hits
- tmux-style tabs (`Ctrl-n` then `n`/`p`/`x`/`m`/`0`–`9`)
- Workspaces you save explicitly; never auto-resumed
- Remappable keys and colors in the config file

## Files

| Path | Purpose |
| --- | --- |
| `~/.normen/sessions.json` | Persisted workspaces (tabs, citations, active tab, MRU) |
| `~/.normen/*.xml` | Downloaded law cache |
| `~/.config/normen/normen.conf` | Keybindings and `[theme]` colors (created on first run) |

A new conf file includes a commented `[theme]` block with the default hex values.
Uncomment and set tokens as quoted `#rrggbb` (`primary = "#9ce5c0"`). Names are
case-insensitive. Invalid or omitted tokens keep the compiled defaults. Restart
to apply. Existing conf files are never patched.

Bare `normen` always starts a **new unsaved** workspace on MENU. Use
`normen attach` for the last saved one, or `normen attach <id>`.
`normen list` / `normen rm` manage the JSON file. **Ctrl-q** then `y`
writes a row (MENU-only quit writes nothing). **Ctrl-c** then `y` quits
without saving and deletes that session id if any. **Esc** cancels either
overlay. `--refresh` re-downloads cached laws (print and TUI paths).

## Usage

`normen --help` is the discovery text for the print CLI (JSON shapes, `/` search, exit codes).

### Print CLI (JSON on stdout)

Law arguments print **pretty JSON** to stdout and never start the TUI.
There is no `--json` flag and no `Lade …` progress line on this path;
errors go to stderr with exit code 1 (load/parse) or 2 (unknown law or
citation).

```bash
normen laws                 # catalog: { "query", "laws": [{ shortcut, slug, title }] }
normen laws bürger          # filter by shortcut, slug, or title substring

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
| `Ctrl-n` `x` | Close current law tab (no confirm) |
| `Ctrl-q` | Save workspace and quit (`y` confirm, `Esc` cancel) |
| `Ctrl-c` | Kill session and quit (`y` confirm, `Esc` cancel; deletes the saved row if any) |

### Search mode (`/`)

On MENU, `/` filters the law list by shortcut, slug, or title (`gg`,
`bürger`, `grund`). `j` and `k` are part of the query until `Enter`; then
they move the filtered list. `Enter` again opens the highlighted law.
`/` continues editing. `Esc` restores the full list.

In a law tab, `/` filters paragraphs (fuzzy list with preview) the same
way: type, `Enter` to move with `j` `k`, `Enter` again to jump. `Esc`
returns to normal mode.

### Para mode (type a number)

Type a citation (`433`, `31a`) and press `Enter` to jump. `Esc` returns to
normal mode.

On the law picker, `i` still focuses the shortcut field (`BGB`, `gg`, …)
for an exact jump. `/` searches the list.

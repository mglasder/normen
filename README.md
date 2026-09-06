# normen

Vim-style CLI to search and read German law from
[gesetze-im-internet.de](https://www.gesetze-im-internet.de/).

Currently included: **BGB**, **GG**, **VwGO**, **VwVfG**.

## Usage

```bash
uv sync
uv run normen
```

Or jump straight in: `uv run normen bgb`, `uv run normen BGB 433`, `uv run normen gg /würde`.

Opening a law shows the full text and **always creates a new tab**. The same
law can be open twice; each tab has its own position. The command line sits
above the text; a tmux-style tab bar at the top always starts with
`0:MENU`, then open laws (`0:MENU  1:BGB*  2:GG-`: `*` current, `-` last).
The status bar at the bottom shows the mode (`NORMAL`, `SEARCH`, `PARA`) on
the left and the current norm over the last numbered norm on the right
(`35a:2385`). `Ctrl-n` then `m` or `0` returns to MENU; `Ctrl-n` then
`x` closes the current law tab (MENU cannot be closed). Closing the last
law tab returns to MENU. Opening a law again starts at the beginning
(CLI `norm` still applies only to that new tab).

### Normal mode

| Key | Action |
| --- | --- |
| `j` `k` | Scroll down / up (line) |
| `J` `K` | Next / previous paragraph |
| `h` `l` | Previous / next norm |
| `g` `G` | Top / bottom |
| `n` `N` | Next / previous search hit |
| `0`–`9` | Para mode (type a number) |
| `/` | Search mode |

### Tabs

Press `Ctrl-n`, then the command key. The status bar shows `PREFIX` until
you press the suffix. Plain `n` still jumps search hits and `0`–`9` still
enter PARA mode.

| Key | Action |
| --- | --- |
| `Ctrl-n` `n` | Next tab (wraps, includes MENU) |
| `Ctrl-n` `p` | Previous tab |
| `Ctrl-n` `0` / `m` | Jump to MENU |
| `Ctrl-n` `1`–`9` | Jump to that law tab |
| `Ctrl-n` `x` | Close current law tab (no confirm) |
| `Ctrl-q` | Quit (confirm with `y`) |

### Search mode (`/`)

Type to filter paragraphs (fuzzy list with preview); `j` and `k` are part of
the query. `Enter` finishes the query so you can move with `j` `k`. `Enter`
again jumps to the highlighted row. `/` continues editing the query. `Esc`
returns to normal mode.

### Para mode (type a number)

Type a citation (`433`, `31a`) and press `Enter` to jump. `Esc` returns to
normal mode.

On the law picker, `i` still focuses the shortcut field (`BGB`, `gg`, …).

Keybindings and other settings live in `~/.config/normen/normen.conf`
(created on first run). Law texts are cached in `~/.normen/`. Use
`--refresh` to download the laws again.

```bash
uv run pytest
```

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

Opening a law shows the full text. The command line sits above it; the status
bar at the bottom shows the mode (`NORMAL`, `SEARCH`, `PARA`) on the left and
the current norm over the last numbered norm on the right (`35a:2385`). The header shows the
abbreviation and the law name. Within one running instance, reopening a law
returns to the last viewed paragraph.

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
| `q` | Back / quit |

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
(created on first run). Law texts are cached in `~/.normen/`. Position in a
law is remembered only while that instance is running, so several copies can
be open at once.
Use `--refresh` to download the laws again.

```bash
uv run pytest
```

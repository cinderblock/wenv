# wenv

A small terminal UI for viewing and setting `.env` secrets **without revealing
their values**. Run it in a project directory and it scans for `.env`,
`.env.local`, and `.env.example` and shows a table of every variable with a
column per file, so you can see at a glance which are set / empty / unset in each
and write new values with masked input.

It renders in an inline region (not a fullscreen takeover), so your scrollback is
preserved, and on quit it prints a compact colored summary of the current state
and anything you changed.

## Features

- Scans the current directory for `.env`, `.env.local`, and `.env.example`.
- Table view: one row per variable, one column per file (`.env`, `.env.local`),
  so you see each variable's status in each file side by side.
- A set value shows a **masked fingerprint** (e.g. `sk-…0a`) — a few leading and
  trailing characters so you can recognize a secret without revealing it. Short
  values show only length dots. **Full values are never displayed.**
- Edit a cell **inline** with masked input; toggle reveal with `Ctrl+R`.
- `Tab` (or `←`/`→`) chooses which file column you're editing.
- Writes preserve existing comments, ordering, and blank lines, and quote values
  only when needed.
- On quit, clears the UI and leaves a plain summary in your scrollback.

## Install

### Download a binary

Grab the latest binary for your platform from the
[Releases](https://github.com/cinderblock/wenv/releases) page, then place it on
your `PATH`.

### Build from source

Requires a recent Rust toolchain.

```sh
cargo install --path .
# or
cargo build --release   # binary at target/release/wenv
```

#### Size-optimized build (optional)

A much smaller binary can be built on the nightly toolchain by rebuilding the
standard library with the `immediate-abort` panic strategy (this strips
unwinding and panic-formatting machinery). One-time setup, then build:

```sh
rustup toolchain install nightly --component rust-src
./scripts/build-min.sh           # or: ./scripts/build-min.ps1 on Windows
```

The binary lands at `target/<host-triple>/release/wenv`. This shrinks the
release binary by roughly half versus a plain `cargo build --release`. It is not
the default because it requires nightly; the stock `cargo build --release` stays
on stable.

## Usage

```sh
cd your-project
wenv
```

You can also pass a directory to operate on instead of `cd`-ing into it:

```sh
wenv path/to/your-project
```

### Keys

| Key             | Action                                |
| --------------- | ------------------------------------- |
| `↑` / `↓` (`k`/`j`) | Move between variables (rows)     |
| `←` / `→` (`h`/`l`) / `Tab` | Switch file column (`.env` / `.env.local`) |
| `Enter`         | Edit the selected cell inline         |
| `s`             | Rescan files from disk                |
| `q` / `Esc`     | Quit (prints a summary)               |
| `Ctrl+R`        | (while editing) reveal/mask the value |

Color output is suppressed automatically when stdout is not a terminal or when
`NO_COLOR` is set.

## License

MIT — see [LICENSE](LICENSE).

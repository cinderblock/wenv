# wenv

A small terminal UI for viewing and setting `.env` secrets **without revealing
their values**. Run it in a project directory and it scans for `.env`,
`.env.local`, and `.env.example`, shows which variables are set / empty / unset,
and lets you write new values with masked input.

It renders in an inline region (not a fullscreen takeover), so your scrollback is
preserved, and on quit it prints a compact colored summary of the current state
and anything you changed.

## Features

- Scans the current directory for `.env`, `.env.local`, and `.env.example`.
- Lists every variable with a `set` / `empty` / `unset` status and the file it
  comes from — **values are never displayed**.
- Set a value with masked input; toggle reveal with `Ctrl+R`.
- Choose whether writes go to `.env` or `.env.local` (`Tab` to switch).
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
| `↑` / `↓` (`k`/`j`) | Move selection                    |
| `Enter`         | Edit the selected variable            |
| `Tab`           | Toggle write target (`.env` / `.env.local`) |
| `s`             | Rescan files from disk                |
| `q` / `Esc`     | Quit (prints a summary)               |
| `Ctrl+R`        | (while editing) reveal/mask the value |

Color output is suppressed automatically when stdout is not a terminal or when
`NO_COLOR` is set.

## License

MIT — see [LICENSE](LICENSE).

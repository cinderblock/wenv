# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] - 2026-06-18

### Added

- Automatic backups: before changing an existing `.env`/`.env.local` file, wenv
  copies the previous on-disk version to a dated `.bak` beside it
  (`<file>.YYYY-MM-DD.bak`, then `-1`, `-2`, … for further changes the same day).
  Creating a new file or re-saving identical content makes no backup, and a save
  is refused if its backup cannot be written.

## [0.4.0] - 2026-06-17

### Changed

- Rewrote wenv as a `#![no_std]` program that talks to the OS directly, dropping
  `std`, `crossterm`, and (on Windows) the C runtime entirely. The Windows binary
  now depends only on `kernel32`/`shell32` (which ship with the OS) and is ~36 KB
  — down from ~200 KB — with nothing to install. Built on plain stable Rust.
- Unix builds use the system `libc` (always present); no runtime install needed.

### Removed

- The optional nightly `build-std` size build and its `scripts/build-min.*` /
  `github-build-setup` CI hook are gone — the default stable build is now the
  small one, so the nightly machinery is no longer needed.

### Notes

- Behavior and keybindings are unchanged from 0.3.0. When stdout/stdin is not a
  terminal, wenv prints the plain summary instead of the interactive UI.

## [0.3.0] - 2026-06-17

### Changed

- Redesigned the UI as a table with a column per file (`.env`, `.env.local`), so
  each variable's status in each file is visible side by side. Values are now
  edited **inline** in the table instead of in a centered popup. `Tab` (or
  `←`/`→`) moves the active file column.
- A set value now shows a masked **fingerprint** (e.g. `sk-…0a`) — a few leading
  and trailing characters to identify a secret without revealing it; short values
  show only length dots. Full values are still never displayed.

### Removed

- Dropped the `ratatui` dependency in favor of a small built-in `crossterm`
  renderer. This trims the default release binary by ~28% with no loss of
  functionality.

### Added

- `scripts/build-min.sh` / `scripts/build-min.ps1`: an optional nightly build
  using `build-std` + the `immediate-abort` panic strategy that produces a
  self-contained binary ~56% smaller than the original. Release builds now use
  this path in CI.

## [0.2.0] - 2026-06-17

### Added

- Optional positional directory argument: `wenv path/to/project` operates on the
  given directory instead of requiring you to `cd` into it. With no argument,
  the current working directory is used as before.

## [0.1.0]

### Added

- Initial release: an inline terminal UI for viewing and setting `.env` secrets
  without revealing their values.
- Scans `.env`, `.env.local`, and `.env.example`; lists every variable with a
  `set` / `empty` / `unset` status and its source file.
- Masked value input with `Ctrl+R` to reveal, and a `Tab`-selectable write
  target (`.env` or `.env.local`).
- Writes preserve existing comments, ordering, and blank lines, quoting values
  only when needed.
- Prints a compact colored summary on quit; color is suppressed when stdout is
  not a terminal or `NO_COLOR` is set.

[0.5.0]: https://github.com/cinderblock/wenv/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/cinderblock/wenv/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/cinderblock/wenv/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/cinderblock/wenv/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/cinderblock/wenv/releases/tag/v0.1.0

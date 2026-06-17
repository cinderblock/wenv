# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.2.0]: https://github.com/cinderblock/wenv/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/cinderblock/wenv/releases/tag/v0.1.0

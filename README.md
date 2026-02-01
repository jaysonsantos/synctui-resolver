# synctui-resolver

A small Rust TUI (Ratatui) for resolving Syncthing `*.sync-conflict-*` files.

It scans a directory tree, groups conflicts by their original/base filename, lets you pick which version to keep (original/newest/specific conflict), and then archives the non-chosen versions.

## Install / Build

```bash
cargo build
```

## Usage

Dry-run (no filesystem changes):

```bash
cargo run -- .
```

Apply changes:

```bash
cargo run -- --apply .
```

Include hidden files/directories:

```bash
cargo run -- --apply --include-hidden .
```

## TUI Controls

- List view: Up/Down, `Enter` pick versions, `Space` select multiple, `a` confirm/apply current, `A` confirm/apply selected, `q` quit
- Pick view: Up/Down, `Enter` choose highlighted, `o` choose original, `n` choose newest, `Esc` back
- Confirm view: `y` run, `n` cancel, `Esc` back

## What “apply” does

- Creates `.stconflict-archive` next to the base/original file
- Moves all non-chosen versions into the archive (unique names)
- If you choose a conflict file, it gets moved into the base/original filename

## Development

```bash
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
```

See `AGENTS.md` for deeper contributor/agent guidance.

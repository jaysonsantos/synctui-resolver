# Agent Guide (synctui-resolver)

This repository is a small Rust CLI/TUI tool for resolving Syncthing `*.sync-conflict-*` files.

## Quick Commands

### Build

- Build (debug): `cargo build`
- Build (release): `cargo build --release`
- Run (dry-run): `cargo run -- .`
- Run (apply changes): `cargo run -- --apply .`
- Run (include hidden): `cargo run -- --apply --include-hidden .`

### Format / Lint

- Format: `cargo fmt`
- Check formatting in CI style: `cargo fmt -- --check`
- Lint (all targets): `cargo clippy --all-targets`
- Lint (treat warnings as errors): `cargo clippy --all-targets -- -D warnings`

### Tests

- Run all tests: `cargo test`
- Run tests with output: `cargo test -- --nocapture`
- Run tests for a single module/file (pattern):
  - `cargo test scan::tests::`
  - `cargo test ops::tests::`
- Run one specific test by name:
  - `cargo test scan::tests::scan_finds_groups_and_original_candidate`
- Run a single test and show stdout/stderr:
  - `cargo test scan::tests::scan_finds_groups_and_original_candidate -- --nocapture`

Notes:
- This crate is a binary-only project (no `lib.rs`), so unit tests live inside modules via `#[cfg(test)]`.

## Repo Layout

- `src/main.rs`: entry point, parses CLI args, runs TUI.
- `src/tui.rs`: Ratatui UI + event loop and “apply” workflow.
- `src/scan.rs`: filesystem scanning/grouping for Syncthing conflict files.
- `src/ops.rs`: filesystem operations (mkdir, move/rename/copy fallback, archive path, unique names).
- `src/model.rs`: data model (`ConflictGroup`, `Candidate`).

## Cursor / Copilot Rules

- No `.cursor/rules/`, `.cursorrules`, or `.github/copilot-instructions.md` were found in this repo.
- If any are added later, keep this file in sync and follow them.

## Code Style Guidelines

### Formatting

- Use standard Rust formatting (rustfmt). Do not hand-format around it.
- Prefer explicit line breaks for long widget layout chains (common in `ratatui`).

### Imports

- Group imports in this order:
  1. `crate::...`
  2. external crates (`anyhow`, `clap`, `crossterm`, `ratatui`, `walkdir`, etc.)
  3. `std::...`
- Avoid unused imports; keep `cargo clippy -- -D warnings` clean.
- Prefer importing structs/enums directly when used heavily (e.g. `EnableMouseCapture`).

### Naming

- Modules/files: `snake_case`.
- Types/structs/enums: `PascalCase`.
- Functions/variables: `snake_case`.
- Keep keybindings as single-char literals in `handle_key` and document them in the header line.

### Types and Data Modeling

- Keep model types dumb and serializable-friendly:
  - `Candidate` holds path + metadata used for display.
  - `ConflictGroup` groups candidates and stores the chosen index.
- Use `Option<T>` for metadata that can be absent (`modified`, `size`) and handle it at render time.
- Prefer `PathBuf` in owned structs and `&Path` in function inputs.

### Error Handling

- Use `anyhow::Result` for fallible operations.
- Add context at boundaries that touch the OS:
  - `with_context(|| format!("..."))` for filesystem ops and path resolution.
- Avoid panics in production code paths. Panics are acceptable in tests.

### Filesystem Safety

- Preserve the “dry-run by default” behavior.
- Any change that writes/moves/deletes files must be gated behind `--apply`.
- Archive policy:
  - Non-chosen files are moved into `.stconflict-archive` next to the base file.
  - Use `unique_name()` to avoid collisions.
- Prefer routing filesystem mutations through `src/ops.rs` so they stay testable.

### Scanning Rules

- Treat any filename containing `.sync-conflict-` as a conflict file.
- Group conflicts by base filename prefix before `.sync-conflict-`.
- Keep group ordering deterministic (BTreeMap + sorted candidate paths).
- Hidden content:
  - default: ignore dotfiles and dot-directories
  - `--include-hidden`: include them

### TUI Guidelines

- Keep UI rendering pure (no filesystem mutations inside `ui()`/draw functions).
- Mutations belong in explicit actions (`apply_group`, `move_file`, etc.).
- Avoid introducing cross-platform terminal assumptions beyond `crossterm` APIs.
- Don’t add interactive prompts that block non-interactive test runs.

## Testing Guidelines

- Prefer unit tests for:
  - scan/grouping logic (`src/scan.rs`)
  - filesystem operations (`src/ops.rs`) using `tempfile`
  - small deterministic helpers (navigation bounds, etc.)
- Avoid testing the interactive event loop; instead test the pure functions it relies on.
- Use `tempfile::tempdir()` for isolated filesystem fixtures.
- Tests should be deterministic and not depend on wall-clock ordering.

## Acceptance Checklist (before you finish a change)

- `cargo fmt`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`

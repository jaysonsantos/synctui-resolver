#!/usr/bin/env bash
set -euo pipefail

# Demo script for synctui-resolver.
# Creates a temporary directory with fake Syncthing conflict files and launches the TUI.

ROOT="$(mktemp -d -t synctui-resolver-demo.XXXXXX)"
export ROOT

cleanup() {
  if [[ "${KEEP_DEMO_DIR:-}" == "1" ]]; then
    printf "\nKeeping demo directory: %s\n" "$ROOT"
  else
    rm -rf "$ROOT"
  fi
}
trap cleanup EXIT

mkdir -p "$ROOT/docs" "$ROOT/photos" "$ROOT/.hidden"

cat >"$ROOT/docs/notes.txt" <<'EOF'
original notes
EOF

cat >"$ROOT/docs/notes.txt.sync-conflict-20260101-101010-DEVICEA" <<'EOF'
conflict A
EOF

cat >"$ROOT/docs/notes.txt.sync-conflict-20260102-121212-DEVICEB" <<'EOF'
conflict B (newer)
EOF

# A group where the "original" base file is missing (only conflicts exist).
cat >"$ROOT/photos/vacation.jpg.sync-conflict-20260103-090909-PHONE" <<'EOF'
fake-jpeg-bytes-1
EOF

cat >"$ROOT/photos/vacation.jpg.sync-conflict-20260104-111111-LAPTOP" <<'EOF'
fake-jpeg-bytes-2
EOF

# Hidden conflicts (requires --include-hidden)
cat >"$ROOT/.hidden/secret.txt" <<'EOF'
hidden original
EOF

cat >"$ROOT/.hidden/secret.txt.sync-conflict-20260105-050505-DEVICE" <<'EOF'
hidden conflict
EOF

printf "\nDemo directory created at:\n  %s\n\n" "$ROOT"
printf "Tips inside the TUI:\n"
printf "  - From the main list: c=current, n=newest, p=oldest (uppercase applies to selected)\n"
printf "  - Space to select multiple, a/A to confirm, y to run\n"
printf "  - Press t to toggle DRY-RUN/APPLY\n"
printf "\nLaunching (debug build) ...\n\n"

cargo build

if [[ ! -t 0 || ! -t 1 ]]; then
  printf "\nNo interactive TTY detected; skipping TUI launch.\n"
  printf "Run this locally instead:\n\n"
  printf "  cargo run -- %q\n" "$ROOT"
  printf "  # or: cargo run -- --include-hidden %q\n\n" "$ROOT"
  exit 0
fi

# Default: don't include dotdirs. Re-run with --include-hidden if you want that group too.
if [[ "${INCLUDE_HIDDEN:-}" == "1" ]]; then
  cargo run -- --include-hidden "$ROOT"
else
  cargo run -- "$ROOT"
fi

printf "\nAfter exiting the TUI, the demo directory is still available while this script runs.\n"
printf "Set KEEP_DEMO_DIR=1 to keep it after the script exits.\n"

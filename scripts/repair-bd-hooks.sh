#!/usr/bin/env sh
set -eu

HOOK_DIR=".git/hooks"

if [ ! -d "$HOOK_DIR" ]; then
  echo "No .git/hooks directory found. Run from repository root." >&2
  exit 1
fi

write_hook() {
  hook_name="$1"
  quiet="${2:-0}"
  hook_path="$HOOK_DIR/$hook_name"

  cat >"$hook_path" <<EOF
#!/usr/bin/env sh
# Compatibility shim for bd hook API differences.

if ! command -v bd >/dev/null 2>&1; then
  exit 0
fi

if bd hook --help >/dev/null 2>&1; then
  exec bd hook $hook_name "\$@"
fi

if bd hooks run --help >/dev/null 2>&1; then
  exec bd hooks run $hook_name "\$@"
fi

if [ "$quiet" != "1" ]; then
  echo "Warning: bd does not provide hook commands; skipping $hook_name hook." >&2
fi
exit 0
EOF

  chmod +x "$hook_path"
}

write_hook "pre-commit"
write_hook "pre-push"
write_hook "prepare-commit-msg"
write_hook "post-checkout" 1
write_hook "post-merge"

echo "Updated bd git hook shims in $HOOK_DIR"

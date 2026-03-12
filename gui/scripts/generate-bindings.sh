#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
GUI_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
REPO_ROOT=$(CDPATH= cd -- "$GUI_DIR/.." && pwd)

if [ -n "${SPACETIMEDB_CLI:-}" ]; then
  CLI="$SPACETIMEDB_CLI"
elif command -v spacetimedb-cli >/dev/null 2>&1; then
  CLI=$(command -v spacetimedb-cli)
elif command -v spacetime >/dev/null 2>&1; then
  CLI=$(command -v spacetime)
elif [ -x "$HOME/.local/share/spacetime/bin/current/spacetimedb-cli" ]; then
  CLI="$HOME/.local/share/spacetime/bin/current/spacetimedb-cli"
else
  echo "Unable to find a SpacetimeDB CLI. Set SPACETIMEDB_CLI or add it to PATH." >&2
  exit 1
fi

mkdir -p "$GUI_DIR/src/generated"
"$CLI" generate --lang typescript --module-path "$REPO_ROOT/spacetimedb" --out-dir "$GUI_DIR/src/generated" --yes

find "$GUI_DIR/src/generated" -name '*.ts' -type f -print | while IFS= read -r file; do
  if ! grep -q '@ts-nocheck' "$file"; then
    tmp_file="${file}.tmp"
    {
      printf '// @ts-nocheck\n'
      cat "$file"
    } >"$tmp_file"
    mv "$tmp_file" "$file"
  fi
done

INDEX_FILE="$GUI_DIR/src/generated/index.ts"
if [ -f "$INDEX_FILE" ]; then
  perl -0pi -e "s/\\{ name: '([^']+)', algorithm: /{ accessor: '\$1', name: '\$1', algorithm: /g" "$INDEX_FILE"
fi

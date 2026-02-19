#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

check_generated_file() {
  local generator_script="$1"
  local target_file="$2"
  local output_file="$TMP_DIR/$(basename "$target_file")"
  local generator_name
  generator_name="$(basename "$generator_script")"

  echo "[CONTRACT_CI] Regenerating ${target_file}..."
  python3 "${REPO_ROOT}/${generator_script}" --repo-root "${REPO_ROOT}" --out "${output_file}"

  echo "[CONTRACT_CI] Comparing with committed version..."
  if ! diff -u "${REPO_ROOT}/${target_file}" "${output_file}"; then
    echo "[CONTRACT_CI] ✗ ${target_file} has manual edits — regenerate with ${generator_name}"
    return 1
  fi

  echo "[CONTRACT_CI] ✓ ${target_file} is up to date"
}

check_generated_file "scripts/gen_contracts_ts.py" "src/types.contracts.ts"
check_generated_file "scripts/gen_contracts_rs.py" "src-tauri/src/contracts.rs"


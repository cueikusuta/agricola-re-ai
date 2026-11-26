#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 || -z "$1" ]]; then
  echo "Usage: $0 <test_target_or_card_id>" >&2
  exit 2
fi

TARGET="$1"

# Resolve Python 3 interpreter (prefer python3, fall back to python if needed).
if command -v python3 >/dev/null 2>&1 && python3 -c "import sys; raise SystemExit(0 if sys.version_info >= (3, 10) else 1)" >/dev/null 2>&1; then
  PYTHON="python3"
elif command -v python >/dev/null 2>&1 && python -c "import sys; raise SystemExit(0 if sys.version_info >= (3, 10) else 1)" >/dev/null 2>&1; then
  PYTHON="python"
else
  echo "Error: Python 3.10+ interpreter (python3) not found in PATH." >&2
  exit 1
fi

echo "[1/6] cargo fmt --all -- --check"
cargo fmt --all -- --check

echo "[2/6] cargo clippy --locked --all-targets --all-features -- -D warnings"
cargo clippy --locked --all-targets --all-features -- -D warnings

if [[ -f "tests/${TARGET}.rs" ]]; then
  echo "[3/6] cargo test --locked --test ${TARGET}"
  cargo test --locked --test "${TARGET}"
else
  echo "[3/6] cargo test --locked --lib ${TARGET}"
  cargo test --locked --lib "${TARGET}"
fi

echo "[4/6] ${PYTHON} scripts/validate_bga_card_specs.py"
"${PYTHON}" scripts/validate_bga_card_specs.py

echo "[5/6] ${PYTHON} scripts/card_verification_blockers.py"
"${PYTHON}" scripts/card_verification_blockers.py

echo "[6/6] ${PYTHON} scripts/validate_card_catalog.py"
"${PYTHON}" scripts/validate_card_catalog.py

echo "All pre-PR local gates passed for '${TARGET}'."

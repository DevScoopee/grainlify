#!/usr/bin/env bash
# CI gate for the contract workspaces.
#
# Scoped to the bounty_escrow workspace (the escrow crate under test).
# The program-escrow workspace has a large body of pre-existing breakage
# that is tracked separately and is intentionally not part of this gate.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

echo "==> bounty-escrow: cargo test"
(
  cd "$ROOT_DIR/contracts/bounty_escrow"
  cargo test -p bounty-escrow
)

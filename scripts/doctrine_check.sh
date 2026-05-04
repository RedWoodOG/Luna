#!/usr/bin/env bash
# Doctrine compliance check.
#
# Run by CI on every push/PR. Can be run locally before commit:
#   bash scripts/doctrine_check.sh
#
# Exit non-zero on any violation. Slice 1 covers the cheapest, highest-signal
# checks. More gates land in later slices as luna-core types stabilize.
#
# Slice 1 checks:
#   1. No hardcoded scenario-entity name comparisons in crate source.
#      ("if name == \"Chris\""-style code is the Aura failure mode.)
#   2. scenarios/runtime/ must be non-empty (don't quietly delete the suite).
#
# Adding a check: append a block, increment VIOLATIONS via fail(), keep output
# greppable (file:line: detail).

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

VIOLATIONS=0

fail() {
  echo "DOCTRINE VIOLATION: $1" >&2
  VIOLATIONS=$((VIOLATIONS + 1))
}

# ---------------------------------------------------------------------------
# Check 1: No hardcoded scenario-entity name comparisons in crate source.
#
# These names appear in scenarios/ and benchmarks/ as test data. They must
# never appear as string-equality branches inside crates/, because that
# means the system is being scripted to a specific entity instead of
# generalized through the entity-graph mechanism.
# ---------------------------------------------------------------------------

SCENARIO_NAMES=("Chris" "Joe" "Francois")

# Match production control flow that branches on a hardcoded scenario name:
#   - `if ... == "Name"`             (conditional branch on identity)
#   - `"Name" => ...`                (match arm dispatching on identity)
#
# Matching positively for the prohibition (rather than filtering out tests
# negatively) avoids the false-positive where a multi-line `assert!` reads
# `claim.value == "Joe"` on a continuation line. Test assertions observe;
# control flow dispatches. Only dispatch is forbidden.
#
# Comment lines are stripped. Doc comments containing examples are intentionally
# allowed -- writing the bad pattern in a docstring as illustration is fine.
for name in "${SCENARIO_NAMES[@]}"; do
  hits=$(grep -rn --include="*.rs" --exclude-dir=tests -E \
    "\bif\b.*==\s*[\"']${name}[\"']|^\s*[\"']${name}[\"']\s*=>" \
    crates/ 2>/dev/null \
    | grep -vE '^[^:]+:[0-9]+:\s*//' \
    || true)
  if [ -n "$hits" ]; then
    fail "Hardcoded entity-name dispatch for '$name' in crate source."
    echo "$hits" >&2
    echo "  Fix: replace the literal with a generic entity-graph lookup." >&2
  fi
done

# ---------------------------------------------------------------------------
# Check 2: scenarios/runtime/ must be non-empty.
#
# The scenario suite is the executable form of the doctrine. Letting it
# silently empty out is the easiest way for the project to drift.
# ---------------------------------------------------------------------------

scenario_count=$(find scenarios/runtime -maxdepth 1 -name "*.json" 2>/dev/null | wc -l | tr -d ' ')
if [ "${scenario_count:-0}" -lt 1 ]; then
  fail "scenarios/runtime/ is empty. Doctrine requires at least one scenario gate."
fi

# ---------------------------------------------------------------------------
# Result
# ---------------------------------------------------------------------------

if [ "$VIOLATIONS" -gt 0 ]; then
  echo "" >&2
  echo "$VIOLATIONS doctrine violation(s) found." >&2
  exit 1
fi

echo "Doctrine check: OK"

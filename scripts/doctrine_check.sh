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
#      ("if name == \"Chris\""-style code is the scripted-memory failure mode.)
#   2. scenarios/runtime/ must be non-empty (don't quietly delete the suite).
#   3. Rust tests and Rust doc examples must not be silently ignored.
#
# Adding a check: append a block, increment VIOLATIONS via fail(), keep output
# greppable (file:line: detail).

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
if REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  :
else
  REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd -P)"
fi
cd "$REPO_ROOT"

VIOLATIONS=0
SCENARIO_MANIFEST="scenarios/runtime/SCENARIO_MANIFEST.txt"

fail() {
  echo "DOCTRINE VIOLATION: $1" >&2
  VIOLATIONS=$((VIOLATIONS + 1))
}

production_rs_lines() {
  find crates -path '*/tests/*' -prune -o -name "*.rs" -print |
    while IFS= read -r file; do
      awk '
        function delta(line, opens, closes) {
          opens = gsub(/\{/, "{", line)
          closes = gsub(/\}/, "}", line)
          return opens - closes
        }
        pending_cfg_test && /^[[:space:]]*mod[[:space:]]+tests[[:space:]]*\{/ {
          in_test = 1
          depth = delta($0)
          pending_cfg_test = 0
          next
        }
        pending_cfg_test { pending_cfg_test = 0 }
        /^[[:space:]]*#\[cfg\(test\)\]/ {
          pending_cfg_test = 1
          next
        }
        in_test {
          depth += delta($0)
          if (depth <= 0) {
            in_test = 0
          }
          next
        }
        { print FILENAME ":" FNR ":" $0 }
      ' "$file"
    done
}

manifest_scenarios() {
  if [ ! -f "$SCENARIO_MANIFEST" ]; then
    fail "Scenario manifest missing at $SCENARIO_MANIFEST."
    return
  fi
  grep -vE '^[[:space:]]*(#|$)' "$SCENARIO_MANIFEST" | sed -E 's/^[[:space:]]+|[[:space:]]+$//g'
}

validate_scenario_manifest() {
  local entries
  entries="$(manifest_scenarios || true)"
  if [ -z "$entries" ]; then
    fail "Scenario manifest is empty."
    return
  fi

  local duplicate_entries
  duplicate_entries="$(printf '%s\n' "$entries" | sort | uniq -d)"
  if [ -n "$duplicate_entries" ]; then
    fail "Scenario manifest has duplicate entries."
    echo "$duplicate_entries" >&2
  fi

  while IFS= read -r name; do
    [ -z "$name" ] && continue
    if [ ! -f "scenarios/runtime/$name" ]; then
      fail "Scenario manifest entry is missing: $name"
    fi
  done <<< "$entries"

  while IFS= read -r path; do
    [ -z "$path" ] && continue
    local name
    name="$(basename "$path")"
    if ! printf '%s\n' "$entries" | grep -Fxq "$name"; then
      fail "Runtime scenario is not registered in $SCENARIO_MANIFEST: $name"
    fi
  done < <(find scenarios/runtime -maxdepth 1 -name "*.json" 2>/dev/null | sort)

  while IFS= read -r path; do
    [ -z "$path" ] && continue
    fail "Nested runtime scenario JSON is not gated by $SCENARIO_MANIFEST: $path"
  done < <(find scenarios/runtime -mindepth 2 -name "*.json" 2>/dev/null | sort)
}

validate_scenario_manifest

# ---------------------------------------------------------------------------
# Check 1: No hardcoded scenario-entity name comparisons in crate source.
#
# These names appear in scenarios/ and benchmarks/ as test data. They must
# never appear as string-equality branches inside crates/, because that
# means the system is being scripted to a specific entity instead of
# generalized through the entity-graph mechanism.
# ---------------------------------------------------------------------------

mapfile -t SCENARIO_NAMES < <(
  manifest_scenarios |
    while IFS= read -r name; do printf '%s\0' "scenarios/runtime/$name"; done |
    xargs -0 grep -hoE '\b[A-Z][A-Za-z0-9_]*( [A-Z][A-Za-z0-9_]*)*\b' 2>/dev/null |
    grep -Ev '^(A|An|And|Actually|Correction|Context|From|Got|I|It|Luna|Memory|What|Who|The|This|They)$' |
    sort -u
)

mapfile -t SCENARIO_VALUE_LITERALS < <(
  manifest_scenarios |
    while IFS= read -r name; do printf '%s\0' "scenarios/runtime/$name"; done |
    xargs -0 grep -hoE '"value"[[:space:]]*:[[:space:]]*"[^"]+"' 2>/dev/null |
    sed -E 's/^"value"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/' |
    grep -E '[[:alpha:]][[:alpha:]]+[[:space:]][[:alpha:]][[:alpha:]]+' |
    sort -u
)

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
  escaped=$(printf '%s\n' "$name" | sed 's/[][\\.^$*+?{}|()]/\\&/g')
  hits=$(grep -rn --include="*.rs" --exclude-dir=tests -E \
    "\bif\b.*==\s*[\"']${escaped}[\"']|^\s*[\"']${escaped}[\"']\s*=>" \
    crates/ 2>/dev/null \
    | grep -vE '^[^:]+:[0-9]+:\s*//' \
    || true)
  if [ -n "$hits" ]; then
    fail "Hardcoded entity-name dispatch for '$name' in crate source."
    echo "$hits" >&2
    echo "  Fix: replace the literal with a generic entity-graph lookup." >&2
  fi
done

for literal in "${SCENARIO_VALUE_LITERALS[@]}"; do
  hits=$(production_rs_lines |
    grep -F "\"${literal}\"" |
    grep -vE '^[^:]+:[0-9]+:\s*//' ||
    true)
  if [ -n "$hits" ]; then
    fail "Hardcoded scenario value literal '$literal' in crate source."
    echo "$hits" >&2
    echo "  Fix: derive the value from input/event data instead of fixture text." >&2
  fi
done

# Broad scenario fixture literal scan for production crate code. Unit tests may
# use concrete fixture names; runtime mechanisms may not.
fixture_pattern=""
for name in "${SCENARIO_NAMES[@]}"; do
  escaped=$(printf '%s\n' "$name" | sed 's/[][\\.^$*+?{}|()]/\\&/g')
  if [ -z "$fixture_pattern" ]; then
    fixture_pattern="$escaped"
  else
    fixture_pattern="$fixture_pattern|$escaped"
  fi
done
for literal in "${SCENARIO_VALUE_LITERALS[@]}"; do
  escaped=$(printf '%s\n' "$literal" | sed 's/[][\\.^$*+?{}|()]/\\&/g')
  if [ -z "$fixture_pattern" ]; then
    fixture_pattern="$escaped"
  else
    fixture_pattern="$fixture_pattern|$escaped"
  fi
done

fixture_hits=""
if [ -n "$fixture_pattern" ]; then
  fixture_hits=$(
    production_rs_lines |
      grep -E "\"(${fixture_pattern})\"" |
      grep -vE '^[^:]+:[0-9]+:\s*//' ||
      true
  )
fi
if [ -n "$fixture_hits" ]; then
  fail "Scenario fixture literal found in production crate code."
  echo "$fixture_hits" >&2
  echo "  Fix: derive entities from input/event data instead of fixture names." >&2
fi

# ---------------------------------------------------------------------------
# Check 1b: No hardcoded phrase-to-StructuredAssertion mappings in production extractor code.
#
# StructuredAssertion::new("domain", "kind", "value") or ::inferred(...) with
# string-literal arguments in production code is a hardcoded extraction fixture.
# Assertions must be derived from LLM output or generic signal processing, not
# from literal phrase matching. This is a separate check from the fixture-literal
# scan above because these phrases may not appear in any scenario manifest.
# ---------------------------------------------------------------------------

hardcoded_assertion_hits=$(
  find crates/luna-extract/src -name '*.rs' -print0 |
    xargs -0 grep -nE 'StructuredAssertion::(new|inferred)\("[^"]*",\s*"[^"]*",\s*"[^"]*"' 2>/dev/null |
    grep -vE '^[^:]+:[0-9]+:\s*//' ||
    true
)
if [ -n "$hardcoded_assertion_hits" ]; then
  fail "Hardcoded StructuredAssertion::new/inferred with string literals in extraction crate."
  echo "$hardcoded_assertion_hits" >&2
  echo "  Fix: derive assertions from LLM output or generic signal processing. No literal domain/kind/value combos." >&2
fi


# ---------------------------------------------------------------------------
# Check 1c: No bare `MemoryProvenance {` construction in production source.
#
# MemoryProvenance must be constructed via from_assertion() or from_system_root()
# so that every provenance record carries at least one source field.
# Bare struct literal construction in production code (not tests) is a
# violation of the provenance non-empty contract.
# ---------------------------------------------------------------------------


bare_provenance_hits=$(
  find crates -path '*/src/*.rs' -print0 |
    xargs -0 grep -n 'MemoryProvenance {' 2>/dev/null |
    grep -v 'struct MemoryProvenance' |
    grep -vE '^[^:]+:[0-9]+:\s*(//|/\*)' ||
    true
)
if [ -n "$bare_provenance_hits" ]; then
  fail "Bare MemoryProvenance struct literal in production source."
  echo "$bare_provenance_hits" >&2
  echo "  Fix: use MemoryProvenance::from_assertion(key) or MemoryProvenance::from_system_root(root)." >&2
fi


# ---------------------------------------------------------------------------
# Check 1d: No phrase-to-answer maps in production source.
#
# A phrase-to-answer map is any control-flow construct that checks user-facing
# text for a specific phrase and returns a hardcoded answer string. Canonical
# examples: match on question text returning literal answers, if/else chains
# with specific phrase checks and hardcoded returns, or lookup tables mapping
# questions to answers. This is the scripted-answer failure mode.
# ---------------------------------------------------------------------------


phrase_answer_hits=$(
  find crates -path '*/src/*.rs' -print0 |
    xargs -0 grep -n '=> "[A-Z]' 2>/dev/null |
    grep -vE '^[^:]+:[0-9]+:\s*(//|/\*)' |
    grep -v '#\[cfg\(test\)\]' |
    grep -v 'expect(' |
    grep -v 'panic!' ||
    true
)
if [ -n "$phrase_answer_hits" ]; then
  fail "Phrase-to-answer map detected in production source (match arm returning hardcoded string)."
  echo "$phrase_answer_hits" >&2
  echo "  Fix: answers must be derived from event-sourced memory, not from hardcoded phrase dispatch." >&2
fi


# ---------------------------------------------------------------------------
# Check 2: scenarios/runtime/ must be non-empty.
#
# The scenario suite is the executable form of the doctrine. Letting it
# silently empty out is the easiest way for the project to drift.
# ---------------------------------------------------------------------------

scenario_count=$(manifest_scenarios | wc -l | tr -d ' ')
if [ "${scenario_count:-0}" -lt 1 ]; then
  fail "Scenario manifest is empty. Doctrine requires registered scenario gates."
fi

while IFS= read -r scenario; do
  scenario="scenarios/runtime/$scenario"
  check_count=$(python3 - "$scenario" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as handle:
    data = json.load(handle)
checks = data.get("checks", {})

def present(value):
    if isinstance(value, bool):
        return value
    if isinstance(value, (list, dict, str)):
        return len(value) > 0
    return value is not None

def count_checks(value):
    if isinstance(value, dict):
        total = 0
        for key, child in value.items():
            if key.startswith("must_") or key.startswith("require_") or key.startswith("forbid_") or key.startswith("max_") or key in {"nodes", "tethers", "answers", "time"}:
                if present(child):
                    total += 1
            total += count_checks(child)
        return total
    if isinstance(value, list):
        return sum(count_checks(item) for item in value)
    return 0

print(count_checks(checks))
PY
  )
  if [ "${check_count:-0}" -lt 1 ]; then
    fail "$scenario has no executable memory checks."
    echo "  Fix: add at least one assertion/entity/relation/working-memory/recall/answer check." >&2
  fi
done < <(manifest_scenarios)

manuscript_one_read_gate=$(python3 - <<'PY'
import glob
import json

manifest = "scenarios/runtime/SCENARIO_MANIFEST.txt"
with open(manifest, "r", encoding="utf-8") as handle:
    paths = [
        "scenarios/runtime/" + line.strip()
        for line in handle
        if line.strip() and not line.lstrip().startswith("#")
    ]

for path in paths:
    with open(path, "r", encoding="utf-8") as handle:
        data = json.load(handle)
    check = data.get("checks", {}).get("manuscript_one_read", {})
    if (
        check.get("require_source_read") is True
        and check.get("require_explicit_close") is True
        and check.get("retrieval_turns")
        and check.get("forbid_source_after_close") is True
        and check.get("forbid_search_or_reread_after_close") is True
        and check.get("require_proof_eligible") is True
    ):
        print(path)
        break
PY
)
if [ -z "$manuscript_one_read_gate" ]; then
  fail "scenarios/runtime/ has no proof-eligible manuscript one-read protocol gate."
  echo "  Fix: add a scenario with checks.manuscript_one_read requiring source read, explicit close, retrieval turns, no source after close, no retrieval-time search/reread, and proof eligibility." >&2
fi

# ---------------------------------------------------------------------------
# Check 3: no ignored Rust tests or ignored Rust doc examples.
#
# A green gate must mean the test/doc example was compiled or run. If an example
# cannot execute locally, use `no_run` with real imports so Rust still compiles it.
# ---------------------------------------------------------------------------

ignored_test_hits=$(
  find crates -name "*.rs" -print0 |
    xargs -0 grep -nE '#\[ignore\]|```ignore' 2>/dev/null ||
    true
)
if [ -n "$ignored_test_hits" ]; then
  fail "Ignored Rust test/doc example found in crate source."
  echo "$ignored_test_hits" >&2
  echo "  Fix: remove #[ignore], or change doc examples from \`\`\`ignore to compiling \`\`\`no_run examples." >&2
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

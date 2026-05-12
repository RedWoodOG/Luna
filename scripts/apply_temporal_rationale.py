"""One-shot migration: apply the PR 0.1b authorial rationale to the five
temporal_disambiguation case JSONs.

Run from the luna repo root:

    python scripts/apply_temporal_rationale.py

For each temporal case, this script:

* Sets per-turn `timestamp` values to match
  `benchmarks/temporal/RATIONALE.md`.
* Flips `proof_eligible` from `false` to `true` (the temporal cases
  become proof material once their timestamps carry intentional
  authorial meaning).
* Replaces `timestamp_origin` with `"authorial"`.

Older "background" disclosures (turn 0 in cases 002-005) get timestamps
that satisfy the qualitative direction in the rationale ("months
earlier", "earlier in the year", "a while back", "one month earlier")
without being load-bearing on the proof. Their assistant turns sit one
minute after the corresponding disclosure to keep each disclosure's
mini-conversation grouped.

Idempotent: re-running with the same rationale produces byte-identical
output.
"""

import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
TEMPORAL_DIR = ROOT / "benchmarks" / "temporal"

# Per-case turn timestamps. Order matches the JSON `turns` array.
# Active-disclosure and probe values come straight from RATIONALE.md.
# Background disclosures are placed within the qualitative bounds the
# rationale gave ("months earlier", "earlier in the year", etc.).
SCHEDULES = {
    "recent_stress_001": [
        "2026-04-27T09:00:00Z",  # disclosure: last week, job stress
        "2026-04-27T09:01:00Z",  # assistant ack
        "2026-05-03T10:00:00Z",  # probe: recently, what was stressing me?
    ],
    "recent_stress_002": [
        "2026-02-03T09:00:00Z",  # background: months ago, moving apartments
        "2026-02-03T09:01:00Z",
        "2026-05-03T08:00:00Z",  # active: this morning, client deadline
        "2026-05-03T08:01:00Z",
        "2026-05-03T14:00:00Z",  # probe: recent pressure?
    ],
    "recent_stress_003": [
        "2026-02-15T10:00:00Z",  # background: earlier this year, tuition
        "2026-02-15T10:01:00Z",
        "2026-05-02T16:00:00Z",  # active: yesterday, manager's feedback
        "2026-05-02T16:01:00Z",
        "2026-05-03T10:00:00Z",  # probe: bothered me lately?
    ],
    "recent_stress_004": [
        "2026-03-10T09:00:00Z",  # background: a while back, commute
        "2026-03-10T09:01:00Z",
        "2026-05-03T09:00:00Z",  # active: today, budget review
        "2026-05-03T09:01:00Z",
        "2026-05-03T14:00:00Z",  # probe: wearing me down now?
    ],
    "recent_stress_005": [
        "2026-03-30T12:00:00Z",  # background: last month, travel
        "2026-03-30T12:01:00Z",
        "2026-04-30T12:00:00Z",  # active: this week, product launch
        "2026-04-30T12:01:00Z",
        "2026-05-03T10:00:00Z",  # probe: current stressful thing?
    ],
}


def apply_one(path: pathlib.Path) -> None:
    case = json.loads(path.read_text(encoding="utf-8"))
    case_id = case["id"]
    if case_id not in SCHEDULES:
        raise ValueError(f"{path}: no schedule for id {case_id!r}")
    schedule = SCHEDULES[case_id]
    if len(schedule) != len(case["turns"]):
        raise ValueError(
            f"{path}: schedule has {len(schedule)} timestamps but case has "
            f"{len(case['turns'])} turns"
        )
    for turn, ts in zip(case["turns"], schedule):
        turn["timestamp"] = ts
    case["proof_eligible"] = True
    case["timestamp_origin"] = "authorial"
    path.write_text(
        json.dumps(case, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def main() -> int:
    files = sorted(TEMPORAL_DIR.glob("*.json"))
    if len(files) != len(SCHEDULES):
        print(
            f"expected {len(SCHEDULES)} temporal files, found {len(files)}"
        )
        return 1
    for path in files:
        apply_one(path)
        print(f"updated {path.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

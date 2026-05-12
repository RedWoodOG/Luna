"""One-shot migration: legacy benchmark JSON -> schema v1.

Run from the luna repo root:

    python scripts/migrate_to_schema_v1.py

Idempotent at the file-shape level: re-running on already-migrated files
overwrites them with identical canonical bytes.

For PR 0.1 the migration is purely mechanical for non-temporal cases. The
five temporal_disambiguation cases receive placeholder timestamps and are
flagged `proof_eligible: false` until PR 0.1b commits authorial timestamps
plus a benchmarks/temporal/RATIONALE.md.
"""

import json
import pathlib
import datetime as dt

ROOT = pathlib.Path(__file__).resolve().parent.parent
BENCH_DIR = ROOT / "benchmarks"
SCHEMA_VERSION = 1
PROOF_CATEGORY = "proof_1_separability"
ORIGIN_START = dt.datetime(2026, 1, 1, 10, 0, 0, tzinfo=dt.timezone.utc)
TURN_GAP = dt.timedelta(hours=1)
TIMESTAMP_ORIGIN = "mechanical_pr_0_1"

CATEGORY_TARGETS = {
    "paraphrase_invariance": ["identity_relevance"],
    "emotional_recall": ["emotional_arousal"],
    "identity_continuity": ["identity_relevance", "goal_pressure"],
    "temporal_disambiguation": ["temporal_relevance", "goal_pressure"],
}

# Temporal cases ship with placeholder mechanical timestamps in PR 0.1.
# PR 0.1b replaces these with authorial values and flips proof_eligible to
# True. Until then, marking them ineligible keeps any computed metrics
# explicitly non-publishable.
INELIGIBLE_CATEGORIES = {"temporal_disambiguation"}


def migrate_one(path: pathlib.Path) -> None:
    raw = json.loads(path.read_text(encoding="utf-8"))
    category = raw["category"]
    if category not in CATEGORY_TARGETS:
        raise ValueError(f"{path}: unknown category {category!r}")

    turns = []
    for index, turn in enumerate(raw["turns"]):
        ts = ORIGIN_START + index * TURN_GAP
        turns.append(
            {
                "role": turn["role"],
                "content": turn["content"],
                "timestamp": ts.isoformat().replace("+00:00", "Z"),
            }
        )

    migrated = {
        "schema_version": SCHEMA_VERSION,
        "id": raw["id"],
        "proof_category": PROOF_CATEGORY,
        "proof_eligible": category not in INELIGIBLE_CATEGORIES,
        "category": category,
        "target_dimensions": list(CATEGORY_TARGETS[category]),
        "timestamp_origin": TIMESTAMP_ORIGIN,
        "turns": turns,
        "expected": {
            "must_recall": list(raw["expected"]["must_recall"]),
            "must_not_claim": list(raw["expected"]["must_not_claim"]),
        },
    }

    path.write_text(
        json.dumps(migrated, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def main() -> int:
    files = sorted(BENCH_DIR.rglob("*.json"))
    if not files:
        print("no benchmark files found under", BENCH_DIR)
        return 1
    for path in files:
        migrate_one(path)
        print(f"migrated {path.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

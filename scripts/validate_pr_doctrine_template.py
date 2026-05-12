#!/usr/bin/env python3
"""Validate that a PR body answered Luna's doctrine gate prompts."""

from __future__ import annotations

import argparse
import os
import re
import sys


REQUIRED_HEADINGS = [
    "Best Idea Check",
    "Memory Doctrine Check",
    "Hardcoding Review",
    "Memory Architecture",
    "Tests",
    "Doctrine Revision",
]

REQUIRED_PROMPTS = [
    "Failure mode this PR addresses",
    "General mechanism introduced or strengthened",
    "Scenario added/updated",
    "What must not happen",
    "Provenance/confidence preserved",
    "Working-memory budget impact",
    "Doctrine revision PR",
]

REQUIRED_CHECKBOXES = [
    "Working",
    "Fixable",
    "Explainable",
    "This PR does not add scripted user facts.",
    "This PR does not add scripted final answers.",
    "New behavior works for more than one entity/value or has a documented generalization plan.",
    "Any temporary phrase patch is covered by a scenario and marked for later generalization.",
    "Event log remains the source of truth.",
    "Derived memory can be rebuilt from events.",
    "Confirmed / inferred / unconfirmed / unknown boundaries are preserved.",
    "Ambiguity, contradictions, and stale facts are not flattened.",
    "Runtime behavior does not weaken proof-track rules.",
    "cargo test --workspace --all-features",
    "Relevant `runtime scenario` command(s)",
    "bash scripts/doctrine_check.sh (run locally; CI runs it too)",
    "powershell -ExecutionPolicy Bypass -File .\\scripts\\gate.ps1",
    "Product smoke or readiness evidence updated when runtime behavior changes",
]


def normalize_heading(value: str) -> str:
    return value.strip().lstrip("\ufeff").lower()


def section_headings(body: str) -> set[str]:
    return {
        normalize_heading(match.group(1))
        for match in re.finditer(r"(?m)^\ufeff?##\s+(.+?)\s*$", body)
    }


def prompt_value(body: str, prompt: str) -> str | None:
    pattern = re.compile(rf"(?mi)^\s*-\s*{re.escape(prompt)}:\s*(.*)$")
    match = pattern.search(body)
    if not match:
        return None
    value = match.group(1).strip()
    if not value:
        return ""
    return re.sub(r"<!--.*?-->", "", value).strip()


def is_substantive(value: str) -> bool:
    lowered = value.strip().lower()
    return bool(lowered) and lowered not in {"n/a", "na", "none", "todo", "tbd"}


def checkbox_state(body: str, label: str) -> str | None:
    normalized_label = re.sub(r"[*_`]", "", label)
    for match in re.finditer(r"(?mi)^\s*-\s*\[([ x])\]\s*(.+?)\s*$", body):
        text = re.sub(r"[*_`]", "", match.group(2)).strip()
        if normalized_label in text:
            return match.group(1).lower()
    return None


def validate_pr_body(body: str) -> list[str]:
    failures: list[str] = []
    if not body.strip():
        return ["PR body is empty; Luna doctrine prompts must be answered."]

    headings = section_headings(body)
    for heading in REQUIRED_HEADINGS:
        if normalize_heading(heading) not in headings:
            failures.append(f"missing required section: ## {heading}")

    for prompt in REQUIRED_PROMPTS:
        value = prompt_value(body, prompt)
        if value is None:
            failures.append(f"missing required prompt: {prompt}:")
        elif prompt == "Doctrine revision PR":
            if not value:
                failures.append(
                    "Doctrine revision PR must be answered with N/A or a link."
                )
        elif not is_substantive(value):
                failures.append(f"prompt has no substantive answer: {prompt}:")

    for checkbox in REQUIRED_CHECKBOXES:
        state = checkbox_state(body, checkbox)
        if state is None:
            failures.append(f"missing required checked item: {checkbox}")
        elif state != "x":
            failures.append(f"required checklist item is not checked: {checkbox}")

    return failures


def read_body(args: argparse.Namespace) -> str:
    if args.body_file:
        with open(args.body_file, "r", encoding="utf-8") as handle:
            return handle.read()
    return os.environ.get("PR_BODY", "")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--body-file")
    args = parser.parse_args()

    failures = validate_pr_body(read_body(args))
    if failures:
        for failure in failures:
            print(f"PR DOCTRINE VIOLATION: {failure}", file=sys.stderr)
        return 1

    print("PR doctrine template check: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

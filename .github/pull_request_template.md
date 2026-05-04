## Best Idea Check

This change is "best" because it is:

- [ ] **Working** — runs end-to-end on a stated case (scenario or test linked above).
- [ ] **Fixable** — surface is small, state is inspectable, next-week-me can debug it.
- [ ] **Explainable** — design choice defended in plain language in the PR description.

If you cannot check all three, this is not the best idea yet. Iterate before opening the PR.

---

## Memory Doctrine Check

- Failure mode this PR addresses:
- General mechanism introduced or strengthened:
- Scenario added/updated:
- What must not happen:
- Provenance/confidence preserved:
- Working-memory budget impact:

## Hardcoding Review

- [ ] This PR does not add scripted user facts.
- [ ] This PR does not add scripted final answers.
- [ ] New behavior works for more than one entity/value or has a documented generalization plan.
- [ ] Any temporary phrase patch is covered by a scenario and marked for later generalization.

## Memory Architecture

- [ ] Event log remains the source of truth.
- [ ] Derived memory can be rebuilt from events.
- [ ] Confirmed / inferred / unconfirmed / unknown boundaries are preserved.
- [ ] Ambiguity, contradictions, and stale facts are not flattened.
- [ ] Runtime behavior does not weaken proof-track rules.

## Tests

- [ ] `cargo test --workspace`
- [ ] Relevant `runtime scenario` command(s)
- [ ] `bash scripts/doctrine_check.sh` (run locally; CI runs it too)

## Doctrine Revision

If this PR weakens, relaxes, or removes any rule in `docs/LUNA_BUILD_DOCTRINE.md`,
link the doctrine-revision PR that justified the change. Erosion happens one
exception at a time; deliberate revision is fine, drift is what kills the project.

- Doctrine revision PR: <!-- N/A or link -->


# v1.0 Release Evidence Packet

This is the packet template for a v1.0 Ready release decision. It does not make
Luna v1.0 by existing. It is an evidence index that ties each release claim to a
regenerable artifact, or marks the claim as not made.

## Claim Boundary

A complete packet may support this claim:

```text
This release candidate has archived evidence for every capability claimed in
the release notes, and the notes identify anything present but not gated,
review-only, or not yet true.
```

It must not imply:

- LLM extraction quality without an LLM Ready packet;
- 24-hour continuity without a passing marathon trial packet;
- full-manuscript one-read memory without a passing manuscript packet;
- baseline superiority without a named baseline protocol and results;
- clean release quality from a dirty packet unless the exact diff is archived
  and named in the release notes.

## Packet Layout

Use this directory shape:

```text
.luna/v1-release/<version-or-rc>/<timestamp>/
  MANIFEST.json
  RELEASE_NOTES_DRAFT.md
  CLAIM_MATRIX.md
  commands.ps1
  gate/
    gate.log
    git_status.txt
    toolchain.txt
  packets/
    testing-ready/
    llm-ready/
    marathon/
    manuscript/
  artifacts/
    release-cli.log
    release-builds.txt
    hashes.txt
  review/
    council_signoff.md
    open_risks.md
    deferred_claims.md
```

Copy or reference immutable packet paths under `packets/`. Do not summarize a
packet as passing unless its own manifest records a pass result.

## Manifest Template

`MANIFEST.json` must be filled in before release approval:

```json
{
  "packet_type": "v1-release-evidence",
  "packet_version": 1,
  "release_candidate": "",
  "created_at_utc": "",
  "repo": {
    "branch": "",
    "commit": "",
    "tag": "",
    "git_status_short": "",
    "allow_dirty": false,
    "dirty_diff_files": []
  },
  "toolchain": {
    "rustc": "",
    "cargo": "",
    "powershell": "",
    "os": ""
  },
  "required_packets": {
    "testing_ready": {
      "path": "",
      "status": "missing"
    },
    "llm_ready": {
      "path": "",
      "status": "not-claimed"
    },
    "marathon": {
      "path": "",
      "status": "not-claimed"
    },
    "manuscript": {
      "path": "",
      "status": "not-claimed"
    }
  },
  "release_artifacts": {
    "release_cli_log": "artifacts/release-cli.log",
    "release_builds": "artifacts/release-builds.txt",
    "hashes": "artifacts/hashes.txt"
  },
  "result": {
    "status": "not-reviewed",
    "approved_by": [],
    "blocked_by": [],
    "deferred_claims": []
  }
}
```

## Claim Matrix Template

`CLAIM_MATRIX.md` is the release truth table. Every public claim needs one row.

```markdown
| Claim | Release wording | Evidence packet or file | Status | Allowed wording |
| --- | --- | --- | --- | --- |
| Deterministic local product loop | | | CI-proven / packet-proven / not claimed | |
| LLM extraction quality | | | packet-proven / not claimed | |
| 24-hour continuity | | | trial-proven / not claimed | |
| Full-manuscript one-read memory | | | trial-proven / not claimed | |
| Baseline superiority | | | benchmark-proven / not claimed | |
```

Allowed statuses:

- `CI-proven`
- `packet-proven`
- `trial-proven`
- `review-only`
- `present-not-gated`
- `not-claimed`
- `not-yet-true`

Do not use `green`, `done`, or `works` as evidence status.

## Required Procedure

1. Start from a clean release-candidate commit unless the release explicitly
   names and archives a dirty diff.
2. Run the local gate and preserve `gate/gate.log`.
3. Attach the Testing Ready packet for the exact commit.
4. Attach the LLM Ready packet only if release notes claim LLM extraction
   quality.
5. Attach the marathon packet only if release notes claim real 24-hour
   continuity.
6. Attach the manuscript packet only if release notes claim real one-read
   manuscript memory.
7. Run release CLI/build commands and preserve logs/hashes.
8. Fill `CLAIM_MATRIX.md` before drafting final release notes.
9. Move unsupported claims to `review/deferred_claims.md`.
10. Record council approval or blockers in `review/council_signoff.md`.

## Pass Criteria

A v1.0 packet can pass only if:

- `git_status_short` is clean, or every dirty file is archived and explicitly
  named in release notes;
- the gate log is present and successful;
- the Testing Ready packet matches the release commit;
- every claimed LLM, marathon, manuscript, or baseline capability has a passing
  packet or is removed from release wording;
- release artifacts are listed with hashes;
- open risks and deferred claims are visible in the packet.

## Failure Criteria

The packet fails if:

- release notes claim 24-hour, full-manuscript, LLM quality, or baseline
  superiority without matching evidence;
- a deterministic scenario is used as the only proof for a real-world claim;
- packet manifests do not identify commit, status, commands, and hashes;
- skipped, ignored, unregistered, or reviewer-only checks are described as
  green release gates;
- dirty-source evidence is omitted.

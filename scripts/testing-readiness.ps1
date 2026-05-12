param(
    [string] $OutDir,
    [switch] $AllowDirty
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $PSCommandPath
$repoRoot = Resolve-Path -LiteralPath (Join-Path $scriptDir "..")

function Invoke-Captured {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Name,
        [Parameter(Mandatory = $true)]
        [string] $OutputPath,
        [Parameter(Mandatory = $true)]
        [scriptblock] $Command
    )

    Write-Host ""
    Write-Host "==> $Name"
    $global:LASTEXITCODE = 0
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & $Command *> $OutputPath
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    Get-Content -LiteralPath $OutputPath
    if ($exitCode -ne 0) {
        throw "$Name failed with exit code $exitCode; see $OutputPath"
    }
}

function Resolve-LunaBinary {
    $base = Join-Path $repoRoot "target/release/luna"
    $candidates = @($base, "$base.exe")
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }

    throw "Built luna binary not found at target/release/luna(.exe). Run the gate first."
}

function Quote-PowerShellArg {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Value
    )

    return "'" + $Value.Replace("'", "''") + "'"
}

function Write-Text {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path,
        [Parameter(Mandatory = $true)]
        [string] $Value
    )

    $encoding = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($Path, $Value, $encoding)
}

function Write-Json {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path,
        [object] $Value,
        [int] $Depth = 8
    )

    $json = ConvertTo-Json -InputObject $Value -Depth $Depth
    if ($null -eq $json) {
        $json = "[]"
    }
    Write-Text -Path $Path -Value $json
}

function Get-DirectoryFileHashes {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        return @()
    }

    $root = [System.IO.Path]::GetFullPath($Path).TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
    return @(Get-ChildItem -LiteralPath $Path -Recurse -File |
        Sort-Object FullName |
        ForEach-Object {
            $relative = $_.FullName.Substring($root.Length).TrimStart([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
            [ordered]@{
                path = $relative
                length = $_.Length
                sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        })
}

Push-Location -LiteralPath $repoRoot
try {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    if (-not $OutDir) {
        $OutDir = Join-Path $repoRoot ".luna\testing-readiness\$stamp"
    }
    $OutDir = [System.IO.Path]::GetFullPath($OutDir)
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

    $porcelain = git status --porcelain
    $dirty = [bool]$porcelain
    if ($dirty -and -not $AllowDirty) {
        $porcelain | Set-Content -LiteralPath (Join-Path $OutDir "dirty-status.txt") -Encoding UTF8
        throw "Working tree is dirty. Commit first, or rerun with -AllowDirty to archive staged/unstaged diffs and untracked files."
    }
    if ($dirty) {
        $porcelain | Set-Content -LiteralPath (Join-Path $OutDir "dirty-status.txt") -Encoding UTF8
        $previousErrorActionPreference = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try {
            cmd /d /c "git -c core.autocrlf=false diff --binary 2>NUL" | Set-Content -LiteralPath (Join-Path $OutDir "source.unstaged.patch") -Encoding UTF8
            cmd /d /c "git -c core.autocrlf=false diff --cached --binary 2>NUL" | Set-Content -LiteralPath (Join-Path $OutDir "source.staged.patch") -Encoding UTF8
            cmd /d /c "git -c core.autocrlf=false diff --name-status 2>NUL" | Set-Content -LiteralPath (Join-Path $OutDir "source.unstaged.name-status.txt") -Encoding UTF8
            cmd /d /c "git -c core.autocrlf=false diff --cached --name-status 2>NUL" | Set-Content -LiteralPath (Join-Path $OutDir "source.staged.name-status.txt") -Encoding UTF8
        } finally {
            $ErrorActionPreference = $previousErrorActionPreference
        }
        $untrackedFiles = $porcelain | Where-Object { $_.StartsWith("?? ") } |
            ForEach-Object { $_.Substring(3) }
        $untrackedFiles | Set-Content -LiteralPath (Join-Path $OutDir "untracked-files.txt") -Encoding UTF8
        $untrackedRoot = Join-Path $OutDir "untracked"
        foreach ($file in $untrackedFiles) {
            $source = Join-Path $repoRoot $file
            if (Test-Path -LiteralPath $source) {
                $destination = Join-Path $untrackedRoot $file
                if (Test-Path -LiteralPath $source -PathType Container) {
                    $destinationParent = Split-Path -Parent $destination
                    New-Item -ItemType Directory -Force -Path $destinationParent | Out-Null
                    Copy-Item -LiteralPath $source -Destination $destination -Recurse -Force
                } else {
                    $destinationParent = Split-Path -Parent $destination
                    New-Item -ItemType Directory -Force -Path $destinationParent | Out-Null
                    Copy-Item -LiteralPath $source -Destination $destination -Force
                }
            }
        }
    }

    $smokeDir = Join-Path $OutDir "product-smoke"
    $protocolDir = Join-Path $OutDir "protocol-loop"
    $llmReadyDir = Join-Path $OutDir "llm-ready"
    $localTrialDir = Join-Path $OutDir "local-runtime-trial"
    New-Item -ItemType Directory -Force -Path $smokeDir | Out-Null
    New-Item -ItemType Directory -Force -Path $protocolDir | Out-Null
    New-Item -ItemType Directory -Force -Path $llmReadyDir | Out-Null
    New-Item -ItemType Directory -Force -Path $localTrialDir | Out-Null

    git rev-parse HEAD | Set-Content -LiteralPath (Join-Path $OutDir "commit.txt") -Encoding UTF8
    git status --short --branch | Set-Content -LiteralPath (Join-Path $OutDir "git-status.txt") -Encoding UTF8
    rustc --version | Set-Content -LiteralPath (Join-Path $OutDir "rustc-version.txt") -Encoding UTF8
    cargo --version | Set-Content -LiteralPath (Join-Path $OutDir "cargo-version.txt") -Encoding UTF8

    Invoke-Captured "local gate" (Join-Path $OutDir "local-gate.log") {
        cmd /d /c "powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\gate.ps1 2>&1"
    }

    $luna = Resolve-LunaBinary
    $llmCorpusDir = Join-Path $llmReadyDir "corpus"
    $llmFixturePath = Join-Path $llmReadyDir "command-extractor-fixture.ps1"
    $llmCommandCallLog = Join-Path $llmReadyDir "command-calls.txt"
    $llmCommandArgs = @(
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        $llmFixturePath,
        "-CallLog",
        $llmCommandCallLog
    )
    $llmCommandArgsFile = Join-Path $llmReadyDir "command-args.json"
    Write-Json -Path $llmCommandArgsFile -Value $llmCommandArgs -Depth 4
    $allowDirtyReplayArg = if ($AllowDirty) { " -AllowDirty" } else { "" }
    $commands = @"
# Luna testing readiness packet commands
cd "$repoRoot"
powershell -ExecutionPolicy Bypass -File .\scripts\gate.ps1
& "$luna" runtime smoke --log "$smokeDir\events.jsonl" --reset --json --report "$smokeDir\smoke-report.json"
& "$luna" runtime turn "Taylor lives in Vermont." --log "$protocolDir\events.jsonl" --format markdown
& "$luna" runtime turn "Taylor is planning a quiet Sunday grocery run." --log "$protocolDir\events.jsonl" --format markdown
& "$luna" runtime inspect --log "$protocolDir\events.jsonl" --format markdown
& "$luna" runtime audit --log "$protocolDir\events.jsonl" --format markdown
& "$luna" runtime turn "Where does Taylor live?" --log "$protocolDir\events.jsonl" --format markdown
& "$luna" runtime turn "Taylor moved to Maine." --log "$protocolDir\events.jsonl" --format markdown
& "$luna" runtime inspect --log "$protocolDir\events.jsonl" --format markdown
& "$luna" runtime audit --log "$protocolDir\events.jsonl" --format markdown
& "$luna" runtime turn "Where does Taylor live?" --log "$protocolDir\events.jsonl" --format markdown
& "$luna" runtime inspect --log "$smokeDir\events.jsonl" --format markdown
& "$luna" runtime audit --log "$smokeDir\events.jsonl" --format markdown
powershell -ExecutionPolicy Bypass -File .\scripts\llm-ready.ps1 -Luna "$luna" -Corpus "$llmCorpusDir" -ModelId "testing-ready-command-fixture@v1" -ExtractorCommand powershell -CommandArgsFile "$llmCommandArgsFile" -Cache "$llmReadyDir\cache" -OutDir "$llmReadyDir\packet"$allowDirtyReplayArg
powershell -ExecutionPolicy Bypass -File .\scripts\local-runtime-trial.ps1 -Controlled -Log "$localTrialDir\events.jsonl" -ResetLog -TrialFile "$localTrialDir\trial.json" -OutDir "$localTrialDir\packet"$allowDirtyReplayArg
"@
    Write-Text -Path (Join-Path $OutDir "commands.ps1") -Value $commands

    $smokeLog = Join-Path $smokeDir "events.jsonl"
    $smokeReport = Join-Path $smokeDir "smoke-report.json"

    Invoke-Captured "runtime smoke evidence" (Join-Path $smokeDir "smoke-stdout.json") {
        & $luna runtime smoke --log $smokeLog --reset --json --report $smokeReport
    }
    Invoke-Captured "runtime inspect evidence" (Join-Path $smokeDir "inspect.md") {
        & $luna runtime inspect --log $smokeLog --format markdown
    }
    Invoke-Captured "runtime inspect JSON evidence" (Join-Path $smokeDir "inspect.json") {
        & $luna runtime inspect --log $smokeLog --format json
    }
    Invoke-Captured "runtime audit evidence" (Join-Path $smokeDir "audit.md") {
        & $luna runtime audit --log $smokeLog --format markdown
    }
    Invoke-Captured "runtime audit JSON evidence" (Join-Path $smokeDir "audit.json") {
        & $luna runtime audit --log $smokeLog --format json
    }
    Invoke-Captured "runtime repeat audit JSON evidence" (Join-Path $smokeDir "audit-repeat.json") {
        & $luna runtime audit --log $smokeLog --format json
    }
    $auditJsonHash = Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $smokeDir "audit.json")
    $repeatAuditJsonHash = Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $smokeDir "audit-repeat.json")
    if ($auditJsonHash.Hash -ne $repeatAuditJsonHash.Hash) {
        throw "Repeat runtime audit JSON changed for $smokeLog; see product-smoke/audit.json and audit-repeat.json"
    }
    $auditJsonHash | Format-List | Out-String | Set-Content -LiteralPath (Join-Path $smokeDir "audit.json.sha256.txt") -Encoding UTF8
    $repeatAuditJsonHash | Format-List | Out-String | Set-Content -LiteralPath (Join-Path $smokeDir "audit-repeat.json.sha256.txt") -Encoding UTF8

    New-Item -ItemType Directory -Force -Path $llmCorpusDir | Out-Null
    $llmScenario = [ordered]@{
        name = "testing_ready_command_extractor_fixture"
        turns = @(
            [ordered]@{
                content = "Chris lives in Iowa."
                timestamp = "2026-05-10T12:00:00Z"
            }
        )
        checks = [ordered]@{
            claims = [ordered]@{
                must_include = @(
                    [ordered]@{
                        domain = "person"
                        kind = "location"
                        value = "Chris lives in Iowa"
                        lifecycle_status = "current"
                    }
                )
            }
        }
    }
    Write-Json -Path (Join-Path $llmCorpusDir "command-extractor-case.json") -Value $llmScenario -Depth 8
    $llmFixture = @'
param(
    [Parameter(Mandatory = $true)]
    [string] $CallLog
)
$null = [Console]::In.ReadToEnd()
Add-Content -LiteralPath $CallLog -Value "valid" -Encoding UTF8
$response = '{"assertions":[{"domain":"person","kind":"location","value":"Chris lives in Iowa","confidence":0.92,"evidence_span":"Chris lives in Iowa"}],"signals":{"emotional_arousal":null,"goal_pressure":null,"identity_relevance":null,"temporal_relevance":null}}'
[Console]::Out.Write($response)
'@
    Write-Text -Path $llmFixturePath -Value $llmFixture
    $llmReadyScript = Join-Path $repoRoot "scripts\llm-ready.ps1"
    $llmInvocationArgs = @{
        Luna = $luna
        Corpus = $llmCorpusDir
        ModelId = "testing-ready-command-fixture@v1"
        ExtractorCommand = "powershell"
        CommandArgsFile = $llmCommandArgsFile
        Cache = (Join-Path $llmReadyDir "cache")
        OutDir = (Join-Path $llmReadyDir "packet")
    }
    if ($AllowDirty) {
        $llmInvocationArgs.AllowDirty = $true
    }
    $llmReadyLog = Join-Path $llmReadyDir "llm-ready.log"
    Write-Host ""
    Write-Host "==> llm-ready deterministic packet"
    $global:LASTEXITCODE = 0
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & $llmReadyScript @llmInvocationArgs *> $llmReadyLog
        $exitCode = 0
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    Get-Content -LiteralPath $llmReadyLog
    if ($exitCode -ne 0) {
        throw "llm-ready deterministic packet failed with exit code $exitCode; see $llmReadyLog"
    }

    $protocolLog = Join-Path $protocolDir "events.jsonl"
    if (Test-Path -LiteralPath $protocolLog) {
        Remove-Item -LiteralPath $protocolLog -Force
    }
    Invoke-Captured "protocol turn seed" (Join-Path $protocolDir "01-seed.md") {
        & $luna runtime turn "Taylor lives in Vermont." --log $protocolLog --format markdown
    }
    Invoke-Captured "protocol turn distract" (Join-Path $protocolDir "02-distract.md") {
        & $luna runtime turn "Taylor is planning a quiet Sunday grocery run." --log $protocolLog --format markdown
    }
    Invoke-Captured "protocol inspect after seed" (Join-Path $protocolDir "03-inspect-after-seed.md") {
        & $luna runtime inspect --log $protocolLog --format markdown
    }
    Invoke-Captured "protocol audit after seed" (Join-Path $protocolDir "04-audit-after-seed.md") {
        & $luna runtime audit --log $protocolLog --format markdown
    }
    Invoke-Captured "protocol recall before correction" (Join-Path $protocolDir "05-recall-before-correction.md") {
        & $luna runtime turn "Where does Taylor live?" --log $protocolLog --format markdown
    }
    Invoke-Captured "protocol correction" (Join-Path $protocolDir "06-correction.md") {
        & $luna runtime turn "Taylor moved to Maine." --log $protocolLog --format markdown
    }
    Invoke-Captured "protocol inspect after correction" (Join-Path $protocolDir "07-inspect-after-correction.md") {
        & $luna runtime inspect --log $protocolLog --format markdown
    }
    Invoke-Captured "protocol inspect JSON after correction" (Join-Path $protocolDir "07-inspect-after-correction.json") {
        & $luna runtime inspect --log $protocolLog --format json
    }
    Invoke-Captured "protocol audit after correction" (Join-Path $protocolDir "08-audit-after-correction.md") {
        & $luna runtime audit --log $protocolLog --format markdown
    }
    Invoke-Captured "protocol audit JSON after correction" (Join-Path $protocolDir "08-audit-after-correction.json") {
        & $luna runtime audit --log $protocolLog --format json
    }
    Invoke-Captured "protocol recall after correction" (Join-Path $protocolDir "09-recall-after-correction.md") {
        & $luna runtime turn "Where does Taylor live?" --log $protocolLog --format markdown
    }

    $trialObject = [ordered]@{
        protocol = "first-human-controlled-runtime-trial-v1"
        source_boundary = [ordered]@{
            source_scope = "Only the trial turns in this JSON are source material for this controlled local runtime trial."
            locked_before_questions = $true
            forbidden_question_terms = @(
                "according to the source",
                "open the source",
                "reread",
                "search the source"
            )
        }
        prompt_boundary = [ordered]@{
            questions_created_before_answers = $true
            no_source_text_in_questions = $true
            no_answer_hints_in_questions = $true
            no_retrieval_time_reread_or_search = $true
        }
        scoring = [ordered]@{
            reviewer = "testing-readiness-fixture"
            scale = "pass|partial|fail|justified_unknown|boundary_violation"
            pass_rule = "All required questions pass or are justified unknowns, and every miss is captured in review/regression_backlog.md."
        }
        regression_capture = [ordered]@{
            required = $true
            backlog_path = "review/regression_backlog.md"
        }
        turns = @(
            "Taylor lives in Vermont.",
            "Taylor is planning a quiet Sunday grocery run.",
            "Taylor prefers quiet morning errands.",
            "Taylor keeps the grocery list on paper.",
            "Taylor moved to Maine."
        )
        questions = @(
            [ordered]@{
                id = "q001"
                category = "correction"
                question = "Where does Taylor live now?"
                expected_evidence = "The current location should reflect the later correction."
                must_not_include = "Vermont as current"
            },
            [ordered]@{
                id = "q002"
                category = "episodic"
                question = "What errand is Taylor planning?"
                expected_evidence = "The answer should come from memory, not from source text in the prompt."
                must_not_include = ""
            },
            [ordered]@{
                id = "q003"
                category = "preference"
                question = "What kind of errands does Taylor prefer?"
                expected_evidence = "The answer should come from memory, not from source text in the prompt."
                must_not_include = ""
            }
        )
    }
    $trialFile = Join-Path $localTrialDir "trial.json"
    Write-Json -Path $trialFile -Value $trialObject -Depth 4
    $localTrialScript = Join-Path $repoRoot "scripts\local-runtime-trial.ps1"
    $localTrialInvocationArgs = @{
        Log = (Join-Path $localTrialDir "events.jsonl")
        ResetLog = $true
        Controlled = $true
        TrialFile = $trialFile
        OutDir = (Join-Path $localTrialDir "packet")
    }
    if ($AllowDirty) {
        $localTrialInvocationArgs.AllowDirty = $true
    }
    $localTrialLog = Join-Path $localTrialDir "local-runtime-trial.log"
    Write-Host ""
    Write-Host "==> local runtime trial packet"
    $global:LASTEXITCODE = 0
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & $localTrialScript @localTrialInvocationArgs *> $localTrialLog
        $exitCode = 0
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    Get-Content -LiteralPath $localTrialLog
    if ($exitCode -ne 0) {
        throw "local runtime trial packet failed with exit code $exitCode; see $localTrialLog"
    }

    $hash = Get-FileHash -Algorithm SHA256 -LiteralPath $smokeLog
    $hash | Format-List | Out-String | Set-Content -LiteralPath (Join-Path $smokeDir "events.sha256.txt") -Encoding UTF8
    $protocolHash = Get-FileHash -Algorithm SHA256 -LiteralPath $protocolLog
    $protocolHash | Format-List | Out-String | Set-Content -LiteralPath (Join-Path $protocolDir "events.sha256.txt") -Encoding UTF8
    $llmReadyManifest = Join-Path $llmReadyDir "packet\manifest.json"
    $localTrialManifest = Join-Path $localTrialDir "packet\manifest.json"
    if (-not (Test-Path -LiteralPath $llmReadyManifest -PathType Leaf)) {
        throw "LLM Ready packet manifest missing: $llmReadyManifest"
    }
    if (-not (Test-Path -LiteralPath $localTrialManifest -PathType Leaf)) {
        throw "Local runtime trial packet manifest missing: $localTrialManifest"
    }
    $llmReadyManifestHash = Get-FileHash -Algorithm SHA256 -LiteralPath $llmReadyManifest
    $localTrialManifestHash = Get-FileHash -Algorithm SHA256 -LiteralPath $localTrialManifest

    $binaryHash = Get-FileHash -Algorithm SHA256 -LiteralPath $luna
    $dialogPath = Join-Path $repoRoot "crates\luna-cli\smoke-dialog.json"
    $dialogHash = Get-FileHash -Algorithm SHA256 -LiteralPath $dialogPath
    Copy-Item -LiteralPath $dialogPath -Destination (Join-Path $smokeDir "smoke-dialog.json") -Force

    $manifestObject = [ordered]@{
        created = (Get-Date -Format o)
        repo = "$repoRoot"
        commit = (Get-Content -LiteralPath (Join-Path $OutDir "commit.txt") -Raw).Trim()
        dirty = $dirty
        allow_dirty = [bool]$AllowDirty
        gate_run = $true
        luna_binary = "$luna"
        luna_binary_sha256 = $binaryHash.Hash
        smoke_dialog_sha256 = $dialogHash.Hash
        smoke_log = $smokeLog
        smoke_log_sha256 = $hash.Hash
        protocol_log = $protocolLog
        protocol_log_sha256 = $protocolHash.Hash
        llm_ready_manifest = $llmReadyManifest
        llm_ready_manifest_sha256 = $llmReadyManifestHash.Hash
        local_runtime_trial_manifest = $localTrialManifest
        local_runtime_trial_manifest_sha256 = $localTrialManifestHash.Hash
        repeat_audit_json_sha256 = $repeatAuditJsonHash.Hash
        artifact_hash_manifest = Join-Path $OutDir "artifact-hashes.json"
        artifacts = @(
            "local-gate.log",
            "product-smoke/events.jsonl",
            "product-smoke/smoke-report.json",
            "product-smoke/inspect.md",
            "product-smoke/inspect.json",
            "product-smoke/audit.md",
            "product-smoke/audit.json",
            "product-smoke/audit-repeat.json",
            "product-smoke/audit.json.sha256.txt",
            "product-smoke/audit-repeat.json.sha256.txt",
            "protocol-loop/events.jsonl",
            "protocol-loop/01-seed.md",
            "protocol-loop/02-distract.md",
            "protocol-loop/03-inspect-after-seed.md",
            "protocol-loop/04-audit-after-seed.md",
            "protocol-loop/05-recall-before-correction.md",
            "protocol-loop/06-correction.md",
            "protocol-loop/07-inspect-after-correction.md",
            "protocol-loop/07-inspect-after-correction.json",
            "protocol-loop/08-audit-after-correction.md",
            "protocol-loop/08-audit-after-correction.json",
            "protocol-loop/09-recall-after-correction.md",
            "llm-ready/packet/manifest.json",
            "llm-ready/packet/commands.ps1",
            "llm-ready/packet/cache-files.json",
            "local-runtime-trial/packet/manifest.json",
            "local-runtime-trial/packet/manifest.md",
            "local-runtime-trial/packet/events.jsonl",
            "artifact-hashes.json"
        )
    }
    Write-Json -Path (Join-Path $OutDir "manifest.json") -Value $manifestObject -Depth 6

    $manifest = @"
# Luna Testing Readiness Packet

- Created: $(Get-Date -Format o)
- Repo: $repoRoot
- Commit: $((Get-Content -LiteralPath (Join-Path $OutDir "commit.txt") -Raw).Trim())
- Output directory: $OutDir
- Gate run: true
- Dirty checkout: $dirty
- Smoke log: $smokeLog
- Smoke log SHA256: $($hash.Hash)
- Protocol log: $protocolLog
- Protocol log SHA256: $($protocolHash.Hash)
- LLM Ready manifest: $llmReadyManifest
- LLM Ready manifest SHA256: $($llmReadyManifestHash.Hash)
- Local runtime trial manifest: $localTrialManifest
- Local runtime trial manifest SHA256: $($localTrialManifestHash.Hash)
- Repeat audit JSON SHA256: $($repeatAuditJsonHash.Hash)

## Evidence

- commands.ps1: exact commands used for this packet.
- local-gate.log: workspace tests, doctrine lint, release build, runtime scenarios, and gate smoke.
- product-smoke/events.jsonl: persisted product runtime event log.
- product-smoke/smoke-report.json: machine-readable smoke assertions.
- product-smoke/inspect.md: rebuilt memory state after smoke.
- product-smoke/audit.md: replay audit after smoke.
- product-smoke/audit-repeat.json: repeat replay audit JSON; hash must match
  product-smoke/audit.json for this packet to finish.
- product-smoke/events.sha256.txt: hash for the captured event log.
- protocol-loop/: separate-process turn, inspect, audit, correction, and recall transcript.
- llm-ready/packet/: deterministic command-backed extractor packet with corpus,
  cache manifest, output hashes, and pass/fail summary.
- local-runtime-trial/packet/: reviewer-style local turn/question packet with
  copied event log, command transcript, inspect/audit outputs, and hashes.
- manifest.json: machine-readable artifact hashes and run metadata.
- artifact-hashes.json: SHA256 inventory for packet files present when the
  packet was finalized.

This packet proves a repeatable local product smoke run, deterministic
command-backed extractor packet generation, and a local runtime trial packet at
this commit. It does not prove real LLM extraction quality, the real 24-hour
continuity trial, or full-manuscript one-read trial.
"@
    Write-Text -Path (Join-Path $OutDir "manifest.md") -Value $manifest
    Write-Json -Path (Join-Path $OutDir "artifact-hashes.json") -Value (Get-DirectoryFileHashes -Path $OutDir) -Depth 8

    Write-Host ""
    Write-Host "Testing readiness packet written to $OutDir"
} finally {
    Pop-Location
}

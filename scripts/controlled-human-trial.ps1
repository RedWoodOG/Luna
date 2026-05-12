[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory = $true)]
    [string] $Log,
    [Parameter(Mandatory = $true)]
    [string] $TrialFile,
    [string] $OutDir,
    [switch] $AllowDirty
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $PSCommandPath
$repoRoot = Resolve-Path -LiteralPath (Join-Path $scriptDir "..")

function Resolve-RepoPath {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }

    return [System.IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
}

function Write-Text {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path,
        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string] $Value
    )

    $encoding = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($Path, $Value, $encoding)
}

function Write-Json {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path,
        [Parameter(Mandatory = $true)]
        [object] $Value,
        [int] $Depth = 8
    )

    Write-Text -Path $Path -Value (ConvertTo-Json -InputObject $Value -Depth $Depth)
}

function Get-ObjectProperty {
    param(
        [AllowNull()]
        [object] $Object,
        [Parameter(Mandatory = $true)]
        [string] $Name
    )

    if ($null -eq $Object) {
        return $null
    }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    return $property.Value
}

function Get-QuestionText {
    param(
        [Parameter(Mandatory = $true)]
        [object] $Question,
        [Parameter(Mandatory = $true)]
        [int] $Index
    )

    if ($Question -is [string]) {
        return [string]$Question
    }
    $text = [string](Get-ObjectProperty -Object $Question -Name "question")
    if ([string]::IsNullOrWhiteSpace($text)) {
        throw "Question $Index must be a string or an object with a non-empty 'question' field."
    }
    return $text
}

Push-Location -LiteralPath $repoRoot
try {
    $preparedAt = Get-Date
    $stamp = $preparedAt.ToString("yyyyMMdd-HHmmss")
    $logPath = Resolve-RepoPath $Log
    $trialFilePath = Resolve-RepoPath $TrialFile
    if (-not (Test-Path -LiteralPath $trialFilePath -PathType Leaf)) {
        throw "Trial file not found at $trialFilePath"
    }

    if (-not $OutDir) {
        $OutDir = Join-Path $repoRoot ".luna\controlled-human-trial\$stamp"
    }
    $OutDir = [System.IO.Path]::GetFullPath($OutDir)
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

    $porcelain = git status --porcelain
    $dirty = [bool]$porcelain
    if ($dirty -and -not $AllowDirty) {
        $porcelain | Set-Content -LiteralPath (Join-Path $OutDir "dirty-status.txt") -Encoding UTF8
        throw "Working tree is dirty. Commit first, or rerun with -AllowDirty to archive source diffs in the packet."
    }
    if ($dirty) {
        $porcelain | Set-Content -LiteralPath (Join-Path $OutDir "dirty-status.txt") -Encoding UTF8
        cmd /d /c "git -c core.autocrlf=false diff --binary 2>NUL" | Set-Content -LiteralPath (Join-Path $OutDir "source.unstaged.patch") -Encoding UTF8
        cmd /d /c "git -c core.autocrlf=false diff --cached --binary 2>NUL" | Set-Content -LiteralPath (Join-Path $OutDir "source.staged.patch") -Encoding UTF8
        cmd /d /c "git -c core.autocrlf=false diff --name-status 2>NUL" | Set-Content -LiteralPath (Join-Path $OutDir "source.unstaged.name-status.txt") -Encoding UTF8
        cmd /d /c "git -c core.autocrlf=false diff --cached --name-status 2>NUL" | Set-Content -LiteralPath (Join-Path $OutDir "source.staged.name-status.txt") -Encoding UTF8
        $untrackedFiles = $porcelain | Where-Object { $_.StartsWith("?? ") } |
            ForEach-Object { $_.Substring(3) }
        $untrackedFiles | Set-Content -LiteralPath (Join-Path $OutDir "untracked-files.txt") -Encoding UTF8
        $untrackedRoot = Join-Path $OutDir "untracked"
        foreach ($file in $untrackedFiles) {
            $source = Join-Path $repoRoot $file
            if (Test-Path -LiteralPath $source) {
                $destination = Join-Path $untrackedRoot $file
                $destinationParent = Split-Path -Parent $destination
                New-Item -ItemType Directory -Force -Path $destinationParent | Out-Null
                if (Test-Path -LiteralPath $source -PathType Container) {
                    Copy-Item -LiteralPath $source -Destination $destination -Recurse -Force
                } else {
                    Copy-Item -LiteralPath $source -Destination $destination -Force
                }
            }
        }
    }

    $trialRaw = Get-Content -LiteralPath $trialFilePath -Raw
    $trial = $trialRaw | ConvertFrom-Json
    if ($null -eq $trial.turns -or $null -eq $trial.questions) {
        throw "$trialFilePath must be a JSON object with 'turns' and 'questions' arrays."
    }
    foreach ($requiredProperty in @("source_boundary", "prompt_boundary", "scoring", "regression_capture")) {
        if ($null -eq (Get-ObjectProperty -Object $trial -Name $requiredProperty)) {
            throw "$trialFilePath must include '$requiredProperty' for a controlled human trial."
        }
    }
    $turns = @($trial.turns | ForEach-Object { [string]$_ } | Where-Object { $_.Trim().Length -gt 0 })
    $questions = @()
    $rawQuestions = @($trial.questions)
    for ($index = 0; $index -lt $rawQuestions.Count; $index++) {
        $questionText = Get-QuestionText -Question $rawQuestions[$index] -Index ($index + 1)
        if ($questionText.Trim().Length -gt 0) {
            $questions += $questionText
        }
    }
    if ($turns.Count -lt 5) {
        throw "Controlled human trial requires at least 5 reviewer-owned turns; found $($turns.Count)."
    }
    if ($questions.Count -lt 3) {
        throw "Controlled human trial requires at least 3 reviewer-owned questions; found $($questions.Count)."
    }

    $copiedTrialFile = Join-Path $OutDir "trial.json"
    Copy-Item -LiteralPath $trialFilePath -Destination $copiedTrialFile -Force

    git rev-parse HEAD | Set-Content -LiteralPath (Join-Path $OutDir "commit.txt") -Encoding UTF8
    git status --short --branch | Set-Content -LiteralPath (Join-Path $OutDir "git-status.txt") -Encoding UTF8
    rustc --version | Set-Content -LiteralPath (Join-Path $OutDir "rustc-version.txt") -Encoding UTF8
    cargo --version | Set-Content -LiteralPath (Join-Path $OutDir "cargo-version.txt") -Encoding UTF8

    $trialPacketDir = Join-Path $OutDir "local-runtime-trial"
    $localRuntimeTrial = Join-Path $repoRoot "scripts\local-runtime-trial.ps1"
    $trialArgs = @(
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        $localRuntimeTrial,
        "-Log",
        $logPath,
        "-OutDir",
        $trialPacketDir,
        "-ResetLog",
        "-Controlled",
        "-TrialFile",
        $copiedTrialFile
    )
    if ($AllowDirty) {
        $trialArgs += "-AllowDirty"
    }

    $global:LASTEXITCODE = 0
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        powershell @trialArgs *> (Join-Path $OutDir "local-runtime-trial.log")
        $trialExitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    if ($trialExitCode -ne 0) {
        throw "local runtime trial failed with exit code $trialExitCode; see local-runtime-trial.log"
    }

    $reviewDir = Join-Path $OutDir "review"
    New-Item -ItemType Directory -Force -Path $reviewDir | Out-Null
    Write-Text -Path (Join-Path $reviewDir "scoring.md") -Value @"
# Controlled Human Trial Scoring

Score each reviewer question before changing the trial file or source prompts.

| Question | Result | Evidence | Notes |
| --- | --- | --- | --- |
$($questions | ForEach-Object { "| $_ | pass / fail / justified unknown | | |" } | Out-String)

Allowed results:

- pass
- fail
- justified unknown

Any fail or unjustified answer must be copied into `regression_backlog.md`.
"@
    Write-Text -Path (Join-Path $reviewDir "misses.md") -Value "# Misses`n`nRecord unsupported answers, stale facts, missing recall, missing reason, and confusing unknowns here.`n"
    Write-Text -Path (Join-Path $reviewDir "regression_backlog.md") -Value "# Regression Backlog`n`nEvery miss from this trial must become a deterministic runtime scenario or an explicit deferred issue.`n"

    $trialFileHash = Get-FileHash -Algorithm SHA256 -LiteralPath $copiedTrialFile
    $eventLogHash = if (Test-Path -LiteralPath $logPath -PathType Leaf) {
        (Get-FileHash -Algorithm SHA256 -LiteralPath $logPath).Hash
    } else {
        $null
    }
    $manifestObject = [ordered]@{
        packet_kind = "luna.controlled_human_trial.v1"
        status = "ready_for_review_not_passed"
        prepared_at = $preparedAt.ToString("o")
        repo = "$repoRoot"
        commit = (Get-Content -LiteralPath (Join-Path $OutDir "commit.txt") -Raw).Trim()
        dirty = $dirty
        allow_dirty = [bool]$AllowDirty
        exact_log_path = $logPath
        event_log_sha256 = $eventLogHash
        trial_file = $copiedTrialFile
        trial_file_sha256 = $trialFileHash.Hash
        source_prompt_boundary = "local-runtime-trial/review/source-prompt-boundary.md"
        questions_lock = "local-runtime-trial/questions-lock.json"
        turn_count = $turns.Count
        reviewer_question_count = $questions.Count
        reviewer_questions = $questions
        local_runtime_trial_packet = $trialPacketDir
        not_passed_reason = "This packet preserves the controlled human trial evidence and review templates. It is not passed until scoring is completed and misses are converted to regression work."
        artifacts = @(
            "manifest.json",
            "manifest.md",
            "trial.json",
            "local-runtime-trial.log",
            "local-runtime-trial/",
            "local-runtime-trial/questions-lock.json",
            "local-runtime-trial/review/source-prompt-boundary.md",
            "review/scoring.md",
            "review/misses.md",
            "review/regression_backlog.md",
            "commit.txt",
            "git-status.txt"
        )
    }
    Write-Json -Path (Join-Path $OutDir "manifest.json") -Value $manifestObject -Depth 8

    Write-Text -Path (Join-Path $OutDir "manifest.md") -Value @"
# Luna Controlled Human Trial Packet

- Status: ready_for_review_not_passed
- Prepared: $($preparedAt.ToString("o"))
- Commit: $((Get-Content -LiteralPath (Join-Path $OutDir "commit.txt") -Raw).Trim())
- Dirty checkout: $dirty
- Exact log path: $logPath
- Event log SHA256: $eventLogHash
- Turn count: $($turns.Count)
- Reviewer question count: $($questions.Count)

## What This Proves

This packet proves Luna can run a reviewer-owned controlled trial through the
local product loop and preserve the evidence needed for review. It does not
prove 24-hour continuity, full-manuscript recall, LLM quality, or v1.0 readiness.

## Review Boundary

Do not edit `trial.json` after reviewing answers. Score in `review/scoring.md`.
Copy every miss into `review/regression_backlog.md` before calling the trial
useful evidence.
"@

    Write-Host "Controlled human trial packet written to $OutDir"
    Write-Host "Status: ready_for_review_not_passed"
} finally {
    Pop-Location
}

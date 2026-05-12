[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory = $true)]
    [string] $Log,
    [Parameter(Mandatory = $true)]
    [string] $TrialFile,
    [string] $OutDir,
    [ValidateRange(24, 2147483647)]
    [int] $MinimumGapHours = 24,
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

    return $null
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
        $OutDir = Join-Path $repoRoot ".luna\marathon-ready\$stamp"
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
    $turns = @($trial.turns | ForEach-Object { [string]$_ } | Where-Object { $_.Trim().Length -gt 0 })
    $questions = @($trial.questions | ForEach-Object { [string]$_ } | Where-Object { $_.Trim().Length -gt 0 })
    if ($turns.Count -lt 10) {
        throw "Marathon Ready requires at least 10 scripted start turns; found $($turns.Count)."
    }
    if ($questions.Count -lt 3) {
        throw "Marathon Ready requires at least 3 reviewer-owned questions; found $($questions.Count)."
    }

    $copiedTrialFile = Join-Path $OutDir "trial.json"
    Copy-Item -LiteralPath $trialFilePath -Destination $copiedTrialFile -Force

    git rev-parse HEAD | Set-Content -LiteralPath (Join-Path $OutDir "commit.txt") -Encoding UTF8
    git status --short --branch | Set-Content -LiteralPath (Join-Path $OutDir "git-status.txt") -Encoding UTF8
    rustc --version | Set-Content -LiteralPath (Join-Path $OutDir "rustc-version.txt") -Encoding UTF8
    cargo --version | Set-Content -LiteralPath (Join-Path $OutDir "cargo-version.txt") -Encoding UTF8

    $luna = Resolve-LunaBinary
    $lunaInvocation = if ($luna) {
        "& " + (Quote-PowerShellArg $luna)
    } else {
        "cargo run -p luna-cli --"
    }

    $rehearsalDir = Join-Path $OutDir "local-runtime-rehearsal"
    $rehearsalLog = Join-Path $OutDir "local-runtime-rehearsal-events.jsonl"
    $localRuntimeTrial = Join-Path $repoRoot "scripts\local-runtime-trial.ps1"
    $rehearsalStartedAt = Get-Date
    Invoke-Captured "local runtime marathon rehearsal" (Join-Path $OutDir "local-runtime-rehearsal.log") {
        $trialArgs = @(
            "-ExecutionPolicy", "Bypass",
            "-File", $localRuntimeTrial,
            "-Log", $rehearsalLog,
            "-OutDir", $rehearsalDir,
            "-ResetLog",
            "-TrialFile", $copiedTrialFile
        )
        if ($AllowDirty) {
            $trialArgs += "-AllowDirty"
        }
        powershell @trialArgs
    }
    $rehearsalCompletedAt = Get-Date

    $closeRecordedAt = $rehearsalCompletedAt
    $reopenNotBeforeAt = $closeRecordedAt.AddHours($MinimumGapHours)

    $startScript = @"
param(
    [switch] `$ResetLog,
    [switch] `$AllowDirty
)

`$ErrorActionPreference = "Stop"
`$repoRoot = $(Quote-PowerShellArg "$repoRoot")
`$logPath = $(Quote-PowerShellArg $logPath)
`$trialFile = Join-Path `$PSScriptRoot "trial.json"
`$startOut = Join-Path `$PSScriptRoot "start-evidence"
New-Item -ItemType Directory -Force -Path `$startOut | Out-Null
Push-Location -LiteralPath `$repoRoot
try {
    if ((git status --porcelain) -and -not `$AllowDirty) {
        throw "Working tree is dirty. Commit first, or rerun with -AllowDirty."
    }
    if ((Test-Path -LiteralPath `$logPath) -and -not `$ResetLog) {
        throw "Marathon log already exists at `$logPath. Rerun with -ResetLog only when starting a new trial."
    }
    if (`$ResetLog) {
        Remove-Item -LiteralPath `$logPath -Force -ErrorAction SilentlyContinue
    }
    `$turns = @((Get-Content -LiteralPath `$trialFile -Raw | ConvertFrom-Json).turns)
    `$startedAt = Get-Date
    `$startedAt.ToString("o") | Set-Content -LiteralPath (Join-Path `$startOut "started-at.txt") -Encoding UTF8
    for (`$i = 0; `$i -lt `$turns.Count; `$i++) {
        `$number = "{0:D2}" -f (`$i + 1)
        `$turn = [string]`$turns[`$i]
        $(if ($luna) { $lunaInvocation } else { $lunaInvocation }) runtime turn `$turn --log `$logPath --format markdown *> (Join-Path `$startOut "`$number-turn.md")
        if (`$LASTEXITCODE -ne 0) { throw "turn `$number failed" }
    }
    $(if ($luna) { $lunaInvocation } else { $lunaInvocation }) runtime inspect --log `$logPath --format markdown *> (Join-Path `$startOut "inspect-after-start.md")
    if (`$LASTEXITCODE -ne 0) { throw "inspect after start failed" }
    $(if ($luna) { $lunaInvocation } else { $lunaInvocation }) runtime audit --log `$logPath --format markdown *> (Join-Path `$startOut "audit-after-start.md")
    if (`$LASTEXITCODE -ne 0) { throw "audit after start failed" }
    `$closedAt = Get-Date
    `$closedAt.ToString("o") | Set-Content -LiteralPath (Join-Path `$startOut "closed-at.txt") -Encoding UTF8
    `$closedAt.AddHours($MinimumGapHours).ToString("o") | Set-Content -LiteralPath (Join-Path `$startOut "reopen-not-before-at.txt") -Encoding UTF8
    Copy-Item -LiteralPath `$logPath -Destination (Join-Path `$startOut "events-after-start.jsonl") -Force
    (Get-FileHash -Algorithm SHA256 -LiteralPath `$logPath).Hash | Set-Content -LiteralPath (Join-Path `$startOut "events-after-start.sha256.txt") -Encoding UTF8
} finally {
    Pop-Location
}
"@
    Write-Text -Path (Join-Path $OutDir "start-marathon.ps1") -Value $startScript

    $reopenScript = @"
param(
)

`$ErrorActionPreference = "Stop"
`$repoRoot = $(Quote-PowerShellArg "$repoRoot")
`$logPath = $(Quote-PowerShellArg $logPath)
`$trialFile = Join-Path `$PSScriptRoot "trial.json"
`$startOut = Join-Path `$PSScriptRoot "start-evidence"
`$reopenOut = Join-Path `$PSScriptRoot "reopen-evidence"
New-Item -ItemType Directory -Force -Path `$reopenOut | Out-Null
Push-Location -LiteralPath `$repoRoot
try {
    `$closedAtPath = Join-Path `$startOut "closed-at.txt"
    if (-not (Test-Path -LiteralPath `$closedAtPath)) {
        throw "Missing `$closedAtPath. Run start-marathon.ps1 first."
    }
    `$closedAt = [DateTimeOffset]::Parse((Get-Content -LiteralPath `$closedAtPath -Raw).Trim())
    `$reopenNotBeforeAt = `$closedAt.AddHours($MinimumGapHours)
    `$now = Get-Date
    if (`$now -lt `$reopenNotBeforeAt.LocalDateTime) {
        throw "Too early to reopen. Reopen no earlier than `$(`$reopenNotBeforeAt.ToString("o"))."
    }
    `$expectedHashPath = Join-Path `$startOut "events-after-start.sha256.txt"
    if (-not (Test-Path -LiteralPath `$expectedHashPath)) {
        throw "Missing `$expectedHashPath. Run start-marathon.ps1 first."
    }
    `$expectedHash = (Get-Content -LiteralPath `$expectedHashPath -Raw).Trim()
    `$currentHash = (Get-FileHash -Algorithm SHA256 -LiteralPath `$logPath).Hash
    `$currentHash | Set-Content -LiteralPath (Join-Path `$reopenOut "events-before-questions.sha256.txt") -Encoding UTF8
    if (`$currentHash -ne `$expectedHash) {
        throw "Marathon log changed between close and reopen. Expected `$expectedHash but found `$currentHash."
    }
    `$now.ToString("o") | Set-Content -LiteralPath (Join-Path `$reopenOut "reopened-at.txt") -Encoding UTF8
    $(if ($luna) { $lunaInvocation } else { $lunaInvocation }) runtime inspect --log `$logPath --format markdown *> (Join-Path `$reopenOut "inspect-before-questions.md")
    if (`$LASTEXITCODE -ne 0) { throw "inspect before questions failed" }
    $(if ($luna) { $lunaInvocation } else { $lunaInvocation }) runtime audit --log `$logPath --format markdown *> (Join-Path `$reopenOut "audit-before-questions.md")
    if (`$LASTEXITCODE -ne 0) { throw "audit before questions failed" }
    `$questions = @((Get-Content -LiteralPath `$trialFile -Raw | ConvertFrom-Json).questions)
    for (`$i = 0; `$i -lt `$questions.Count; `$i++) {
        `$number = "{0:D2}" -f (`$i + 1)
        `$question = [string]`$questions[`$i]
        $(if ($luna) { $lunaInvocation } else { $lunaInvocation }) runtime turn `$question --log `$logPath --format markdown *> (Join-Path `$reopenOut "question-`$number.md")
        if (`$LASTEXITCODE -ne 0) { throw "question `$number failed" }
    }
    $(if ($luna) { $lunaInvocation } else { $lunaInvocation }) runtime inspect --log `$logPath --format json *> (Join-Path `$reopenOut "inspect-final.json")
    if (`$LASTEXITCODE -ne 0) { throw "final inspect failed" }
    $(if ($luna) { $lunaInvocation } else { $lunaInvocation }) runtime audit --log `$logPath --format json *> (Join-Path `$reopenOut "audit-final.json")
    if (`$LASTEXITCODE -ne 0) { throw "final audit failed" }
    Copy-Item -LiteralPath `$logPath -Destination (Join-Path `$reopenOut "events-final.jsonl") -Force
    Get-FileHash -Algorithm SHA256 -LiteralPath `$logPath | Format-List | Out-String | Set-Content -LiteralPath (Join-Path `$reopenOut "events-final.sha256.txt") -Encoding UTF8
} finally {
    Pop-Location
}
"@
    Write-Text -Path (Join-Path $OutDir "reopen-after-24h.ps1") -Value $reopenScript

    $trialFileHash = Get-FileHash -Algorithm SHA256 -LiteralPath $copiedTrialFile
    $rehearsalManifest = Join-Path $rehearsalDir "manifest.json"
    $manifestObject = [ordered]@{
        status = "eligible_to_run_not_passed"
        prepared_at = $preparedAt.ToString("o")
        local_rehearsal_started_at = $rehearsalStartedAt.ToString("o")
        local_rehearsal_completed_at = $rehearsalCompletedAt.ToString("o")
        recorded_close_at = $closeRecordedAt.ToString("o")
        reopen_not_before_at = $reopenNotBeforeAt.ToString("o")
        actual_reopen_at = $null
        minimum_gap_hours = $MinimumGapHours
        repo = "$repoRoot"
        commit = (Get-Content -LiteralPath (Join-Path $OutDir "commit.txt") -Raw).Trim()
        dirty = $dirty
        allow_dirty = [bool]$AllowDirty
        exact_marathon_log_path = $logPath
        trial_file = $copiedTrialFile
        trial_file_sha256 = $trialFileHash.Hash
        turn_count = $turns.Count
        reviewer_question_count = $questions.Count
        reviewer_questions = $questions
        local_rehearsal_packet = $rehearsalDir
        local_rehearsal_manifest = if (Test-Path -LiteralPath $rehearsalManifest) { $rehearsalManifest } else { $null }
        not_passed_reason = "This packet prepares and rehearses the marathon command path. It does not include a real 24-hour gap or reopened reviewer-question answers."
        artifacts = @(
            "manifest.json",
            "manifest.md",
            "trial.json",
            "start-marathon.ps1",
            "reopen-after-24h.ps1",
            "local-runtime-rehearsal.log",
            "local-runtime-rehearsal/",
            "commit.txt",
            "git-status.txt",
            "rustc-version.txt",
            "cargo-version.txt"
        )
    }
    $manifestObject | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $OutDir "manifest.json") -Encoding UTF8

    $manifest = @"
# Luna Marathon Ready Packet

- Status: eligible_to_run_not_passed
- Prepared: $($preparedAt.ToString("o"))
- Ready-packet close timestamp: $($closeRecordedAt.ToString("o"))
- Earliest eligible reopen if this were the actual close: $($reopenNotBeforeAt.ToString("o"))
- Actual marathon reopen timestamp: not recorded yet
- Exact marathon log path: $logPath
- Commit: $((Get-Content -LiteralPath (Join-Path $OutDir "commit.txt") -Raw).Trim())
- Turn count: $($turns.Count)
- Reviewer question count: $($questions.Count)

## What This Proves

This packet proves that the repo can generate a Marathon Ready command packet and
that the same trial inputs can run through the local runtime trial harness now.
It does not prove the 10-turn / 24-hour / 3-question marathon has passed.

## How To Run The Actual Marathon

1. Run `start-marathon.ps1 -ResetLog` from this packet. It records the actual
   `started-at.txt`, `closed-at.txt`, and `reopen-not-before-at.txt` timestamps.
2. Close Luna and wait until the recorded `reopen-not-before-at.txt` time.
3. Run `reopen-after-24h.ps1`.
4. Review `reopen-evidence/audit-final.json`, `reopen-evidence/inspect-final.json`,
   and every `reopen-evidence/question-*.md` transcript.

## Reviewer Questions

$($questions | ForEach-Object { "- $_" } | Out-String)

## Evidence

- trial.json: reviewer-authored turns and questions.
- start-marathon.ps1: actual start/close command script.
- reopen-after-24h.ps1: actual reopen/question/audit command script.
- local-runtime-rehearsal/: immediate non-marathon rehearsal packet.
- manifest.json: machine-readable timestamps, log path, questions, and status.
"@
    Write-Text -Path (Join-Path $OutDir "manifest.md") -Value $manifest

    Write-Host ""
    Write-Host "Marathon Ready packet written to $OutDir"
    Write-Host "Status: eligible_to_run_not_passed"
    Write-Host "Reopen no earlier than: $($reopenNotBeforeAt.ToString("o"))"
} finally {
    Pop-Location
}

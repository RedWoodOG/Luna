[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory = $true)]
    [string] $Log,
    [string] $OutDir,
    [string] $TrialFile,
    [string[]] $Turn = @(),
    [string] $TurnsFile,
    [string[]] $Question = @(),
    [string] $QuestionsFile,
    [switch] $Live,
    [switch] $Controlled,
    [switch] $ResetLog,
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
        [AllowEmptyString()]
        [string] $Value
    )

    Set-Content -LiteralPath $Path -Value $Value -Encoding UTF8
}

function Read-StringListFile {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path,
        [Parameter(Mandatory = $true)]
        [string] $PropertyName
    )

    $resolved = Resolve-RepoPath $Path
    $raw = Get-Content -LiteralPath $resolved -Raw
    $json = $raw | ConvertFrom-Json
    $items = if ($json -is [array]) {
        @($json)
    } elseif ($null -ne $json.$PropertyName) {
        @($json.$PropertyName)
    } else {
        throw "$Path must be a JSON array or an object with a '$PropertyName' array."
    }

    return @($items | ForEach-Object { [string]$_ } | Where-Object { $_.Trim().Length -gt 0 })
}

function Read-TrialFile {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    $resolved = Resolve-RepoPath $Path
    $raw = Get-Content -LiteralPath $resolved -Raw
    $json = $raw | ConvertFrom-Json
    if ($null -eq $json.turns -or $null -eq $json.questions) {
        throw "$Path must be a JSON object with 'turns' and 'questions' arrays."
    }

    return [pscustomobject][ordered]@{
        path = $resolved
        raw = $json
        turns = @($json.turns | ForEach-Object { [string]$_ } | Where-Object { $_.Trim().Length -gt 0 })
        raw_questions = @($json.questions)
    }
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

function New-QuestionSpec {
    param(
        [Parameter(Mandatory = $true)]
        [object] $Item,
        [Parameter(Mandatory = $true)]
        [int] $Index
    )

    $id = "q{0:D3}" -f $Index
    $category = ""
    $expectedEvidence = ""
    $mustNotInclude = ""
    $notes = ""

    if ($Item -is [string]) {
        $questionText = [string]$Item
    } else {
        $questionText = [string](Get-ObjectProperty -Object $Item -Name "question")
        $configuredId = Get-ObjectProperty -Object $Item -Name "id"
        if ($configuredId) {
            $id = [string]$configuredId
        }
        $category = [string](Get-ObjectProperty -Object $Item -Name "category")
        $expectedEvidence = [string](Get-ObjectProperty -Object $Item -Name "expected_evidence")
        $mustNotInclude = [string](Get-ObjectProperty -Object $Item -Name "must_not_include")
        $notes = [string](Get-ObjectProperty -Object $Item -Name "notes")
    }

    if ([string]::IsNullOrWhiteSpace($questionText)) {
        throw "Question $Index is empty. Questions may be strings or objects with a non-empty 'question' field."
    }

    return [pscustomobject][ordered]@{
        id = $id
        question = $questionText
        category = $category
        expected_evidence = $expectedEvidence
        must_not_include = $mustNotInclude
        notes = $notes
    }
}

function Get-StringArrayProperty {
    param(
        [object] $Object,
        [string] $Name
    )

    $value = Get-ObjectProperty -Object $Object -Name $Name
    if ($null -eq $value) {
        return @()
    }
    return @($value | ForEach-Object { [string]$_ } | Where-Object { $_.Trim().Length -gt 0 })
}

function Assert-ControlledTrial {
    param(
        [Parameter(Mandatory = $true)]
        [object] $TrialJson,
        [Parameter(Mandatory = $true)]
        [string[]] $Turns,
        [Parameter(Mandatory = $true)]
        [object[]] $QuestionSpecs
    )

    if ($null -eq (Get-ObjectProperty -Object $TrialJson -Name "source_boundary")) {
        throw "Controlled trials require a 'source_boundary' object in the trial JSON."
    }
    if ($null -eq (Get-ObjectProperty -Object $TrialJson -Name "prompt_boundary")) {
        throw "Controlled trials require a 'prompt_boundary' object in the trial JSON."
    }
    if ($null -eq (Get-ObjectProperty -Object $TrialJson -Name "scoring")) {
        throw "Controlled trials require a 'scoring' object in the trial JSON."
    }
    if ($null -eq (Get-ObjectProperty -Object $TrialJson -Name "regression_capture")) {
        throw "Controlled trials require a 'regression_capture' object in the trial JSON."
    }

    $answerLeakProperties = @("answer", "expected_answer", "gold_answer", "correct_answer", "expected_output", "model_answer")
    $rawQuestions = @((Get-ObjectProperty -Object $TrialJson -Name "questions"))
    for ($index = 0; $index -lt $rawQuestions.Count; $index++) {
        $rawQuestion = $rawQuestions[$index]
        if ($rawQuestion -isnot [string]) {
            foreach ($propertyName in $answerLeakProperties) {
                $value = Get-ObjectProperty -Object $rawQuestion -Name $propertyName
                if ($null -ne $value -and ([string]$value).Trim().Length -gt 0) {
                    throw "Controlled question $($index + 1) contains '$propertyName'. Put answers only in review/scoring.md after Luna answers."
                }
            }
        }
    }

    $defaultForbiddenTerms = @(
        "according to the source",
        "from the source packet",
        "look at the source",
        "open the source",
        "reread",
        "re-read",
        "search the source",
        "source text:",
        "use the source"
    )
    $sourceBoundary = Get-ObjectProperty -Object $TrialJson -Name "source_boundary"
    $promptBoundary = Get-ObjectProperty -Object $TrialJson -Name "prompt_boundary"
    $configuredForbiddenTerms = @()
    $configuredForbiddenTerms += Get-StringArrayProperty -Object $sourceBoundary -Name "forbidden_question_terms"
    $configuredForbiddenTerms += Get-StringArrayProperty -Object $promptBoundary -Name "forbidden_question_terms"
    $forbiddenTerms = @($defaultForbiddenTerms + $configuredForbiddenTerms |
        ForEach-Object { [string]$_ } |
        Where-Object { $_.Trim().Length -gt 0 } |
        Select-Object -Unique)

    foreach ($questionSpec in $QuestionSpecs) {
        $questionText = [string]$questionSpec.question
        foreach ($term in $forbiddenTerms) {
            if ($questionText.IndexOf($term, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
                throw "Controlled question '$($questionSpec.id)' contains forbidden source/prompt term '$term'."
            }
        }
        foreach ($turn in $Turns) {
            $normalizedTurn = ([string]$turn).Trim()
            if ($normalizedTurn.Length -ge 24 -and
                $questionText.IndexOf($normalizedTurn, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
                throw "Controlled question '$($questionSpec.id)' repeats a source turn. Ask from memory without copying source text."
            }
        }
    }
}

function Write-ReviewTemplates {
    param(
        [Parameter(Mandatory = $true)]
        [string] $ReviewDir,
        [Parameter(Mandatory = $true)]
        [object] $TrialJson,
        [Parameter(Mandatory = $true)]
        [object[]] $QuestionSpecs
    )

    New-Item -ItemType Directory -Force -Path $ReviewDir | Out-Null

    $sourceBoundaryJson = (Get-ObjectProperty -Object $TrialJson -Name "source_boundary") | ConvertTo-Json -Depth 8
    $promptBoundaryJson = (Get-ObjectProperty -Object $TrialJson -Name "prompt_boundary") | ConvertTo-Json -Depth 8
    $scoringJson = (Get-ObjectProperty -Object $TrialJson -Name "scoring") | ConvertTo-Json -Depth 8
    $regressionJson = (Get-ObjectProperty -Object $TrialJson -Name "regression_capture") | ConvertTo-Json -Depth 8

    $newline = [Environment]::NewLine
    $boundary = @(
        "# Source And Prompt Boundary",
        "",
        "This packet is a first human controlled runtime trial, not a 24-hour marathon",
        "result and not a manuscript one-read proof.",
        "",
        "## Source Boundary",
        "",
        '```json',
        $sourceBoundaryJson,
        '```',
        "",
        "## Prompt Boundary",
        "",
        '```json',
        $promptBoundaryJson,
        '```',
        "",
        "The reviewer questions were locked in `questions-lock.json` before Luna answers",
        "were generated. Question prompts must not include source text, reread/search",
        "instructions, or answer hints.",
        "",
        "## Scoring Contract",
        "",
        '```json',
        $scoringJson,
        '```',
        "",
        "## Regression Capture Contract",
        "",
        '```json',
        $regressionJson,
        '```',
        ""
    ) -join $newline
    Write-Text -Path (Join-Path $ReviewDir "source-prompt-boundary.md") -Value $boundary

    $scoringRows = New-Object System.Collections.Generic.List[string]
    $scoringRows.Add("# Scoring")
    $scoringRows.Add("")
    $scoringRows.Add("Score only after all `question-*.md` answer files exist.")
    $scoringRows.Add("")
    $scoringRows.Add("| Question id | Answer artifact | Score | Evidence checked | Notes |")
    $scoringRows.Add("|-------------|-----------------|-------|------------------|-------|")
    for ($index = 0; $index -lt $QuestionSpecs.Count; $index++) {
        $questionNumber = "{0:D2}" -f ($index + 1)
        $scoringRows.Add("| $($QuestionSpecs[$index].id) | `question-$questionNumber.md` | unreviewed |  |  |")
    }
    $scoringRows.Add("")
    $scoringRows.Add("Allowed scores: pass, partial, fail, justified_unknown, boundary_violation.")
    $scoringRows.Add("Every partial, fail, invented detail, stale fact, unsupported answer, or boundary violation must appear in `regression_backlog.md`.")
    Write-Text -Path (Join-Path $ReviewDir "scoring.md") -Value (($scoringRows.ToArray() -join $newline) + $newline)

    $regressions = @(
        "# Regression Backlog",
        "",
        "Add one row for every miss, invented detail, stale fact, unsupported answer,",
        "working-memory boundary failure, audit failure, or prompt-boundary violation.",
        "",
        "| ID | Question id | Failure type | Reproduction command | Proposed deterministic scenario/test | Owner/status |",
        "|----|-------------|--------------|----------------------|--------------------------------------|--------------|",
        ""
    ) -join $newline
    Write-Text -Path (Join-Path $ReviewDir "regression_backlog.md") -Value $regressions
}

function Read-LiveList {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Prompt
    )

    $items = @()
    while ($true) {
        $value = Read-Host $Prompt
        if ([string]::IsNullOrWhiteSpace($value)) {
            break
        }
        $items += $value
    }
    return $items
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

function Invoke-Luna {
    param(
        [Parameter(Mandatory = $true)]
        [string[]] $Arguments
    )

    if ($script:lunaBinary) {
        & $script:lunaBinary @Arguments
    } else {
        cargo run -p luna-cli -- @Arguments
    }
}

function Luna-CommandLine {
    if ($script:lunaBinary) {
        return "& " + (Quote-PowerShellArg $script:lunaBinary)
    }

    return "cargo run -p luna-cli --"
}

Push-Location -LiteralPath $repoRoot
try {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $logPath = Resolve-RepoPath $Log
    if (-not $OutDir) {
        $OutDir = Join-Path $repoRoot ".luna\local-runtime-trial\$stamp"
    }
    $OutDir = [System.IO.Path]::GetFullPath($OutDir)
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

    if ($Controlled) {
        if (-not $TrialFile) {
            throw "Controlled trials require -TrialFile so the source boundary, prompt boundary, questions, scoring, and regression contract are archived together."
        }
        if ($Live -or $QuestionsFile -or $Question.Count -gt 0 -or $TurnsFile -or $Turn.Count -gt 0) {
            throw "Controlled trials must take all turns and questions from -TrialFile. Do not mix -Live, -Turn, -TurnsFile, -Question, or -QuestionsFile."
        }
    }

    $porcelain = git status --porcelain
    $dirty = [bool]$porcelain
    if ($dirty -and -not $AllowDirty) {
        $porcelain | Set-Content -LiteralPath (Join-Path $OutDir "dirty-status.txt") -Encoding UTF8
        throw "Working tree is dirty. Commit first, or rerun with -AllowDirty to archive source diffs in the packet."
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

    $turns = @()
    $rawQuestions = @()
    $trial = $null
    if ($TrialFile) {
        $trial = Read-TrialFile -Path $TrialFile
        $turns += $trial.turns
        $rawQuestions += $trial.raw_questions
    }
    if ($TurnsFile) {
        $turns += Read-StringListFile -Path $TurnsFile -PropertyName "turns"
    }
    $turns += @($Turn | Where-Object { $_.Trim().Length -gt 0 })

    if ($QuestionsFile) {
        $rawQuestions += Read-StringListFile -Path $QuestionsFile -PropertyName "questions"
    }
    $rawQuestions += @($Question | Where-Object { $_.Trim().Length -gt 0 })

    if ($Live) {
        Write-Host "Enter local-trial turns. Submit a blank line when done."
        $turns += Read-LiveList -Prompt "turn"
        Write-Host "Enter reviewer-owned questions. Submit a blank line when done."
        $rawQuestions += Read-LiveList -Prompt "question"
    }

    $questionSpecs = @()
    for ($index = 0; $index -lt $rawQuestions.Count; $index++) {
        $questionSpecs += New-QuestionSpec -Item $rawQuestions[$index] -Index ($index + 1)
    }
    $questions = @($questionSpecs | ForEach-Object { [string]$_.question })

    if ($turns.Count -lt 1) {
        throw "At least one scripted or live turn is required. Use -Turn, -TurnsFile, or -Live."
    }
    if ($questions.Count -lt 1) {
        throw "At least one reviewer-owned question is required. Use -Question, -QuestionsFile, or -Live."
    }
    if ($Controlled) {
        Assert-ControlledTrial -TrialJson $trial.raw -Turns $turns -QuestionSpecs $questionSpecs
    }

    if (Test-Path -LiteralPath $logPath) {
        if ($ResetLog) {
            Remove-Item -LiteralPath $logPath -Force
        } else {
            throw "Log already exists at $logPath. Rerun with -ResetLog or choose a new -Log path."
        }
    }
    $logParent = Split-Path -Parent $logPath
    if ($logParent) {
        New-Item -ItemType Directory -Force -Path $logParent | Out-Null
    }

    $script:lunaBinary = Resolve-LunaBinary
    $lunaPrefix = Luna-CommandLine

    git rev-parse HEAD | Set-Content -LiteralPath (Join-Path $OutDir "commit.txt") -Encoding UTF8
    git status --short --branch | Set-Content -LiteralPath (Join-Path $OutDir "git-status.txt") -Encoding UTF8
    rustc --version | Set-Content -LiteralPath (Join-Path $OutDir "rustc-version.txt") -Encoding UTF8
    cargo --version | Set-Content -LiteralPath (Join-Path $OutDir "cargo-version.txt") -Encoding UTF8

    $inputObject = [ordered]@{
        log = $logPath
        reset_log = [bool]$ResetLog
        controlled = [bool]$Controlled
        turns = $turns
        questions = $questionSpecs
        live = [bool]$Live
        dirty = $dirty
        allow_dirty = [bool]$AllowDirty
    }
    $inputObject | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutDir "trial-inputs.json") -Encoding UTF8

    if ($TrialFile) {
        Copy-Item -LiteralPath $trial.path -Destination (Join-Path $OutDir "trial-source.json") -Force
        Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $OutDir "trial-source.json") |
            Format-List |
            Out-String |
            Set-Content -LiteralPath (Join-Path $OutDir "trial-source.sha256.txt") -Encoding UTF8
    }
    $questionLock = [ordered]@{
        created = (Get-Date -Format o)
        controlled = [bool]$Controlled
        questions_locked_before_answers = $true
        questions = $questionSpecs
    }
    $questionLockPath = Join-Path $OutDir "questions-lock.json"
    $questionLock | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $questionLockPath -Encoding UTF8
    Get-FileHash -Algorithm SHA256 -LiteralPath $questionLockPath |
        Format-List |
        Out-String |
        Set-Content -LiteralPath (Join-Path $OutDir "questions-lock.sha256.txt") -Encoding UTF8
    if ($Controlled) {
        Write-ReviewTemplates -ReviewDir (Join-Path $OutDir "review") -TrialJson $trial.raw -QuestionSpecs $questionSpecs
    }

    $commandLines = New-Object System.Collections.Generic.List[string]
    $commandLines.Add("# Luna local runtime trial replay commands")
    $commandLines.Add("cd " + (Quote-PowerShellArg "$repoRoot"))
    if ($ResetLog) {
        $commandLines.Add("Remove-Item -LiteralPath " + (Quote-PowerShellArg $logPath) + " -Force -ErrorAction SilentlyContinue")
    }

    for ($index = 0; $index -lt $turns.Count; $index++) {
        $number = "{0:D2}" -f ($index + 1)
        $output = Join-Path $OutDir "$number-turn.md"
        $turnText = $turns[$index]
        $commandLines.Add("$lunaPrefix runtime turn $(Quote-PowerShellArg $turnText) --log $(Quote-PowerShellArg $logPath) --format markdown")
        Invoke-Captured "trial turn $number" $output {
            Invoke-Luna -Arguments @("runtime", "turn", $turnText, "--log", $logPath, "--format", "markdown")
        }
    }

    $commandLines.Add("$lunaPrefix runtime inspect --log $(Quote-PowerShellArg $logPath) --format markdown")
    Invoke-Captured "trial inspect after turns" (Join-Path $OutDir "inspect-after-turns.md") {
        Invoke-Luna -Arguments @("runtime", "inspect", "--log", $logPath, "--format", "markdown")
    }
    $commandLines.Add("$lunaPrefix runtime audit --log $(Quote-PowerShellArg $logPath) --format markdown")
    Invoke-Captured "trial audit after turns" (Join-Path $OutDir "audit-after-turns.md") {
        Invoke-Luna -Arguments @("runtime", "audit", "--log", $logPath, "--format", "markdown")
    }

    for ($index = 0; $index -lt $questions.Count; $index++) {
        $number = "{0:D2}" -f ($index + 1)
        $output = Join-Path $OutDir "question-$number.md"
        $questionText = $questions[$index]
        $commandLines.Add("$lunaPrefix runtime turn $(Quote-PowerShellArg $questionText) --log $(Quote-PowerShellArg $logPath) --format markdown")
        Invoke-Captured "reviewer question $number" $output {
            Invoke-Luna -Arguments @("runtime", "turn", $questionText, "--log", $logPath, "--format", "markdown")
        }
    }

    $commandLines.Add("$lunaPrefix runtime inspect --log $(Quote-PowerShellArg $logPath) --format markdown")
    Invoke-Captured "trial final inspect" (Join-Path $OutDir "inspect-final.md") {
        Invoke-Luna -Arguments @("runtime", "inspect", "--log", $logPath, "--format", "markdown")
    }
    $commandLines.Add("$lunaPrefix runtime inspect --log $(Quote-PowerShellArg $logPath) --format json")
    Invoke-Captured "trial final inspect JSON" (Join-Path $OutDir "inspect-final.json") {
        Invoke-Luna -Arguments @("runtime", "inspect", "--log", $logPath, "--format", "json")
    }
    $commandLines.Add("$lunaPrefix runtime audit --log $(Quote-PowerShellArg $logPath) --format markdown")
    Invoke-Captured "trial final audit" (Join-Path $OutDir "audit-final.md") {
        Invoke-Luna -Arguments @("runtime", "audit", "--log", $logPath, "--format", "markdown")
    }
    $commandLines.Add("$lunaPrefix runtime audit --log $(Quote-PowerShellArg $logPath) --format json")
    Invoke-Captured "trial final audit JSON" (Join-Path $OutDir "audit-final.json") {
        Invoke-Luna -Arguments @("runtime", "audit", "--log", $logPath, "--format", "json")
    }

    if (-not (Test-Path -LiteralPath $logPath)) {
        throw "Trial did not produce an event log at $logPath."
    }
    if ((Get-Item -LiteralPath $logPath).Length -le 0) {
        throw "Trial produced an empty event log at $logPath."
    }

    $packetLog = Join-Path $OutDir "events.jsonl"
    Copy-Item -LiteralPath $logPath -Destination $packetLog -Force
    $logHash = Get-FileHash -Algorithm SHA256 -LiteralPath $logPath
    $packetLogHash = Get-FileHash -Algorithm SHA256 -LiteralPath $packetLog
    $logHash | Format-List | Out-String | Set-Content -LiteralPath (Join-Path $OutDir "events.sha256.txt") -Encoding UTF8
    $commandLines | Set-Content -LiteralPath (Join-Path $OutDir "commands.ps1") -Encoding UTF8

    $artifactNames = @(
        "commands.ps1",
        "trial-inputs.json",
        "questions-lock.json",
        "questions-lock.sha256.txt",
        "commit.txt",
        "git-status.txt",
        "rustc-version.txt",
        "cargo-version.txt",
        "events.jsonl",
        "events.sha256.txt"
    )
    if ($TrialFile) {
        $artifactNames += @(
            "trial-source.json",
            "trial-source.sha256.txt"
        )
    }
    for ($index = 0; $index -lt $turns.Count; $index++) {
        $artifactNames += ("{0:D2}-turn.md" -f ($index + 1))
    }
    $artifactNames += @(
        "inspect-after-turns.md",
        "audit-after-turns.md"
    )
    for ($index = 0; $index -lt $questions.Count; $index++) {
        $artifactNames += ("question-{0:D2}.md" -f ($index + 1))
    }
    $artifactNames += @(
        "inspect-final.md",
        "inspect-final.json",
        "audit-final.md",
        "audit-final.json"
    )
    if ($Controlled) {
        $artifactNames += @(
            "review/source-prompt-boundary.md",
            "review/scoring.md",
            "review/regression_backlog.md"
        )
    }
    if ($dirty) {
        $artifactNames += @(
            "dirty-status.txt",
            "source.unstaged.patch",
            "source.staged.patch",
            "source.unstaged.name-status.txt",
            "source.staged.name-status.txt",
            "untracked-files.txt",
            "untracked/"
        )
    }

    $manifestObject = [ordered]@{
        created = (Get-Date -Format o)
        repo = "$repoRoot"
        commit = (Get-Content -LiteralPath (Join-Path $OutDir "commit.txt") -Raw).Trim()
        dirty = $dirty
        allow_dirty = [bool]$AllowDirty
        controlled = [bool]$Controlled
        questions_locked_before_answers = $true
        log = $logPath
        log_sha256 = $logHash.Hash
        packet_log = $packetLog
        packet_log_sha256 = $packetLogHash.Hash
        luna_binary = if ($script:lunaBinary) { $script:lunaBinary } else { "cargo run -p luna-cli --" }
        turn_count = $turns.Count
        reviewer_question_count = $questions.Count
        scoring_template = if ($Controlled) { "review/scoring.md" } else { $null }
        regression_backlog = if ($Controlled) { "review/regression_backlog.md" } else { $null }
        artifacts = $artifactNames
    }
    $manifestObject | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutDir "manifest.json") -Encoding UTF8

    $manifest = @"
# Luna Local Runtime Trial Packet

- Created: $(Get-Date -Format o)
- Repo: $repoRoot
- Commit: $((Get-Content -LiteralPath (Join-Path $OutDir "commit.txt") -Raw).Trim())
- Source log: $logPath
- Source log SHA256: $($logHash.Hash)
- Packet log: $packetLog
- Packet log SHA256: $($packetLogHash.Hash)
- Turn count: $($turns.Count)
- Reviewer question count: $($questions.Count)
- Controlled human-trial boundary: $([bool]$Controlled)
- Questions locked before answers: true
- Dirty checkout: $dirty

## Evidence

- commands.ps1: replay commands for the trial.
- trial-inputs.json: exact scripted turns and reviewer-owned questions.
- questions-lock.json and questions-lock.sha256.txt: reviewer-owned questions
  archived before answer generation.
- events.jsonl: copied event log captured after the trial.
- inspect-after-turns.md and audit-after-turns.md: reopened-log checks before reviewer questions.
- question-*.md: reviewer-owned question transcripts.
- inspect-final.md/json and audit-final.md/json: final replay and memory-state evidence.
- review/source-prompt-boundary.md, review/scoring.md, and
  review/regression_backlog.md: controlled-review artifacts when `-Controlled`
  is used.
- dirty-status.txt, source.* patches, and untracked/: source snapshot evidence
  when `-AllowDirty` is used.

This is a local, non-marathon readiness trial. It does not prove a 24-hour gap,
full-manuscript one-read continuity, or broad LLM extraction quality.
"@
    Write-Text -Path (Join-Path $OutDir "manifest.md") -Value $manifest

    Write-Host ""
    Write-Host "Local runtime trial packet written to $OutDir"
} finally {
    Pop-Location
}

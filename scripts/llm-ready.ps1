[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory = $true)]
    [string] $Corpus,
    [Parameter(Mandatory = $true)]
    [string] $ModelId,
    [Parameter(Mandatory = $true)]
    [string] $ExtractorCommand,
    [string[]] $CommandArg = @(),
    [string] $CommandArgsJson,
    [string] $CommandArgsFile,
    [int] $TimeoutSecs = 120,
    [string] $Cache,
    [string] $OutDir,
    [string] $Luna,
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

function Get-FileSha256 {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $null
    }

    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Convert-CapturedTextToUtf8 {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return
    }

    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -eq 0) {
        Write-Text -Path $Path -Value ""
        return
    }

    if ($bytes.Length -ge 2 -and $bytes[0] -eq 0xff -and $bytes[1] -eq 0xfe) {
        $text = [System.Text.Encoding]::Unicode.GetString($bytes)
    } elseif ($bytes.Length -ge 2 -and $bytes[0] -eq 0xfe -and $bytes[1] -eq 0xff) {
        $text = [System.Text.Encoding]::BigEndianUnicode.GetString($bytes)
    } elseif ($bytes.Length -ge 3 -and $bytes[0] -eq 0xef -and $bytes[1] -eq 0xbb -and $bytes[2] -eq 0xbf) {
        $text = [System.Text.Encoding]::UTF8.GetString($bytes, 3, $bytes.Length - 3)
    } else {
        $text = [System.Text.Encoding]::UTF8.GetString($bytes)
    }

    Write-Text -Path $Path -Value $text
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
                sha256 = Get-FileSha256 -Path $_.FullName
            }
        })
}

function Resolve-CommandPath {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    if (Test-Path -LiteralPath $Path) {
        return [System.IO.Path]::GetFullPath($Path)
    }

    $command = Get-Command $Path -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($command -and $command.Source) {
        return [System.IO.Path]::GetFullPath($command.Source)
    }

    return $Path
}

function Resolve-LunaBinary {
    if ($Luna) {
        return Resolve-RepoPath $Luna
    }

    $base = Join-Path $repoRoot "target/release/luna"
    $candidates = @($base, "$base.exe")
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return [System.IO.Path]::GetFullPath($candidate)
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

function Get-CorpusFiles {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    if (Test-Path -LiteralPath $Path -PathType Container) {
        return @(Get-ChildItem -LiteralPath $Path -Filter "*.json" -File | Sort-Object Name)
    }
    if (Test-Path -LiteralPath $Path -PathType Leaf) {
        return @(Get-Item -LiteralPath $Path)
    }

    throw "Corpus path does not exist: $Path"
}

function Get-SafeCaseName {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Name
    )

    $invalid = [System.IO.Path]::GetInvalidFileNameChars()
    $chars = $Name.ToCharArray() | ForEach-Object {
        if ($invalid -contains $_) { "_" } else { [string]$_ }
    }
    return -join $chars
}

Push-Location -LiteralPath $repoRoot
try {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $corpusPath = Resolve-RepoPath $Corpus
    $extractorCommandPath = Resolve-CommandPath $ExtractorCommand
    $script:lunaBinary = Resolve-LunaBinary

    if (-not $Cache) {
        $Cache = Join-Path $repoRoot ".luna\llm-ready\cache"
    }
    $cachePath = Resolve-RepoPath $Cache

    if (-not $OutDir) {
        $OutDir = Join-Path $repoRoot ".luna\llm-ready\$stamp"
    }
    $OutDir = [System.IO.Path]::GetFullPath($OutDir)
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
    New-Item -ItemType Directory -Force -Path $cachePath | Out-Null

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

    $parsedArgs = @()
    if ($CommandArgsFile) {
        $argsFilePath = Resolve-RepoPath $CommandArgsFile
        if (-not (Test-Path -LiteralPath $argsFilePath -PathType Leaf)) {
            throw "Command args file does not exist: $argsFilePath"
        }
        $jsonArgs = ConvertFrom-Json -InputObject (Get-Content -LiteralPath $argsFilePath -Raw)
        foreach ($arg in @($jsonArgs)) {
            $parsedArgs += [string]$arg
        }
    }
    if ($CommandArgsJson) {
        $jsonArgs = ConvertFrom-Json -InputObject $CommandArgsJson
        foreach ($arg in @($jsonArgs)) {
            $parsedArgs += [string]$arg
        }
    }
    $parsedArgs += @($CommandArg | ForEach-Object { [string]$_ })

    $corpusFiles = Get-CorpusFiles -Path $corpusPath
    if ($corpusFiles.Count -eq 0) {
        throw "Corpus contains no JSON scenario files: $corpusPath"
    }

    $casesDir = Join-Path $OutDir "cases"
    $copiedCorpusDir = Join-Path $OutDir "corpus"
    New-Item -ItemType Directory -Force -Path $casesDir | Out-Null
    New-Item -ItemType Directory -Force -Path $copiedCorpusDir | Out-Null

    $config = [ordered]@{
        generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
        repo_root = [string]$repoRoot
        corpus_path = $corpusPath
        model_id = $ModelId
        extractor_command = $extractorCommandPath
        extractor_command_sha256 = Get-FileSha256 -Path $extractorCommandPath
        command_args = @($parsedArgs)
        timeout_secs = $TimeoutSecs
        cache = $cachePath
        prompt_schema_version = "prompt_v3"
        luna = if ($script:lunaBinary) { $script:lunaBinary } else { "cargo run -p luna-cli --" }
        harness_network_dependency = $false
        extractor_network_policy = "caller_supplied_unverified"
    }
    Write-Json -Path (Join-Path $OutDir "config.json") -Value $config -Depth 8

    $commandLines = @(
        "# Luna LLM Ready packet replay commands",
        "cd " + (Quote-PowerShellArg ([string]$repoRoot))
    )
    $caseReports = @()

    foreach ($corpusFile in $corpusFiles) {
        $caseName = Get-SafeCaseName -Name $corpusFile.BaseName
        $caseDir = Join-Path $casesDir $caseName
        New-Item -ItemType Directory -Force -Path $caseDir | Out-Null

        $copiedCorpusPath = Join-Path $copiedCorpusDir $corpusFile.Name
        Copy-Item -LiteralPath $corpusFile.FullName -Destination $copiedCorpusPath -Force

        $stdoutPath = Join-Path $caseDir "stdout.txt"
        $stderrPath = Join-Path $caseDir "stderr.txt"
        $logPath = Join-Path $caseDir "events.jsonl"

        $arguments = @(
            "runtime",
            "scenario",
            $corpusFile.FullName,
            "--log",
            $logPath,
            "--extractor",
            "command",
            "--command",
            $extractorCommandPath,
            "--model-id",
            $ModelId,
            "--cache",
            $cachePath,
            "--timeout-secs",
            [string]$TimeoutSecs,
            "--keep-log"
        )
        foreach ($arg in $parsedArgs) {
            $arguments += "--command-arg=$arg"
        }

        $commandLineArgs = @($arguments | ForEach-Object { Quote-PowerShellArg $_ })
        $commandLines += (Luna-CommandLine) + " " + ($commandLineArgs -join " ")

        $global:LASTEXITCODE = 0
        $previousErrorActionPreference = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try {
            Invoke-Luna -Arguments $arguments > $stdoutPath 2> $stderrPath
            $exitCode = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = $previousErrorActionPreference
        }

        Convert-CapturedTextToUtf8 -Path $stdoutPath
        Convert-CapturedTextToUtf8 -Path $stderrPath

        $passed = $exitCode -eq 0
        $caseReports += [ordered]@{
            name = $caseName
            corpus = [ordered]@{
                source_path = $corpusFile.FullName
                copied_path = $copiedCorpusPath
                sha256 = Get-FileSha256 -Path $copiedCorpusPath
            }
            exit_code = $exitCode
            passed = $passed
            outputs = [ordered]@{
                stdout = $stdoutPath
                stderr = $stderrPath
                log = $logPath
            }
            hashes = [ordered]@{
                stdout_sha256 = Get-FileSha256 -Path $stdoutPath
                stderr_sha256 = Get-FileSha256 -Path $stderrPath
                log_sha256 = Get-FileSha256 -Path $logPath
            }
        }
    }

    Write-Text -Path (Join-Path $OutDir "commands.ps1") -Value ($commandLines -join [Environment]::NewLine)

    $cacheFiles = Get-DirectoryFileHashes -Path $cachePath
    Write-Json -Path (Join-Path $OutDir "cache-files.json") -Value $cacheFiles -Depth 8

    $passedCount = @($caseReports | Where-Object { $_.passed }).Count
    $failedCount = $caseReports.Count - $passedCount
    $manifest = [ordered]@{
        packet_kind = "luna.llm_ready.command_extractor.v1"
        generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
        config = $config
        corpus = [ordered]@{
            path = $corpusPath
            file_count = $corpusFiles.Count
            files = @(Get-DirectoryFileHashes -Path $copiedCorpusDir)
        }
        cache = [ordered]@{
            path = $cachePath
            file_count = $cacheFiles.Count
            files_manifest = Join-Path $OutDir "cache-files.json"
        }
        summary = [ordered]@{
            total = $caseReports.Count
            passed = $passedCount
            failed = $failedCount
            success = $failedCount -eq 0
        }
        cases = @($caseReports)
    }

    $manifestPath = Join-Path $OutDir "manifest.json"
    Write-Json -Path $manifestPath -Value $manifest -Depth 10

    Write-Host "LLM Ready packet: $OutDir"
    Write-Host "Corpus cases: $($caseReports.Count)"
    Write-Host "Passed: $passedCount"
    Write-Host "Failed: $failedCount"
    Write-Host "Manifest: $manifestPath"

    if ($failedCount -gt 0) {
        Write-Error "LLM Ready evaluation failed: $failedCount of $($caseReports.Count) case(s) failed. See $manifestPath"
        exit 1
    }
} finally {
    Pop-Location
}

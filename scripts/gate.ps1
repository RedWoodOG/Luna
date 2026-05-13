$ErrorActionPreference = "Stop"
if (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

$scriptDir = Split-Path -Parent $PSCommandPath
$repoRoot = Resolve-Path -LiteralPath (Join-Path $scriptDir "..")

function Invoke-Step {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Name,
        [Parameter(Mandatory = $true)]
        [scriptblock] $Command
    )

    Write-Host ""
    Write-Host "==> $Name"
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$Name failed with exit code $LASTEXITCODE"
    }
}

function Find-Bash {
    $candidates = @(
        "C:\Program Files\Git\bin\bash.exe",
        "C:\Program Files\Git\usr\bin\bash.exe",
        "C:\Program Files (x86)\Git\bin\bash.exe",
        "C:\Program Files (x86)\Git\usr\bin\bash.exe",
        (Get-Command bash -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -First 1)
    ) | Where-Object { $_ }

    foreach ($candidate in $candidates) {
        if ((Test-Path -LiteralPath $candidate) -and (Test-BashCandidate $candidate)) {
            return $candidate
        }
    }

    return $null
}

function Test-BashCandidate {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    try {
        & $Path --version *> $null
        return $LASTEXITCODE -eq 0
    } catch {
        return $false
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

    throw "Built luna binary not found at target/release/luna(.exe)"
}

function Get-WorkspacePackages {
    $metadataOutput = & cargo metadata --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata failed with exit code $LASTEXITCODE"
    }

    $metadata = ($metadataOutput -join [Environment]::NewLine) | ConvertFrom-Json
    $packagesById = @{}
    foreach ($package in $metadata.packages) {
        $packagesById[$package.id] = $package.name
    }

    $packages = @()
    foreach ($member in $metadata.workspace_members) {
        if (-not $packagesById.ContainsKey($member)) {
            throw "Workspace member missing from cargo metadata packages: $member"
        }
        $packages += $packagesById[$member]
    }

    if (-not $packages) {
        throw "No workspace packages found in cargo metadata"
    }

    $packages
}

function Get-ManifestScenarios {
    $manifest = Join-Path $repoRoot "scenarios/runtime/SCENARIO_MANIFEST.txt"
    if (-not (Test-Path -LiteralPath $manifest)) {
        throw "Scenario manifest not found at scenarios/runtime/SCENARIO_MANIFEST.txt"
    }

    $names = Get-Content -LiteralPath $manifest |
        ForEach-Object { $_.Trim() } |
        Where-Object { $_ -and -not $_.StartsWith("#") }
    if (-not $names) {
        throw "Scenario manifest is empty"
    }

    $registered = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($name in $names) {
        if (-not $registered.Add($name)) {
            throw "Duplicate scenario manifest entry: $name"
        }
        $path = Join-Path $repoRoot "scenarios/runtime/$name"
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Scenario manifest entry is missing: $name"
        }
    }

    $scenarioRoot = Resolve-Path -LiteralPath (Join-Path $repoRoot "scenarios/runtime")
    $nested = Get-ChildItem -LiteralPath $scenarioRoot -Filter "*.json" -File -Recurse |
        Where-Object { $_.DirectoryName -ne $scenarioRoot.Path }
    foreach ($file in $nested) {
        $repoRootText = ([string]$repoRoot).TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
        $relative = $file.FullName.Substring($repoRootText.Length).TrimStart([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
        throw "Nested runtime scenario JSON is not registered by the manifest gate: $relative"
    }

    $actual = Get-ChildItem -LiteralPath $scenarioRoot -Filter "*.json" -File |
        Select-Object -ExpandProperty Name
    foreach ($name in $actual) {
        if (-not $registered.Contains($name)) {
            throw "Runtime scenario is not registered in SCENARIO_MANIFEST.txt: $name"
        }
    }

    $names | ForEach-Object { Get-Item -LiteralPath (Join-Path $repoRoot "scenarios/runtime/$_") }
}

Push-Location -LiteralPath $repoRoot
$scenarioLogRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("luna-gate-" + [System.Guid]::NewGuid().ToString("N"))
try {
    # Windows Application Control can block the monolithic workspace all-features
    # luna_runtime test executable even though the direct package invocation is
    # allowed. Run every workspace package explicitly; this preserves coverage
    # without silently skipping luna-runtime integration tests or doctests.
    $workspacePackages = Get-WorkspacePackages
    foreach ($package in $workspacePackages) {
        Invoke-Step "cargo test -p $package --all-features" {
            cargo test -p $package --all-features
        }
    }

    Invoke-Step "cargo clippy --workspace --all-features -- -D warnings" {
        cargo clippy --workspace --all-features -- -D warnings
    }

    Invoke-Step "workspace member check" {
        python scripts/check_workspace_members.py
    }

    $bash = Find-Bash
    if ($bash) {
        Invoke-Step "scripts/doctrine_check.sh" {
            & $bash "scripts/doctrine_check.sh"
        }
    } else {
        Write-Host ""
        Write-Host "==> scripts/doctrine_check.sh"
        throw "Doctrine check cannot run: bash/Git Bash was not found."
    }

    Invoke-Step "cargo build -p luna-cli --release" {
        cargo build -p luna-cli --release
    }

    $luna = Resolve-LunaBinary
    $scenarios = Get-ManifestScenarios

    foreach ($scenario in $scenarios) {
        New-Item -ItemType Directory -Force -Path $scenarioLogRoot | Out-Null
        $scenarioLog = Join-Path $scenarioLogRoot ($scenario.BaseName + ".jsonl")
        Invoke-Step "runtime scenario $($scenario.Name)" {
            & $luna runtime scenario $scenario.FullName --log $scenarioLog
        }
    }

    $smokeLog = Join-Path $scenarioLogRoot "local_product_smoke.jsonl"
    Invoke-Step "runtime smoke" {
        & $luna runtime smoke --log $smokeLog --reset --json
    }

    Invoke-Step "repeat runtime audit over product smoke log" {
        $auditOut = Join-Path $scenarioLogRoot "repeat-runtime-audit"
        powershell -NoProfile -ExecutionPolicy Bypass -File scripts/audit-runtime-log.ps1 `
            -Log $smokeLog `
            -Luna $luna `
            -Count 3 `
            -IntervalSeconds 0 `
            -OutDir $auditOut
    }
} finally {
    if (Test-Path -LiteralPath $scenarioLogRoot) {
        Remove-Item -LiteralPath $scenarioLogRoot -Recurse -Force
    }
    Pop-Location
}

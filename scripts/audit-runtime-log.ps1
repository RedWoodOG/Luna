param(
    [Parameter(Mandatory = $true)]
    [string] $Log,

    [string] $Luna,

    [ValidateRange(1, 2147483647)]
    [int] $Count = 3,

    [ValidateRange(0, 2147483647)]
    [int] $IntervalSeconds = 0,

    [string] $OutDir
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$scriptDir = Split-Path -Parent $PSCommandPath
$repoRoot = Resolve-Path -LiteralPath (Join-Path $scriptDir "..")

function Resolve-LunaBinary {
    param([string] $Candidate)

    if ($Candidate) {
        if (-not (Test-Path -LiteralPath $Candidate -PathType Leaf)) {
            throw "Luna binary not found at $Candidate"
        }
        return (Resolve-Path -LiteralPath $Candidate).Path
    }

    $base = Join-Path $repoRoot "target/release/luna"
    foreach ($path in @($base, "$base.exe")) {
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            return (Resolve-Path -LiteralPath $path).Path
        }
    }

    throw "Built luna binary not found at target/release/luna(.exe); run cargo build -p luna-cli --release first."
}

$logPath = Resolve-Path -LiteralPath $Log -ErrorAction Stop
if (-not (Test-Path -LiteralPath $logPath -PathType Leaf)) {
    throw "Runtime log not found at $Log"
}

$lunaPath = Resolve-LunaBinary -Candidate $Luna

if ($OutDir) {
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
}

for ($i = 1; $i -le $Count; $i++) {
    Write-Host "Runtime replay audit pass ${i}/${Count}: $($logPath.Path)"
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = & $lunaPath runtime audit --log $logPath.Path --format json 2>&1
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    $text = ($output | Out-String).Trim()

    if ($OutDir) {
        $reportPath = Join-Path $OutDir ("runtime-audit-{0:D3}.json" -f $i)
        $text | Set-Content -LiteralPath $reportPath -Encoding UTF8
    }

    if ($exitCode -ne 0) {
        throw "Runtime replay audit pass $i failed with exit code $exitCode. Output: $text"
    }

    try {
        $report = $text | ConvertFrom-Json -ErrorAction Stop
    } catch {
        throw "Runtime replay audit pass $i did not return valid JSON. Output: $text"
    }

    if ($report.quarantine_required -or $report.status -ne "clean") {
        throw "Runtime replay audit pass $i quarantined the log. Status=$($report.status); replay_error=$($report.replay_error)"
    }

    if ($i -lt $Count -and $IntervalSeconds -gt 0) {
        Start-Sleep -Seconds $IntervalSeconds
    }
}

Write-Host "Runtime replay audit completed $Count clean pass(es)."

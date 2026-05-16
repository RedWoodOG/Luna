$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $repoRoot
try {
    if ($args.Count -eq 0) {
        cargo run -p luna-cli -- runtime smoke --log ".\.luna\runtime\local-memory-loop.jsonl" --reset
    }
    else {
        cargo run -p luna-cli -- runtime smoke @args
    }
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}

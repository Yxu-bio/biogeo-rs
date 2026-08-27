$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

& (Join-Path $PSScriptRoot "compare-biogeobears-detection-combination-optim.ps1") `
    -Manifest "validation/detection_full_stack_optimization_fixtures.tsv" `
    -GoldenPath "validation/golden/biogeobears-detection-full-stack-optim.tsv"
if ($LASTEXITCODE -ne 0) {
    throw "Full-stack detection optimization validation failed with exit code $LASTEXITCODE"
}

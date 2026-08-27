param(
    [string]$WeightsGolden = "validation/golden/biogeobears-model-average-weights.tsv",
    [string]$AncestralGolden = "validation/golden/biogeobears-model-average-ancestral.tsv",
    [double]$LikelihoodTolerance = 1e-5,
    [double]$WeightTolerance = 1e-6,
    [double]$ProbabilityTolerance = 1e-5,
    [string]$OutputRoot,
    [switch]$KeepOutput
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$temporaryOutput = [string]::IsNullOrWhiteSpace($OutputRoot)
if ($temporaryOutput) {
    $OutputRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
        "biogeo-model-average-" + [Guid]::NewGuid().ToString("N")
    )
}
$OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)

Push-Location $repoRoot
try {
    cargo build --release -q -p biogeo-cli
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }

    $cli = Join-Path $repoRoot "target/release/biogeo-cli.exe"
    $arguments = @(
        "model-batch",
        "--manifest", "validation/model_average_models.tsv",
        "--output-dir", $OutputRoot,
        "--tree", "validation/fixtures/three_area_tri_tip_null/tree.nwk",
        "--ranges", "validation/fixtures/three_area_tri_tip_null/ranges.tsv",
        "--include-null-range",
        "--max-range-size", "2",
        "--max-iterations", "1000"
    )
    & $cli @arguments | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "model-batch failed with exit code $LASTEXITCODE"
    }

    $resultPath = Join-Path $OutputRoot "model-averaged-ancestral-ranges.tsv"
    $lines = @(Get-Content -LiteralPath $resultPath)
    if ($lines[0] -ne "format`tbiogeo-model-averaged-ancestral-ranges-v2") {
        throw "unexpected model-average format header"
    }
    if ($lines -notcontains "status`tavailable") {
        throw "model-average result is not available"
    }
    if ($lines -notcontains "criteria`t1" -or $lines -notcontains "aicc_models`t0") {
        throw "small-sample fixture should contain AIC only and no AICc average"
    }

    $weightsHeader = "criterion`tmodel_id`tanalysis_result`tlnL`tinformation_criterion`tdelta`tweight"
    $weightsHeaderIndex = [array]::IndexOf($lines, $weightsHeader)
    $nodesMarker = [array]::IndexOf($lines, "nodes")
    $splitNodesMarker = [array]::IndexOf($lines, "split_nodes")
    $probabilitiesMarker = [array]::IndexOf($lines, "ancestral_state_probabilities")
    if ($weightsHeaderIndex -lt 0 -or $nodesMarker -lt 0 -or
        $splitNodesMarker -lt 0 -or $probabilitiesMarker -lt 0) {
        throw "model-average result is missing a required table"
    }
    $rustWeights = @(
        $lines[($weightsHeaderIndex + 1)..($nodesMarker - 2)] |
            ConvertFrom-Csv -Delimiter "`t" -Header @(
                "criterion", "model_id", "analysis_result", "lnL",
                "information_criterion", "delta", "weight"
            )
    )
    $goldenWeights = @(Import-Csv -LiteralPath $WeightsGolden -Delimiter "`t")
    foreach ($golden in $goldenWeights) {
        $rust = $rustWeights | Where-Object {
            $_.criterion -eq $golden.criterion -and $_.model_id -eq $golden.model_id
        }
        if ($null -eq $rust) {
            throw "missing Rust model weight for $($golden.criterion)/$($golden.model_id)"
        }
        $lnLDelta = [Math]::Abs([double]$rust.lnL - [double]$golden.lnL)
        $weightDelta = [Math]::Abs([double]$rust.weight - [double]$golden.weight)
        if ($lnLDelta -gt $LikelihoodTolerance) {
            throw "lnL mismatch for $($golden.model_id): delta=$lnLDelta"
        }
        if ($weightDelta -gt $WeightTolerance) {
            throw "AIC weight mismatch for $($golden.model_id): delta=$weightDelta"
        }
    }

    $areasMarker = [array]::IndexOf($lines, "areas")
    $statesMarker = [array]::IndexOf($lines, "states")
    if ($nodesMarker -lt 0 -or $areasMarker -lt 0 -or $statesMarker -lt 0) {
        throw "model-average result is missing node, area, or state metadata"
    }
    $rustNodes = @(
        $lines[($nodesMarker + 2)..($splitNodesMarker - 2)] |
            ConvertFrom-Csv -Delimiter "`t" -Header @("node", "label", "kind", "clade")
    )
    $rustStates = @(
        $lines[($statesMarker + 2)..($probabilitiesMarker - 2)] |
            ConvertFrom-Csv -Delimiter "`t" -Header @("state_index", "range_bits", "range")
    )

    $probabilityHeader = "criterion`tnode`tstate_index`tprobability"
    $probabilityHeaderIndex = [array]::IndexOf($lines, $probabilityHeader)
    $splitScenariosMarker = [array]::IndexOf($lines, "split_scenarios")
    $rustProbabilities = @(
        $lines[($probabilityHeaderIndex + 1)..($splitScenariosMarker - 2)] |
            ConvertFrom-Csv -Delimiter "`t" -Header @(
                "criterion", "node", "state_index", "probability"
            )
    )
    $goldenProbabilities = @(Import-Csv -LiteralPath $AncestralGolden -Delimiter "`t")
    $maxProbabilityDelta = 0.0
    foreach ($golden in $goldenProbabilities) {
        $node = $rustNodes | Where-Object { $_.clade -eq $golden.clade }
        $state = $rustStates | Where-Object { $_.range_bits -eq $golden.range_bits }
        if ($null -eq $node -or $null -eq $state) {
            throw "missing Rust node/state metadata for $($golden.clade)/$($golden.range_bits)"
        }
        $rust = $rustProbabilities | Where-Object {
            $_.criterion -eq $golden.criterion -and
            $_.node -eq $node.node -and
            $_.state_index -eq $state.state_index
        }
        if ($null -eq $rust) {
            throw "missing Rust probability for $($golden.criterion)/$($golden.clade)/$($golden.range_bits)"
        }
        $delta = [Math]::Abs(
            [double]$rust.probability - [double]$golden.biogeobears_probability
        )
        $maxProbabilityDelta = [Math]::Max($maxProbabilityDelta, $delta)
        if ($delta -gt $ProbabilityTolerance) {
            throw "model-average probability mismatch for $($golden.clade)/$($golden.range_bits): delta=$delta"
        }
    }

    Write-Output "biogeobears_model_average=passed"
    Write-Output "models=$($goldenWeights.Count)"
    Write-Output "probability_rows=$($goldenProbabilities.Count)"
    Write-Output "max_probability_delta=$maxProbabilityDelta"
    Write-Output "output_dir=$OutputRoot"
}
finally {
    Pop-Location
    if ($temporaryOutput -and -not $KeepOutput -and (Test-Path -LiteralPath $OutputRoot)) {
        $resolvedOutput = [System.IO.Path]::GetFullPath($OutputRoot)
        $resolvedTemp = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
        if (-not $resolvedOutput.StartsWith($resolvedTemp, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "refusing to remove non-temporary output path $resolvedOutput"
        }
        Remove-Item -LiteralPath $resolvedOutput -Recurse -Force
    }
}

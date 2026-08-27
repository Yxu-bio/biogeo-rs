param(
    [string]$Manifest = "validation/detection_combination_fixtures.tsv",
    [string]$CaseId = "psychotria_detection_constrained_full_stack",
    [string]$PosteriorGoldenPath = "validation/golden/biogeobears-detection-full-stack-fixnode-posterior.tsv",
    [string]$SplitGoldenPath = "validation/golden/biogeobears-detection-full-stack-fixnode-split.tsv",
    [double]$ProbabilityTolerance = 0.0000002,
    [double]$WeightTolerance = 0.00000001
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$case = @(Import-Csv -LiteralPath (Join-Path $repoRoot $Manifest) -Delimiter "`t" |
    Where-Object case_id -eq $CaseId)
if ($case.Count -ne 1) {
    throw "Expected exactly one full-stack case named: $CaseId"
}
$case = $case[0]
$posteriorGolden = @(Import-Csv -LiteralPath (Join-Path $repoRoot $PosteriorGoldenPath) -Delimiter "`t")
$splitGolden = @(Import-Csv -LiteralPath (Join-Path $repoRoot $SplitGoldenPath) -Delimiter "`t")
$template = Get-Content -LiteralPath (Join-Path $repoRoot "examples/parameter_tables/dec.tsv")
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) (
    "biogeo-full-stack-fixnode-" + [guid]::NewGuid().ToString("N")
)
[System.IO.Directory]::CreateDirectory($tempDir) | Out-Null

function New-FixedParameterTable {
    param([object]$Case, [string]$Path)
    $names = @(
        "d", "e", "a", "b", "x", "n", "w", "u", "j", "y", "s", "v",
        "mx01", "mx01j", "mx01y", "mx01s", "mx01v", "mf", "dp", "fdp"
    )
    $values = @{}
    foreach ($name in $names) {
        $values[$name] = [string]$Case.$name
    }
    $lines = foreach ($line in $template) {
        $fields = $line -split "`t", -1
        if ($fields.Count -eq 7 -and $values.ContainsKey($fields[0])) {
            $fields[1] = "fixed"
            $fields[2] = $values[$fields[0]]
            $fields[6] = ""
            $fields -join "`t"
        }
        else {
            $line
        }
    }
    [System.IO.File]::WriteAllText(
        $Path,
        ($lines -join "`n") + "`n",
        [System.Text.UTF8Encoding]::new($false)
    )
}

function Add-OptionalPath {
    param([System.Collections.ArrayList]$Arguments, [string]$Option, [string]$Value)
    if (-not [string]::IsNullOrWhiteSpace($Value) -and $Value -ne "-") {
        [void]$Arguments.Add($Option)
        [void]$Arguments.Add((Join-Path $repoRoot $Value))
    }
}

function Ancestral-Key {
    param([object]$Row)
    "$($Row.clade)|$($Row.range_bits)"
}

function Split-Key {
    param([object]$Row)
    "$($Row.clade)|$($Row.left_clade)|$($Row.right_clade)|$($Row.ancestor_range_bits)|$($Row.left_range_bits)|$($Row.right_range_bits)"
}

Push-Location $repoRoot
try {
    $parameterPath = Join-Path $tempDir "parameters.tsv"
    New-FixedParameterTable -Case $case -Path $parameterPath
    [System.Collections.ArrayList]$arguments = @(
        "run", "--release", "-q", "-p", "biogeo-cli", "--",
        "model-evaluate",
        "--tree", (Join-Path $repoRoot $case.tree),
        "--use-detection-model",
        "--detections", (Join-Path $repoRoot $case.detections),
        "--controls", (Join-Path $repoRoot $case.controls),
        "--parameters", $parameterPath,
        "--max-range-size", $case.max_range_size,
        "--root-prior", $case.root_prior,
        "--ancestral-probs",
        "--split-probs"
    )
    Add-OptionalPath -Arguments $arguments -Option "--dispersal-multipliers" -Value $case.dispersal_multipliers
    Add-OptionalPath -Arguments $arguments -Option "--dispersal-strata" -Value $case.dispersal_strata
    Add-OptionalPath -Arguments $arguments -Option "--distance-matrix" -Value $case.distance_matrix
    Add-OptionalPath -Arguments $arguments -Option "--environment-distance-matrix" -Value $case.environment_distance_matrix
    Add-OptionalPath -Arguments $arguments -Option "--area-sizes" -Value $case.area_sizes
    if ($case.include_null_range -eq "true") {
        [void]$arguments.Add("--include-null-range")
    }

    $output = @(& cargo @arguments)
    if ($LASTEXITCODE -ne 0) {
        throw "${CaseId}: Rust model-evaluate exited with code $LASTEXITCODE"
    }
    $ancestralHeader = "node`tlabel`tkind`tclade`tstate_index`trange_bits`trange`tprobability"
    $splitHeader = "node`tlabel`tkind`tclade`tleft_clade`tright_clade`tancestor_state_index`tancestor_range_bits`tancestor_range`tleft_state_index`tleft_range_bits`tleft_range`tright_state_index`tright_range_bits`tright_range`tscenario_weight`tprobability"
    $ancestralIndex = [array]::IndexOf($output, $ancestralHeader)
    $splitIndex = [array]::IndexOf($output, $splitHeader)
    if ($ancestralIndex -lt 0 -or $splitIndex -le $ancestralIndex) {
        throw "${CaseId}: CLI output did not contain ordered ancestral and split tables"
    }
    $rustPosterior = @(
        $output[$ancestralIndex..($splitIndex - 1)] |
            ConvertFrom-Csv -Delimiter "`t" |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_.clade) }
    )
    $rustSplit = @(
        $output[$splitIndex..($output.Count - 1)] |
            ConvertFrom-Csv -Delimiter "`t" |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_.clade) }
    )

    if ($rustPosterior.Count -ne $posteriorGolden.Count) {
        throw "${CaseId}: ancestral row count differs rust=$($rustPosterior.Count) bgb=$($posteriorGolden.Count)"
    }
    $rustPosteriorByKey = @{}
    foreach ($row in $rustPosterior) {
        $key = Ancestral-Key $row
        if ($rustPosteriorByKey.ContainsKey($key)) {
            throw "${CaseId}: duplicate Rust ancestral key $key"
        }
        $rustPosteriorByKey[$key] = $row
    }
    $maxPosteriorDelta = 0.0
    foreach ($golden in $posteriorGolden) {
        $key = Ancestral-Key $golden
        if (-not $rustPosteriorByKey.ContainsKey($key)) {
            throw "${CaseId}: missing Rust ancestral key $key"
        }
        $delta = [Math]::Abs(
            [double]$rustPosteriorByKey[$key].probability - [double]$golden.fixnode_probability
        )
        $maxPosteriorDelta = [Math]::Max($maxPosteriorDelta, $delta)
        if ($delta -gt $ProbabilityTolerance) {
            throw "${CaseId}: fixnode ancestral mismatch key=$key delta=$delta"
        }
    }

    if ($rustSplit.Count -ne $splitGolden.Count) {
        throw "${CaseId}: split row count differs rust=$($rustSplit.Count) bgb=$($splitGolden.Count)"
    }
    $rustSplitByKey = @{}
    foreach ($row in $rustSplit) {
        $key = Split-Key $row
        if ($rustSplitByKey.ContainsKey($key)) {
            throw "${CaseId}: duplicate Rust split key $key"
        }
        $rustSplitByKey[$key] = $row
    }
    $maxSplitDelta = 0.0
    $maxWeightDelta = 0.0
    foreach ($golden in $splitGolden) {
        $key = Split-Key $golden
        if (-not $rustSplitByKey.ContainsKey($key)) {
            throw "${CaseId}: missing Rust split key $key"
        }
        $probabilityDelta = [Math]::Abs(
            [double]$rustSplitByKey[$key].probability - [double]$golden.fixnode_probability
        )
        $weightDelta = [Math]::Abs(
            [double]$rustSplitByKey[$key].scenario_weight -
                [double]$golden.biogeobears_scenario_weight
        )
        $maxSplitDelta = [Math]::Max($maxSplitDelta, $probabilityDelta)
        $maxWeightDelta = [Math]::Max($maxWeightDelta, $weightDelta)
        if ($probabilityDelta -gt $ProbabilityTolerance) {
            throw "${CaseId}: corrected split mismatch key=$key delta=$probabilityDelta"
        }
        if ($weightDelta -gt $WeightTolerance) {
            throw "${CaseId}: split weight mismatch key=$key delta=$weightDelta"
        }
    }

    $directPosteriorDelta = ($posteriorGolden | Measure-Object absolute_delta -Maximum).Maximum
    $directSplitDelta = ($splitGolden | Measure-Object absolute_delta -Maximum).Maximum
    Write-Host "$CaseId ok ancestral_rows=$($posteriorGolden.Count) max_fixnode_delta=$maxPosteriorDelta"
    Write-Host "$CaseId ok split_rows=$($splitGolden.Count) max_probability_delta=$maxSplitDelta max_weight_delta=$maxWeightDelta"
    Write-Host "BioGeoBEARS direct stratified uppass audit deltas: ancestral=$directPosteriorDelta split=$directSplitDelta"
}
finally {
    Pop-Location
    Remove-Item -LiteralPath $tempDir -Recurse -Force
}

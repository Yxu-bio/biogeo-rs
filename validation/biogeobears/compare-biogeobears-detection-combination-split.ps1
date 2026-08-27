param(
    [string]$Manifest = "validation/detection_combination_fixtures.tsv",
    [string]$GoldenPath = "validation/golden/biogeobears-detection-combination-split.tsv",
    [double]$ProbabilityTolerance = 0.00002,
    [double]$WeightTolerance = 0.0000001
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$cases = Import-Csv -LiteralPath (Join-Path $repoRoot $Manifest) -Delimiter "`t"
$goldenRows = Import-Csv -LiteralPath (Join-Path $repoRoot $GoldenPath) -Delimiter "`t"
$template = Get-Content -LiteralPath (Join-Path $repoRoot "examples/parameter_tables/dec.tsv")
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("biogeo-detection-split-" + [guid]::NewGuid().ToString("N"))
[System.IO.Directory]::CreateDirectory($tempDir) | Out-Null
$rustRowsByCase = @{}

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

function Split-Key {
    param([object]$Row)
    "$($Row.clade)|$($Row.left_clade)|$($Row.right_clade)|$($Row.ancestor_range_bits)|$($Row.left_range_bits)|$($Row.right_range_bits)"
}

Push-Location $repoRoot
try {
    foreach ($case in $cases) {
        $posteriorReady = $case.posterior_ready -ne "false"
        $caseGoldenRows = @($goldenRows | Where-Object { $_.case_id -eq $case.case_id })
        if ($posteriorReady -and $caseGoldenRows.Count -eq 0) {
            throw "$($case.case_id): missing BioGeoBEARS split golden rows"
        }
        $parameterPath = Join-Path $tempDir "$($case.case_id).tsv"
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
            throw "$($case.case_id): Rust model-evaluate exited with code $LASTEXITCODE"
        }
        $header = "node`tlabel`tkind`tclade`tleft_clade`tright_clade`tancestor_state_index`tancestor_range_bits`tancestor_range`tleft_state_index`tleft_range_bits`tleft_range`tright_state_index`tright_range_bits`tright_range`tscenario_weight`tprobability"
        $headerIndex = [array]::IndexOf($output, $header)
        if ($headerIndex -lt 0) {
            throw "$($case.case_id): CLI output did not contain split probabilities"
        }
        $rustRows = @($output[$headerIndex..($output.Count - 1)] | ConvertFrom-Csv -Delimiter "`t")
        $rustByKey = @{}
        foreach ($row in $rustRows) {
            $key = Split-Key $row
            if ($rustByKey.ContainsKey($key)) {
                throw "$($case.case_id): duplicate Rust split key $key"
            }
            $rustByKey[$key] = $row
        }
        $rustRowsByCase[$case.case_id] = $rustByKey
        if (-not $posteriorReady) {
            Write-Host "$($case.case_id) audit-only Rust split_rows=$($rustRows.Count)"
            continue
        }
        if ($rustRows.Count -ne $caseGoldenRows.Count) {
            throw "$($case.case_id): split row count mismatch rust=$($rustRows.Count) bgb=$($caseGoldenRows.Count)"
        }

        $maxProbabilityDelta = 0.0
        $maxWeightDelta = 0.0
        foreach ($golden in $caseGoldenRows) {
            $key = Split-Key $golden
            if (-not $rustByKey.ContainsKey($key)) {
                throw "$($case.case_id): missing Rust split key $key"
            }
            $probabilityDelta = [Math]::Abs(
                [double]$rustByKey[$key].probability - [double]$golden.biogeobears_probability
            )
            $weightDelta = [Math]::Abs(
                [double]$rustByKey[$key].scenario_weight - [double]$golden.biogeobears_scenario_weight
            )
            $maxProbabilityDelta = [Math]::Max($maxProbabilityDelta, $probabilityDelta)
            $maxWeightDelta = [Math]::Max($maxWeightDelta, $weightDelta)
            if ($probabilityDelta -gt $ProbabilityTolerance) {
                throw "$($case.case_id): split probability mismatch key=$key delta=$probabilityDelta"
            }
            if ($weightDelta -gt $WeightTolerance) {
                throw "$($case.case_id): split weight mismatch key=$key delta=$weightDelta"
            }
        }
        Write-Host "$($case.case_id) ok split_rows=$($caseGoldenRows.Count) max_probability_delta=$maxProbabilityDelta max_weight_delta=$maxWeightDelta"
    }

    $stratified = $rustRowsByCase["psychotria_detection_full_stratified"]
    $staticEquivalent = $rustRowsByCase["psychotria_detection_stratified_static_equivalent"]
    if ($null -ne $stratified -and $null -ne $staticEquivalent) {
        if ($stratified.Count -ne $staticEquivalent.Count) {
            throw "Rust stratified/static-equivalent split row counts differ"
        }
        $maxProbabilityDelta = 0.0
        $maxWeightDelta = 0.0
        foreach ($key in $stratified.Keys) {
            if (-not $staticEquivalent.ContainsKey($key)) {
                throw "Rust static-equivalent split output is missing key $key"
            }
            $maxProbabilityDelta = [Math]::Max(
                $maxProbabilityDelta,
                [Math]::Abs([double]$stratified[$key].probability - [double]$staticEquivalent[$key].probability)
            )
            $maxWeightDelta = [Math]::Max(
                $maxWeightDelta,
                [Math]::Abs([double]$stratified[$key].scenario_weight - [double]$staticEquivalent[$key].scenario_weight)
            )
        }
        if ($maxProbabilityDelta -gt 0.0000001 -or $maxWeightDelta -gt 0.0000001) {
            throw "Rust stratified/static-equivalent split output differs: probability=$maxProbabilityDelta weight=$maxWeightDelta"
        }
        Write-Host "psychotria detection Rust stratified/static-equivalent split probability_delta=$maxProbabilityDelta weight_delta=$maxWeightDelta"
    }
}
finally {
    Pop-Location
    Remove-Item -LiteralPath $tempDir -Recurse -Force
}

param(
    [string]$Manifest = "validation/dec_fixtures.tsv",
    [string]$Golden = "validation/golden/biogeobears-dec-split.tsv",
    [double]$ProbabilityTolerance = 1e-6,
    [double]$WeightTolerance = 1e-12,
    [switch]$IgnoreZeroProbabilityPlaceholders,
    [double]$ZeroProbabilityTolerance = 1e-14,
    [ValidateSet("dec", "divalike", "bayarealike")]
    [string]$Command = "dec"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$manifestPath = Resolve-Path (Join-Path $repoRoot $Manifest)
$goldenPath = Resolve-Path (Join-Path $repoRoot $Golden)

$cases = Import-Csv -Path $manifestPath -Delimiter "`t"
$goldenRows = Import-Csv -Path $goldenPath -Delimiter "`t"

function Split-Key($row) {
    return "$($row.clade)|$($row.left_clade)|$($row.right_clade)|$($row.ancestor_range_bits)|$($row.left_range_bits)|$($row.right_range_bits)"
}

foreach ($case in $cases) {
    if ($case.biogeobears_ready -ne "true") {
        continue
    }
    if (($case.PSObject.Properties.Name -contains "biogeobears_posterior_ready") -and $case.biogeobears_posterior_ready -ne "true") {
        continue
    }
    if (($case.PSObject.Properties.Name -contains "biogeobears_split_ready") -and $case.biogeobears_split_ready -ne "true") {
        continue
    }

    $caseGoldenRows = @($goldenRows | Where-Object { $_.case_id -eq $case.case_id })
    if ($caseGoldenRows.Count -eq 0) {
        throw "$($case.case_id): missing BioGeoBEARS split golden rows in $goldenPath"
    }

    $treePath = Join-Path $repoRoot $case.tree
    $rangesPath = Join-Path $repoRoot $case.ranges

    $args = @(
        "run", "-q", "-p", "biogeo-cli", "--",
        $Command,
        "--tree", $treePath,
        "--ranges", $rangesPath,
        "--d", $case.d,
        "--e", $case.e,
        "--max-range-size", $case.max_range_size,
        "--root-prior", $case.root_prior,
        "--split-probs"
    )

    if ($case.include_null_range -eq "true") {
        $args += "--include-null-range"
    }
    if (($case.PSObject.Properties.Name -contains "min_branch_length") -and -not [string]::IsNullOrWhiteSpace($case.min_branch_length)) {
        $args += @("--min-branch-length", $case.min_branch_length)
    }
    if (($case.PSObject.Properties.Name -contains "dispersal_multipliers") -and -not [string]::IsNullOrWhiteSpace($case.dispersal_multipliers)) {
        $args += @("--dispersal-multipliers", (Join-Path $repoRoot $case.dispersal_multipliers))
    }
    if (($case.PSObject.Properties.Name -contains "dispersal_strata") -and -not [string]::IsNullOrWhiteSpace($case.dispersal_strata)) {
        $args += @("--dispersal-strata", (Join-Path $repoRoot $case.dispersal_strata))
    }
    if (($case.PSObject.Properties.Name -contains "distance_matrix") -and -not [string]::IsNullOrWhiteSpace($case.distance_matrix)) {
        $args += @("--distance-matrix", (Join-Path $repoRoot $case.distance_matrix), "--distance-exponent", $case.distance_exponent)
    }
    elseif (($case.PSObject.Properties.Name -contains "distance_exponent") -and -not [string]::IsNullOrWhiteSpace($case.distance_exponent)) {
        $args += @("--distance-exponent", $case.distance_exponent)
    }
    if (($case.PSObject.Properties.Name -contains "environment_distance_matrix") -and -not [string]::IsNullOrWhiteSpace($case.environment_distance_matrix)) {
        $args += @("--environment-distance-matrix", (Join-Path $repoRoot $case.environment_distance_matrix), "--environment-distance-exponent", $case.environment_distance_exponent)
    }
    elseif (($case.PSObject.Properties.Name -contains "environment_distance_exponent") -and -not [string]::IsNullOrWhiteSpace($case.environment_distance_exponent)) {
        $args += @("--environment-distance-exponent", $case.environment_distance_exponent)
    }
    if (($case.PSObject.Properties.Name -contains "extirpation_multipliers") -and -not [string]::IsNullOrWhiteSpace($case.extirpation_multipliers)) {
        $args += @("--extirpation-multipliers", (Join-Path $repoRoot $case.extirpation_multipliers))
    }
    if (($case.PSObject.Properties.Name -contains "area_sizes") -and -not [string]::IsNullOrWhiteSpace($case.area_sizes)) {
        $args += @("--area-sizes", (Join-Path $repoRoot $case.area_sizes), "--area-exponent", $case.area_exponent)
    }
    elseif (($case.PSObject.Properties.Name -contains "area_exponent") -and -not [string]::IsNullOrWhiteSpace($case.area_exponent)) {
        $args += @("--area-exponent", $case.area_exponent)
    }
    if (($case.PSObject.Properties.Name -contains "j") -and -not [string]::IsNullOrWhiteSpace($case.j)) {
        $args += @("--j", $case.j)
    }
    foreach ($name in @("mx01y", "mx01s", "mx01v", "mx01j")) {
        if (($case.PSObject.Properties.Name -contains $name) -and -not [string]::IsNullOrWhiteSpace($case.$name)) {
            $args += @("--$name", $case.$name)
        }
    }

    Push-Location $repoRoot
    try {
        $output = & cargo @args
        if ($LASTEXITCODE -ne 0) {
            throw "cargo exited with code $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }

    $header = "node`tlabel`tkind`tclade`tleft_clade`tright_clade`tancestor_state_index`tancestor_range_bits`tancestor_range`tleft_state_index`tleft_range_bits`tleft_range`tright_state_index`tright_range_bits`tright_range`tscenario_weight`tprobability"
    $headerIndex = [array]::IndexOf($output, $header)
    if ($headerIndex -lt 0) {
        throw "$($case.case_id): CLI output did not contain split probability table"
    }

    $tableLines = @($output[$headerIndex..($output.Count - 1)])
    $rustRows = @($tableLines | ConvertFrom-Csv -Delimiter "`t")
    $comparisonRustRows = $rustRows
    $comparisonGoldenRows = $caseGoldenRows
    if ($IgnoreZeroProbabilityPlaceholders) {
        $comparisonRustRows = @($rustRows | Where-Object {
            [Math]::Abs([double]$_.probability) -gt $ZeroProbabilityTolerance
        })
        $comparisonGoldenRows = @($caseGoldenRows | Where-Object {
            [Math]::Abs([double]$_.biogeobears_probability) -gt $ZeroProbabilityTolerance
        })
    }

    $rustByKey = @{}
    foreach ($row in $comparisonRustRows) {
        $key = Split-Key $row
        if ($rustByKey.ContainsKey($key)) {
            throw "$($case.case_id): duplicate Rust split row key $key"
        }
        $rustByKey[$key] = $row
    }

    $goldenByKey = @{}
    foreach ($goldenRow in $comparisonGoldenRows) {
        $key = Split-Key $goldenRow
        if ($goldenByKey.ContainsKey($key)) {
            throw "$($case.case_id): duplicate BioGeoBEARS split row key $key"
        }
        $goldenByKey[$key] = $goldenRow
    }

    if ($comparisonRustRows.Count -ne $comparisonGoldenRows.Count) {
        throw "$($case.case_id): split row count mismatch rust=$($comparisonRustRows.Count) biogeobears=$($comparisonGoldenRows.Count)"
    }

    foreach ($key in $rustByKey.Keys) {
        if (-not $goldenByKey.ContainsKey($key)) {
            throw "$($case.case_id): extra Rust split row for key $key"
        }
    }

    $maxProbabilityDelta = 0.0
    $maxWeightDelta = 0.0
    foreach ($goldenRow in $comparisonGoldenRows) {
        $key = Split-Key $goldenRow
        if (-not $rustByKey.ContainsKey($key)) {
            throw "$($case.case_id): missing Rust split row for key $key"
        }

        $rustProbability = [double]$rustByKey[$key].probability
        $bgbProbability = [double]$goldenRow.biogeobears_probability
        $probabilityDelta = [Math]::Abs($rustProbability - $bgbProbability)
        $maxProbabilityDelta = [Math]::Max($maxProbabilityDelta, $probabilityDelta)

        if ($probabilityDelta -gt $ProbabilityTolerance) {
            throw "$($case.case_id): split probability mismatch key=$key rust=$rustProbability biogeobears=$bgbProbability delta=$probabilityDelta tolerance=$ProbabilityTolerance"
        }

        $rustWeight = [double]$rustByKey[$key].scenario_weight
        $bgbWeight = [double]$goldenRow.biogeobears_scenario_weight
        $weightDelta = [Math]::Abs($rustWeight - $bgbWeight)
        $maxWeightDelta = [Math]::Max($maxWeightDelta, $weightDelta)

        if ($weightDelta -gt $WeightTolerance) {
            throw "$($case.case_id): split scenario weight mismatch key=$key rust=$rustWeight biogeobears=$bgbWeight delta=$weightDelta tolerance=$WeightTolerance"
        }
    }

    $placeholderCount = $caseGoldenRows.Count - $comparisonGoldenRows.Count
    Write-Host "$($case.case_id) ok split_rows=$($comparisonGoldenRows.Count) ignored_zero_placeholders=$placeholderCount max_probability_delta=$maxProbabilityDelta max_weight_delta=$maxWeightDelta"
}

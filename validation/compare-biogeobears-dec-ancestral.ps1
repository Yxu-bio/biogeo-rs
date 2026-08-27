param(
    [string]$Manifest = "validation/dec_fixtures.tsv",
    [string]$Golden = "validation/golden/biogeobears-dec-ancestral.tsv",
    [double]$ProbabilityTolerance = 1e-6,
    [ValidateSet("dec", "divalike", "bayarealike")]
    [string]$Command = "dec"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$manifestPath = Resolve-Path (Join-Path $repoRoot $Manifest)
$goldenPath = Resolve-Path (Join-Path $repoRoot $Golden)

$cases = Import-Csv -Path $manifestPath -Delimiter "`t"
$goldenRows = Import-Csv -Path $goldenPath -Delimiter "`t"

foreach ($case in $cases) {
    if ($case.biogeobears_ready -ne "true") {
        continue
    }
    if (($case.PSObject.Properties.Name -contains "biogeobears_posterior_ready") -and $case.biogeobears_posterior_ready -ne "true") {
        continue
    }

    $caseGoldenRows = @($goldenRows | Where-Object { $_.case_id -eq $case.case_id })
    if ($caseGoldenRows.Count -eq 0) {
        throw "$($case.case_id): missing BioGeoBEARS ancestral golden rows in $goldenPath"
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
        "--ancestral-probs"
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

    $header = "node`tlabel`tkind`tclade`tstate_index`trange_bits`trange`tprobability"
    $headerIndex = [array]::IndexOf($output, $header)
    if ($headerIndex -lt 0) {
        throw "$($case.case_id): CLI output did not contain ancestral probability table"
    }

    $tableLines = @($output[$headerIndex..($output.Count - 1)])
    $rustRows = @($tableLines | ConvertFrom-Csv -Delimiter "`t")
    $rustByKey = @{}
    foreach ($row in $rustRows) {
        $key = "$($row.clade)|$($row.range_bits)"
        if ($rustByKey.ContainsKey($key)) {
            throw "$($case.case_id): duplicate Rust ancestral row key $key"
        }
        $rustByKey[$key] = $row
    }

    $maxDelta = 0.0
    foreach ($goldenRow in $caseGoldenRows) {
        $key = "$($goldenRow.clade)|$($goldenRow.range_bits)"
        if (-not $rustByKey.ContainsKey($key)) {
            throw "$($case.case_id): missing Rust ancestral row for key $key"
        }

        $rustProbability = [double]$rustByKey[$key].probability
        $bgbProbability = [double]$goldenRow.biogeobears_probability
        $delta = [Math]::Abs($rustProbability - $bgbProbability)
        $maxDelta = [Math]::Max($maxDelta, $delta)

        if ($delta -gt $ProbabilityTolerance) {
            throw "$($case.case_id): ancestral probability mismatch key=$key rust=$rustProbability biogeobears=$bgbProbability delta=$delta tolerance=$ProbabilityTolerance"
        }
    }

    Write-Host "$($case.case_id) ok ancestral_rows=$($caseGoldenRows.Count) max_delta=$maxDelta"
}

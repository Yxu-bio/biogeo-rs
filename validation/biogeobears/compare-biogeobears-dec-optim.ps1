param(
    [string]$Manifest = "validation/dec_fixtures.tsv",
    [string]$Golden = "validation/golden/biogeobears-dec-optim.tsv",
    [double]$LnLTolerance = 1e-5,
    [int]$MultiStartPoints = 1,
    [ValidateSet("dec-optimize", "divalike-optimize", "bayarealike-optimize")]
    [string]$Command = "dec-optimize"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$manifestPath = Resolve-Path (Join-Path $repoRoot $Manifest)
$goldenPath = Resolve-Path (Join-Path $repoRoot $Golden)

$cases = Import-Csv -Path $manifestPath -Delimiter "`t"
$goldenRows = Import-Csv -Path $goldenPath -Delimiter "`t"

if ($MultiStartPoints -lt 1) {
    throw "MultiStartPoints must be greater than zero"
}

foreach ($case in $cases) {
    if ($case.biogeobears_ready -ne "true") {
        continue
    }
    if (($case.PSObject.Properties.Name -contains "biogeobears_optim_ready") -and $case.biogeobears_optim_ready -ne "true") {
        continue
    }

    $directGoldenRow = $goldenRows | Where-Object { $_.case_id -eq $case.case_id } | Select-Object -First 1
    if ($null -eq $directGoldenRow) {
        throw "$($case.case_id): missing BioGeoBEARS optimized golden row in $goldenPath"
    }
    $referenceCaseId = $case.case_id
    if (($case.PSObject.Properties.Name -contains "optimization_reference_case_id") -and -not [string]::IsNullOrWhiteSpace($case.optimization_reference_case_id)) {
        $referenceCaseId = $case.optimization_reference_case_id
    }
    $goldenRow = $goldenRows | Where-Object { $_.case_id -eq $referenceCaseId } | Select-Object -First 1
    if ($null -eq $goldenRow) {
        throw "$($case.case_id): missing BioGeoBEARS optimization reference $referenceCaseId in $goldenPath"
    }
    if (($goldenRow.PSObject.Properties.Name -contains "convergence") -and [int]$goldenRow.convergence -ne 0) {
        throw "$($case.case_id): BioGeoBEARS optimization golden did not converge (code=$($goldenRow.convergence))"
    }

    $treePath = Join-Path $repoRoot $case.tree
    $rangesPath = Join-Path $repoRoot $case.ranges
    $initD = [string]($goldenRow.init_d)
    $initE = [string]($goldenRow.init_e)
    $minRate = [string]($goldenRow.min_rate)
    $maxRate = [string]($goldenRow.max_rate)

    if ([string]::IsNullOrWhiteSpace($initD)) {
        throw "$($case.case_id): missing init_d in $goldenPath"
    }
    if ([string]::IsNullOrWhiteSpace($initE)) {
        throw "$($case.case_id): missing init_e in $goldenPath"
    }
    if ([string]::IsNullOrWhiteSpace($minRate)) {
        throw "$($case.case_id): missing min_rate in $goldenPath"
    }
    if ([string]::IsNullOrWhiteSpace($maxRate)) {
        throw "$($case.case_id): missing max_rate in $goldenPath"
    }

    $args = @(
        "run", "-q", "-p", "biogeo-cli", "--",
        $Command,
        "--tree", $treePath,
        "--ranges", $rangesPath,
        "--max-range-size", $case.max_range_size,
        "--root-prior", $case.root_prior,
        "--init-d", $initD,
        "--init-e", $initE,
        "--min-rate", $minRate,
        "--max-rate", $maxRate,
        "--multi-start-points", ([string]$MultiStartPoints)
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
        $args += @(
            "--area-sizes", (Join-Path $repoRoot $case.area_sizes),
            "--area-exponent", $case.area_exponent
        )
    }
    elseif (($case.PSObject.Properties.Name -contains "area_exponent") -and -not [string]::IsNullOrWhiteSpace($case.area_exponent)) {
        $args += @("--area-exponent", $case.area_exponent)
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

    $values = @{}
    foreach ($line in $output) {
        $parts = $line -split "`t", 2
        if ($parts.Count -eq 2) {
            $values[$parts[0]] = $parts[1]
        }
    }

    foreach ($key in @("lnL", "d", "e", "converged")) {
        if (-not $values.ContainsKey($key)) {
            throw "$($case.case_id): CLI output did not contain $key"
        }
    }
    if ($values["converged"] -ne "true") {
        throw "$($case.case_id): Rust optimization did not converge"
    }

    $rustLnL = [double]$values["lnL"]
    $bgbLnL = [double]$goldenRow.biogeobears_lnL
    $lnLDelta = [Math]::Abs($rustLnL - $bgbLnL)
    $lnLAdvantage = $rustLnL - $bgbLnL
    $dDelta = [Math]::Abs([double]$values["d"] - [double]$goldenRow.biogeobears_d)
    $eDelta = [Math]::Abs([double]$values["e"] - [double]$goldenRow.biogeobears_e)
    $allowRustImprovement = (
        ($case.PSObject.Properties.Name -contains "allow_rust_optimization_improvement") -and
        $case.allow_rust_optimization_improvement -eq "true"
    )

    if ($allowRustImprovement) {
        if ($lnLAdvantage -lt -$LnLTolerance) {
            throw "$($case.case_id): Rust optimized lnL is worse than BioGeoBEARS rust=$rustLnL biogeobears=$bgbLnL shortfall=$(-$lnLAdvantage) tolerance=$LnLTolerance"
        }
    }
    elseif ($lnLDelta -gt $LnLTolerance) {
        throw "$($case.case_id): optimized lnL mismatch rust=$rustLnL biogeobears=$bgbLnL delta=$lnLDelta tolerance=$LnLTolerance"
    }

    $message = "$($case.case_id) ok reference=$referenceCaseId rust_lnL=$rustLnL biogeobears_lnL=$bgbLnL lnL_delta=$lnLDelta rust_advantage=$lnLAdvantage allow_rust_improvement=$allowRustImprovement d_delta=$dDelta e_delta=$eDelta"
    if ($referenceCaseId -ne $case.case_id) {
        $directDelta = [Math]::Abs($rustLnL - [double]$directGoldenRow.biogeobears_lnL)
        $message += " direct_biogeobears_audit_delta=$directDelta"
    }
    Write-Host $message
}

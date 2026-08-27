param(
    [string]$Manifest = "validation/decj_fixtures.tsv",
    [string]$Golden = "validation/golden/biogeobears-decj-optim.tsv",
    [ValidateSet("decj-optimize", "divalikej-optimize", "bayarealikej-optimize")]
    [string]$Command = "decj-optimize",
    [double]$LnLTolerance = 1e-5,
    [int]$MultiStartPoints = 1
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
    if (($case.PSObject.Properties.Name -contains "biogeobears_optimization_ready") -and $case.biogeobears_optimization_ready -ne "true") {
        continue
    }

    $goldenRow = $goldenRows | Where-Object { $_.case_id -eq $case.case_id } | Select-Object -First 1
    if ($null -eq $goldenRow) {
        throw "$($case.case_id): missing BioGeoBEARS DEC+J optimized golden row in $goldenPath"
    }
    if (($goldenRow.PSObject.Properties.Name -contains "biogeobears_convergence") -and [int]$goldenRow.biogeobears_convergence -ne 0) {
        throw "$($case.case_id): BioGeoBEARS golden optimizer did not converge (code=$($goldenRow.biogeobears_convergence), message=$($goldenRow.biogeobears_message))"
    }

    $treePath = Join-Path $repoRoot $case.tree
    $rangesPath = Join-Path $repoRoot $case.ranges
    $initD = [string]($goldenRow.init_d)
    $initE = [string]($goldenRow.init_e)
    $initJ = [string]($goldenRow.init_j)
    $minRate = [string]($goldenRow.min_rate)
    $maxRate = [string]($goldenRow.max_rate)
    $minJ = [string]($goldenRow.min_j)
    $maxJ = [string]($goldenRow.max_j)

    foreach ($required in @("initD", "initE", "initJ", "minRate", "maxRate", "minJ", "maxJ")) {
        if ([string]::IsNullOrWhiteSpace((Get-Variable -Name $required -ValueOnly))) {
            throw "$($case.case_id): missing $required in $goldenPath"
        }
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
        "--init-j", $initJ,
        "--min-rate", $minRate,
        "--max-rate", $maxRate,
        "--min-j", $minJ,
        "--max-j", $maxJ,
        "--multi-start-points", ([string]$MultiStartPoints)
    )

    if ($case.include_null_range -eq "true") {
        $args += "--include-null-range"
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
    if (($case.PSObject.Properties.Name -contains "environment_distance_matrix") -and -not [string]::IsNullOrWhiteSpace($case.environment_distance_matrix)) {
        $args += @("--environment-distance-matrix", (Join-Path $repoRoot $case.environment_distance_matrix), "--environment-distance-exponent", $case.environment_distance_exponent)
    }
    if (($case.PSObject.Properties.Name -contains "extirpation_multipliers") -and -not [string]::IsNullOrWhiteSpace($case.extirpation_multipliers)) {
        $args += @("--extirpation-multipliers", (Join-Path $repoRoot $case.extirpation_multipliers))
    }
    if (($case.PSObject.Properties.Name -contains "area_sizes") -and -not [string]::IsNullOrWhiteSpace($case.area_sizes)) {
        $args += @("--area-sizes", (Join-Path $repoRoot $case.area_sizes), "--area-exponent", $case.area_exponent)
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

    foreach ($key in @("lnL", "d", "e", "j")) {
        if (-not $values.ContainsKey($key)) {
            throw "$($case.case_id): CLI output did not contain $key"
        }
    }

    $rustLnL = [double]$values["lnL"]
    $bgbLnL = [double]$goldenRow.biogeobears_lnL
    $lnLDelta = [Math]::Abs($rustLnL - $bgbLnL)
    $dDelta = [Math]::Abs([double]$values["d"] - [double]$goldenRow.biogeobears_d)
    $eDelta = [Math]::Abs([double]$values["e"] - [double]$goldenRow.biogeobears_e)
    $jDelta = [Math]::Abs([double]$values["j"] - [double]$goldenRow.biogeobears_j)

    if ($lnLDelta -gt $LnLTolerance) {
        throw "$($case.case_id): $Command optimized lnL mismatch rust=$rustLnL biogeobears=$bgbLnL delta=$lnLDelta tolerance=$LnLTolerance"
    }

    Write-Host "$($case.case_id) ok rust_lnL=$rustLnL biogeobears_lnL=$bgbLnL lnL_delta=$lnLDelta d_delta=$dDelta e_delta=$eDelta j_delta=$jDelta"
}

param(
    [string]$Manifest = "validation/dec_fixtures.tsv",
    [string]$Golden = "validation/golden/biogeobears-dec.tsv",
    [ValidateSet("dec", "divalike", "bayarealike")]
    [string]$Command = "dec"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$manifestPath = Resolve-Path (Join-Path $repoRoot $Manifest)
$goldenPath = Resolve-Path (Join-Path $repoRoot $Golden)

$cases = Import-Csv -Path $manifestPath -Delimiter "`t"
$goldenRows = Import-Csv -Path $goldenPath -Delimiter "`t"
$goldenByCase = @{}
foreach ($row in $goldenRows) {
    $goldenByCase[$row.case_id] = $row
}

foreach ($case in $cases) {
    if ($case.biogeobears_ready -ne "true") {
        continue
    }

    if (-not $goldenByCase.ContainsKey($case.case_id)) {
        throw "$($case.case_id): missing BioGeoBEARS golden row in $goldenPath"
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
        "--root-prior", $case.root_prior
    )

    if ($case.include_null_range -eq "true") {
        $args += "--include-null-range"
    }
    if (($case.PSObject.Properties.Name -contains "min_branch_length") -and -not [string]::IsNullOrWhiteSpace($case.min_branch_length)) {
        $args += @("--min-branch-length", $case.min_branch_length)
    }
    if (($case.PSObject.Properties.Name -contains "j") -and -not [string]::IsNullOrWhiteSpace($case.j)) {
        $args += @("--j", $case.j)
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

    if (-not $values.ContainsKey("lnL")) {
        throw "$($case.case_id): CLI output did not contain lnL"
    }

    $rustLnL = [double]$values["lnL"]
    $bgbLnL = [double]$goldenByCase[$case.case_id].biogeobears_lnL
    $tolerance = [double]$case.external_tolerance
    $delta = [Math]::Abs($rustLnL - $bgbLnL)

    if ($delta -gt $tolerance) {
        throw "$($case.case_id): BioGeoBEARS mismatch rust=$rustLnL biogeobears=$bgbLnL delta=$delta tolerance=$tolerance"
    }

    Write-Host "$($case.case_id) ok rust=$rustLnL biogeobears=$bgbLnL delta=$delta"
}

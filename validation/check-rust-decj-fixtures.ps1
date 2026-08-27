param(
    [string]$Manifest = "validation/decj_fixtures.tsv"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$manifestPath = Resolve-Path (Join-Path $repoRoot $Manifest)
$cases = Import-Csv -Path $manifestPath -Delimiter "`t"

foreach ($case in $cases) {
    if ($case.expected_rust_lnL -eq "NaN" -or [string]::IsNullOrWhiteSpace($case.expected_rust_lnL)) {
        Write-Host "$($case.case_id) skipped: expected_rust_lnL is not set"
        continue
    }

    $treePath = Join-Path $repoRoot $case.tree
    $rangesPath = Join-Path $repoRoot $case.ranges

    $args = @(
        "run", "-q", "-p", "biogeo-cli", "--",
        "dec",
        "--tree", $treePath,
        "--ranges", $rangesPath,
        "--d", $case.d,
        "--e", $case.e,
        "--j", $case.j,
        "--max-range-size", $case.max_range_size,
        "--root-prior", $case.root_prior
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

    if (-not $values.ContainsKey("lnL")) {
        throw "$($case.case_id): CLI output did not contain lnL"
    }

    $actual = [double]$values["lnL"]
    $expected = [double]$case.expected_rust_lnL
    $tolerance = [double]$case.tolerance
    $delta = [Math]::Abs($actual - $expected)

    if ($delta -gt $tolerance) {
        throw "$($case.case_id): lnL mismatch actual=$actual expected=$expected delta=$delta tolerance=$tolerance"
    }

    Write-Host "$($case.case_id) ok lnL=$actual delta=$delta"
}

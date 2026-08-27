param(
    [string]$Manifest = "validation/pair_profile_fixtures.tsv"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$manifestPath = Resolve-Path (Join-Path $repoRoot $Manifest)
$cases = Import-Csv -Path $manifestPath -Delimiter "`t"

Push-Location $repoRoot
try {
    & cargo build --release -q -p biogeo-cli
    if ($LASTEXITCODE -ne 0) {
        throw "release build failed with exit code $LASTEXITCODE"
    }

    $cli = Join-Path $repoRoot "target/release/biogeo-cli.exe"
    foreach ($case in $cases) {
        $args = @(
            $case.command,
            "--tree", (Join-Path $repoRoot $case.tree),
            "--ranges", (Join-Path $repoRoot $case.ranges),
            "--distance-matrix", (Join-Path $repoRoot $case.distance_matrix),
            "--environment-distance-matrix", (Join-Path $repoRoot $case.environment_distance_matrix),
            "--area-sizes", (Join-Path $repoRoot $case.area_sizes),
            "--dispersal-multipliers", (Join-Path $repoRoot $case.dispersal_multipliers),
            $case.fixed_option, $case.fixed_exponent,
            "--$($case.first_parameter)-min", $case.first_min,
            "--$($case.first_parameter)-max", $case.first_max,
            "--$($case.first_parameter)-points", $case.first_points,
            "--$($case.second_parameter)-min", $case.second_min,
            "--$($case.second_parameter)-max", $case.second_max,
            "--$($case.second_parameter)-points", $case.second_points,
            "--max-range-size", $case.max_range_size,
            "--root-prior", $case.root_prior,
            "--max-iterations", $case.max_iterations,
            "--multi-start-points", $case.multi_start_points
        )
        if ($case.include_null_range -eq "true") {
            $args += "--include-null-range"
        }

        $output = & $cli @args
        if ($LASTEXITCODE -ne 0) {
            throw "$($case.case_id): CLI exited with code $LASTEXITCODE"
        }

        $values = @{}
        foreach ($line in $output) {
            if ($line -eq "profile_points") {
                break
            }
            $parts = $line -split "`t", 2
            if ($parts.Count -eq 2) {
                $values[$parts[0]] = $parts[1]
            }
        }
        foreach ($key in @(
            "lnL", "best_x", "best_n", "best_u", "support_points", "total_points",
            "converged_points", "likelihood_weighted_correlation",
            "$($case.first_parameter)_support_grid_values",
            "$($case.second_parameter)_support_grid_values"
        )) {
            if (-not $values.ContainsKey($key)) {
                throw "$($case.case_id): CLI output did not contain $key"
            }
        }

        $lnLDelta = [Math]::Abs([double]$values.lnL - [double]$case.expected_lnL)
        if ($lnLDelta -gt [double]$case.lnL_tolerance) {
            throw "$($case.case_id): lnL delta $lnLDelta exceeded $($case.lnL_tolerance)"
        }
        foreach ($parameter in @("x", "n", "u")) {
            $key = "best_$parameter"
            $expectedKey = "expected_best_$parameter"
            if ([Math]::Abs([double]$values[$key] - [double]$case.$expectedKey) -gt 1e-12) {
                throw "$($case.case_id): $key mismatch actual=$($values[$key]) expected=$($case.$expectedKey)"
            }
        }
        if ([int]$values.support_points -ne [int]$case.expected_support_points) {
            throw "$($case.case_id): support-point count mismatch"
        }
        if ([int]$values.total_points -ne [int]$case.expected_total_points) {
            throw "$($case.case_id): total-point count mismatch"
        }
        if ([int]$values.converged_points -ne [int]$case.expected_total_points) {
            throw "$($case.case_id): not all d/e profile optimizations converged"
        }
        if ([int]$values["$($case.first_parameter)_support_grid_values"] -ne [int]$case.expected_first_support_grid_values) {
            throw "$($case.case_id): first-axis support span mismatch"
        }
        if ([int]$values["$($case.second_parameter)_support_grid_values"] -ne [int]$case.expected_second_support_grid_values) {
            throw "$($case.case_id): second-axis support span mismatch"
        }
        $correlationDelta = [Math]::Abs(
            [double]$values.likelihood_weighted_correlation - [double]$case.expected_correlation
        )
        if ($correlationDelta -gt [double]$case.correlation_tolerance) {
            throw "$($case.case_id): weighted-correlation delta $correlationDelta exceeded $($case.correlation_tolerance)"
        }

        Write-Host "$($case.case_id) ok lnL=$($values.lnL) support=$($values.support_points)/$($values.total_points) correlation=$($values.likelihood_weighted_correlation)"
    }
}
finally {
    Pop-Location
}

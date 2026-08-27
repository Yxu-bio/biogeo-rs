param(
    [string]$Manifest = "validation/xnu_optimization_fixtures.tsv",
    [string]$BioGeoBEARSGolden = "validation/golden/biogeobears-dec-xnu-optim.tsv",
    [string]$RustGolden = "validation/golden/rust-dec-xnu-optim.tsv",
    [double]$RustLnLTolerance = 1e-8,
    [double]$RustParameterTolerance = 1e-6,
    [double]$BioGeoBEARSAdvantageTolerance = 1e-5
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$manifestRows = Import-Csv -Path (Resolve-Path (Join-Path $repoRoot $Manifest)) -Delimiter "`t"
$bgbRows = Import-Csv -Path (Resolve-Path (Join-Path $repoRoot $BioGeoBEARSGolden)) -Delimiter "`t"
$rustRows = Import-Csv -Path (Resolve-Path (Join-Path $repoRoot $RustGolden)) -Delimiter "`t"

foreach ($case in $manifestRows) {
    if ($case.biogeobears_ready -ne "true") {
        continue
    }
    $bgb = $bgbRows | Where-Object { $_.case_id -eq $case.case_id } | Select-Object -First 1
    $expected = $rustRows | Where-Object { $_.case_id -eq $case.case_id } | Select-Object -First 1
    if ($null -eq $bgb -or $null -eq $expected) {
        throw "$($case.case_id): missing joint optimization golden"
    }
    if ([int]$bgb.convergence -ne 0) {
        throw "$($case.case_id): BioGeoBEARS optimizer convergence code is $($bgb.convergence)"
    }

    $args = @(
        "run", "--release", "-q", "-p", "biogeo-cli", "--",
        "dec-xnu-optimize",
        "--tree", (Join-Path $repoRoot $case.tree),
        "--ranges", (Join-Path $repoRoot $case.ranges),
        "--distance-matrix", (Join-Path $repoRoot $case.distance_matrix),
        "--environment-distance-matrix", (Join-Path $repoRoot $case.environment_distance_matrix),
        "--area-sizes", (Join-Path $repoRoot $case.area_sizes),
        "--max-range-size", $case.max_range_size,
        "--root-prior", $case.root_prior,
        "--init-d", $case.init_d,
        "--init-e", $case.init_e,
        "--min-rate", $case.min_rate,
        "--max-rate", $case.max_rate,
        "--init-x", $case.init_x,
        "--min-x", $case.min_x,
        "--max-x", $case.max_x,
        "--init-n", $case.init_n,
        "--min-n", $case.min_n,
        "--max-n", $case.max_n,
        "--init-u", $case.init_u,
        "--min-u", $case.min_u,
        "--max-u", $case.max_u,
        "--max-iterations", $case.max_iterations,
        "--multi-start-points", "1"
    )
    if ($case.include_null_range -eq "true") {
        $args += "--include-null-range"
    }
    if (-not [string]::IsNullOrWhiteSpace($case.dispersal_multipliers)) {
        $args += @("--dispersal-multipliers", (Join-Path $repoRoot $case.dispersal_multipliers))
    }

    Push-Location $repoRoot
    try {
        $output = & cargo @args
        if ($LASTEXITCODE -ne 0) {
            throw "$($case.case_id): Rust joint optimization exited with code $LASTEXITCODE"
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
    foreach ($key in @("lnL", "d", "e", "x", "n", "u", "converged", "starts")) {
        if (-not $values.ContainsKey($key)) {
            throw "$($case.case_id): Rust output did not contain $key"
        }
    }
    if ($values.converged -ne "true") {
        throw "$($case.case_id): Rust joint optimization did not converge"
    }

    $rustLnLDelta = [Math]::Abs([double]$values.lnL - [double]$expected.rust_lnL)
    if ($rustLnLDelta -gt $RustLnLTolerance) {
        throw "$($case.case_id): Rust lnL regression delta=$rustLnLDelta"
    }
    foreach ($parameter in @("d", "e", "x", "n", "u")) {
        $delta = [Math]::Abs([double]$values[$parameter] - [double]$expected."rust_$parameter")
        if ($delta -gt $RustParameterTolerance) {
            throw "$($case.case_id): Rust $parameter regression delta=$delta"
        }
    }
    $advantage = [double]$values.lnL - [double]$bgb.biogeobears_lnL
    if ($advantage -lt -$BioGeoBEARSAdvantageTolerance) {
        throw "$($case.case_id): Rust optimized lnL is lower than BioGeoBEARS by $(-$advantage)"
    }

    $parameterDeltas = @{}
    foreach ($parameter in @("d", "e", "x", "n", "u")) {
        $parameterDeltas[$parameter] = [Math]::Abs(
            [double]$values[$parameter] - [double]$bgb."biogeobears_$parameter"
        )
    }
    Write-Host "$($case.case_id) ok rust_lnL=$($values.lnL) bgb_lnL=$($bgb.biogeobears_lnL) rust_advantage=$advantage kkt=$($bgb.kkt1)/$($bgb.kkt2) bgb_seconds=$($bgb.optimizer_seconds) deltas=$($parameterDeltas | ConvertTo-Json -Compress)"
}

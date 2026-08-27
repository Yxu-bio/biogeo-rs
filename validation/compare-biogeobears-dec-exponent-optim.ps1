param(
    [string]$Manifest = "validation/exponent_optimization_fixtures.tsv",
    [string]$Golden = "validation/golden/biogeobears-dec-exponent-optim.tsv",
    [double]$LnLTolerance = 1e-5,
    [double]$FixedLnLTolerance = 1e-6,
    [double]$ExponentTolerance = 1e-3,
    [int]$MultiStartPoints = 2
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$manifestPath = Resolve-Path (Join-Path $repoRoot $Manifest)
$goldenPath = Resolve-Path (Join-Path $repoRoot $Golden)
$cases = Import-Csv -Path $manifestPath -Delimiter "`t"
$goldenRows = Import-Csv -Path $goldenPath -Delimiter "`t"

if ($MultiStartPoints -lt 1) {
    throw "MultiStartPoints must be greater than zero"
}

function Get-ModifierArguments {
    param(
        [Parameter(Mandatory = $true)]$Case,
        [Parameter(Mandatory = $true)][ValidateSet("x", "n", "u")][string]$ExponentParameter,
        [Parameter(Mandatory = $true)][bool]$Optimizing,
        [string]$OptimizedExponent = ""
    )

    $arguments = @()
    if (-not [string]::IsNullOrWhiteSpace($Case.dispersal_strata)) {
        if ($ExponentParameter -ne "u") {
            throw "$($Case.case_id): free x/n fixtures do not support dispersal_strata"
        }
        $arguments += @(
            "--dispersal-strata",
            (Join-Path $repoRoot $Case.dispersal_strata)
        )
    }
    if (-not [string]::IsNullOrWhiteSpace($Case.dispersal_multipliers)) {
        $arguments += @(
            "--dispersal-multipliers",
            (Join-Path $repoRoot $Case.dispersal_multipliers)
        )
    }
    if (-not [string]::IsNullOrWhiteSpace($Case.distance_matrix)) {
        $arguments += @(
            "--distance-matrix",
            (Join-Path $repoRoot $Case.distance_matrix)
        )
        if ($ExponentParameter -ne "x") {
            $arguments += @("--distance-exponent", $Case.distance_exponent)
        }
        elseif (-not $Optimizing) {
            $arguments += @("--distance-exponent", $OptimizedExponent)
        }
    }
    if (-not [string]::IsNullOrWhiteSpace($Case.environment_distance_matrix)) {
        $arguments += @(
            "--environment-distance-matrix",
            (Join-Path $repoRoot $Case.environment_distance_matrix)
        )
        if ($ExponentParameter -ne "n") {
            $arguments += @(
                "--environment-distance-exponent",
                $Case.environment_distance_exponent
            )
        }
        elseif (-not $Optimizing) {
            $arguments += @(
                "--environment-distance-exponent",
                $OptimizedExponent
            )
        }
    }
    if (-not [string]::IsNullOrWhiteSpace($Case.extirpation_multipliers)) {
        $arguments += @(
            "--extirpation-multipliers",
            (Join-Path $repoRoot $Case.extirpation_multipliers)
        )
    }
    if (-not [string]::IsNullOrWhiteSpace($Case.area_sizes)) {
        $arguments += @(
            "--area-sizes",
            (Join-Path $repoRoot $Case.area_sizes)
        )
        if ($ExponentParameter -ne "u") {
            $arguments += @("--area-exponent", $Case.area_exponent)
        }
        elseif (-not $Optimizing) {
            $arguments += @("--area-exponent", $OptimizedExponent)
        }
    }
    $arguments
}

function Invoke-BiogeoCli {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)

    $output = & cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo exited with code $LASTEXITCODE"
    }
    $values = @{}
    foreach ($line in $output) {
        $parts = $line -split "`t", 2
        if ($parts.Count -eq 2) {
            $values[$parts[0]] = $parts[1]
        }
    }
    $values
}

Push-Location $repoRoot
try {
    foreach ($case in $cases) {
        if ($case.biogeobears_ready -ne "true") {
            continue
        }

        $parameter = $case.exponent_parameter.ToLowerInvariant()
        if ($parameter -notin @("x", "n", "u")) {
            throw "$($case.case_id): exponent_parameter must be x, n, or u"
        }
        $goldenRow = $goldenRows |
            Where-Object { $_.case_id -eq $case.case_id } |
            Select-Object -First 1
        if ($null -eq $goldenRow) {
            throw "$($case.case_id): missing BioGeoBEARS golden row in $goldenPath"
        }
        if ($goldenRow.exponent_parameter -ne $parameter) {
            throw "$($case.case_id): golden exponent parameter does not match manifest"
        }
        if ($goldenRow.strategy -ne $case.biogeobears_strategy) {
            throw "$($case.case_id): golden BioGeoBEARS strategy does not match manifest"
        }
        if ($goldenRow.convergence -ne "0") {
            throw "$($case.case_id): BioGeoBEARS optimization did not converge (code=$($goldenRow.convergence))"
        }
        if ($goldenRow.exponent_bound -ne $case.expected_exponent_bound) {
            throw "$($case.case_id): BioGeoBEARS bound classification is $($goldenRow.exponent_bound), expected $($case.expected_exponent_bound)"
        }

        $command = switch ($parameter) {
            "x" { "dec-x-optimize" }
            "n" { "dec-n-optimize" }
            "u" { "dec-u-optimize" }
        }
        $optimizationArgs = @(
            "run", "-q", "-p", "biogeo-cli", "--",
            $command,
            "--tree", (Join-Path $repoRoot $case.tree),
            "--ranges", (Join-Path $repoRoot $case.ranges),
            "--max-range-size", $case.max_range_size,
            "--root-prior", $case.root_prior,
            "--init-d", $case.init_d,
            "--init-e", $case.init_e,
            "--min-rate", $case.min_rate,
            "--max-rate", $case.max_rate,
            "--init-exponent", $case.init_exponent,
            "--min-exponent", $case.min_exponent,
            "--max-exponent", $case.max_exponent,
            "--initial-exponent-step", $case.initial_exponent_step,
            "--max-iterations", $case.max_iterations,
            "--multi-start-points", ([string]$MultiStartPoints)
        )
        if ($case.include_null_range -eq "true") {
            $optimizationArgs += "--include-null-range"
        }
        $optimizationArgs += Get-ModifierArguments `
            -Case $case `
            -ExponentParameter $parameter `
            -Optimizing $true

        $optimized = Invoke-BiogeoCli -Arguments $optimizationArgs
        foreach ($key in @(
            "lnL",
            "d",
            "e",
            "exponent",
            "exponent_bound",
            "converged",
            "converged_starts",
            "starts"
        )) {
            if (-not $optimized.ContainsKey($key)) {
                throw "$($case.case_id): optimized CLI output did not contain $key"
            }
        }
        if ($optimized["converged"] -ne "true") {
            throw "$($case.case_id): Rust optimization did not converge"
        }
        if ([int]$optimized["converged_starts"] -lt [int]$case.min_converged_starts) {
            throw "$($case.case_id): only $($optimized['converged_starts'])/$($optimized['starts']) Rust starts converged"
        }
        if ($optimized["exponent_bound"] -ne $case.expected_exponent_bound) {
            throw "$($case.case_id): Rust bound classification is $($optimized['exponent_bound']), expected $($case.expected_exponent_bound)"
        }

        $rustLnL = [double]$optimized["lnL"]
        $bgbLnL = [double]$goldenRow.biogeobears_lnL
        $lnLDelta = [Math]::Abs($rustLnL - $bgbLnL)
        if ($lnLDelta -gt $LnLTolerance) {
            throw "$($case.case_id): optimized lnL mismatch rust=$rustLnL biogeobears=$bgbLnL delta=$lnLDelta tolerance=$LnLTolerance"
        }

        $caseExponentTolerance = $ExponentTolerance
        if (-not [string]::IsNullOrWhiteSpace($case.exponent_tolerance)) {
            $caseExponentTolerance = [double]$case.exponent_tolerance
        }
        $exponentDelta = [Math]::Abs(
            [double]$optimized["exponent"] - [double]$goldenRow.biogeobears_exponent
        )
        if ($exponentDelta -gt $caseExponentTolerance) {
            throw "$($case.case_id): exponent mismatch delta=$exponentDelta tolerance=$caseExponentTolerance"
        }

        $fixedArgs = @(
            "run", "-q", "-p", "biogeo-cli", "--",
            "dec",
            "--tree", (Join-Path $repoRoot $case.tree),
            "--ranges", (Join-Path $repoRoot $case.ranges),
            "--d", $goldenRow.biogeobears_d,
            "--e", $goldenRow.biogeobears_e,
            "--max-range-size", $case.max_range_size,
            "--root-prior", $case.root_prior
        )
        if ($case.include_null_range -eq "true") {
            $fixedArgs += "--include-null-range"
        }
        $fixedArgs += Get-ModifierArguments `
            -Case $case `
            -ExponentParameter $parameter `
            -Optimizing $false `
            -OptimizedExponent $goldenRow.biogeobears_exponent

        $fixed = Invoke-BiogeoCli -Arguments $fixedArgs
        if (-not $fixed.ContainsKey("lnL")) {
            throw "$($case.case_id): fixed CLI output did not contain lnL"
        }
        $fixedLnLDelta = [Math]::Abs([double]$fixed["lnL"] - $bgbLnL)
        if ($fixedLnLDelta -gt $FixedLnLTolerance) {
            throw "$($case.case_id): Rust likelihood at BioGeoBEARS parameters mismatch delta=$fixedLnLDelta tolerance=$FixedLnLTolerance"
        }

        $dDelta = [Math]::Abs([double]$optimized["d"] - [double]$goldenRow.biogeobears_d)
        $eDelta = [Math]::Abs([double]$optimized["e"] - [double]$goldenRow.biogeobears_e)
        Write-Host "$($case.case_id) ok lnL_delta=$lnLDelta fixed_lnL_delta=$fixedLnLDelta exponent_delta=$exponentDelta d_delta=$dDelta e_delta=$eDelta starts=$($optimized['converged_starts'])/$($optimized['starts']) bound=$($optimized['exponent_bound'])"
    }
}
finally {
    Pop-Location
}

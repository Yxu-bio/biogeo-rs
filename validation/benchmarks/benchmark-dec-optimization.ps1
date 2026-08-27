param(
    [Parameter(Mandatory = $true)]
    [string]$Tree,
    [Parameter(Mandatory = $true)]
    [string]$Ranges,
    [int]$MaxRangeSize,
    [bool]$IncludeNullRange = $true,
    [double]$Mx01 = 0.0001,
    [double]$InitD = 0.01,
    [double]$InitE = 0.01,
    [double]$MinRate = 1e-12,
    [double]$MaxRate = 10.0,
    [double]$InitialLogStep = 0.5,
    [double]$Tolerance = 1e-8,
    [int]$MaxIterations = 200,
    [int]$MultiStartPoints = 1,
    [int]$RustRepeats = 3,
    [int]$BioGeoBEARSRepeats = 1,
    [double]$LikelihoodTolerance = 0.0001,
    [string]$OutputDirectory = ""
)

$ErrorActionPreference = "Stop"
$culture = [Globalization.CultureInfo]::InvariantCulture

function Format-Number([double]$Value) {
    return $Value.ToString("0.######", $culture)
}

function Resolve-InputPath([string]$Path, [string]$RepoRoot) {
    if ([IO.Path]::IsPathRooted($Path)) {
        return (Resolve-Path $Path).Path
    }
    return (Resolve-Path (Join-Path $RepoRoot $Path)).Path
}

function Convert-KeyValueOutput([object[]]$Output) {
    $values = @{}
    foreach ($line in $Output) {
        $parts = [string]$line -split "`t", 2
        if ($parts.Count -eq 2) {
            $values[$parts[0]] = $parts[1]
        }
    }
    return $values
}

function Require-Value($Values, [string]$Name) {
    if (-not $Values.ContainsKey($Name)) {
        throw "CLI output did not contain $Name"
    }
    return $Values[$Name]
}

function Invoke-RustOptimization([string]$RustExe, [string]$TreePath, [string]$RangesPath, [int]$Iteration) {
    $args = @(
        "dec-optimize",
        "--tree", $TreePath,
        "--ranges", $RangesPath,
        "--max-range-size", $MaxRangeSize.ToString($culture),
        "--root-prior", "flat",
        "--mx01", $Mx01.ToString($culture),
        "--init-d", $InitD.ToString($culture),
        "--init-e", $InitE.ToString($culture),
        "--min-rate", $MinRate.ToString($culture),
        "--max-rate", $MaxRate.ToString($culture),
        "--initial-log-step", $InitialLogStep.ToString($culture),
        "--tolerance", $Tolerance.ToString($culture),
        "--max-iterations", $MaxIterations.ToString($culture),
        "--multi-start-points", $MultiStartPoints.ToString($culture)
    )
    if ($IncludeNullRange) {
        $args += "--include-null-range"
    }

    $stopwatch = [Diagnostics.Stopwatch]::StartNew()
    $output = & $RustExe @args 2>&1
    $exitCode = $LASTEXITCODE
    $stopwatch.Stop()
    if ($exitCode -ne 0) {
        throw "Rust CLI exited with code $exitCode`n$($output -join "`n")"
    }

    $values = Convert-KeyValueOutput $output
    [pscustomobject]@{
        tool = "rust"
        iteration = $Iteration
        seconds = $stopwatch.Elapsed.TotalSeconds.ToString("0.######", $culture)
        lnL = Require-Value $values "lnL"
        d = Require-Value $values "d"
        e = Require-Value $values "e"
        states = Require-Value $values "states"
        areas = Require-Value $values "areas"
        tips = Require-Value $values "tips"
        evaluations = Require-Value $values "evaluations"
        gradient_evaluations = "NA"
        iterations = Require-Value $values "iterations"
        converged = Require-Value $values "converged"
        optimizer = "nelder-mead-log-rate"
    }
}

function Write-TimingTable($Rows, [string]$Path) {
    $lines = [Collections.Generic.List[string]]::new()
    $lines.Add("tool`titeration`tseconds`tlnL`td`te`tevaluations`tgradient_evaluations`titerations`tconverged`toptimizer")
    foreach ($row in $Rows) {
        $lines.Add(
            "$($row.tool)`t$($row.iteration)`t$($row.seconds)`t$($row.lnL)`t$($row.d)`t$($row.e)`t$($row.evaluations)`t$($row.gradient_evaluations)`t$($row.iterations)`t$($row.converged)`t$($row.optimizer)"
        )
    }
    Set-Content -Path $Path -Value $lines -Encoding utf8
}

function Get-Mean([double[]]$Values) {
    return (($Values | Measure-Object -Average).Average)
}

function Get-Median([double[]]$Values) {
    $sorted = @($Values | Sort-Object)
    $middle = [int][Math]::Floor($sorted.Count / 2)
    if (($sorted.Count % 2) -eq 1) {
        return $sorted[$middle]
    }
    return (($sorted[$middle - 1] + $sorted[$middle]) / 2.0)
}

function Parse-Number([string]$Value, [string]$Name) {
    $parsed = 0.0
    if (-not [double]::TryParse($Value, [Globalization.NumberStyles]::Float, $culture, [ref]$parsed)) {
        throw "Invalid numeric $Name value: $Value"
    }
    return $parsed
}

function Test-TrueValue($Value) {
    return @("true", "1", "yes") -contains ([string]$Value).ToLowerInvariant()
}

function Get-OptionalMeanEvaluationSeconds($Rows) {
    $values = @()
    foreach ($row in $Rows) {
        $evaluations = 0
        if ([int]::TryParse([string]$row.evaluations, [ref]$evaluations) -and $evaluations -gt 0) {
            $seconds = Parse-Number ([string]$row.seconds) "seconds"
            $values += $seconds / $evaluations
        }
    }
    if ($values.Count -eq 0) {
        return $null
    }
    return Get-Mean ([double[]]$values)
}

if ($MaxRangeSize -lt 1) {
    throw "MaxRangeSize must be positive"
}
if ([double]::IsNaN($Mx01) -or [double]::IsInfinity($Mx01) -or $Mx01 -lt 0.00001 -or $Mx01 -gt 0.99999) {
    throw "Mx01 must be finite and between 0.00001 and 0.99999"
}
if ($MinRate -le 0 -or $MinRate -ge $MaxRate) {
    throw "MinRate and MaxRate must be positive and increasing"
}
if ($InitD -le $MinRate -or $InitD -ge $MaxRate -or $InitE -le $MinRate -or $InitE -ge $MaxRate) {
    throw "InitD and InitE must be strictly inside the rate bounds"
}
if ($InitialLogStep -le 0 -or $Tolerance -le 0) {
    throw "InitialLogStep and Tolerance must be positive"
}
if ($MaxIterations -lt 1 -or $MultiStartPoints -lt 1) {
    throw "MaxIterations and MultiStartPoints must be positive"
}
if ($RustRepeats -lt 1 -or $BioGeoBEARSRepeats -lt 1) {
    throw "RustRepeats and BioGeoBEARSRepeats must be positive"
}
if ($LikelihoodTolerance -lt 0) {
    throw "LikelihoodTolerance must be non-negative"
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$treePath = Resolve-InputPath $Tree $repoRoot
$rangesPath = Resolve-InputPath $Ranges $repoRoot
$mxCase = (Format-Number $Mx01) -replace "\.", "p"
$inputCase = Split-Path (Split-Path $treePath -Parent) -Leaf
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $runRoot = Join-Path $repoRoot "validation/benchmark-runs/$inputCase-optimization-mx$mxCase"
} elseif ([IO.Path]::IsPathRooted($OutputDirectory)) {
    $runRoot = $OutputDirectory
} else {
    $runRoot = Join-Path $repoRoot $OutputDirectory
}
New-Item -ItemType Directory -Force -Path $runRoot | Out-Null

$rustTimesPath = Join-Path $runRoot "rust-optimization-times.tsv"
$bgbTimesPath = Join-Path $runRoot "biogeobears-optimization-times.tsv"
$summaryPath = Join-Path $runRoot "summary.tsv"

Write-Host "DEC optimization benchmark"
Write-Host "Tree: $treePath"
Write-Host "Ranges: $rangesPath"
Write-Host "max_range_size=$MaxRangeSize mx01=$(Format-Number $Mx01) include_null_range=$IncludeNullRange"

Push-Location $repoRoot
try {
    Write-Host "Building release Rust CLI..."
    & cargo build --release -q -p biogeo-cli
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with code $LASTEXITCODE"
    }

    $rustExe = Join-Path $repoRoot "target/release/biogeo-cli.exe"
    $rustRows = @()
    for ($iteration = 1; $iteration -le $RustRepeats; $iteration++) {
        $row = Invoke-RustOptimization $rustExe $treePath $rangesPath $iteration
        $rustRows += $row
        Write-Host "Rust optimization $iteration seconds=$($row.seconds) lnL=$($row.lnL) d=$($row.d) e=$($row.e) evaluations=$($row.evaluations)"
    }
    Write-TimingTable $rustRows $rustTimesPath

    Write-Host "Running BioGeoBEARS optimization benchmark..."
    $rArgs = @(
        "validation/biogeobears/benchmark-biogeobears-dec-optimize.R",
        $treePath,
        $rangesPath,
        $MaxRangeSize.ToString($culture),
        $IncludeNullRange.ToString().ToLowerInvariant(),
        $Mx01.ToString($culture),
        $InitD.ToString($culture),
        $InitE.ToString($culture),
        $MinRate.ToString($culture),
        $MaxRate.ToString($culture),
        $BioGeoBEARSRepeats.ToString($culture),
        $bgbTimesPath
    )
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $rOutput = & Rscript @rArgs 2>&1
        $rExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    if ($rExitCode -ne 0) {
        throw "Rscript exited with code $rExitCode`n$($rOutput -join "`n")"
    }
    $rOutput | ForEach-Object { Write-Host $_ }
}
finally {
    Pop-Location
}

$bgbRows = Import-Csv -Path $bgbTimesPath -Delimiter "`t"
$rustSeconds = [double[]]@($rustRows | ForEach-Object { Parse-Number ([string]$_.seconds) "rust seconds" })
$bgbSeconds = [double[]]@($bgbRows | ForEach-Object { Parse-Number ([string]$_.seconds) "BioGeoBEARS seconds" })
$rustMean = Get-Mean $rustSeconds
$rustMedian = Get-Median $rustSeconds
$bgbMean = Get-Mean $bgbSeconds
$bgbMedian = Get-Median $bgbSeconds
$totalMeanSpeedup = $bgbMean / $rustMean
$totalMedianSpeedup = $bgbMedian / $rustMedian
$rustPerEvaluation = Get-OptionalMeanEvaluationSeconds $rustRows
$bgbPerEvaluation = Get-OptionalMeanEvaluationSeconds $bgbRows
if ($null -ne $rustPerEvaluation -and $null -ne $bgbPerEvaluation) {
    $perEvaluationSpeedup = $bgbPerEvaluation / $rustPerEvaluation
} else {
    $perEvaluationSpeedup = $null
}

$rustResult = $rustRows[0]
$bgbResult = $bgbRows[0]
$rustLnL = Parse-Number ([string]$rustResult.lnL) "rust lnL"
$bgbLnL = Parse-Number ([string]$bgbResult.lnL) "BioGeoBEARS lnL"
$lnLDelta = [Math]::Abs($rustLnL - $bgbLnL)
$dDelta = [Math]::Abs(
    (Parse-Number ([string]$rustResult.d) "rust d") -
    (Parse-Number ([string]$bgbResult.d) "BioGeoBEARS d")
)
$eDelta = [Math]::Abs(
    (Parse-Number ([string]$rustResult.e) "rust e") -
    (Parse-Number ([string]$bgbResult.e) "BioGeoBEARS e")
)
$rustAllConverged = @($rustRows | Where-Object { -not (Test-TrueValue $_.converged) }).Count -eq 0
$bgbAllConverged = @($bgbRows | Where-Object { -not (Test-TrueValue $_.converged) }).Count -eq 0

$summaryLines = [Collections.Generic.List[string]]::new()
$summaryLines.Add("metric`tvalue")
$summaryLines.Add("tree`t$treePath")
$summaryLines.Add("ranges`t$rangesPath")
$summaryLines.Add("max_range_size`t$MaxRangeSize")
$summaryLines.Add("include_null_range`t$IncludeNullRange")
$summaryLines.Add("mx01`t$(Format-Number $Mx01)")
$summaryLines.Add("states`t$($rustResult.states)")
$summaryLines.Add("areas`t$($rustResult.areas)")
$summaryLines.Add("tips`t$($rustResult.tips)")
$summaryLines.Add("init_d`t$($InitD.ToString('G17', $culture))")
$summaryLines.Add("init_e`t$($InitE.ToString('G17', $culture))")
$summaryLines.Add("min_rate`t$($MinRate.ToString('G17', $culture))")
$summaryLines.Add("max_rate`t$($MaxRate.ToString('G17', $culture))")
$summaryLines.Add("rust_optimizer`tnelder-mead-log-rate")
$summaryLines.Add("biogeobears_optimizer`t$($bgbResult.optimizer)")
$summaryLines.Add("rust_repeats`t$RustRepeats")
$summaryLines.Add("biogeobears_repeats`t$BioGeoBEARSRepeats")
$summaryLines.Add("rust_mean_seconds`t$(Format-Number $rustMean)")
$summaryLines.Add("rust_median_seconds`t$(Format-Number $rustMedian)")
$summaryLines.Add("biogeobears_mean_seconds`t$(Format-Number $bgbMean)")
$summaryLines.Add("biogeobears_median_seconds`t$(Format-Number $bgbMedian)")
$summaryLines.Add("total_mean_speedup_biogeobears_over_rust`t$(Format-Number $totalMeanSpeedup)")
$summaryLines.Add("total_median_speedup_biogeobears_over_rust`t$(Format-Number $totalMedianSpeedup)")
$summaryLines.Add("rust_mean_seconds_per_reported_evaluation`t$(if ($null -eq $rustPerEvaluation) { 'NA' } else { $rustPerEvaluation.ToString('G17', $culture) })")
$summaryLines.Add("biogeobears_mean_seconds_per_reported_evaluation`t$(if ($null -eq $bgbPerEvaluation) { 'NA' } else { $bgbPerEvaluation.ToString('G17', $culture) })")
$summaryLines.Add("reported_evaluation_cost_speedup`t$(if ($null -eq $perEvaluationSpeedup) { 'NA' } else { Format-Number $perEvaluationSpeedup })")
$summaryLines.Add("rust_lnL`t$($rustResult.lnL)")
$summaryLines.Add("biogeobears_lnL`t$($bgbResult.lnL)")
$summaryLines.Add("lnL_abs_delta`t$($lnLDelta.ToString('G17', $culture))")
$summaryLines.Add("lnL_tolerance`t$($LikelihoodTolerance.ToString('G17', $culture))")
$summaryLines.Add("rust_d`t$($rustResult.d)")
$summaryLines.Add("biogeobears_d`t$($bgbResult.d)")
$summaryLines.Add("d_abs_delta`t$($dDelta.ToString('G17', $culture))")
$summaryLines.Add("rust_e`t$($rustResult.e)")
$summaryLines.Add("biogeobears_e`t$($bgbResult.e)")
$summaryLines.Add("e_abs_delta`t$($eDelta.ToString('G17', $culture))")
$summaryLines.Add("rust_evaluations`t$($rustResult.evaluations)")
$summaryLines.Add("biogeobears_evaluations`t$($bgbResult.evaluations)")
$summaryLines.Add("biogeobears_gradient_evaluations`t$($bgbResult.gradient_evaluations)")
$summaryLines.Add("rust_iterations`t$($rustResult.iterations)")
$summaryLines.Add("biogeobears_iterations`t$($bgbResult.iterations)")
$summaryLines.Add("rust_converged`t$($rustResult.converged)")
$summaryLines.Add("biogeobears_converged`t$($bgbResult.converged)")
$summaryLines.Add("rust_all_repeats_converged`t$rustAllConverged")
$summaryLines.Add("biogeobears_all_repeats_converged`t$bgbAllConverged")
Set-Content -Path $summaryPath -Value $summaryLines -Encoding utf8

Write-Host ""
Write-Host "Summary written: $summaryPath"
Write-Host "Rust optimization mean seconds: $(Format-Number $rustMean)"
Write-Host "BioGeoBEARS optimization mean seconds: $(Format-Number $bgbMean)"
Write-Host "Total optimization speedup (BioGeoBEARS / Rust): $(Format-Number $totalMeanSpeedup)x"
Write-Host "Optimized lnL absolute delta: $($lnLDelta.ToString('G17', $culture))"
if ($null -ne $perEvaluationSpeedup) {
    Write-Host "Reported evaluation-cost speedup: $(Format-Number $perEvaluationSpeedup)x"
}
if (-not $rustAllConverged -or -not $bgbAllConverged) {
    throw "Optimization benchmark is invalid because at least one run did not converge"
}
if ($lnLDelta -gt $LikelihoodTolerance) {
    throw "Optimized lnL mismatch: absolute delta $lnLDelta exceeds tolerance $LikelihoodTolerance"
}

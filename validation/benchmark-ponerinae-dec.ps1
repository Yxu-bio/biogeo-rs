param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$DatasetDir,

    [ValidateSet("evaluate", "optimize")]
    [string]$Mode = "evaluate",

    [string]$CliPath = "target/release/biogeo-cli.exe",
    [string]$RscriptPath = "Rscript",
    [string]$OutputDirectory = "",
    [int]$RustRepeats = 1,
    [int]$BioGeoBEARSRepeats = 1,
    [switch]$SkipBioGeoBEARS,
    [double]$D = 0.01,
    [double]$E = 0.01,
    [double]$MinRate = 1e-12,
    [double]$MaxRate = 10.0,
    [double]$Mx01 = 0.0001,
    [double]$InitialLogStep = 0.5,
    [double]$Tolerance = 1e-8,
    [int]$MaxIterations = 200,
    [int]$MultiStartPoints = 1,
    [double]$LikelihoodTolerance = 0.0001
)

$ErrorActionPreference = "Stop"
$culture = [Globalization.CultureInfo]::InvariantCulture
$repoRoot = Split-Path -Parent $PSScriptRoot

function Resolve-RepoPath {
    param([string]$Path)
    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
}

function Require-File {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Required input does not exist: $Path"
    }
}

function Format-Number {
    param([double]$Value)
    return $Value.ToString("G17", $culture)
}

function Convert-KeyValueOutput {
    param([object[]]$Output)
    $values = @{}
    foreach ($line in $Output) {
        $parts = [string]$line -split "`t", 2
        if ($parts.Count -eq 2) {
            $values[$parts[0]] = $parts[1]
        }
    }
    return $values
}

function Require-Value {
    param($Values, [string]$Name)
    if (-not $Values.ContainsKey($Name)) {
        throw "CLI output did not contain $Name"
    }
    return [string]$Values[$Name]
}

function Parse-Number {
    param([string]$Value, [string]$Name)
    $parsed = 0.0
    if (-not [double]::TryParse(
            $Value,
            [Globalization.NumberStyles]::Float,
            $culture,
            [ref]$parsed
        )) {
        throw "Invalid numeric $Name value: $Value"
    }
    return $parsed
}

function Get-Mean {
    param([double[]]$Values)
    return ($Values | Measure-Object -Average).Average
}

function Get-Median {
    param([double[]]$Values)
    $sorted = @($Values | Sort-Object)
    $middle = [int][Math]::Floor($sorted.Count / 2)
    if (($sorted.Count % 2) -eq 1) {
        return $sorted[$middle]
    }
    return ($sorted[$middle - 1] + $sorted[$middle]) / 2.0
}

function Test-TrueValue {
    param($Value)
    return @("true", "1", "yes") -contains ([string]$Value).ToLowerInvariant()
}

function Invoke-Biogeo {
    param([string[]]$Arguments)
    $stopwatch = [Diagnostics.Stopwatch]::StartNew()
    $output = & $cli @Arguments 2>&1
    $exitCode = $LASTEXITCODE
    $stopwatch.Stop()
    if ($exitCode -ne 0) {
        throw "biogeo-cli failed with exit code ${exitCode}: $($Arguments -join ' ')`n$($output -join "`n")"
    }
    return [pscustomobject]@{
        Output = $output
        Seconds = $stopwatch.Elapsed.TotalSeconds
    }
}

if ($RustRepeats -lt 1 -or $BioGeoBEARSRepeats -lt 1) {
    throw "Repeat counts must be positive"
}
if ($D -le 0 -or $E -le 0 -or $MinRate -le 0 -or $MinRate -ge $MaxRate) {
    throw "d/e and rate bounds must be positive, with increasing bounds"
}
if ($Mode -eq "optimize" -and
    ($D -le $MinRate -or $D -ge $MaxRate -or $E -le $MinRate -or $E -ge $MaxRate)) {
    throw "Optimization initial d/e values must be strictly inside the bounds"
}
if ($Mx01 -lt 0.00001 -or $Mx01 -gt 0.99999) {
    throw "Mx01 must be between 0.00001 and 0.99999"
}
if ($MaxIterations -lt 1 -or $MultiStartPoints -lt 1 -or
    $InitialLogStep -le 0 -or $Tolerance -le 0 -or $LikelihoodTolerance -lt 0) {
    throw "Optimization controls and likelihood tolerance are invalid"
}

$dataset = [System.IO.Path]::GetFullPath($DatasetDir)
$finalInputs = Join-Path $dataset "final_inputs"
$tree = Join-Path $finalInputs "Ponerinae_MCC_phylogeny_1534t_short_names.tree"
$ranges = Join-Path $finalInputs "lagrange_area_data_file_7_regions_PaleA.data"
$boundaries = Join-Path $finalInputs "time_boundaries.txt"
$adjacency = Join-Path $finalInputs "Dore_2024_BioGeoBears_Adjacency_matrix_7areas_7TS.txt"
$cli = Resolve-RepoPath $CliPath
$rScript = Join-Path $repoRoot "validation/benchmark-biogeobears-ponerinae-dec.R"

foreach ($path in @($tree, $ranges, $boundaries, $adjacency, $rScript)) {
    Require-File $path
}
if (-not (Test-Path -LiteralPath $cli -PathType Leaf)) {
    Push-Location $repoRoot
    try {
        & cargo build --release -q -p biogeo-cli
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
}
Require-File $cli

if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $stamp = [DateTime]::Now.ToString("yyyyMMdd-HHmmss", $culture)
    $runRoot = Join-Path $repoRoot "validation/benchmark-runs/ponerinae-dec-$Mode-$stamp"
} else {
    $runRoot = Resolve-RepoPath $OutputDirectory
}
if (Test-Path -LiteralPath $runRoot) {
    throw "Output directory already exists: $runRoot"
}
New-Item -ItemType Directory -Path $runRoot | Out-Null
$strataDir = Join-Path $runRoot "strata"

$import = Invoke-Biogeo @(
    "convert-biogeobears-strata",
    "--time-boundaries", $boundaries,
    "--adjacency-matrices", $adjacency,
    "--adjacency-range-rule", "edge-covered",
    "--max-range-size", "5",
    "--output-dir", $strataDir
)
$importValues = Convert-KeyValueOutput $import.Output
$allowedCounts = Require-Value $importValues "allowed_range_counts"
if ($allowedCounts -ne "36,36,27,20,24,20,38") {
    throw "Ponerinae allowed-state counts drifted: $allowedCounts"
}
$strata = Join-Path $strataDir "strata.tsv"

$inputValidation = Invoke-Biogeo @(
    "validate-inputs",
    "--tree", $tree,
    "--ranges", $ranges
)
[System.IO.File]::WriteAllLines(
    (Join-Path $runRoot "input-validation.tsv"),
    [string[]]$inputValidation.Output,
    [System.Text.UTF8Encoding]::new($false)
)

$rustRows = @()
for ($iteration = 1; $iteration -le $RustRepeats; $iteration++) {
    if ($Mode -eq "evaluate") {
        $arguments = @(
            "dec",
            "--tree", $tree,
            "--ranges", $ranges,
            "--d", (Format-Number $D),
            "--e", (Format-Number $E),
            "--max-range-size", "5",
            "--include-null-range",
            "--root-prior", "flat",
            "--mx01", (Format-Number $Mx01),
            "--dispersal-strata", $strata
        )
    } else {
        $arguments = @(
            "dec-optimize",
            "--tree", $tree,
            "--ranges", $ranges,
            "--max-range-size", "5",
            "--include-null-range",
            "--root-prior", "flat",
            "--mx01", (Format-Number $Mx01),
            "--init-d", (Format-Number $D),
            "--init-e", (Format-Number $E),
            "--min-rate", (Format-Number $MinRate),
            "--max-rate", (Format-Number $MaxRate),
            "--initial-log-step", (Format-Number $InitialLogStep),
            "--tolerance", (Format-Number $Tolerance),
            "--max-iterations", $MaxIterations.ToString($culture),
            "--multi-start-points", $MultiStartPoints.ToString($culture),
            "--dispersal-strata", $strata
        )
    }

    $result = Invoke-Biogeo $arguments
    $values = Convert-KeyValueOutput $result.Output
    $rustRows += [pscustomobject]@{
        tool = "rust"
        mode = $Mode
        iteration = $iteration
        process_seconds = $result.Seconds
        lnL = Require-Value $values "lnL"
        d = Require-Value $values "d"
        e = Require-Value $values "e"
        states = Require-Value $values "states"
        strata = "7"
        allowed_range_counts = $allowedCounts
        evaluations = if ($Mode -eq "evaluate") { "1" } else { Require-Value $values "evaluations" }
        iterations = if ($Mode -eq "evaluate") { "0" } else { Require-Value $values "iterations" }
        converged = if ($Mode -eq "evaluate") { "true" } else { Require-Value $values "converged" }
        optimizer = if ($Mode -eq "evaluate") { "none" } else { "nelder-mead-log-rate" }
    }
}
$rustRows | Export-Csv -LiteralPath (Join-Path $runRoot "rust.tsv") -Delimiter "`t" -NoTypeInformation
$rustFailures = @($rustRows | Where-Object { -not (Test-TrueValue $_.converged) })
if ($rustFailures.Count -gt 0) {
    throw "Rust did not converge in every repeated run"
}

$bgbRows = @()
$bgbProcessSeconds = $null
if (-not $SkipBioGeoBEARS) {
    $bgbPath = Join-Path $runRoot "biogeobears.tsv"
    $rArguments = @(
        $rScript,
        $tree,
        $ranges,
        $boundaries,
        $strata,
        $Mode,
        (Format-Number $D),
        (Format-Number $E),
        (Format-Number $MinRate),
        (Format-Number $MaxRate),
        (Format-Number $Mx01),
        $BioGeoBEARSRepeats.ToString($culture),
        $bgbPath
    )
    $stopwatch = [Diagnostics.Stopwatch]::StartNew()
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $rOutput = & $RscriptPath @rArguments 2>&1
        $rExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
        $stopwatch.Stop()
    }
    [System.IO.File]::WriteAllLines(
        (Join-Path $runRoot "biogeobears-process-output.txt"),
        [string[]]$rOutput,
        [System.Text.UTF8Encoding]::new($false)
    )
    if ($rExitCode -ne 0) {
        throw "BioGeoBEARS benchmark failed with exit code ${rExitCode}:`n$($rOutput -join "`n")"
    }
    $bgbProcessSeconds = $stopwatch.Elapsed.TotalSeconds
    $bgbRows = @(Import-Csv -LiteralPath $bgbPath -Delimiter "`t")
}

$summary = [Collections.Generic.List[string]]::new()
$summary.Add("metric`tvalue")
$summary.Add("format`tbiogeo-ponerinae-dec-benchmark-v1")
$summary.Add("mode`t$Mode")
$summary.Add("tree`t$tree")
$summary.Add("ranges`t$ranges")
$summary.Add("states`t120")
$summary.Add("strata`t7")
$summary.Add("allowed_range_counts`t$allowedCounts")
$summary.Add("mx01`t$(Format-Number $Mx01)")
$summary.Add("rust_repeats`t$RustRepeats")
$rustProcessSeconds = [double[]]@($rustRows.process_seconds)
$rustMeanProcessSeconds = Get-Mean $rustProcessSeconds
$rustMedianProcessSeconds = Get-Median $rustProcessSeconds
$rustEvaluations = Parse-Number $rustRows[0].evaluations "Rust evaluations"
$rustSecondsPerEvaluation = $rustMeanProcessSeconds / $rustEvaluations
$summary.Add("rust_mean_process_seconds`t$(Format-Number $rustMeanProcessSeconds)")
$summary.Add("rust_median_process_seconds`t$(Format-Number $rustMedianProcessSeconds)")
$summary.Add("rust_mean_process_seconds_per_reported_evaluation`t$(Format-Number $rustSecondsPerEvaluation)")
$summary.Add("rust_lnL`t$($rustRows[0].lnL)")
$summary.Add("rust_d`t$($rustRows[0].d)")
$summary.Add("rust_e`t$($rustRows[0].e)")
$summary.Add("rust_evaluations`t$($rustRows[0].evaluations)")
$summary.Add("rust_iterations`t$($rustRows[0].iterations)")
$summary.Add("rust_converged`t$($rustRows[0].converged)")

if ($SkipBioGeoBEARS) {
    $summary.Add("biogeobears_status`tskipped")
} else {
    $rustLnL = Parse-Number $rustRows[0].lnL "Rust lnL"
    $bgbLnL = Parse-Number $bgbRows[0].lnL "BioGeoBEARS lnL"
    $lnLDelta = [Math]::Abs($rustLnL - $bgbLnL)
    $rustMean = $rustMeanProcessSeconds
    $bgbEngineMean = Get-Mean ([double[]]@($bgbRows | ForEach-Object {
        Parse-Number $_.seconds "BioGeoBEARS engine seconds"
    }))
    $bgbMeanProcessPerRepeat = $bgbProcessSeconds / $BioGeoBEARSRepeats
    $bgbEvaluations = Parse-Number $bgbRows[0].evaluations "BioGeoBEARS evaluations"
    $bgbEngineSecondsPerEvaluation = $bgbEngineMean / $bgbEvaluations

    $summary.Add("biogeobears_status`tcompleted")
    $summary.Add("biogeobears_repeats`t$BioGeoBEARSRepeats")
    $summary.Add("biogeobears_mean_engine_seconds`t$(Format-Number $bgbEngineMean)")
    $summary.Add("biogeobears_total_process_seconds`t$(Format-Number $bgbProcessSeconds)")
    $summary.Add("biogeobears_mean_process_seconds_per_repeat`t$(Format-Number $bgbMeanProcessPerRepeat)")
    $summary.Add("biogeobears_mean_engine_seconds_per_reported_evaluation`t$(Format-Number $bgbEngineSecondsPerEvaluation)")
    $summary.Add("biogeobears_lnL`t$($bgbRows[0].lnL)")
    $summary.Add("biogeobears_d`t$($bgbRows[0].d)")
    $summary.Add("biogeobears_e`t$($bgbRows[0].e)")
    $summary.Add("biogeobears_evaluations`t$($bgbRows[0].evaluations)")
    $summary.Add("biogeobears_iterations`t$($bgbRows[0].iterations)")
    $summary.Add("biogeobears_converged`t$($bgbRows[0].converged)")
    $summary.Add("biogeobears_optimizer`t$($bgbRows[0].optimizer)")
    $summary.Add("d_abs_delta`t$(Format-Number ([Math]::Abs(
        (Parse-Number $rustRows[0].d 'Rust d') - (Parse-Number $bgbRows[0].d 'BioGeoBEARS d')
    )))")
    $summary.Add("e_abs_delta`t$(Format-Number ([Math]::Abs(
        (Parse-Number $rustRows[0].e 'Rust e') - (Parse-Number $bgbRows[0].e 'BioGeoBEARS e')
    )))")
    $summary.Add("lnL_abs_delta`t$(Format-Number $lnLDelta)")
    $summary.Add("lnL_tolerance`t$(Format-Number $LikelihoodTolerance)")
    $summary.Add("process_speedup_biogeobears_over_rust`t$(Format-Number ($bgbMeanProcessPerRepeat / $rustMean))")
    $summary.Add("engine_to_rust_process_ratio`t$(Format-Number ($bgbEngineMean / $rustMean))")
    $summary.Add("reported_evaluation_cost_ratio`t$(Format-Number ($bgbEngineSecondsPerEvaluation / $rustSecondsPerEvaluation))")
    if ($lnLDelta -gt $LikelihoodTolerance) {
        throw "Rust/BioGeoBEARS lnL delta $lnLDelta exceeds tolerance $LikelihoodTolerance"
    }
    if ($Mode -eq "optimize" -and -not (Test-TrueValue $bgbRows[0].converged)) {
        throw "BioGeoBEARS optimization did not report convergence"
    }
}

$summaryPath = Join-Path $runRoot "summary.tsv"
[System.IO.File]::WriteAllLines(
    $summaryPath,
    [string[]]$summary,
    [System.Text.UTF8Encoding]::new($false)
)

Write-Output "format`tbiogeo-ponerinae-dec-benchmark-v1"
Write-Output "mode`t$Mode"
Write-Output "status`tpassed"
Write-Output "rust_lnL`t$($rustRows[0].lnL)"
Write-Output "rust_mean_process_seconds`t$(Format-Number $rustMeanProcessSeconds)"
Write-Output "rust_median_process_seconds`t$(Format-Number $rustMedianProcessSeconds)"
Write-Output "biogeobears_status`t$(if ($SkipBioGeoBEARS) { 'skipped' } else { 'completed' })"
if (-not $SkipBioGeoBEARS) {
    Write-Output "biogeobears_lnL`t$($bgbRows[0].lnL)"
    Write-Output "biogeobears_total_process_seconds`t$(Format-Number $bgbProcessSeconds)"
}
Write-Output "output_directory`t$runRoot"

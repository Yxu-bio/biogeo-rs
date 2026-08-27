param(
    [int]$SampleCount = 20000,
    [int64]$Seed = 20260718,
    [string]$Threads = "auto",
    [double]$ZLimit = 7.0,
    [double]$NodeTvLimit = 0.04,
    [double]$SplitTvLimit = 0.06,
    [string]$Report = "validation/benchmark-runs/detection-full-stack-bsm-fixnode-report.tsv"
)

$ErrorActionPreference = "Stop"
$culture = [Globalization.CultureInfo]::InvariantCulture
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
. (Join-Path $PSScriptRoot "rscript-portable.ps1")
$targetRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot "target"))
[IO.Directory]::CreateDirectory($targetRoot) | Out-Null

if ($SampleCount -lt 100) {
    throw "SampleCount must be at least 100"
}
if ($Threads -notmatch "(?i)^(auto|[1-9][0-9]*)$") {
    throw "Threads must be auto or a positive integer"
}
if ($ZLimit -le 0 -or $NodeTvLimit -le 0 -or $SplitTvLimit -le 0) {
    throw "Distribution thresholds must be positive"
}

function Resolve-RepoPath([string]$Path, [bool]$MustExist = $true) {
    $candidate = if ([IO.Path]::IsPathRooted($Path)) { $Path } else { Join-Path $repoRoot $Path }
    if ($MustExist) {
        return (Resolve-Path $candidate).Path
    }
    [IO.Path]::GetFullPath($candidate)
}

function Add-OptionalPath {
    param([System.Collections.ArrayList]$Arguments, [string]$Option, [string]$Value)
    if (-not [string]::IsNullOrWhiteSpace($Value) -and $Value -ne "-") {
        [void]$Arguments.Add($Option)
        [void]$Arguments.Add((Resolve-RepoPath $Value))
    }
}

$case = @(
    Import-Csv -LiteralPath (Join-Path $repoRoot "validation/detection_combination_fixtures.tsv") -Delimiter "`t" |
        Where-Object case_id -eq "psychotria_detection_constrained_full_stack"
)
if ($case.Count -ne 1) {
    throw "Could not load the constrained full-stack detection fixture"
}
$case = $case[0]
$template = Get-Content -LiteralPath (Join-Path $repoRoot "examples/parameter_tables/dec.tsv")
$modelParameterNames = @(
    "d", "e", "a", "b", "x", "n", "w", "u", "j", "y", "s", "v",
    "mx01", "mx01j", "mx01y", "mx01s", "mx01v", "mf", "dp", "fdp"
)

$scratchRoot = [IO.Path]::GetFullPath((Join-Path $targetRoot (
    "detection-full-stack-bsm-" + [guid]::NewGuid().ToString("N")
)))
$targetPrefix = $targetRoot.TrimEnd(
    [IO.Path]::DirectorySeparatorChar,
    [IO.Path]::AltDirectorySeparatorChar
) + [IO.Path]::DirectorySeparatorChar
if (-not $scratchRoot.StartsWith($targetPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to use a BSM scratch directory outside target"
}
[IO.Directory]::CreateDirectory($scratchRoot) | Out-Null

$parameterPath = Join-Path $scratchRoot "parameters.tsv"
$analysisResultDir = Join-Path $scratchRoot "analysis-result"
$bsmOutputDir = Join-Path $scratchRoot "bsm"
$evaluateLog = Join-Path $scratchRoot "evaluate.tsv"
$bsmLog = Join-Path $scratchRoot "bsm.tsv"
$reportPath = Resolve-RepoPath $Report $false
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($reportPath)) | Out-Null

$values = @{}
foreach ($name in $modelParameterNames) {
    $values[$name] = [string]$case.$name
}
$parameterLines = foreach ($line in $template) {
    $fields = $line -split "`t", -1
    if ($fields.Count -eq 7 -and $values.ContainsKey($fields[0])) {
        $fields[1] = "fixed"
        $fields[2] = $values[$fields[0]]
        $fields[6] = ""
        $fields -join "`t"
    }
    else {
        $line
    }
}
[IO.File]::WriteAllText(
    $parameterPath,
    ($parameterLines -join "`n") + "`n",
    [Text.UTF8Encoding]::new($false)
)

Push-Location $repoRoot
try {
    Write-Host "== Build release CLI =="
    & cargo build --release -q -p biogeo-cli
    if ($LASTEXITCODE -ne 0) {
        throw "Release build failed with exit code $LASTEXITCODE"
    }
    $rustExe = Resolve-RepoPath "target/release/biogeo-cli.exe"

    [System.Collections.ArrayList]$evaluateArguments = @(
        "model-evaluate",
        "--tree", (Resolve-RepoPath $case.tree),
        "--use-detection-model",
        "--detections", (Resolve-RepoPath $case.detections),
        "--controls", (Resolve-RepoPath $case.controls),
        "--parameters", $parameterPath,
        "--max-range-size", $case.max_range_size,
        "--root-prior", $case.root_prior,
        "--analysis-result-dir", $analysisResultDir
    )
    Add-OptionalPath -Arguments $evaluateArguments -Option "--dispersal-multipliers" -Value $case.dispersal_multipliers
    Add-OptionalPath -Arguments $evaluateArguments -Option "--dispersal-strata" -Value $case.dispersal_strata
    Add-OptionalPath -Arguments $evaluateArguments -Option "--distance-matrix" -Value $case.distance_matrix
    Add-OptionalPath -Arguments $evaluateArguments -Option "--environment-distance-matrix" -Value $case.environment_distance_matrix
    Add-OptionalPath -Arguments $evaluateArguments -Option "--area-sizes" -Value $case.area_sizes
    if ($case.include_null_range -eq "true") {
        [void]$evaluateArguments.Add("--include-null-range")
    }

    Write-Host "== Write and replay the full-stack analysis result =="
    $evaluateOutput = @(& $rustExe @evaluateArguments)
    if ($LASTEXITCODE -ne 0) {
        throw "model-evaluate failed with exit code $LASTEXITCODE"
    }
    [IO.File]::WriteAllLines($evaluateLog, $evaluateOutput, [Text.UTF8Encoding]::new($false))

    $bsmArguments = @(
        "model-bsm",
        "--analysis-result", $analysisResultDir,
        "--bsm-samples", $SampleCount.ToString($culture),
        "--bsm-output-dir", $bsmOutputDir,
        "--bsm-threads", $Threads,
        "--bsm-checkpoint-samples", ([Math]::Min(1000, $SampleCount)).ToString($culture),
        "--seed", $Seed.ToString($culture)
    )
    $elapsed = Measure-Command {
        $bsmOutput = @(& $rustExe @bsmArguments)
        if ($LASTEXITCODE -ne 0) {
            throw "model-bsm failed with exit code $LASTEXITCODE"
        }
        [IO.File]::WriteAllLines($bsmLog, $bsmOutput, [Text.UTF8Encoding]::new($false))
    }
    Write-Host "Rust sampled and streamed $SampleCount histories in $($elapsed.TotalSeconds.ToString('0.###', $culture)) s"

    Write-Host "== Compare empirical node and split distributions with BioGeoBEARS fixnode =="
    Invoke-PortableRScript `
        -Arguments @(
            (Join-Path $repoRoot "validation/compare-detection-full-stack-bsm-to-fixnode.R"),
            $bsmOutputDir,
            (Resolve-RepoPath "validation/golden/biogeobears-detection-full-stack-fixnode-posterior.tsv"),
            (Resolve-RepoPath "validation/golden/biogeobears-detection-full-stack-fixnode-split.tsv"),
            $reportPath,
            $ZLimit.ToString("R", $culture),
            $NodeTvLimit.ToString("R", $culture),
            $SplitTvLimit.ToString("R", $culture)
        ) `
        -FailureMessage "Full-stack BSM distribution comparison failed"
}
finally {
    Pop-Location
    if (Test-Path -LiteralPath $scratchRoot -PathType Container) {
        [IO.Directory]::Delete($scratchRoot, $true)
    }
}

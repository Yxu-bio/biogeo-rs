[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$DatasetDir,

    [string]$CliPath = "target/release/biogeo-cli.exe",
    [string]$OutputDirectory = "",

    [ValidateRange(3, 1000000)]
    [int]$SampleCount = 10,

    [ValidateRange(1, 2147483647)]
    [int]$InterruptedEventBudget = 2500,

    [ValidateRange(1, 2147483647)]
    [int]$ResumeEventBudget = 50000,

    [ValidateRange(1, 2147483647)]
    [int]$MaxEventsPerSample = 10000,

    [ValidateRange(1, 2147483647)]
    [int]$MemoryBudgetMb = 512,

    [ValidateRange(1, 2147483647)]
    [int]$ShardSamples = 5,

    [ValidateRange(1, 2147483647)]
    [int]$CheckpointSamples = 1,

    [ValidateRange(0, [long]::MaxValue)]
    [long]$Seed = 20260821,

    [ValidateNotNullOrEmpty()]
    [string]$BsmThreads = "auto"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$culture = [Globalization.CultureInfo]::InvariantCulture
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))

function Resolve-RepoPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
}

function Require-File {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Required Ponerinae input does not exist: $Path"
    }
}

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Text
    )

    [System.IO.File]::WriteAllText($Path, $Text, [System.Text.UTF8Encoding]::new($false))
}

function Convert-KeyValueLines {
    param([Parameter(Mandatory = $true)][object[]]$Lines)

    $values = @{}
    foreach ($lineObject in $Lines) {
        $line = [string]$lineObject
        $parts = $line -split "`t", 2
        if ($parts.Count -eq 2) {
            $values[$parts[0]] = $parts[1]
        }
    }
    return $values
}

function Read-KeyValueFile {
    param([Parameter(Mandatory = $true)][string]$Path)

    Require-File $Path
    return Convert-KeyValueLines ([System.IO.File]::ReadAllLines($Path))
}

function Require-Value {
    param(
        [Parameter(Mandatory = $true)]$Values,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if (-not $Values.ContainsKey($Name)) {
        throw "Machine output is missing field $Name"
    }
    return [string]$Values[$Name]
}

function Assert-Value {
    param(
        [Parameter(Mandatory = $true)]$Values,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Expected
    )

    $actual = Require-Value $Values $Name
    if ($actual -ne $Expected) {
        throw "Unexpected ${Name}: expected $Expected, got $actual"
    }
}

function Parse-Int64 {
    param(
        [Parameter(Mandatory = $true)][string]$Value,
        [Parameter(Mandatory = $true)][string]$Name
    )

    $parsed = 0L
    if (-not [long]::TryParse($Value, [Globalization.NumberStyles]::Integer, $culture, [ref]$parsed)) {
        throw "Invalid integer $Name value: $Value"
    }
    return $parsed
}

function Parse-Double {
    param(
        [Parameter(Mandatory = $true)][string]$Value,
        [Parameter(Mandatory = $true)][string]$Name
    )

    $parsed = 0.0
    if (-not [double]::TryParse($Value, [Globalization.NumberStyles]::Float, $culture, [ref]$parsed)) {
        throw "Invalid numeric $Name value: $Value"
    }
    return $parsed
}

function Invoke-Cli {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][int[]]$ExpectedExitCodes,
        [Parameter(Mandatory = $true)][string]$OutputPath
    )

    $stopwatch = [Diagnostics.Stopwatch]::StartNew()
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = @(& $script:cli @Arguments 2>&1)
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    $stopwatch.Stop()
    $text = (($output | ForEach-Object { [string]$_ }) -join "`n") + "`n"
    Write-Utf8NoBom -Path $OutputPath -Text $text
    if ($ExpectedExitCodes -notcontains $exitCode) {
        throw "$Label failed with exit code $exitCode; expected $($ExpectedExitCodes -join ','). See $OutputPath"
    }
    return [pscustomobject]@{
        Lines = $output
        Values = Convert-KeyValueLines $output
        ExitCode = $exitCode
        Seconds = $stopwatch.Elapsed.TotalSeconds
    }
}

if ($InterruptedEventBudget -ge $ResumeEventBudget) {
    throw "ResumeEventBudget must be greater than InterruptedEventBudget"
}
if ($ShardSamples -gt $SampleCount) {
    throw "ShardSamples must not exceed SampleCount"
}

$dataset = [System.IO.Path]::GetFullPath($DatasetDir)
$finalInputs = Join-Path $dataset "final_inputs"
$treeSource = Join-Path $finalInputs "Ponerinae_MCC_phylogeny_1534t_short_names.tree"
$rangesSource = Join-Path $finalInputs "lagrange_area_data_file_7_regions_PaleA.data"
$boundariesSource = Join-Path $finalInputs "time_boundaries.txt"
$adjacencySource = Join-Path $finalInputs "Dore_2024_BioGeoBears_Adjacency_matrix_7areas_7TS.txt"
$cli = Resolve-RepoPath $CliPath

foreach ($path in @($treeSource, $rangesSource, $boundariesSource, $adjacencySource)) {
    Require-File $path
}
if (-not (Test-Path -LiteralPath $cli -PathType Leaf)) {
    Push-Location $repoRoot
    try {
        & cargo build --release --locked -p biogeo-cli
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
    $runRoot = Join-Path $repoRoot "validation/benchmark-runs/ponerinae-analysis-workflow-$stamp"
}
else {
    $runRoot = Resolve-RepoPath $OutputDirectory
}
if (Test-Path -LiteralPath $runRoot) {
    throw "Output directory already exists: $runRoot"
}
[System.IO.Directory]::CreateDirectory($runRoot) | Out-Null

$requestDir = Join-Path $runRoot "request"
$workflowDir = Join-Path $runRoot "workflow-result"
$templateOutput = Join-Path $runRoot "analysis-template.tsv"
$strataOutput = Join-Path $runRoot "strata-conversion.tsv"
$planOutput = Join-Path $runRoot "analysis-plan.tsv"
$interruptedOutput = Join-Path $runRoot "workflow-interrupted-error.tsv"
$interruptedInspectionOutput = Join-Path $runRoot "bsm-interrupted-inspection.tsv"
$resumedOutput = Join-Path $runRoot "workflow-resumed.tsv"
$finalInspectionOutput = Join-Path $runRoot "bsm-final-inspection.tsv"
$baselineOutput = Join-Path $runRoot "bsm-baseline-run.tsv"
$baselineInspectionOutput = Join-Path $runRoot "bsm-baseline-inspection.tsv"

$template = Invoke-Cli -Label "analysis-template" -Arguments @(
    "analysis-template", "--preset", "dec", "--mode", "optimize", "--output-dir", $requestDir
) -ExpectedExitCodes @(0) -OutputPath $templateOutput
Assert-Value $template.Values "format" "biogeo-analysis-template-v1"

$tree = Join-Path $requestDir "tree.nwk"
$ranges = Join-Path $requestDir "ranges.data"
Copy-Item -LiteralPath $treeSource -Destination $tree
Copy-Item -LiteralPath $rangesSource -Destination $ranges

$strataDir = Join-Path $requestDir "strata"
$strataConversion = Invoke-Cli -Label "convert-biogeobears-strata" -Arguments @(
    "convert-biogeobears-strata",
    "--time-boundaries", $boundariesSource,
    "--adjacency-matrices", $adjacencySource,
    "--adjacency-range-rule", "edge-covered",
    "--max-range-size", "5",
    "--output-dir", $strataDir
) -ExpectedExitCodes @(0) -OutputPath $strataOutput
Assert-Value $strataConversion.Values "allowed_range_counts" "36,36,27,20,24,20,38"

$requestPath = Join-Path $requestDir "analysis.tsv"
$requestText = @"
key`tvalue
format`tbiogeo-analysis-request-v1
mode`toptimize
tree`ttree.nwk
observation`texact_ranges
ranges`tranges.data
parameters`tparameters.tsv
max_range_size`t5
include_null_range`ttrue
root_prior`tflat
min_branch_length`t0
ancestral_probabilities`tfalse
split_probabilities`tfalse
dispersal_strata`tstrata/strata.tsv
optimization_initial_step`t0.5
optimization_tolerance`t1e-8
optimization_max_iterations`t200
"@
Write-Utf8NoBom -Path $requestPath -Text ($requestText.TrimStart("`r", "`n").Replace("`r`n", "`n"))

$sourceRows = @("role`tpath`tbytes`tsha256")
foreach ($source in @(
        [pscustomobject]@{ Role = "tree"; Path = $treeSource },
        [pscustomobject]@{ Role = "ranges"; Path = $rangesSource },
        [pscustomobject]@{ Role = "time_boundaries"; Path = $boundariesSource },
        [pscustomobject]@{ Role = "adjacency"; Path = $adjacencySource }
    )) {
    $item = Get-Item -LiteralPath $source.Path
    $hash = (Get-FileHash -LiteralPath $source.Path -Algorithm SHA256).Hash.ToLowerInvariant()
    $sourceRows += "$($source.Role)`t$($source.Path)`t$($item.Length)`t$hash"
}
Write-Utf8NoBom -Path (Join-Path $runRoot "source-provenance.tsv") -Text (($sourceRows -join "`n") + "`n")

$plan = Invoke-Cli -Label "analysis-plan" -Arguments @(
    "analysis-plan", "--request", $requestPath
) -ExpectedExitCodes @(0) -OutputPath $planOutput
foreach ($expectation in @(
        @("format", "biogeo-analysis-plan-v1"),
        @("status", "valid"),
        @("portable", "true"),
        @("mode", "optimize"),
        @("tips", "1534"),
        @("areas", "7"),
        @("states", "120"),
        @("max_range_size", "5"),
        @("include_null_range", "true"),
        @("free_parameters", "d,e"),
        @("anagenetic_periods", "7"),
        @("stratum_allowed_state_counts", "36,36,27,20,24,20,38")
    )) {
    Assert-Value $plan.Values $expectation[0] $expectation[1]
}

$commonWorkflowArguments = @(
    "--error-format", "tsv",
    "analysis-workflow",
    "--request", $requestPath,
    "--output-dir", $workflowDir,
    "--bsm-samples", $SampleCount.ToString($culture),
    "--bsm-output-level", "compact",
    "--bsm-threads", $BsmThreads,
    "--bsm-max-events-per-sample", $MaxEventsPerSample.ToString($culture),
    "--bsm-memory-budget-mb", $MemoryBudgetMb.ToString($culture),
    "--bsm-shard-samples", $ShardSamples.ToString($culture),
    "--bsm-checkpoint-samples", $CheckpointSamples.ToString($culture),
    "--seed", $Seed.ToString($culture),
    "--deep"
)

$interrupted = Invoke-Cli -Label "event-limited analysis-workflow" -Arguments (
    $commonWorkflowArguments + @(
        "--bsm-max-events-total", $InterruptedEventBudget.ToString($culture)
    )
) -ExpectedExitCodes @(3) -OutputPath $interruptedOutput
Assert-Value $interrupted.Values "format" "biogeo-cli-error-v1"
Assert-Value $interrupted.Values "code" "bsm_event_limit"
Assert-Value $interrupted.Values "exit_code" "3"

$bsmDir = Join-Path $workflowDir "bsm-result"
$interruptedInspection = Invoke-Cli -Label "interrupted bsm-inspect" -Arguments @(
    "bsm-inspect", "--bsm-result", $bsmDir, "--deep"
) -ExpectedExitCodes @(0) -OutputPath $interruptedInspectionOutput
foreach ($expectation in @(
        @("format", "biogeo-bsm-inspection-v1"),
        @("status", "valid"),
        @("run_status", "event_limit"),
        @("requested_samples", $SampleCount.ToString($culture)),
        @("states", "120"),
        @("areas", "7"),
        @("validation", "deep"),
        @("event_count_validation", "passed"),
        @("occupancy_validation", "passed"),
        @("path_validation", "passed")
    )) {
    Assert-Value $interruptedInspection.Values $expectation[0] $expectation[1]
}
$interruptedSamples = Parse-Int64 (Require-Value $interruptedInspection.Values "completed_samples") "completed_samples"
$interruptedEvents = Parse-Int64 (Require-Value $interruptedInspection.Values "completed_anagenetic_events") "completed_anagenetic_events"
if ($interruptedSamples -le 0 -or $interruptedSamples -ge $SampleCount) {
    throw "Event budget did not preserve a non-empty incomplete sample prefix: $interruptedSamples/$SampleCount"
}
if ($interruptedEvents -le 0 -or $interruptedEvents -gt $InterruptedEventBudget) {
    throw "Interrupted event prefix is outside its budget: $interruptedEvents/$InterruptedEventBudget"
}

$withheld = @(
    [pscustomobject]@{ Source = $tree; Destination = "$tree.withheld" },
    [pscustomobject]@{ Source = $ranges; Destination = "$ranges.withheld" },
    [pscustomobject]@{ Source = (Join-Path $requestDir "parameters.tsv"); Destination = (Join-Path $requestDir "parameters.tsv.withheld") },
    [pscustomobject]@{ Source = $strataDir; Destination = "$strataDir.withheld" }
)
foreach ($item in $withheld) {
    Move-Item -LiteralPath $item.Source -Destination $item.Destination
}

try {
    $resumed = Invoke-Cli -Label "resumed analysis-workflow" -Arguments (
        $commonWorkflowArguments + @(
            "--bsm-max-events-total", $ResumeEventBudget.ToString($culture),
            "--resume"
        )
    ) -ExpectedExitCodes @(0) -OutputPath $resumedOutput
}
finally {
    foreach ($item in $withheld) {
        if (Test-Path -LiteralPath $item.Destination) {
            Move-Item -LiteralPath $item.Destination -Destination $item.Source
        }
    }
}

foreach ($expectation in @(
        @("format", "biogeo-analysis-workflow-v1"),
        @("status", "complete"),
        @("analysis_result_format", "biogeo-analysis-result-v2"),
        @("analysis_reused", "true"),
        @("mode", "optimize"),
        @("states", "120"),
        @("areas", "7"),
        @("tips", "1534"),
        @("bsm_output_level", "compact"),
        @("bsm_layout", "sharded"),
        @("bsm_requested_samples", $SampleCount.ToString($culture)),
        @("bsm_completed_samples", $SampleCount.ToString($culture)),
        @("bsm_resumed", "true"),
        @("bsm_validation", "deep"),
        @("bsm_validation_status", "valid")
    )) {
    Assert-Value $resumed.Values $expectation[0] $expectation[1]
}

$lnL = Parse-Double (Require-Value $resumed.Values "lnL") "lnL"
$expectedLnL = -3049.873438616853
if ([Math]::Abs($lnL - $expectedLnL) -gt 0.00001) {
    throw "Optimized Ponerinae lnL drifted: expected $expectedLnL within 1e-5, got $lnL"
}

$finalInspection = Invoke-Cli -Label "final bsm-inspect" -Arguments @(
    "bsm-inspect", "--bsm-result", $bsmDir, "--deep"
) -ExpectedExitCodes @(0) -OutputPath $finalInspectionOutput
foreach ($expectation in @(
        @("status", "valid"),
        @("run_status", "complete"),
        @("completed_samples", $SampleCount.ToString($culture)),
        @("requested_samples", $SampleCount.ToString($culture)),
        @("validation", "deep"),
        @("event_count_validation", "passed"),
        @("occupancy_validation", "passed"),
        @("path_validation", "passed"),
        @("state_constraint_validation", "passed"),
        @("diagnostic_violations", "0")
    )) {
    Assert-Value $finalInspection.Values $expectation[0] $expectation[1]
}

$baselineDir = Join-Path $runRoot "bsm-one-shot-baseline"
$baseline = Invoke-Cli -Label "one-shot model-bsm baseline" -Arguments @(
    "model-bsm",
    "--analysis-result", (Join-Path $workflowDir "analysis-result"),
    "--bsm-samples", $SampleCount.ToString($culture),
    "--bsm-output-dir", $baselineDir,
    "--bsm-output-level", "compact",
    "--bsm-threads", $BsmThreads,
    "--bsm-max-events-per-sample", $MaxEventsPerSample.ToString($culture),
    "--bsm-max-events-total", $ResumeEventBudget.ToString($culture),
    "--bsm-memory-budget-mb", $MemoryBudgetMb.ToString($culture),
    "--bsm-shard-samples", $ShardSamples.ToString($culture),
    "--bsm-checkpoint-samples", $CheckpointSamples.ToString($culture),
    "--seed", $Seed.ToString($culture)
) -ExpectedExitCodes @(0) -OutputPath $baselineOutput
Assert-Value $baseline.Values "bsm_format" "biogeo-bsm-compact-sharded-tsv-v2"

$baselineInspection = Invoke-Cli -Label "baseline bsm-inspect" -Arguments @(
    "bsm-inspect", "--bsm-result", $baselineDir, "--deep"
) -ExpectedExitCodes @(0) -OutputPath $baselineInspectionOutput
foreach ($expectation in @(
        @("status", "valid"),
        @("run_status", "complete"),
        @("completed_samples", $SampleCount.ToString($culture)),
        @("diagnostic_violations", "0")
    )) {
    Assert-Value $baselineInspection.Values $expectation[0] $expectation[1]
}

$resumedRoot = [System.IO.Path]::GetFullPath($bsmDir).TrimEnd('\', '/')
$baselineRoot = [System.IO.Path]::GetFullPath($baselineDir).TrimEnd('\', '/')
$resumedFiles = @(Get-ChildItem -LiteralPath $resumedRoot -Recurse -File | Sort-Object FullName)
$baselineFiles = @(Get-ChildItem -LiteralPath $baselineRoot -Recurse -File | Sort-Object FullName)
if ($resumedFiles.Count -ne $baselineFiles.Count) {
    throw "Resumed and one-shot BSM file counts differ: $($resumedFiles.Count) versus $($baselineFiles.Count)"
}
$comparisonRows = @("relative_path`tbytes`tresumed_sha256`tbaseline_sha256")
for ($index = 0; $index -lt $resumedFiles.Count; $index++) {
    $resumedRelative = $resumedFiles[$index].FullName.Substring($resumedRoot.Length + 1).Replace('\', '/')
    $baselineRelative = $baselineFiles[$index].FullName.Substring($baselineRoot.Length + 1).Replace('\', '/')
    if ($resumedRelative -ne $baselineRelative) {
        throw "Resumed and one-shot BSM paths differ: $resumedRelative versus $baselineRelative"
    }
    if ($resumedFiles[$index].Length -ne $baselineFiles[$index].Length) {
        throw "Resumed and one-shot BSM lengths differ for ${resumedRelative}"
    }
    $resumedHash = (Get-FileHash -LiteralPath $resumedFiles[$index].FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $baselineHash = (Get-FileHash -LiteralPath $baselineFiles[$index].FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($resumedHash -ne $baselineHash) {
        throw "Resumed and one-shot BSM bytes differ for ${resumedRelative}"
    }
    $comparisonRows += "$resumedRelative`t$($resumedFiles[$index].Length)`t$resumedHash`t$baselineHash"
}
Write-Utf8NoBom -Path (Join-Path $runRoot "bsm-byte-comparison.tsv") `
    -Text (($comparisonRows -join "`n") + "`n")

$metadata = Read-KeyValueFile (Join-Path $bsmDir "metadata.tsv")
$finalEvents = Parse-Int64 (Require-Value $metadata "completed_anagenetic_events") "completed_anagenetic_events"
$resultFiles = @(Get-ChildItem -LiteralPath $workflowDir -Recurse -File)
$resultBytes = ($resultFiles | Measure-Object -Property Length -Sum).Sum
$analysisMetadata = Read-KeyValueFile (Join-Path $workflowDir "analysis-result/metadata.tsv")
$d = Require-Value $metadata "parameter_d"
$e = Require-Value $metadata "parameter_e"

$report = @(
    "key`tvalue",
    "format`tbiogeo-ponerinae-analysis-workflow-acceptance-v1",
    "status`tpassed",
    "request_portable`ttrue",
    "source_dependencies_withheld_during_resume`ttrue",
    "tips`t1534",
    "areas`t7",
    "states`t120",
    "anagenetic_periods`t7",
    "stratum_allowed_state_counts`t36,36,27,20,24,20,38",
    "mode`toptimize",
    "optimization_converged`t$(Require-Value $analysisMetadata 'optimization_converged')",
    "optimization_evaluations`t$(Require-Value $analysisMetadata 'optimization_evaluations')",
    "lnL`t$($lnL.ToString('G17', $culture))",
    "d`t$d",
    "e`t$e",
    "bsm_output_level`tcompact",
    "bsm_layout`tsharded",
    "bsm_threads`t$(Require-Value $metadata 'threads')",
    "bsm_max_in_flight`t$(Require-Value $metadata 'max_in_flight')",
    "bsm_requested_samples`t$SampleCount",
    "interrupted_exit_code`t3",
    "interrupted_run_status`tevent_limit",
    "interrupted_event_budget`t$InterruptedEventBudget",
    "interrupted_completed_samples`t$interruptedSamples",
    "interrupted_completed_anagenetic_events`t$interruptedEvents",
    "resumed_event_budget`t$ResumeEventBudget",
    "final_completed_samples`t$SampleCount",
    "final_completed_anagenetic_events`t$finalEvents",
    "deep_validation`tpassed",
    "diagnostic_violations`t0",
    "resume_byte_identical_to_one_shot`ttrue",
    "compared_bsm_files`t$($resumedFiles.Count)",
    "initial_workflow_seconds`t$($interrupted.Seconds.ToString('F9', $culture))",
    "resume_workflow_seconds`t$($resumed.Seconds.ToString('F9', $culture))",
    "one_shot_bsm_seconds`t$($baseline.Seconds.ToString('F9', $culture))",
    "workflow_result_files`t$($resultFiles.Count)",
    "workflow_result_bytes`t$resultBytes",
    "run_directory`t$runRoot"
)
$reportPath = Join-Path $runRoot "acceptance.tsv"
Write-Utf8NoBom -Path $reportPath -Text (($report -join "`n") + "`n")

$report | ForEach-Object { Write-Output $_ }

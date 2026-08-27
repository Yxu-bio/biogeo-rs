[CmdletBinding()]
param(
    [string]$CliPath = "target/release/biogeo-cli.exe",
    [string]$ExamplesRoot = "",
    [string]$OutputRoot = "",
    [switch]$SkipBuild,
    [switch]$KeepOutput
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
if ([string]::IsNullOrWhiteSpace($ExamplesRoot)) {
    $resolvedExamplesRoot = Join-Path $repoRoot "examples"
}
elseif ([System.IO.Path]::IsPathRooted($ExamplesRoot)) {
    $resolvedExamplesRoot = [System.IO.Path]::GetFullPath($ExamplesRoot)
}
else {
    $resolvedExamplesRoot = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $ExamplesRoot))
}
$temporaryOutput = [string]::IsNullOrWhiteSpace($OutputRoot)
if ($temporaryOutput) {
    $OutputRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
        "biogeo-public-examples-" + [Guid]::NewGuid().ToString("N")
    )
}
$OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)

function Resolve-RepoPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
}

function Convert-KeyValueLines {
    param([Parameter(Mandatory = $true)][object[]]$Lines)

    $values = @{}
    foreach ($item in $Lines) {
        $line = [string]$item
        $parts = $line -split "`t", 2
        if ($parts.Count -eq 2) {
            $values[$parts[0]] = $parts[1]
        }
    }
    return $values
}

function Read-KeyValueFile {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Required key/value file does not exist: $Path"
    }
    return Convert-KeyValueLines ([System.IO.File]::ReadAllLines($Path))
}

function Assert-Value {
    param(
        [Parameter(Mandatory = $true)]$Values,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Expected
    )

    if (-not $Values.ContainsKey($Name)) {
        throw "Machine record is missing field $Name"
    }
    $actual = [string]$Values[$Name]
    if ($actual -ne $Expected) {
        throw "Unexpected ${Name}: expected $Expected, got $actual"
    }
}

function Invoke-Cli {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [int[]]$ExpectedExitCodes = @(0)
    )

    $previousErrorAction = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $raw = @(& $script:cli @Arguments 2>&1)
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorAction
    }
    $lines = @($raw | ForEach-Object { [string]$_ })
    if ($ExpectedExitCodes -notcontains $exitCode) {
        throw "$Label exited with $exitCode; expected $($ExpectedExitCodes -join ', '): $($lines -join ' | ')"
    }
    return [pscustomobject]@{
        ExitCode = $exitCode
        Lines = $lines
        Values = Convert-KeyValueLines $lines
    }
}

Push-Location $repoRoot
try {
    if (-not $SkipBuild) {
        cargo build --release -q -p biogeo-cli
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    }
    $script:cli = Resolve-RepoPath $CliPath
    if (-not (Test-Path -LiteralPath $script:cli -PathType Leaf)) {
        throw "CLI executable does not exist: $script:cli"
    }
    if (-not (Test-Path -LiteralPath $resolvedExamplesRoot -PathType Container)) {
        throw "Examples root does not exist: $resolvedExamplesRoot"
    }
    New-Item -ItemType Directory -Path $OutputRoot | Out-Null

    $presets = @(
        @("dec", "d,e"),
        @("decj", "d,e,j"),
        @("divalike", "d,e"),
        @("divalikej", "d,e,j"),
        @("bayarealike", "d,e"),
        @("bayarealikej", "d,e,j")
    )
    foreach ($preset in $presets) {
        $id = $preset[0]
        $request = Join-Path $resolvedExamplesRoot "preset_requests\$id.tsv"
        $plan = Invoke-Cli -Label "$id plan" -Arguments @(
            "analysis-plan", "--request", $request
        )
        Assert-Value $plan.Values "format" "biogeo-analysis-plan-v1"
        Assert-Value $plan.Values "status" "valid"
        Assert-Value $plan.Values "portable" "true"
        Assert-Value $plan.Values "free_parameters" $preset[1]

        $resultDir = Join-Path $OutputRoot "preset-$id"
        $run = Invoke-Cli -Label "$id run" -Arguments @(
            "analysis-run", "--request", $request, "--output-dir", $resultDir
        )
        Assert-Value $run.Values "format" "biogeo-analysis-run-v2"
        Assert-Value $run.Values "status" "complete"
        Assert-Value $run.Values "portable_request" "true"
        Assert-Value (Read-KeyValueFile (Join-Path $resultDir "metadata.tsv")) `
            "optimization_converged" "true"

        $inspection = Invoke-Cli -Label "$id replay" -Arguments @(
            "analysis-result-inspect", "--analysis-result", $resultDir, "--replay"
        )
        Assert-Value $inspection.Values "status" "valid"
        Assert-Value $inspection.Values "replay_validation" "passed"
    }

    $quickstartRequest = Join-Path $resolvedExamplesRoot "analysis_request\analysis.tsv"
    $quickstartResult = Join-Path $OutputRoot "quickstart-dec"
    $quickstartBsm = Join-Path $OutputRoot "quickstart-dec-bsm"
    $quickstartPlan = Invoke-Cli -Label "README quickstart plan" -Arguments @(
        "analysis-plan", "--request", $quickstartRequest
    )
    Assert-Value $quickstartPlan.Values "status" "valid"
    Assert-Value $quickstartPlan.Values "risk_level" "low"

    $quickstartRun = Invoke-Cli -Label "README quickstart run" -Arguments @(
        "analysis-run", "--request", $quickstartRequest,
        "--output-dir", $quickstartResult
    )
    Assert-Value $quickstartRun.Values "status" "complete"
    Assert-Value $quickstartRun.Values "optimization_converged" "true"

    $quickstartReplay = Invoke-Cli -Label "README quickstart replay" -Arguments @(
        "analysis-result-inspect", "--analysis-result", $quickstartResult, "--replay"
    )
    Assert-Value $quickstartReplay.Values "status" "valid"
    Assert-Value $quickstartReplay.Values "replay_validation" "passed"

    $quickstartBsmRun = Invoke-Cli -Label "README quickstart BSM" -Arguments @(
        "model-bsm", "--analysis-result", $quickstartResult,
        "--bsm-samples", "100",
        "--bsm-output-dir", $quickstartBsm,
        "--bsm-output-level", "compact",
        "--bsm-threads", "auto",
        "--bsm-shard-samples", "50"
    )
    Assert-Value $quickstartBsmRun.Values "source_optimization_converged" "true"
    Assert-Value $quickstartBsmRun.Values "bsm_samples" "100"
    Assert-Value $quickstartBsmRun.Values "bsm_format" "biogeo-bsm-compact-sharded-tsv-v2"

    $quickstartBsmInspection = Invoke-Cli -Label "README quickstart BSM inspection" -Arguments @(
        "bsm-inspect", "--bsm-result", $quickstartBsm, "--deep"
    )
    Assert-Value $quickstartBsmInspection.Values "status" "valid"
    Assert-Value $quickstartBsmInspection.Values "completed_samples" "100"
    Assert-Value $quickstartBsmInspection.Values "diagnostic_violations" "0"

    $stratifiedDir = Join-Path $OutputRoot "stratified"
    $stratifiedRequest = Join-Path $resolvedExamplesRoot "stratified_analysis\analysis.tsv"
    $stratifiedPlan = Invoke-Cli -Label "stratified plan" -Arguments @(
        "analysis-plan", "--request", $stratifiedRequest
    )
    Assert-Value $stratifiedPlan.Values "portable" "true"
    Assert-Value $stratifiedPlan.Values "tips" "19"
    Assert-Value $stratifiedPlan.Values "states" "16"
    Assert-Value $stratifiedPlan.Values "anagenetic_periods" "5"
    $null = Invoke-Cli -Label "stratified run" -Arguments @(
        "analysis-run", "--request", $stratifiedRequest, "--output-dir", $stratifiedDir
    )
    $stratifiedInspection = Invoke-Cli -Label "stratified replay" -Arguments @(
        "analysis-result-inspect", "--analysis-result", $stratifiedDir, "--replay"
    )
    Assert-Value $stratifiedInspection.Values "status" "valid"
    Assert-Value $stratifiedInspection.Values "replay_validation" "passed"
    Assert-Value $stratifiedInspection.Values "dependency_count" "15"

    $recoveryDir = Join-Path $OutputRoot "recovery"
    $recoveryStopRequest = Join-Path $resolvedExamplesRoot "recovery\workflow-stop.tsv"
    $recoveryResumeRequest = Join-Path $resolvedExamplesRoot "recovery\workflow-resume.tsv"
    $stopped = Invoke-Cli -Label "recovery stop" -Arguments @(
        "--error-format", "tsv", "model-workflow",
        "--request", $recoveryStopRequest,
        "--output-dir", $recoveryDir
    ) -ExpectedExitCodes @(124)
    Assert-Value $stopped.Values "format" "biogeo-cli-error-v1"
    Assert-Value $stopped.Values "code" "bsm_time_limit"
    Assert-Value $stopped.Values "exit_code" "124"
    if (-not (Test-Path -LiteralPath (Join-Path $recoveryDir "model-batch/complete.tsv"))) {
        throw "Recovery example did not retain the completed model batch"
    }
    if (Test-Path -LiteralPath (Join-Path $recoveryDir "complete.tsv")) {
        throw "Recovery example published top-level completion before BSM finished"
    }

    $modelIds = @("DEC", "DEC+J")
    $beforeHashes = @{}
    foreach ($modelId in $modelIds) {
        $metadataPath = Join-Path $recoveryDir "model-batch/models/$modelId/metadata.tsv"
        $beforeHashes[$modelId] = (Get-FileHash -LiteralPath $metadataPath -Algorithm SHA256).Hash
    }

    $resumed = Invoke-Cli -Label "recovery resume" -Arguments @(
        "model-workflow", "--request", $recoveryResumeRequest,
        "--output-dir", $recoveryDir, "--resume"
    )
    Assert-Value $resumed.Values "format" "biogeo-model-workflow-run-v1"
    Assert-Value $resumed.Values "status" "complete"
    Assert-Value $resumed.Values "model_batch_resumed" "true"
    Assert-Value $resumed.Values "bsm_resumed" "true"
    Assert-Value $resumed.Values "bsm_completed_samples" "8"
    Assert-Value $resumed.Values "bsm_validation" "deep"

    foreach ($modelId in $modelIds) {
        $metadataPath = Join-Path $recoveryDir "model-batch/models/$modelId/metadata.tsv"
        $afterHash = (Get-FileHash -LiteralPath $metadataPath -Algorithm SHA256).Hash
        if ($afterHash -ne $beforeHashes[$modelId]) {
            throw "Recovery unexpectedly changed fitted model $modelId"
        }
    }
    $deep = Invoke-Cli -Label "recovery deep inspection" -Arguments @(
        "bsm-inspect", "--bsm-result", (Join-Path $recoveryDir "bsm-result"), "--deep"
    )
    Assert-Value $deep.Values "status" "valid"
    Assert-Value $deep.Values "run_status" "complete"
    Assert-Value $deep.Values "completed_samples" "8"
    Assert-Value $deep.Values "diagnostic_violations" "0"

    Write-Output "format`tbiogeo-public-cli-examples-check-v1"
    Write-Output "status`tpassed"
    Write-Output "preset_requests`t6"
    Write-Output "readme_quickstart_converged`ttrue"
    Write-Output "readme_quickstart_bsm_samples`t100"
    Write-Output "stratified_periods`t5"
    Write-Output "recovery_stop_exit_code`t124"
    Write-Output "recovery_models_reused`t2"
    Write-Output "recovery_bsm_samples`t8"
    Write-Output "examples_root`t$resolvedExamplesRoot"
    Write-Output "output_root`t$OutputRoot"
}
finally {
    Pop-Location
    if ($temporaryOutput -and -not $KeepOutput -and (Test-Path -LiteralPath $OutputRoot)) {
        $resolvedOutput = [System.IO.Path]::GetFullPath($OutputRoot)
        $resolvedTemp = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
        if (-not $resolvedOutput.StartsWith($resolvedTemp, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove non-temporary output path: $resolvedOutput"
        }
        Remove-Item -LiteralPath $resolvedOutput -Recurse -Force
    }
}

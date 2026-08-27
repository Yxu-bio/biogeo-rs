[CmdletBinding()]
param(
    [string]$CliPath = "target/release/biogeo-cli.exe",
    [string]$OutputRoot = "",
    [switch]$SkipBuild,
    [switch]$KeepOutput
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$culture = [Globalization.CultureInfo]::InvariantCulture
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$temporaryOutput = [string]::IsNullOrWhiteSpace($OutputRoot)
if ($temporaryOutput) {
    $OutputRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
        "biogeo-real-data-workflows-" + [Guid]::NewGuid().ToString("N")
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
        if ($line -eq "key`tvalue") {
            continue
        }
        $parts = $line -split "`t", 2
        if ($parts.Count -eq 2) {
            if ($values.ContainsKey($parts[0])) {
                throw "Duplicate key $($parts[0]) in machine record"
            }
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

function Assert-SchemaValue {
    param(
        [Parameter(Mandatory = $true)][string]$Value,
        [Parameter(Mandatory = $true)][string]$ValueType,
        [Parameter(Mandatory = $true)][string]$Constraint,
        [Parameter(Mandatory = $true)][string]$Label
    )

    switch ($ValueType) {
        "literal" {
            if ($Value -ne $Constraint) { throw "$Label must equal $Constraint, got $Value" }
        }
        "enum" {
            if (($Constraint -split "\|") -notcontains $Value) {
                throw "$Label is outside enum ${Constraint}: $Value"
            }
        }
        "bool" {
            if (@("true", "false") -notcontains $Value) { throw "$Label is not bool: $Value" }
        }
        "usize" {
            $parsed = 0L
            if (-not [long]::TryParse($Value, [Globalization.NumberStyles]::None, $culture, [ref]$parsed) -or $parsed -lt 0) {
                throw "$Label is not usize: $Value"
            }
        }
        "u8" {
            $parsed = 0
            if (-not [int]::TryParse($Value, [Globalization.NumberStyles]::None, $culture, [ref]$parsed) -or $parsed -lt 0 -or $parsed -gt 255) {
                throw "$Label is not u8: $Value"
            }
        }
        "f64" {
            $parsed = 0.0
            if (-not [double]::TryParse($Value, [Globalization.NumberStyles]::Float, $culture, [ref]$parsed) -or [double]::IsNaN($parsed) -or [double]::IsInfinity($parsed)) {
                throw "$Label is not finite f64: $Value"
            }
        }
        "hex16" {
            if ($Value -notmatch "^[0-9a-f]{16}$") { throw "$Label is not lowercase hex16: $Value" }
        }
        "portable_path" {
            if ([System.IO.Path]::IsPathRooted($Value) -or $Value.Contains("\") -or ($Value -split "/") -contains "..") {
                throw "$Label is not a portable path: $Value"
            }
        }
        "na_or_bool" {
            if ($Value -ne "NA" -and @("true", "false") -notcontains $Value) { throw "$Label is not NA or bool: $Value" }
        }
        "na_or_f64" {
            if ($Value -ne "NA") { Assert-SchemaValue $Value "f64" "-" $Label }
        }
        "na_or_usize" {
            if ($Value -ne "NA") { Assert-SchemaValue $Value "usize" "-" $Label }
        }
        "na_or_u64" {
            if ($Value -ne "NA") { Assert-SchemaValue $Value "usize" "-" $Label }
        }
        "none_or_usize" {
            if ($Value -ne "none") { Assert-SchemaValue $Value "usize" "-" $Label }
        }
        "unlimited_or_usize" {
            if ($Value -ne "unlimited") { Assert-SchemaValue $Value "usize" "-" $Label }
        }
        "unlimited_or_f64" {
            if ($Value -ne "unlimited") { Assert-SchemaValue $Value "f64" "-" $Label }
        }
        "encoded_string" { }
        default { throw "Unsupported schema value_type ${ValueType} for $Label" }
    }
}

function Read-SchemaRows {
    param([Parameter(Mandatory = $true)][string]$SchemaPath)

    $lines = [System.IO.File]::ReadAllLines($SchemaPath)
    if ($lines.Count -lt 2 -or $lines[0] -ne "biogeo-schema-contract-v1") {
        throw "Invalid schema contract: $SchemaPath"
    }
    return @($lines[1..($lines.Count - 1)] | ConvertFrom-Csv -Delimiter "`t")
}

function Assert-KeyValueSchema {
    param(
        [Parameter(Mandatory = $true)]$Values,
        [Parameter(Mandatory = $true)][object[]]$SchemaRows,
        [Parameter(Mandatory = $true)][string]$Location,
        [Parameter(Mandatory = $true)][string]$Label
    )

    $keyRows = @($SchemaRows | Where-Object { $_.record_kind -eq "key" -and $_.location -eq $Location })
    $allowed = @{}
    foreach ($row in $keyRows) {
        $allowed[$row.name] = $true
        if ($row.requirement -eq "required" -and -not $Values.ContainsKey($row.name)) {
            throw "$Label is missing required key $($row.name)"
        }
        if ($Values.ContainsKey($row.name)) {
            Assert-SchemaValue ([string]$Values[$row.name]) $row.value_type $row.constraint "$Label/$($row.name)"
        }
    }
    foreach ($key in $Values.Keys) {
        if (-not $allowed.ContainsKey($key)) {
            throw "$Label contains unknown key $key"
        }
    }
}

function Assert-WorkflowResultSchema {
    param([Parameter(Mandatory = $true)][string]$ResultDir)

    $schemaPath = Resolve-RepoPath "schemas/biogeo-model-workflow-result-v1.schema.tsv"
    $rows = Read-SchemaRows $schemaPath
    foreach ($row in @($rows | Where-Object { $_.location -eq "." })) {
        $path = Join-Path $ResultDir $row.name
        if ($row.requirement -eq "required") {
            if ($row.record_kind -eq "file" -and -not (Test-Path -LiteralPath $path -PathType Leaf)) {
                throw "Workflow result is missing required file $($row.name)"
            }
            if ($row.record_kind -eq "directory" -and -not (Test-Path -LiteralPath $path -PathType Container)) {
                throw "Workflow result is missing required directory $($row.name)"
            }
        }
    }
    foreach ($location in @($rows | Where-Object { $_.record_kind -eq "key" } | Select-Object -ExpandProperty location -Unique)) {
        $values = Read-KeyValueFile (Join-Path $ResultDir $location)
        Assert-KeyValueSchema $values $rows $location "workflow-result/$location"
    }
}

$cases = @(
    [pscustomobject]@{
        Id = "psychotria"
        Fixture = "validation/fixtures/model_workflow_psychotria"
        Tips = "19"
        Areas = "4"
        States = "16"
    },
    [pscustomobject]@{
        Id = "ponerinae"
        Fixture = "validation/fixtures/ponerinae_32tip_7area"
        Tips = "32"
        Areas = "7"
        States = "120"
    }
)
$modelIds = @("DEC", "DEC+J", "DIVALIKE", "DIVALIKE+J", "BAYAREALIKE", "BAYAREALIKE+J")

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
    New-Item -ItemType Directory -Path $OutputRoot | Out-Null
    $planSchema = Read-SchemaRows (Resolve-RepoPath "schemas/biogeo-model-workflow-plan-v1.schema.tsv")
    $runSchema = Read-SchemaRows (Resolve-RepoPath "schemas/biogeo-model-workflow-run-v1.schema.tsv")

    foreach ($case in $cases) {
        $stopRequest = "$($case.Fixture)/workflow-stop.tsv"
        $resumeRequest = "$($case.Fixture)/workflow-resume.tsv"
        $stopPlan = Invoke-Cli -Label "$($case.Id) stop plan" -Arguments @(
            "model-workflow-plan", "--request", $stopRequest
        )
        $resumePlan = Invoke-Cli -Label "$($case.Id) resume plan" -Arguments @(
            "model-workflow-plan", "--request", $resumeRequest
        )
        Assert-KeyValueSchema $stopPlan.Values $planSchema "stdout" "$($case.Id) stop plan"
        Assert-KeyValueSchema $resumePlan.Values $planSchema "stdout" "$($case.Id) resume plan"
        Assert-Value $stopPlan.Values "request_paths_portable" "true"
        Assert-Value $stopPlan.Values "candidate_models" "6"
        Assert-Value $stopPlan.Values "tips" $case.Tips
        Assert-Value $stopPlan.Values "areas" $case.Areas
        Assert-Value $stopPlan.Values "states" $case.States
        Assert-Value $stopPlan.Values "bsm_samples" "4"
        Assert-Value $stopPlan.Values "bsm_time_limit_seconds" "0"
        Assert-Value $resumePlan.Values "bsm_time_limit_seconds" $(if ($case.Id -eq "psychotria") { "60" } else { "120" })
        Assert-Value $resumePlan.Values "request_fingerprint" $stopPlan.Values["request_fingerprint"]

        $resultDir = Join-Path $OutputRoot $case.Id
        $stopped = Invoke-Cli -Label "$($case.Id) budget stop" -Arguments @(
            "--error-format", "tsv", "model-workflow", "--request", $stopRequest,
            "--output-dir", $resultDir
        ) -ExpectedExitCodes @(124)
        Assert-Value $stopped.Values "format" "biogeo-cli-error-v1"
        Assert-Value $stopped.Values "code" "bsm_time_limit"
        Assert-Value $stopped.Values "exit_code" "124"
        if (-not (Test-Path -LiteralPath (Join-Path $resultDir "model-batch/complete.tsv"))) {
            throw "$($case.Id) did not retain its completed six-model batch"
        }
        if (Test-Path -LiteralPath (Join-Path $resultDir "complete.tsv")) {
            throw "$($case.Id) published workflow completion before BSM finished"
        }
        $partial = Invoke-Cli -Label "$($case.Id) partial BSM inspection" -Arguments @(
            "bsm-inspect", "--bsm-result", (Join-Path $resultDir "bsm-result"), "--deep"
        )
        Assert-Value $partial.Values "status" "valid"
        Assert-Value $partial.Values "run_status" "time_limit"
        Assert-Value $partial.Values "requested_samples" "4"
        Assert-Value $partial.Values "completed_samples" "0"

        $beforeFingerprints = @{}
        foreach ($modelId in $modelIds) {
            $modelDir = Join-Path $resultDir "model-batch/models/$modelId"
            $metadata = Read-KeyValueFile (Join-Path $modelDir "metadata.tsv")
            Assert-Value $metadata "format" "biogeo-analysis-result-v2"
            Assert-Value $metadata "optimization_converged" "true"
            $inspection = Invoke-Cli -Label "$($case.Id) $modelId replay before resume" -Arguments @(
                "analysis-result-inspect", "--analysis-result", $modelDir, "--replay"
            )
            Assert-Value $inspection.Values "status" "valid"
            Assert-Value $inspection.Values "replay_validation" "passed"
            $beforeFingerprints[$modelId] = $inspection.Values["analysis_result_fingerprint"]
        }

        $resumed = Invoke-Cli -Label "$($case.Id) resume" -Arguments @(
            "model-workflow", "--request", $resumeRequest, "--output-dir", $resultDir, "--resume"
        )
        Assert-KeyValueSchema $resumed.Values $runSchema "stdout" "$($case.Id) resumed run"
        Assert-Value $resumed.Values "status" "complete"
        Assert-Value $resumed.Values "candidate_models" "6"
        Assert-Value $resumed.Values "model_batch_resumed" "true"
        Assert-Value $resumed.Values "selected_model_id" "DEC"
        Assert-Value $resumed.Values "bsm_status" "complete"
        Assert-Value $resumed.Values "bsm_completed_samples" "4"
        Assert-Value $resumed.Values "bsm_resumed" "true"
        Assert-Value $resumed.Values "bsm_validation" "deep"

        Assert-WorkflowResultSchema $resultDir
        $completion = Read-KeyValueFile (Join-Path $resultDir "complete.tsv")
        Assert-Value $completion "candidate_models" "6"
        Assert-Value $completion "selected_model_id" "DEC"
        Assert-Value $completion "bsm_status" "complete"
        Assert-Value $completion "bsm_completed_samples" "4"
        $selection = Read-KeyValueFile (Join-Path $resultDir "selection.tsv")
        Assert-Value $selection "selection_reason" "explicit_model_id"
        Assert-Value $selection "selected_model_id" "DEC"

        $comparisonFirst = [System.IO.File]::ReadLines(
            (Join-Path $resultDir "model-batch/comparison.tsv")
        ) | Select-Object -First 1
        if ($comparisonFirst -ne "format`tbiogeo-model-comparison-v3") {
            throw "$($case.Id) has an unexpected comparison format"
        }
        $averageFirst = [System.IO.File]::ReadLines(
            (Join-Path $resultDir "model-batch/model-averaged-ancestral-ranges.tsv")
        ) | Select-Object -First 1
        if ($averageFirst -ne "format`tbiogeo-model-averaged-ancestral-ranges-v2") {
            throw "$($case.Id) has an unexpected model-average format"
        }

        foreach ($modelId in $modelIds) {
            $modelDir = Join-Path $resultDir "model-batch/models/$modelId"
            $inspection = Invoke-Cli -Label "$($case.Id) $modelId replay after resume" -Arguments @(
                "analysis-result-inspect", "--analysis-result", $modelDir, "--replay"
            )
            Assert-Value $inspection.Values "analysis_result_fingerprint" $beforeFingerprints[$modelId]
            Assert-Value $inspection.Values "replay_validation" "passed"
        }
        $deep = Invoke-Cli -Label "$($case.Id) final BSM inspection" -Arguments @(
            "bsm-inspect", "--bsm-result", (Join-Path $resultDir "bsm-result"), "--deep"
        )
        Assert-Value $deep.Values "status" "valid"
        Assert-Value $deep.Values "run_status" "complete"
        Assert-Value $deep.Values "completed_samples" "4"
        Assert-Value $deep.Values "diagnostic_violations" "0"
    }

    Write-Output "format`tbiogeo-real-data-model-workflow-check-v1"
    Write-Output "status`tpassed"
    Write-Output "datasets`t2"
    Write-Output "candidate_models_per_dataset`t6"
    Write-Output "fitted_models_replayed`t12"
    Write-Output "interrupted_workflows`t2"
    Write-Output "resumed_bsm_samples`t8"
    Write-Output "workflow_result_schema_checks`t2"
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

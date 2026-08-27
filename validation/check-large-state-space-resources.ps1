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
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$temporaryOutput = [string]::IsNullOrWhiteSpace($OutputRoot)
if ($temporaryOutput) {
    $OutputRoot = Join-Path ([IO.Path]::GetTempPath()) (
        "biogeo-large-state-space-" + [Guid]::NewGuid().ToString("N")
    )
}
$OutputRoot = [IO.Path]::GetFullPath($OutputRoot)

function Resolve-RepoPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    if ([IO.Path]::IsPathRooted($Path)) {
        return [IO.Path]::GetFullPath($Path)
    }
    return [IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
}

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Text
    )

    [IO.File]::WriteAllText($Path, $Text, [Text.UTF8Encoding]::new($false))
}

function Read-KeyValues {
    param([Parameter(Mandatory = $true)][object[]]$Lines)

    $values = @{}
    foreach ($item in $Lines) {
        $parts = ([string]$item) -split "`t", 2
        if ($parts.Count -eq 2) {
            $values[$parts[0]] = $parts[1]
        }
    }
    return $values
}

function Assert-Value {
    param(
        [Parameter(Mandatory = $true)]$Values,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Expected
    )

    if (-not $Values.ContainsKey($Name) -or [string]$Values[$Name] -ne $Expected) {
        $actual = if ($Values.ContainsKey($Name)) { [string]$Values[$Name] } else { "<missing>" }
        throw "Unexpected ${Name}: expected $Expected, got $actual"
    }
}

function Write-Request {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][int]$MaxRangeSize,
        [Parameter(Mandatory = $true)][long]$MaxStates
    )

    $text = @(
        "key`tvalue",
        "format`tbiogeo-analysis-request-v1",
        "mode`tevaluate",
        "tree`ttree.nwk",
        "observation`texact_ranges",
        "ranges`tranges.tsv",
        "parameters`tparameters.tsv",
        "max_range_size`t$MaxRangeSize",
        "max_states`t$MaxStates",
        "include_null_range`ttrue",
        "root_prior`tflat",
        "min_branch_length`t0",
        "ancestral_probabilities`tfalse",
        "split_probabilities`tfalse"
    ) -join "`n"
    Write-Utf8NoBom -Path $Path -Text "$text`n"
}

Push-Location $repoRoot
try {
    if (-not $SkipBuild) {
        & cargo build --release --locked -p biogeo-cli
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    }
    $cli = Resolve-RepoPath $CliPath
    if (-not (Test-Path -LiteralPath $cli -PathType Leaf)) {
        throw "CLI executable does not exist: $cli"
    }
    if (Test-Path -LiteralPath $OutputRoot) {
        throw "Output root already exists and will not be overwritten: $OutputRoot"
    }
    [IO.Directory]::CreateDirectory($OutputRoot) | Out-Null

    $cases = @(
        [pscustomobject]@{
            Name = "20-area"
            Source = "validation/benchmark-runs/dec-region-scale-100t-20a-m5"
            Areas = 20
            MaxRangeSize = 5
            States = 21700
            QTransitions = 201420
            Scenarios = 402440
        },
        [pscustomobject]@{
            Name = "30-area"
            Source = "validation/benchmark-runs/dec-region-scale-100t-30a-m5"
            Areas = 30
            MaxRangeSize = 5
            States = 174437
            QTransitions = 1670430
            Scenarios = 3339960
        }
    )

    $timings = @{}
    foreach ($case in $cases) {
        $caseRoot = Join-Path $OutputRoot $case.Name
        [IO.Directory]::CreateDirectory($caseRoot) | Out-Null
        $sourceRoot = Resolve-RepoPath $case.Source
        foreach ($file in @("tree.nwk", "ranges.tsv", "parameters.tsv")) {
            Copy-Item -LiteralPath (Join-Path $sourceRoot $file) -Destination $caseRoot
        }
        $requestPath = Join-Path $caseRoot "analysis.tsv"
        Write-Request -Path $requestPath -MaxRangeSize $case.MaxRangeSize -MaxStates $case.States

        $stopwatch = [Diagnostics.Stopwatch]::StartNew()
        $output = @(& $cli analysis-plan --request $requestPath)
        $stopwatch.Stop()
        if ($LASTEXITCODE -ne 0) {
            throw "$($case.Name) analysis-plan failed with exit code $LASTEXITCODE"
        }
        $values = Read-KeyValues $output
        Assert-Value $values "format" "biogeo-analysis-plan-v1"
        Assert-Value $values "status" "valid"
        Assert-Value $values "areas" ([string]$case.Areas)
        Assert-Value $values "max_range_size" ([string]$case.MaxRangeSize)
        Assert-Value $values "state_space_limit" ([string]$case.States)
        Assert-Value $values "state_count_estimate" ([string]$case.States)
        Assert-Value $values "states" ([string]$case.States)
        Assert-Value $values "q_off_diagonal_transitions" ([string]$case.QTransitions)
        Assert-Value $values "cladogenetic_scenarios" ([string]$case.Scenarios)
        Assert-Value $values "risk_level" "high"
        $timings[$case.Name] = $stopwatch.Elapsed.TotalSeconds
    }

    $rejectedRoot = Join-Path $OutputRoot "30-area-rejected"
    [IO.Directory]::CreateDirectory($rejectedRoot) | Out-Null
    $source30 = Resolve-RepoPath "validation/benchmark-runs/dec-region-scale-100t-30a-m5"
    foreach ($file in @("tree.nwk", "ranges.tsv", "parameters.tsv")) {
        Copy-Item -LiteralPath (Join-Path $source30 $file) -Destination $rejectedRoot
    }
    $rejectedRequest = Join-Path $rejectedRoot "analysis.tsv"
    Write-Request -Path $rejectedRequest -MaxRangeSize 15 -MaxStates 1000000
    $previousErrorAction = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $failure = @(& $cli --error-format tsv analysis-plan --request $rejectedRequest 2>&1)
        $failureExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorAction
    }
    if ($failureExitCode -ne 2) {
        throw "oversized analysis-plan exited with $failureExitCode instead of 2"
    }
    $failureValues = Read-KeyValues $failure
    Assert-Value $failureValues "format" "biogeo-cli-error-v1"
    Assert-Value $failureValues "code" "resource_limit"
    Assert-Value $failureValues "exit_code" "2"
    $message = [Uri]::UnescapeDataString([string]$failureValues["message"])
    if (-not $message.Contains("estimated state space has 614429672 states") -or
        -not $message.Contains("exceeding --max-states 1000000")) {
        throw "oversized analysis returned an unexpected diagnostic: $message"
    }

    Write-Output "format`tbiogeo-large-state-space-resource-check-v1"
    Write-Output "status`tpassed"
    Write-Output "successful_cases`t2"
    Write-Output "largest_successful_areas`t30"
    Write-Output "largest_successful_states`t174437"
    Write-Output "rejected_areas`t30"
    Write-Output "rejected_max_range_size`t15"
    Write-Output "rejected_estimated_states`t614429672"
    Write-Output "rejected_limit`t1000000"
    Write-Output "area20_seconds`t$($timings['20-area'].ToString('R', $culture))"
    Write-Output "area30_seconds`t$($timings['30-area'].ToString('R', $culture))"
    Write-Output "output_root`t$OutputRoot"
}
finally {
    Pop-Location
    if ($temporaryOutput -and -not $KeepOutput -and (Test-Path -LiteralPath $OutputRoot)) {
        $resolvedOutput = [IO.Path]::GetFullPath($OutputRoot)
        $resolvedTemp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
        if (-not $resolvedOutput.StartsWith($resolvedTemp, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove non-temporary output path: $resolvedOutput"
        }
        [IO.Directory]::Delete($resolvedOutput, $true)
    }
}

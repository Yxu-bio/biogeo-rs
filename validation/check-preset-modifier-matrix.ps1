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
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$temporaryOutput = [string]::IsNullOrWhiteSpace($OutputRoot)
if ($temporaryOutput) {
    $OutputRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
        "biogeo-preset-modifier-matrix-" + [Guid]::NewGuid().ToString("N")
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

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Text
    )

    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
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

function Write-AnalysisRequest {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Parameters,
        [Parameter(Mandatory = $true)]
        [AllowNull()]
        [AllowEmptyCollection()]
        [string[]]$ModifierRows
    )

    $rows = @(
        "key`tvalue",
        "format`tbiogeo-analysis-request-v1",
        "mode`tevaluate",
        "tree`ttree.nwk",
        "observation`texact_ranges",
        "ranges`tranges.tsv",
        "parameters`t$Parameters",
        "max_range_size`t3",
        "include_null_range`ttrue",
        "root_prior`tflat",
        "min_branch_length`t0",
        "ancestral_probabilities`ttrue",
        "split_probabilities`ttrue"
    ) + @($ModifierRows)
    Write-Utf8NoBom -Path $Path -Text "$(($rows -join "`n"))`n"
}

function Get-SupportModifierRows {
    param([Parameter(Mandatory = $true)][string]$InputMode)

    switch ($InputMode) {
        "static" {
            return @(
                "dispersal_multipliers`tstatic_manual.tsv",
                "distance_matrix`tstatic_distance.tsv",
                "environment_distance_matrix`tstatic_environment.tsv",
                "area_sizes`tstatic_area_sizes.tsv"
            )
        }
        "stratified" { return @("dispersal_strata`tstrata.tsv") }
        default { throw "Unknown support input_mode $InputMode" }
    }
}

function Get-RejectionModifierRows {
    param([Parameter(Mandatory = $true)][string]$Setup)

    switch ($Setup) {
        "none" { return @() }
        "missing_x" { return @() }
        "missing_n" { return @("distance_matrix`tstatic_distance.tsv") }
        "missing_u" {
            return @(
                "distance_matrix`tstatic_distance.tsv",
                "environment_distance_matrix`tstatic_environment.tsv"
            )
        }
        "missing_w" {
            return @(
                "distance_matrix`tstatic_distance.tsv",
                "environment_distance_matrix`tstatic_environment.tsv",
                "area_sizes`tstatic_area_sizes.tsv"
            )
        }
        "conflict_x" {
            return @(
                "dispersal_strata`tstrata.tsv",
                "distance_matrix`tstatic_distance.tsv"
            )
        }
        "conflict_n" {
            return @(
                "dispersal_strata`tstrata.tsv",
                "environment_distance_matrix`tstatic_environment.tsv"
            )
        }
        "conflict_u" {
            return @(
                "dispersal_strata`tstrata.tsv",
                "area_sizes`tstatic_area_sizes.tsv"
            )
        }
        "conflict_manual" {
            return @(
                "dispersal_strata`tstrata.tsv",
                "dispersal_multipliers`tstatic_manual.tsv"
            )
        }
        "conflict_extirpation" {
            return @(
                "dispersal_strata`tstrata.tsv",
                "extirpation_multipliers`tstatic_extirpation.tsv"
            )
        }
        "stratified_b" { return @("dispersal_strata`tstrata.tsv") }
        default { throw "Unknown rejection setup $Setup" }
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
    if (Test-Path -LiteralPath $OutputRoot) {
        throw "Output root already exists and will not be overwritten: $OutputRoot"
    }
    [System.IO.Directory]::CreateDirectory($OutputRoot) | Out-Null
    $fixtureSource = Resolve-RepoPath "validation/fixtures/preset_modifier_matrix"
    $fixtureRoot = Join-Path $OutputRoot "fixture"
    Copy-Item -LiteralPath $fixtureSource -Destination $fixtureRoot -Recurse
    $resultsRoot = Join-Path $OutputRoot "results"
    [System.IO.Directory]::CreateDirectory($resultsRoot) | Out-Null

    $supportRows = @(
        Import-Csv -LiteralPath (
            Resolve-RepoPath "validation/preset-modifier-combination-matrix.tsv"
        ) -Delimiter "`t"
    )
    if ($supportRows.Count -ne 12) {
        throw "Support matrix must contain exactly 12 cases, got $($supportRows.Count)"
    }
    $expectedPresets = @(
        "DEC", "DEC+J", "DIVALIKE", "DIVALIKE+J", "BAYAREALIKE", "BAYAREALIKE+J"
    )
    foreach ($preset in $expectedPresets) {
        $modes = @(
            $supportRows |
                Where-Object { $_.preset -eq $preset } |
                Select-Object -ExpandProperty input_mode |
                Sort-Object
        )
        if (($modes -join ",") -ne "static,stratified") {
            throw "$preset must have exactly one static and one stratified support case"
        }
    }

    $fingerprints = @{}
    $lnlByPresetMode = @{}
    foreach ($case in $supportRows) {
        $requestPath = Join-Path $fixtureRoot "request-$($case.case_id).tsv"
        Write-AnalysisRequest -Path $requestPath -Parameters $case.parameters `
            -ModifierRows (Get-SupportModifierRows $case.input_mode)

        $plan = Invoke-Cli -Label "$($case.case_id) plan" -Arguments @(
            "analysis-plan", "--request", $requestPath
        )
        Assert-Value $plan.Values "format" "biogeo-analysis-plan-v1"
        Assert-Value $plan.Values "status" "valid"
        Assert-Value $plan.Values "portable" "true"
        Assert-Value $plan.Values "mode" "evaluate"
        Assert-Value $plan.Values "states" $case.expected_states
        Assert-Value $plan.Values "free_parameter_count" "0"
        Assert-Value $plan.Values "anagenetic_periods" $case.expected_periods
        Assert-Value $plan.Values "q_off_diagonal_transitions" `
            $case.expected_q_off_diagonal_transitions
        Assert-Value $plan.Values "cladogenetic_scenarios" `
            $case.expected_cladogenetic_scenarios
        Assert-Value $plan.Values "branch_segments" $case.expected_branch_segments

        $resultDir = Join-Path $resultsRoot $case.case_id
        $run = Invoke-Cli -Label "$($case.case_id) run" -Arguments @(
            "analysis-run", "--request", $requestPath, "--output-dir", $resultDir
        )
        Assert-Value $run.Values "format" "biogeo-analysis-run-v2"
        Assert-Value $run.Values "status" "complete"
        Assert-Value $run.Values "portable_request" "true"
        $actualLnL = [double]::Parse(
            [string]$run.Values["lnL"],
            [Globalization.NumberStyles]::Float,
            $culture
        )
        $expectedLnL = [double]::Parse(
            [string]$case.expected_lnL,
            [Globalization.NumberStyles]::Float,
            $culture
        )
        $tolerance = [double]::Parse(
            [string]$case.lnL_tolerance,
            [Globalization.NumberStyles]::Float,
            $culture
        )
        if ([Math]::Abs($actualLnL - $expectedLnL) -gt $tolerance) {
            throw "$($case.case_id) lnL differs: expected $expectedLnL, got $actualLnL"
        }
        $lnlByPresetMode["$($case.preset)/$($case.input_mode)"] = $actualLnL

        $inspection = Invoke-Cli -Label "$($case.case_id) replay" -Arguments @(
            "analysis-result-inspect", "--analysis-result", $resultDir, "--replay"
        )
        Assert-Value $inspection.Values "status" "valid"
        Assert-Value $inspection.Values "replay_validation" "passed"
        $fingerprint = [string]$inspection.Values["analysis_result_fingerprint"]
        if ($fingerprints.ContainsKey($fingerprint)) {
            throw "$($case.case_id) unexpectedly shares a result fingerprint with $($fingerprints[$fingerprint])"
        }
        $fingerprints[$fingerprint] = $case.case_id
    }

    foreach ($preset in $expectedPresets) {
        $staticLnL = $lnlByPresetMode["$preset/static"]
        $stratifiedLnL = $lnlByPresetMode["$preset/stratified"]
        if ([Math]::Abs($staticLnL - $stratifiedLnL) -le 1e-6) {
            throw "$preset static and stratified modifiers did not produce distinct likelihoods"
        }
    }

    $rejectionRows = @(
        Import-Csv -LiteralPath (
            Resolve-RepoPath "validation/preset-modifier-rejection-matrix.tsv"
        ) -Delimiter "`t"
    )
    if ($rejectionRows.Count -ne 12) {
        throw "Rejection matrix must contain exactly 12 cases, got $($rejectionRows.Count)"
    }
    foreach ($case in $rejectionRows) {
        $requestPath = Join-Path $fixtureRoot "reject-$($case.case_id).tsv"
        Write-AnalysisRequest -Path $requestPath -Parameters $case.parameters `
            -ModifierRows (Get-RejectionModifierRows $case.setup)
        $expectedExitCode = [int]$case.expected_exit_code
        $failure = Invoke-Cli -Label "$($case.case_id) rejection" -Arguments @(
            "--error-format", "tsv", "analysis-plan", "--request", $requestPath
        ) -ExpectedExitCodes @($expectedExitCode)
        Assert-Value $failure.Values "format" "biogeo-cli-error-v1"
        Assert-Value $failure.Values "code" $case.expected_code
        Assert-Value $failure.Values "exit_code" $case.expected_exit_code
        if (-not $failure.Values.ContainsKey("message")) {
            throw "$($case.case_id) error record has no message"
        }
        $message = [Uri]::UnescapeDataString([string]$failure.Values["message"])
        if (-not $message.Contains([string]$case.expected_message_fragment)) {
            throw "$($case.case_id) message did not contain expected fragment: $message"
        }
    }

    Write-Output "format`tbiogeo-preset-modifier-matrix-check-v1"
    Write-Output "status`tpassed"
    Write-Output "presets`t6"
    Write-Output "supported_combinations`t$($supportRows.Count)"
    Write-Output "static_combinations`t6"
    Write-Output "stratified_combinations`t6"
    Write-Output "replayed_results`t$($fingerprints.Count)"
    Write-Output "rejection_rules`t$($rejectionRows.Count)"
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

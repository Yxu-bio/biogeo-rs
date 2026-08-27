param(
    [string]$Manifest = "validation/dec_fixtures.tsv",
    [string]$Output = "validation/lagrange-ng-output.tsv",
    [string]$ScratchRoot = ""
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$manifestPath = Resolve-Path (Join-Path $repoRoot $Manifest)
$outputPath = Join-Path $repoRoot $Output

function Test-AsciiPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    return $Path -cmatch '^[\x00-\x7F]+$'
}

if ([string]::IsNullOrWhiteSpace($ScratchRoot)) {
    if (-not [string]::IsNullOrWhiteSpace($env:BGB_LAGRANGE_SCRATCH)) {
        $ScratchRoot = $env:BGB_LAGRANGE_SCRATCH
    } elseif (-not [string]::IsNullOrWhiteSpace($env:TEMP) -and (Test-AsciiPath $env:TEMP)) {
        $ScratchRoot = $env:TEMP
    } elseif (Test-Path -LiteralPath "C:\Temp" -PathType Container) {
        $ScratchRoot = "C:\Temp"
    } else {
        throw "No ASCII scratch directory found. Set BGB_LAGRANGE_SCRATCH to an ASCII-only path."
    }
}

if (-not (Test-AsciiPath $ScratchRoot)) {
    throw "LAGRANGE-ng scratch path must be ASCII-only: $ScratchRoot"
}

$runRoot = Join-Path $ScratchRoot "biogeo-lagrange-ng-runs"
New-Item -ItemType Directory -Force -Path $runRoot | Out-Null

$lagrangeExe = & (Join-Path $PSScriptRoot "find-lagrange-ng.ps1")
$cases = Import-Csv -Path $manifestPath -Delimiter "`t" |
    Where-Object { $_.lagrange_ng_ready -eq "true" }

function Convert-RangesToPhylip {
    param(
        [Parameter(Mandatory = $true)][string]$RangesPath,
        [Parameter(Mandatory = $true)][string]$OutputPath
    )

    $rows = Import-Csv -Path $RangesPath -Delimiter "`t"
    if ($rows.Count -eq 0) {
        throw "Range table is empty: $RangesPath"
    }

    $columns = $rows[0].PSObject.Properties.Name
    if ($columns[0] -ne "tip" -or $columns.Count -lt 2) {
        throw "Range table first column must be tip and include at least one area: $RangesPath"
    }

    $areas = @($columns | Select-Object -Skip 1)
    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add("$($rows.Count) $($areas.Count)")

    foreach ($row in $rows) {
        $bits = -join ($areas | ForEach-Object { $row.$_ })
        $lines.Add("$($row.tip) $bits")
    }

    Set-Content -LiteralPath $OutputPath -Value $lines -Encoding ASCII
    return $areas
}

function Read-LagrangeRunSummary {
    param([Parameter(Mandatory = $true)][string[]]$LogLines)

    $text = $LogLines -join "`n"
    $llhMatch = [regex]::Match($text, "LLH:\s*(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)")
    if (-not $llhMatch.Success) {
        throw "Could not find LLH in LAGRANGE-ng stdout"
    }

    $periodMatch = [regex]::Match(
        $text,
        "Period:\s*([^,]+),\s*Dispersion:\s*(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?),\s*Extinction:\s*(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)"
    )
    if (-not $periodMatch.Success) {
        throw "Could not find actual period parameters in LAGRANGE-ng stdout"
    }

    return [pscustomobject]@{
        lnL = [double]$llhMatch.Groups[1].Value
        period = $periodMatch.Groups[1].Value
        dispersion = [double]$periodMatch.Groups[2].Value
        extinction = [double]$periodMatch.Groups[3].Value
    }
}

function Quote-LagrangeValue {
    param([Parameter(Mandatory = $true)][string]$Value)
    return "'" + ($Value -replace "'", "''") + "'"
}

$rows = New-Object System.Collections.Generic.List[object]
foreach ($case in $cases) {
    $caseRunDir = Join-Path $runRoot $case.case_id
    New-Item -ItemType Directory -Force -Path $caseRunDir | Out-Null

    $rangesPath = Resolve-Path (Join-Path $repoRoot $case.ranges)
    $sourceTreePath = Resolve-Path (Join-Path $repoRoot $case.tree)
    $treePath = Join-Path $caseRunDir "tree.nwk"
    $dataPath = Join-Path $caseRunDir "ranges.phy"
    Copy-Item -LiteralPath $sourceTreePath -Destination $treePath -Force
    $areas = Convert-RangesToPhylip -RangesPath $rangesPath -OutputPath $dataPath

    $prefix = "result"
    $configPath = Join-Path $caseRunDir "lagrange-ng.conf"
    $config = @(
        "treefile = tree.nwk",
        "datafile = ranges.phy",
        "areanames = $($areas -join ' ')",
        "maxareas = $($case.max_range_size)",
        "mode = evaluate",
        "dispersion = $($case.d)",
        "extinction = $($case.e)",
        "output-type = json",
        "workers = 1",
        "threads-per-worker = 1",
        "prefix = $prefix",
        "states",
        "splits"
    )
    Set-Content -LiteralPath $configPath -Value $config -Encoding ASCII

    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    Push-Location $caseRunDir
    try {
        $logLines = & $lagrangeExe $configPath 2>&1
        if ($LASTEXITCODE -ne 0) {
            $logLines | Set-Content -LiteralPath (Join-Path $caseRunDir "lagrange-ng.stdout.txt") -Encoding UTF8
            throw "lagrange-ng exited with code $LASTEXITCODE for $($case.case_id)"
        }
        $logLines | Set-Content -LiteralPath (Join-Path $caseRunDir "lagrange-ng.stdout.txt") -Encoding UTF8
    }
    finally {
        Pop-Location
        $stopwatch.Stop()
    }

    $summary = Read-LagrangeRunSummary -LogLines $logLines
    $requestedD = [double]$case.d
    $requestedE = [double]$case.e
    $rateDelta = [Math]::Max(
        [Math]::Abs($summary.dispersion - $requestedD),
        [Math]::Abs($summary.extinction - $requestedE)
    )
    $parameterStatus = if ($rateDelta -le 1e-12) { "requested_rates_used" } else { "requested_rates_ignored" }

    $rows.Add([pscustomobject]@{
        case_id = $case.case_id
        lagrange_ng_lnL = "{0:R}" -f $summary.lnL
        requested_d = $case.d
        requested_e = $case.e
        actual_d = "{0:R}" -f $summary.dispersion
        actual_e = "{0:R}" -f $summary.extinction
        max_range_size = $case.max_range_size
        parameter_status = $parameterStatus
        elapsed_seconds = "{0:R}" -f $stopwatch.Elapsed.TotalSeconds
    })
    Write-Host "$($case.case_id) lagrange-ng lnL=$($summary.lnL) d=$($summary.dispersion) e=$($summary.extinction) seconds=$($stopwatch.Elapsed.TotalSeconds) $parameterStatus"
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $outputPath) | Out-Null
$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("case_id`tlagrange_ng_lnL`trequested_d`trequested_e`tactual_d`tactual_e`tmax_range_size`tparameter_status`telapsed_seconds")
foreach ($row in $rows) {
    $lines.Add("$($row.case_id)`t$($row.lagrange_ng_lnL)`t$($row.requested_d)`t$($row.requested_e)`t$($row.actual_d)`t$($row.actual_e)`t$($row.max_range_size)`t$($row.parameter_status)`t$($row.elapsed_seconds)")
}
Set-Content -LiteralPath $outputPath -Value $lines -Encoding UTF8

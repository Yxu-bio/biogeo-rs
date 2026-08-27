param(
    [string]$Manifest = "validation/dec_fixtures.tsv",
    [string]$Output = "validation/lagrange-ng-benchmark.tsv",
    [string]$ScratchRoot = "",
    [int]$Repeats = 3
)

$ErrorActionPreference = "Stop"

if ($Repeats -lt 1) {
    throw "Repeats must be greater than zero"
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$currentOutput = "validation/lagrange-ng-output.tsv"
$currentPath = Join-Path $repoRoot $currentOutput
$outputPath = Join-Path $repoRoot $Output
$samplesByCase = @{}
$lastLnLByCase = @{}

for ($repeat = 1; $repeat -le $Repeats; $repeat++) {
    Write-Host "LAGRANGE-ng benchmark repeat $repeat/$Repeats"
    $runnerParams = @{
        Manifest = $Manifest
        Output = $currentOutput
    }
    if (-not [string]::IsNullOrWhiteSpace($ScratchRoot)) {
        $runnerParams.ScratchRoot = $ScratchRoot
    }

    & (Join-Path (Split-Path -Parent $PSScriptRoot) "lagrange-ng\run-lagrange-ng-dec.ps1") @runnerParams
    $rows = @(Import-Csv -Path $currentPath -Delimiter "`t")
    foreach ($row in $rows) {
        if (-not $samplesByCase.ContainsKey($row.case_id)) {
            $samplesByCase[$row.case_id] = New-Object System.Collections.Generic.List[double]
        }
        $samplesByCase[$row.case_id].Add([double]$row.elapsed_seconds)
        $lastLnLByCase[$row.case_id] = $row.lagrange_ng_lnL
    }
}

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("case_id`trepeats`tmean_seconds`tmin_seconds`tmax_seconds`tlast_lagrange_ng_lnL")
foreach ($caseId in @($samplesByCase.Keys | Sort-Object)) {
    $samples = @($samplesByCase[$caseId])
    $mean = ($samples | Measure-Object -Average).Average
    $min = ($samples | Measure-Object -Minimum).Minimum
    $max = ($samples | Measure-Object -Maximum).Maximum
    $lines.Add(
        "$caseId`t$($samples.Count)`t$("{0:R}" -f $mean)`t$("{0:R}" -f $min)`t$("{0:R}" -f $max)`t$($lastLnLByCase[$caseId])"
    )
    Write-Host "$caseId mean=$mean seconds min=$min max=$max repeats=$($samples.Count)"
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $outputPath) | Out-Null
Set-Content -LiteralPath $outputPath -Value $lines -Encoding UTF8

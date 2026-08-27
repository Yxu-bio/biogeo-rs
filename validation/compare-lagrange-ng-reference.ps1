param(
    [string]$Manifest = "validation/dec_fixtures.tsv",
    [string]$Reference = "validation/reference/lagrange-ng-dec.tsv",
    [string]$CurrentOutput = "validation/lagrange-ng-output.tsv",
    [string]$ScratchRoot = "",
    [double]$LnLTolerance = 1e-6,
    [double]$RateTolerance = 1e-12
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$referencePath = Resolve-Path (Join-Path $repoRoot $Reference)
$currentPath = Join-Path $repoRoot $CurrentOutput

$runnerParams = @{
    Manifest = $Manifest
    Output = $CurrentOutput
}
if (-not [string]::IsNullOrWhiteSpace($ScratchRoot)) {
    $runnerParams.ScratchRoot = $ScratchRoot
}

& (Join-Path $PSScriptRoot "run-lagrange-ng-dec.ps1") @runnerParams

$referenceRows = @(Import-Csv -Path $referencePath -Delimiter "`t")
$currentRows = @(Import-Csv -Path $currentPath -Delimiter "`t")
$currentByCase = @{}
foreach ($row in $currentRows) {
    if ($currentByCase.ContainsKey($row.case_id)) {
        throw "duplicate current LAGRANGE-ng row for $($row.case_id)"
    }
    $currentByCase[$row.case_id] = $row
}

if ($currentRows.Count -ne $referenceRows.Count) {
    throw "LAGRANGE-ng reference row count mismatch current=$($currentRows.Count) reference=$($referenceRows.Count)"
}

foreach ($referenceRow in $referenceRows) {
    $caseId = $referenceRow.case_id
    if (-not $currentByCase.ContainsKey($caseId)) {
        throw "$caseId`: missing current LAGRANGE-ng reference row"
    }

    $currentRow = $currentByCase[$caseId]
    $lnLDelta = [Math]::Abs(
        ([double]$currentRow.lagrange_ng_lnL) - ([double]$referenceRow.lagrange_ng_lnL)
    )
    $dDelta = [Math]::Abs(([double]$currentRow.actual_d) - ([double]$referenceRow.actual_d))
    $eDelta = [Math]::Abs(([double]$currentRow.actual_e) - ([double]$referenceRow.actual_e))

    if ($currentRow.parameter_status -ne "requested_rates_used") {
        throw "$caseId`: LAGRANGE-ng did not use requested rates: $($currentRow.parameter_status)"
    }
    if ($currentRow.parameter_status -ne $referenceRow.parameter_status) {
        throw "$caseId`: parameter status changed current=$($currentRow.parameter_status) reference=$($referenceRow.parameter_status)"
    }
    if ($lnLDelta -gt $LnLTolerance) {
        throw "$caseId`: LAGRANGE-ng likelihood changed current=$($currentRow.lagrange_ng_lnL) reference=$($referenceRow.lagrange_ng_lnL) delta=$lnLDelta tolerance=$LnLTolerance"
    }
    if ([Math]::Max($dDelta, $eDelta) -gt $RateTolerance) {
        throw "$caseId`: LAGRANGE-ng actual rates changed d_delta=$dDelta e_delta=$eDelta tolerance=$RateTolerance"
    }

    Write-Host "$caseId reference ok lnL=$($currentRow.lagrange_ng_lnL) delta=$lnLDelta seconds=$($currentRow.elapsed_seconds)"
}

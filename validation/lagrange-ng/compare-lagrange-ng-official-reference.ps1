param(
    [string]$Reference = "validation/reference/lagrange-ng-official.tsv",
    [string]$CurrentOutput = "validation/lagrange-ng-official-output.tsv",
    [string]$ScratchRoot = "",
    [double]$LikelihoodTolerance = 1e-6,
    [double]$RateTolerance = 1e-12
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$referencePath = Resolve-Path (Join-Path $repoRoot $Reference)
$currentPath = Join-Path $repoRoot $CurrentOutput
$auditParams = @{ Output = $CurrentOutput }
if (-not [string]::IsNullOrWhiteSpace($ScratchRoot)) {
    $auditParams.ScratchRoot = $ScratchRoot
}

& (Join-Path $PSScriptRoot "audit-lagrange-ng-official.ps1") @auditParams

$referenceRows = @(Import-Csv -Path $referencePath -Delimiter "`t")
$currentRows = @(Import-Csv -Path $currentPath -Delimiter "`t")
$currentByCase = @{}
foreach ($row in $currentRows) {
    if ($currentByCase.ContainsKey($row.case_id)) {
        throw "duplicate current official LAGRANGE-ng row for $($row.case_id)"
    }
    $currentByCase[$row.case_id] = $row
}

if ($currentRows.Count -ne $referenceRows.Count) {
    throw "official LAGRANGE-ng row count mismatch current=$($currentRows.Count) reference=$($referenceRows.Count)"
}

function Compare-OptionalNumber {
    param(
        [Parameter(Mandatory = $true)][string]$CaseId,
        [Parameter(Mandatory = $true)][string]$Field,
        [AllowEmptyString()][string]$Current,
        [AllowEmptyString()][string]$Expected,
        [Parameter(Mandatory = $true)][double]$Tolerance
    )

    $currentBlank = [string]::IsNullOrWhiteSpace($Current)
    $expectedBlank = [string]::IsNullOrWhiteSpace($Expected)
    if ($currentBlank -and $expectedBlank) {
        return
    }
    if ($currentBlank -ne $expectedBlank) {
        throw "$CaseId`: $Field presence changed current='$Current' reference='$Expected'"
    }

    $delta = [Math]::Abs(([double]$Current) - ([double]$Expected))
    if ($delta -gt $Tolerance) {
        throw "$CaseId`: $Field changed current=$Current reference=$Expected delta=$delta tolerance=$Tolerance"
    }
}

foreach ($referenceRow in $referenceRows) {
    $caseId = $referenceRow.case_id
    if (-not $currentByCase.ContainsKey($caseId)) {
        throw "$caseId`: missing current official LAGRANGE-ng row"
    }

    $currentRow = $currentByCase[$caseId]
    foreach ($field in @("exit_code", "observed_mode", "parameter_status", "config")) {
        if ($currentRow.$field -ne $referenceRow.$field) {
            throw "$caseId`: $field changed current=$($currentRow.$field) reference=$($referenceRow.$field)"
        }
    }

    foreach ($field in @("initial_llh", "final_llh", "evaluate_llh")) {
        Compare-OptionalNumber `
            -CaseId $caseId `
            -Field $field `
            -Current $currentRow.$field `
            -Expected $referenceRow.$field `
            -Tolerance $LikelihoodTolerance
    }
    foreach ($field in @("requested_d", "requested_e", "actual_d", "actual_e")) {
        Compare-OptionalNumber `
            -CaseId $caseId `
            -Field $field `
            -Current $currentRow.$field `
            -Expected $referenceRow.$field `
            -Tolerance $RateTolerance
    }

    Write-Host "$caseId official reference ok mode=$($currentRow.observed_mode)"
}

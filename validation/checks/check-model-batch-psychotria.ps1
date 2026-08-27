param(
    [string]$OutputRoot,
    [switch]$KeepOutput
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$temporaryOutput = [string]::IsNullOrWhiteSpace($OutputRoot)
if ($temporaryOutput) {
    $OutputRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
        'biogeo-model-batch-psychotria-' + [Guid]::NewGuid().ToString('N')
    )
}
$OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)

Push-Location $repoRoot
try {
    cargo build --release -q -p biogeo-cli
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }

    $cli = Join-Path $repoRoot 'target/release/biogeo-cli.exe'
    $arguments = @(
        'model-batch',
        '--manifest', 'examples/model_batch/psychotria-six-models.tsv',
        '--output-dir', $OutputRoot,
        '--tree', 'validation/fixtures/biogeobears_official/psychotria_m4/tree.nwk',
        '--ranges', 'validation/fixtures/biogeobears_official/psychotria_m4/ranges.tsv',
        '--include-null-range',
        '--max-range-size', '4',
        '--max-iterations', '1000'
    )

    $firstLines = & $cli @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "initial model-batch run failed with exit code $LASTEXITCODE"
    }
    $first = ($firstLines -join "`n") + "`n"
    if ($first -notmatch "(?m)^models`t6$" -or $first -notmatch "(?m)^eligible_models`t6$") {
        throw 'Psychotria model-batch did not report six eligible models'
    }
    foreach ($pair in @(
        @('DEC', 'DEC+J'),
        @('DIVALIKE', 'DIVALIKE+J'),
        @('BAYAREALIKE', 'BAYAREALIKE+J')
    )) {
        $relationship = [regex]::Escape($pair[0] + "`t" + $pair[1] + "`tnested_boundary`t")
        if ($first -notmatch "(?m)^$relationship") {
            throw "missing boundary nesting relationship for $($pair[0]) -> $($pair[1])"
        }
    }

    $stored = (Get-Content -LiteralPath (Join-Path $OutputRoot 'comparison.tsv') -Raw).Replace("`r`n", "`n")
    if ($stored -ne $first) {
        throw 'stdout comparison differs from comparison.tsv'
    }

    $averagePath = Join-Path $OutputRoot 'model-averaged-ancestral-ranges.tsv'
    $averageLines = @(Get-Content -LiteralPath $averagePath)
    if ($averageLines[0] -ne "format`tbiogeo-model-averaged-ancestral-ranges-v2") {
        throw 'Psychotria model-average output has an unexpected format'
    }
    if ($averageLines -notcontains "status`tavailable" -or
        $averageLines -notcontains "criteria`t2" -or
        $averageLines -notcontains "aic_models`t6" -or
        $averageLines -notcontains "aicc_models`t6") {
        throw 'Psychotria model-average output does not contain both six-model criteria'
    }
    $probabilityHeader = "criterion`tnode`tstate_index`tprobability"
    $probabilityHeaderIndex = [array]::IndexOf($averageLines, $probabilityHeader)
    $splitScenariosMarker = [array]::IndexOf($averageLines, 'split_scenarios')
    if ($probabilityHeaderIndex -lt 0) {
        throw 'Psychotria model-average output is missing its probability table'
    }
    $probabilityRows = @(
        $averageLines[($probabilityHeaderIndex + 1)..($splitScenariosMarker - 2)] |
            ConvertFrom-Csv -Delimiter "`t" -Header @(
                'criterion', 'node', 'state_index', 'probability'
            )
    )
    $probabilityGroups = $probabilityRows | Group-Object criterion, node
    foreach ($group in $probabilityGroups) {
        $sum = ($group.Group | Measure-Object -Property probability -Sum).Sum
        if ([Math]::Abs([double]$sum - 1.0) -gt 1e-10) {
            throw "Psychotria model-average probabilities do not sum to 1 for $($group.Name)"
        }
    }
    $splitProbabilitiesMarker = [array]::IndexOf(
        $averageLines,
        'cladogenetic_split_probabilities'
    )
    if ($splitScenariosMarker -lt 0 -or $splitProbabilitiesMarker -lt 0) {
        throw 'Psychotria model-average output is missing split scenario tables'
    }
    $scenarioRows = @(
        $averageLines[($splitScenariosMarker + 2)..($splitProbabilitiesMarker - 2)] |
            ConvertFrom-Csv -Delimiter "`t" -Header @(
                'scenario_index', 'node', 'ancestor_state_index', 'left_state_index',
                'right_state_index', 'event'
            )
    )
    $splitRows = @(
        $averageLines[($splitProbabilitiesMarker + 2)..($averageLines.Count - 1)] |
            ConvertFrom-Csv -Delimiter "`t" -Header @(
                'criterion', 'scenario_index', 'probability'
            )
    )
    $scenarioNode = @{}
    foreach ($scenario in $scenarioRows) {
        $scenarioNode[$scenario.scenario_index] = $scenario.node
    }
    $splitSums = @{}
    foreach ($row in $splitRows) {
        $key = $row.criterion + '|' + $scenarioNode[$row.scenario_index]
        if (-not $splitSums.ContainsKey($key)) {
            $splitSums[$key] = 0.0
        }
        $splitSums[$key] += [double]$row.probability
    }
    foreach ($entry in $splitSums.GetEnumerator()) {
        if ([Math]::Abs([double]$entry.Value - 1.0) -gt 1e-10) {
            throw "Psychotria model-average split probabilities do not sum to 1 for $($entry.Key)"
        }
    }
    $storedAverage = (Get-Content -LiteralPath $averagePath -Raw).Replace("`r`n", "`n")

    $resumeLines = & $cli @arguments '--resume'
    if ($LASTEXITCODE -ne 0) {
        throw "model-batch resume failed with exit code $LASTEXITCODE"
    }
    $resumed = ($resumeLines -join "`n") + "`n"
    if ($resumed -ne $first) {
        throw 'resumed comparison differs from the initial comparison'
    }
    $resumedAverage = (Get-Content -LiteralPath $averagePath -Raw).Replace("`r`n", "`n")
    if ($resumedAverage -ne $storedAverage) {
        throw 'resumed model-average output differs from the initial output'
    }

    Write-Output "model_batch_psychotria=passed"
    Write-Output "models=6"
    Write-Output "eligible_models=6"
    Write-Output "model_average_criteria=2"
    Write-Output "model_average_probability_groups=$($probabilityGroups.Count)"
    Write-Output "model_average_split_probability_groups=$($splitSums.Count)"
    Write-Output "output_dir=$OutputRoot"
}
finally {
    Pop-Location
    if ($temporaryOutput -and -not $KeepOutput -and (Test-Path -LiteralPath $OutputRoot)) {
        $resolvedOutput = [System.IO.Path]::GetFullPath($OutputRoot)
        $resolvedTemp = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
        if (-not $resolvedOutput.StartsWith($resolvedTemp, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "refusing to remove non-temporary output path $resolvedOutput"
        }
        Remove-Item -LiteralPath $resolvedOutput -Recurse -Force
    }
}

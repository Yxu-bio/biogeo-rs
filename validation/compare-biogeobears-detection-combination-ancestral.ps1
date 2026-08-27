param(
    [string]$Manifest = "validation/detection_combination_fixtures.tsv",
    [string]$GoldenPath = "validation/golden/biogeobears-detection-combination-ancestral.tsv",
    [double]$ProbabilityTolerance = 0.00002
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$cases = Import-Csv -LiteralPath (Join-Path $repoRoot $Manifest) -Delimiter "`t"
$goldenRows = Import-Csv -LiteralPath (Join-Path $repoRoot $GoldenPath) -Delimiter "`t"
$template = Get-Content -LiteralPath (Join-Path $repoRoot "examples/parameter_tables/dec.tsv")
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("biogeo-detection-ancestral-" + [guid]::NewGuid().ToString("N"))
[System.IO.Directory]::CreateDirectory($tempDir) | Out-Null
$rustRowsByCase = @{}

function New-FixedParameterTable {
    param([object]$Case, [string]$Path)
    $names = @(
        "d", "e", "a", "b", "x", "n", "w", "u", "j", "y", "s", "v",
        "mx01", "mx01j", "mx01y", "mx01s", "mx01v", "mf", "dp", "fdp"
    )
    $values = @{}
    foreach ($name in $names) {
        $values[$name] = [string]$Case.$name
    }
    $lines = foreach ($line in $template) {
        $fields = $line -split "`t", -1
        if ($fields.Count -eq 7 -and $values.ContainsKey($fields[0])) {
            $fields[1] = "fixed"
            $fields[2] = $values[$fields[0]]
            $fields[6] = ""
            $fields -join "`t"
        }
        else {
            $line
        }
    }
    [System.IO.File]::WriteAllText(
        $Path,
        ($lines -join "`n") + "`n",
        [System.Text.UTF8Encoding]::new($false)
    )
}

function Add-OptionalPath {
    param([System.Collections.ArrayList]$Arguments, [string]$Option, [string]$Value)
    if (-not [string]::IsNullOrWhiteSpace($Value) -and $Value -ne "-") {
        [void]$Arguments.Add($Option)
        [void]$Arguments.Add((Join-Path $repoRoot $Value))
    }
}

Push-Location $repoRoot
try {
    foreach ($case in $cases) {
        $caseGoldenRows = @($goldenRows | Where-Object { $_.case_id -eq $case.case_id })
        $posteriorReady = $case.posterior_ready -ne "false"
        if ($posteriorReady -and $caseGoldenRows.Count -eq 0) {
            throw "$($case.case_id): missing BioGeoBEARS ancestral golden rows"
        }
        $parameterPath = Join-Path $tempDir "$($case.case_id).tsv"
        New-FixedParameterTable -Case $case -Path $parameterPath
        [System.Collections.ArrayList]$arguments = @(
            "run", "--release", "-q", "-p", "biogeo-cli", "--",
            "model-evaluate",
            "--tree", (Join-Path $repoRoot $case.tree),
            "--use-detection-model",
            "--detections", (Join-Path $repoRoot $case.detections),
            "--controls", (Join-Path $repoRoot $case.controls),
            "--parameters", $parameterPath,
            "--max-range-size", $case.max_range_size,
            "--root-prior", $case.root_prior,
            "--ancestral-probs"
        )
        Add-OptionalPath -Arguments $arguments -Option "--dispersal-multipliers" -Value $case.dispersal_multipliers
        Add-OptionalPath -Arguments $arguments -Option "--dispersal-strata" -Value $case.dispersal_strata
        Add-OptionalPath -Arguments $arguments -Option "--distance-matrix" -Value $case.distance_matrix
        Add-OptionalPath -Arguments $arguments -Option "--environment-distance-matrix" -Value $case.environment_distance_matrix
        Add-OptionalPath -Arguments $arguments -Option "--area-sizes" -Value $case.area_sizes
        if ($case.include_null_range -eq "true") {
            [void]$arguments.Add("--include-null-range")
        }

        $output = @(& cargo @arguments)
        if ($LASTEXITCODE -ne 0) {
            throw "$($case.case_id): Rust model-evaluate exited with code $LASTEXITCODE"
        }
        $header = "node`tlabel`tkind`tclade`tstate_index`trange_bits`trange`tprobability"
        $headerIndex = [array]::IndexOf($output, $header)
        if ($headerIndex -lt 0) {
            throw "$($case.case_id): CLI output did not contain ancestral probabilities"
        }
        $rustRows = @($output[$headerIndex..($output.Count - 1)] | ConvertFrom-Csv -Delimiter "`t")
        $rustByKey = @{}
        foreach ($row in $rustRows) {
            $key = "$($row.clade)|$($row.range_bits)"
            if ($rustByKey.ContainsKey($key)) {
                throw "$($case.case_id): duplicate Rust ancestral key $key"
            }
            $rustByKey[$key] = $row
        }
        $rustRowsByCase[$case.case_id] = $rustByKey
        if (-not $posteriorReady) {
            Write-Host "$($case.case_id) audit-only Rust ancestral_rows=$($rustRows.Count)"
            continue
        }
        if ($rustRows.Count -ne $caseGoldenRows.Count) {
            throw "$($case.case_id): ancestral row count mismatch rust=$($rustRows.Count) bgb=$($caseGoldenRows.Count)"
        }

        $maxDelta = 0.0
        foreach ($golden in $caseGoldenRows) {
            $key = "$($golden.clade)|$($golden.range_bits)"
            if (-not $rustByKey.ContainsKey($key)) {
                throw "$($case.case_id): missing Rust ancestral key $key"
            }
            $delta = [Math]::Abs(
                [double]$rustByKey[$key].probability - [double]$golden.biogeobears_probability
            )
            $maxDelta = [Math]::Max($maxDelta, $delta)
            if ($delta -gt $ProbabilityTolerance) {
                throw "$($case.case_id): ancestral probability mismatch key=$key delta=$delta"
            }
        }
        Write-Host "$($case.case_id) ok ancestral_rows=$($caseGoldenRows.Count) max_delta=$maxDelta"
    }

    $stratified = $rustRowsByCase["psychotria_detection_full_stratified"]
    $staticEquivalent = $rustRowsByCase["psychotria_detection_stratified_static_equivalent"]
    if ($null -ne $stratified -and $null -ne $staticEquivalent) {
        if ($stratified.Count -ne $staticEquivalent.Count) {
            throw "Rust stratified/static-equivalent ancestral row counts differ"
        }
        $maxEquivalentDelta = 0.0
        foreach ($key in $stratified.Keys) {
            if (-not $staticEquivalent.ContainsKey($key)) {
                throw "Rust static-equivalent ancestral output is missing key $key"
            }
            $delta = [Math]::Abs(
                [double]$stratified[$key].probability - [double]$staticEquivalent[$key].probability
            )
            $maxEquivalentDelta = [Math]::Max($maxEquivalentDelta, $delta)
        }
        if ($maxEquivalentDelta -gt 0.0000001) {
            throw "Rust stratified/static-equivalent ancestral probabilities differ by $maxEquivalentDelta"
        }
        Write-Host "psychotria detection Rust stratified/static-equivalent ancestral max_delta=$maxEquivalentDelta"
    }
}
finally {
    Pop-Location
    Remove-Item -LiteralPath $tempDir -Recurse -Force
}

param(
    [string]$Manifest = "validation/detection_combination_fixtures.tsv",
    [string]$GoldenPath = "validation/golden/biogeobears-detection-combinations.tsv"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$cases = Import-Csv -LiteralPath (Join-Path $repoRoot $Manifest) -Delimiter "`t"
$goldenRows = Import-Csv -LiteralPath (Join-Path $repoRoot $GoldenPath) -Delimiter "`t"
$goldenByCase = @{}
foreach ($row in $goldenRows) {
    if ($goldenByCase.ContainsKey($row.case_id)) {
        throw "duplicate BioGeoBEARS detection combination golden row: $($row.case_id)"
    }
    $goldenByCase[$row.case_id] = $row
}

$template = Get-Content -LiteralPath (Join-Path $repoRoot "examples/parameter_tables/dec.tsv")
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("biogeo-detection-combination-" + [guid]::NewGuid().ToString("N"))
[System.IO.Directory]::CreateDirectory($tempDir) | Out-Null

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
        $expected = $goldenByCase[$case.case_id]
        if ($null -eq $expected) {
            throw "$($case.case_id): missing BioGeoBEARS detection combination golden"
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
            "--root-prior", $case.root_prior
        )
        Add-OptionalPath -Arguments $arguments -Option "--dispersal-multipliers" -Value $case.dispersal_multipliers
        Add-OptionalPath -Arguments $arguments -Option "--dispersal-strata" -Value $case.dispersal_strata
        Add-OptionalPath -Arguments $arguments -Option "--distance-matrix" -Value $case.distance_matrix
        Add-OptionalPath -Arguments $arguments -Option "--environment-distance-matrix" -Value $case.environment_distance_matrix
        Add-OptionalPath -Arguments $arguments -Option "--area-sizes" -Value $case.area_sizes
        if ($case.include_null_range -eq "true") {
            [void]$arguments.Add("--include-null-range")
        }

        $lines = @(& cargo @arguments)
        if ($LASTEXITCODE -ne 0) {
            throw "$($case.case_id): Rust model-evaluate exited with code $LASTEXITCODE"
        }
        $lnLLine = $lines | Where-Object { $_ -like "lnL`t*" } | Select-Object -First 1
        if ($null -eq $lnLLine) {
            throw "$($case.case_id): Rust output did not contain lnL"
        }
        $rustLnL = [double](($lnLLine -split "`t", 2)[1])
        $expectedLnL = [double]$expected.biogeobears_lnL
        $delta = [Math]::Abs($rustLnL - $expectedLnL)
        $tolerance = [double]$case.lnL_tolerance
        if ($delta -gt $tolerance) {
            throw "$($case.case_id): lnL mismatch rust=$rustLnL BioGeoBEARS=$expectedLnL delta=$delta tolerance=$tolerance"
        }
        Write-Host "$($case.case_id) ok rust_lnL=$rustLnL bgb_lnL=$expectedLnL delta=$delta"
    }
}
finally {
    Pop-Location
    Remove-Item -LiteralPath $tempDir -Recurse -Force
}

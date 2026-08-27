param(
    [string]$Manifest = "validation/abw_profile_fixtures.tsv",
    [string]$GoldenPath = "validation/golden/biogeobears-abw-profile.tsv",
    [double]$LnLTolerance = 5e-7
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$cases = Import-Csv -LiteralPath (Join-Path $repoRoot $Manifest) -Delimiter "`t"
$goldenRows = Import-Csv -LiteralPath (Join-Path $repoRoot $GoldenPath) -Delimiter "`t"
$goldenByCase = @{}
foreach ($row in $goldenRows) {
    if ($goldenByCase.ContainsKey($row.case_id)) {
        throw "duplicate BioGeoBEARS golden row: $($row.case_id)"
    }
    $goldenByCase[$row.case_id] = $row
}
$template = Get-Content -LiteralPath (Join-Path $repoRoot "examples/parameter_tables/dec.tsv")
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("biogeo-abw-" + [guid]::NewGuid().ToString("N"))
[System.IO.Directory]::CreateDirectory($tempDir) | Out-Null

function New-FixedParameterTable {
    param([object]$Case, [string]$Path)
    $values = @{
        d = $Case.d
        e = $Case.e
        a = $Case.a
        b = $Case.b
        w = $Case.w
    }
    $lines = foreach ($line in $template) {
        $fields = $line -split "`t", -1
        if ($fields.Count -eq 7 -and $values.ContainsKey($fields[0])) {
            $fields[1] = "fixed"
            $fields[2] = [string]$values[$fields[0]]
            $fields[6] = ""
            $fields -join "`t"
        }
        else {
            $line
        }
    }
    [System.IO.File]::WriteAllText($Path, ($lines -join "`n") + "`n", [System.Text.UTF8Encoding]::new($false))
}

Push-Location $repoRoot
try {
    foreach ($case in $cases) {
        if (-not $goldenByCase.ContainsKey($case.case_id)) {
            throw "$($case.case_id): missing BioGeoBEARS golden row"
        }
        $expected = $goldenByCase[$case.case_id]
        $parameterPath = Join-Path $tempDir "$($case.case_id).tsv"
        New-FixedParameterTable -Case $case -Path $parameterPath
        $args = @(
            "run", "--release", "-q", "-p", "biogeo-cli", "--",
            "model-evaluate",
            "--tree", (Join-Path $repoRoot $case.tree),
            "--ranges", (Join-Path $repoRoot $case.ranges),
            "--parameters", $parameterPath,
            "--dispersal-multipliers", (Join-Path $repoRoot $case.dispersal_multipliers),
            "--max-range-size", $case.max_range_size,
            "--root-prior", $case.root_prior
        )
        if ($case.include_null_range -eq "true") {
            $args += "--include-null-range"
        }
        $output = @(& cargo @args)
        if ($LASTEXITCODE -ne 0) {
            throw "$($case.case_id): Rust model-evaluate exited with code $LASTEXITCODE"
        }
        $lnLLine = $output | Where-Object { $_ -like "lnL`t*" } | Select-Object -First 1
        if ($null -eq $lnLLine) {
            throw "$($case.case_id): Rust output has no lnL field"
        }
        $rustLnL = [double](($lnLLine -split "`t", 2)[1])
        $delta = [Math]::Abs($rustLnL - [double]$expected.biogeobears_lnL)
        if ($delta -gt $LnLTolerance) {
            throw "$($case.case_id): lnL mismatch rust=$rustLnL BioGeoBEARS=$($expected.biogeobears_lnL) delta=$delta"
        }
        Write-Host "$($case.case_id) ok rust_lnL=$rustLnL bgb_lnL=$($expected.biogeobears_lnL) delta=$delta"
    }
}
finally {
    Pop-Location
    Remove-Item -LiteralPath $tempDir -Recurse -Force
}

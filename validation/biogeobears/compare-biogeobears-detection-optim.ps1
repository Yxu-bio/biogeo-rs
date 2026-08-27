param(
    [string]$Manifest = "validation/detection_optimization_fixtures.tsv",
    [string]$GoldenPath = "validation/golden/biogeobears-detection-optim.tsv",
    [double]$LnLTolerance = 2e-5,
    [double]$SingleParameterTolerance = 2e-3,
    [int]$MaxIterations = 1200
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$cases = Import-Csv -LiteralPath (Join-Path $repoRoot $Manifest) -Delimiter "`t"
$goldenRows = Import-Csv -LiteralPath (Join-Path $repoRoot $GoldenPath) -Delimiter "`t"
$goldenByCase = @{}
foreach ($row in $goldenRows) {
    $goldenByCase[$row.case_id] = $row
}
$template = Get-Content -LiteralPath (Join-Path $repoRoot "examples/parameter_tables/dec.tsv")
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("biogeo-detection-optim-" + [guid]::NewGuid().ToString("N"))
[System.IO.Directory]::CreateDirectory($tempDir) | Out-Null

function New-OptimizationParameterTable {
    param([object]$Case, [string]$Path)
    $free = @($Case.free_parameters -split ",")
    $fixedValues = @{
        d = $Case.d
        e = $Case.e
        mf = $Case.fixed_mf
        dp = $Case.fixed_dp
        fdp = $Case.fixed_fdp
    }
    $initialValues = @{
        mf = $Case.init_mf
        dp = $Case.init_dp
        fdp = $Case.init_fdp
    }
    $lines = foreach ($line in $template) {
        $fields = $line -split "`t", -1
        if ($fields.Count -eq 7 -and $fixedValues.ContainsKey($fields[0])) {
            $name = $fields[0]
            if ($free -contains $name) {
                $fields[1] = "free"
                $fields[2] = [string]$initialValues[$name]
                $fields[3] = [string]$Case.min_probability
                $fields[4] = [string]$Case.max_probability
                $fields[5] = "logit"
            }
            else {
                $fields[1] = "fixed"
                $fields[2] = [string]$fixedValues[$name]
            }
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

function Read-ModelOutput {
    param([string[]]$Lines)
    $metadata = @{}
    $parameters = @{}
    $inParameters = $false
    foreach ($line in $Lines) {
        if ($line -eq "parameters") {
            $inParameters = $true
            continue
        }
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        $fields = $line -split "`t"
        if ($inParameters) {
            if ($fields[0] -ne "name" -and $fields.Count -ge 3) {
                $parameters[$fields[0]] = [double]$fields[2]
            }
        }
        elseif ($fields.Count -eq 2) {
            $metadata[$fields[0]] = $fields[1]
        }
    }
    @{ Metadata = $metadata; Parameters = $parameters }
}

Push-Location $repoRoot
try {
    foreach ($case in $cases) {
        $expected = $goldenByCase[$case.case_id]
        if ($null -eq $expected) {
            throw "$($case.case_id): missing BioGeoBEARS optimization golden"
        }
        if ([int]$expected.convergence -ne 0) {
            throw "$($case.case_id): BioGeoBEARS golden did not converge"
        }
        $parameterPath = Join-Path $tempDir "$($case.case_id).tsv"
        New-OptimizationParameterTable -Case $case -Path $parameterPath
        $args = @(
            "run", "--release", "-q", "-p", "biogeo-cli", "--",
            "model-optimize",
            "--tree", (Join-Path $repoRoot $case.tree),
            "--use-detection-model",
            "--detections", (Join-Path $repoRoot $case.detections),
            "--controls", (Join-Path $repoRoot $case.controls),
            "--parameters", $parameterPath,
            "--max-range-size", $case.max_range_size,
            "--root-prior", $case.root_prior,
            "--max-iterations", $MaxIterations,
            "--tolerance", "1e-9"
        )
        if ($case.include_null_range -eq "true") {
            $args += "--include-null-range"
        }
        $lines = @(& cargo @args)
        if ($LASTEXITCODE -ne 0) {
            throw "$($case.case_id): Rust model-optimize exited with code $LASTEXITCODE"
        }
        $parsed = Read-ModelOutput -Lines $lines
        $metadata = $parsed.Metadata
        $parameters = $parsed.Parameters
        if ($metadata.converged -ne "true") {
            throw "$($case.case_id): Rust optimization did not converge in $MaxIterations iterations"
        }
        $lnLDelta = [Math]::Abs([double]$metadata.lnL - [double]$expected.biogeobears_lnL)
        if ($lnLDelta -gt $LnLTolerance) {
            throw "$($case.case_id): optimized lnL mismatch rust=$($metadata.lnL) BioGeoBEARS=$($expected.biogeobears_lnL) delta=$lnLDelta"
        }

        $free = @($case.free_parameters -split ",")
        if ($free.Count -eq 1) {
            $name = $free[0]
            $delta = [Math]::Abs($parameters[$name] - [double]$expected."biogeobears_$name")
            if ($delta -gt $SingleParameterTolerance) {
                throw "$($case.case_id): $name estimate mismatch delta=$delta"
            }
        }
        else {
            $ridgeGap = [Math]::Abs($parameters.dp - $parameters.fdp)
            if ($ridgeGap -gt $SingleParameterTolerance) {
                throw "$($case.case_id): joint optimum did not reach the BioGeoBEARS dp=fdp ridge; gap=$ridgeGap"
            }
        }
        Write-Host "$($case.case_id) ok rust_lnL=$($metadata.lnL) bgb_lnL=$($expected.biogeobears_lnL) delta=$lnLDelta"
    }
}
finally {
    Pop-Location
    Remove-Item -LiteralPath $tempDir -Recurse -Force
}

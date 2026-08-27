param(
    [string]$Manifest = "validation/detection_combination_optimization_fixtures.tsv",
    [string]$GoldenPath = "validation/golden/biogeobears-detection-combination-optim.tsv"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$cases = Import-Csv -LiteralPath (Join-Path $repoRoot $Manifest) -Delimiter "`t"
$goldenRows = Import-Csv -LiteralPath (Join-Path $repoRoot $GoldenPath) -Delimiter "`t"
$goldenByCase = @{}
foreach ($row in $goldenRows) {
    $goldenByCase[$row.case_id] = $row
}
$template = Get-Content -LiteralPath (Join-Path $repoRoot "examples/parameter_tables/dec.tsv")
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("biogeo-detection-combination-optim-" + [guid]::NewGuid().ToString("N"))
[System.IO.Directory]::CreateDirectory($tempDir) | Out-Null

$modelParameterNames = @(
    "d", "e", "a", "b", "x", "n", "w", "u", "j", "y", "s", "v",
    "mx01", "mx01j", "mx01y", "mx01s", "mx01v", "mf", "dp", "fdp"
)
$fixedOwnRustByCase = @{}
$fixedStrictRustByCase = @{}
$strictReferenceByCase = @{}

function New-ParameterTable {
    param(
        [object]$Case,
        [string]$Path,
        [string[]]$FreeParameters,
        [hashtable]$Overrides = @{}
    )
    $values = @{}
    foreach ($name in $modelParameterNames) {
        $values[$name] = if ($Overrides.ContainsKey($name)) {
            [string]$Overrides[$name]
        }
        else {
            [string]$Case.$name
        }
    }
    $lines = foreach ($line in $template) {
        $fields = $line -split "`t", -1
        if ($fields.Count -eq 7 -and $values.ContainsKey($fields[0])) {
            $name = $fields[0]
            $fields[1] = if ($FreeParameters -contains $name) { "free" } else { "fixed" }
            $fields[2] = $values[$name]
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

function Invoke-Model {
    param(
        [object]$Case,
        [string]$Mode,
        [string]$ParameterPath
    )
    [System.Collections.ArrayList]$arguments = @(
        "run", "--release", "-q", "-p", "biogeo-cli", "--",
        $Mode,
        "--tree", (Join-Path $repoRoot $Case.tree),
        "--use-detection-model",
        "--detections", (Join-Path $repoRoot $Case.detections),
        "--controls", (Join-Path $repoRoot $Case.controls),
        "--parameters", $ParameterPath,
        "--max-range-size", $Case.max_range_size,
        "--root-prior", $Case.root_prior
    )
    Add-OptionalPath -Arguments $arguments -Option "--dispersal-multipliers" -Value $Case.dispersal_multipliers
    Add-OptionalPath -Arguments $arguments -Option "--dispersal-strata" -Value $Case.dispersal_strata
    Add-OptionalPath -Arguments $arguments -Option "--distance-matrix" -Value $Case.distance_matrix
    Add-OptionalPath -Arguments $arguments -Option "--environment-distance-matrix" -Value $Case.environment_distance_matrix
    Add-OptionalPath -Arguments $arguments -Option "--area-sizes" -Value $Case.area_sizes
    if ($Case.include_null_range -eq "true") {
        [void]$arguments.Add("--include-null-range")
    }
    if ($Mode -eq "model-optimize") {
        [void]$arguments.Add("--max-iterations")
        [void]$arguments.Add([string]$Case.max_iterations)
        [void]$arguments.Add("--tolerance")
        [void]$arguments.Add("1e-9")
        foreach ($start in @($Case.additional_starts -split ";")) {
            if (-not [string]::IsNullOrWhiteSpace($start)) {
                [void]$arguments.Add("--additional-start")
                [void]$arguments.Add($start)
            }
        }
    }

    $lines = @(& cargo @arguments)
    if ($LASTEXITCODE -ne 0) {
        throw "$($Case.case_id): Rust $Mode exited with code $LASTEXITCODE"
    }
    Read-ModelOutput -Lines $lines
}

function Get-GoldenOverrides {
    param(
        [object]$Golden,
        [string[]]$FreeParameters,
        [string]$CaseId
    )
    $overrides = @{}
    foreach ($name in $FreeParameters) {
        $column = "biogeobears_$name"
        $value = $Golden.$column
        if ([string]::IsNullOrWhiteSpace([string]$value)) {
            throw "$CaseId`: strict golden is missing $column"
        }
        $overrides[$name] = $value
    }
    $overrides
}

Push-Location $repoRoot
try {
    foreach ($case in $cases) {
        $expected = $goldenByCase[$case.case_id]
        if ($null -eq $expected) {
            throw "$($case.case_id): missing BioGeoBEARS joint-module optimization golden"
        }
        if ([int]$expected.convergence -ne 0) {
            throw "$($case.case_id): BioGeoBEARS golden did not converge"
        }
        $goldenColumns = @($expected.PSObject.Properties.Name)
        if ($goldenColumns -contains "nonworsening_starts") {
            if ([int]$expected.nonworsening_starts -lt 1) {
                throw "$($case.case_id): BioGeoBEARS golden has no non-worsening converged start"
            }
            $replayDelta = [Math]::Abs([double]$expected.optimizer_replay_delta)
            if ($replayDelta -gt [double]$case.fixed_lnL_tolerance) {
                throw "$($case.case_id): BioGeoBEARS optimizer endpoint does not replay; delta=$replayDelta"
            }
            if ([double]$expected.optimizer_improvement -lt -0.00000001) {
                throw "$($case.case_id): selected BioGeoBEARS endpoint is worse than its fixed start"
            }
        }
        $free = @($case.free_parameters -split ",")
        $strictReferenceId = [string]$case.strict_reference_case_id
        if ([string]::IsNullOrWhiteSpace($strictReferenceId) -or $strictReferenceId -eq "-") {
            $strictReferenceId = [string]$case.case_id
        }
        $strictExpected = $goldenByCase[$strictReferenceId]
        if ($null -eq $strictExpected) {
            throw "$($case.case_id): missing strict BioGeoBEARS reference $strictReferenceId"
        }
        if ([int]$strictExpected.convergence -ne 0) {
            throw "$($case.case_id): strict BioGeoBEARS reference $strictReferenceId did not converge"
        }
        if ([string]$strictExpected.free_parameters -ne [string]$case.free_parameters) {
            throw "$($case.case_id): strict reference must use the same free-parameter order"
        }
        $strictReferenceByCase[$case.case_id] = $strictReferenceId
        $bgbOverrides = Get-GoldenOverrides `
            -Golden $expected `
            -FreeParameters $free `
            -CaseId $case.case_id

        $fixedPath = Join-Path $tempDir "$($case.case_id)-bgb-point.tsv"
        New-ParameterTable -Case $case -Path $fixedPath -FreeParameters @() -Overrides $bgbOverrides
        $fixed = Invoke-Model -Case $case -Mode "model-evaluate" -ParameterPath $fixedPath
        $fixedDelta = [Math]::Abs(
            [double]$fixed.Metadata.lnL - [double]$expected.biogeobears_lnL
        )
        if ($fixedDelta -gt [double]$case.fixed_lnL_tolerance) {
            throw "$($case.case_id): Rust fixed evaluation at the BioGeoBEARS optimum differs by $fixedDelta"
        }
        $fixedOwnRustByCase[$case.case_id] = [double]$fixed.Metadata.lnL

        $strictFixedDelta = $fixedDelta
        if ($strictReferenceId -ne $case.case_id) {
            $strictOverrides = Get-GoldenOverrides `
                -Golden $strictExpected `
                -FreeParameters $free `
                -CaseId $case.case_id
            $strictFixedPath = Join-Path $tempDir "$($case.case_id)-strict-bgb-point.tsv"
            New-ParameterTable `
                -Case $case `
                -Path $strictFixedPath `
                -FreeParameters @() `
                -Overrides $strictOverrides
            $strictFixed = Invoke-Model `
                -Case $case `
                -Mode "model-evaluate" `
                -ParameterPath $strictFixedPath
            $fixedStrictRustByCase[$case.case_id] = [double]$strictFixed.Metadata.lnL
            $strictFixedDelta = [Math]::Abs(
                [double]$strictFixed.Metadata.lnL - [double]$strictExpected.biogeobears_lnL
            )
            if ($strictFixedDelta -gt [double]$case.fixed_lnL_tolerance) {
                throw "$($case.case_id): Rust fixed evaluation at strict reference $strictReferenceId differs by $strictFixedDelta"
            }
        }
        else {
            $fixedStrictRustByCase[$case.case_id] = [double]$fixed.Metadata.lnL
        }

        $runRustOptimization = [string]$case.run_rust_optimization
        if ([string]::IsNullOrWhiteSpace($runRustOptimization)) {
            $runRustOptimization = "true"
        }
        if ($runRustOptimization -eq "false") {
            Write-Host "$($case.case_id) reference ok fixed_delta=$fixedDelta rust_optimization=skipped"
            continue
        }
        if ($runRustOptimization -ne "true") {
            throw "$($case.case_id): run_rust_optimization must be true or false"
        }

        $optimizationPath = Join-Path $tempDir "$($case.case_id)-optimize.tsv"
        New-ParameterTable -Case $case -Path $optimizationPath -FreeParameters $free
        $optimized = Invoke-Model -Case $case -Mode "model-optimize" -ParameterPath $optimizationPath
        if ($optimized.Metadata.converged -ne "true") {
            throw "$($case.case_id): Rust optimization did not converge"
        }
        $rustLnL = [double]$optimized.Metadata.lnL
        $bgbLnL = [double]$strictExpected.biogeobears_lnL
        $shortfall = $bgbLnL - $rustLnL
        if ($shortfall -gt [double]$case.optimized_lnL_tolerance) {
            throw "$($case.case_id): Rust optimized lnL is below BioGeoBEARS by $shortfall"
        }
        Write-Host "$($case.case_id) ok audit_fixed_delta=$fixedDelta strict_fixed_delta=$strictFixedDelta rust_lnL=$rustLnL strict_bgb_lnL=$bgbLnL rust_advantage=$(-$shortfall)"
    }

    foreach ($case in $cases) {
        $strictReferenceId = $strictReferenceByCase[$case.case_id]
        if ($strictReferenceId -eq $case.case_id) {
            continue
        }
        $strictLnL = $fixedStrictRustByCase[$case.case_id]
        $referenceLnL = $fixedOwnRustByCase[$strictReferenceId]
        if ($null -eq $referenceLnL) {
            throw "$($case.case_id): strict Rust reference $strictReferenceId was not evaluated"
        }
        $equivalenceDelta = [Math]::Abs([double]$strictLnL - [double]$referenceLnL)
        if ($equivalenceDelta -gt 1e-10) {
            throw "$($case.case_id): stratified and static-equivalent Rust lnL differ by $equivalenceDelta"
        }
        Write-Host "$($case.case_id) strict-equivalence ok reference=$strictReferenceId lnL_delta=$equivalenceDelta"
    }
}
finally {
    Pop-Location
    Remove-Item -LiteralPath $tempDir -Recurse -Force
}

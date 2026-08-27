param(
    [double]$RustLnLTolerance = 1e-8,
    [double]$RustParameterTolerance = 1e-6,
    [double]$BioGeoBEARSAdvantageTolerance = 1e-5
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path

function Normalize-Text {
    param([Parameter(Mandatory = $true)][string]$Text)
    return $Text.Replace("`r`n", "`n").TrimEnd("`r", "`n")
}

Push-Location $repoRoot
try {
    Write-Host "== Check versioned preset templates =="
    $templates = @(
        @{ Preset = "dec"; File = "examples/parameter_tables/dec.tsv" },
        @{ Preset = "dec+j"; File = "examples/parameter_tables/decj.tsv" },
        @{ Preset = "divalike"; File = "examples/parameter_tables/divalike.tsv" },
        @{ Preset = "divalike+j"; File = "examples/parameter_tables/divalikej.tsv" },
        @{ Preset = "bayarealike"; File = "examples/parameter_tables/bayarealike.tsv" },
        @{ Preset = "bayarealike+j"; File = "examples/parameter_tables/bayarealikej.tsv" }
    )
    foreach ($template in $templates) {
        $generated = @(& cargo run -q -p biogeo-cli -- parameter-template --preset $template.Preset)
        if ($LASTEXITCODE -ne 0) {
            throw "parameter-template $($template.Preset) failed with exit code $LASTEXITCODE"
        }
        $expectedPath = Join-Path $repoRoot $template.File
        $expected = Get-Content -LiteralPath $expectedPath -Raw
        if ((Normalize-Text ($generated -join "`n")) -cne (Normalize-Text $expected)) {
            throw "$($template.Preset): generated parameter table differs from $($template.File)"
        }
        Write-Host "$($template.Preset) template ok"
    }

    Write-Host "`n== Check generic five-parameter optimization on official Conifer fixture =="
    $args = @(
        "run", "--release", "-q", "-p", "biogeo-cli", "--",
        "model-optimize",
        "--tree", "validation/fixtures/biogeobears_official/conifer_decx/tree.nwk",
        "--ranges", "validation/fixtures/biogeobears_official/conifer_decx/sim_ranges_seed20260712.tsv",
        "--parameters", "validation/parameter_tables/conifer_197tip_xnu.tsv",
        "--distance-matrix", "validation/fixtures/biogeobears_official/conifer_decx/distances.tsv",
        "--environment-distance-matrix", "validation/fixtures/biogeobears_official/conifer_decx/sim_environment_distances.tsv",
        "--area-sizes", "validation/fixtures/biogeobears_official/conifer_decx/sim_area_sizes_geomean1.tsv",
        "--max-range-size", "3",
        "--max-iterations", "1500"
    )
    $output = @(& cargo @args)
    if ($LASTEXITCODE -ne 0) {
        throw "generic Conifer parameter optimization failed with exit code $LASTEXITCODE"
    }

    $metadata = @{}
    $parameters = @{}
    $inParameters = $false
    foreach ($line in $output) {
        if ($line -eq "parameters") {
            $inParameters = $true
            continue
        }
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        $fields = $line -split "`t"
        if ($inParameters) {
            if ($fields[0] -eq "name") {
                continue
            }
            if ($fields.Count -ge 3) {
                $parameters[$fields[0]] = [double]$fields[2]
            }
        }
        elseif ($fields.Count -eq 2) {
            $metadata[$fields[0]] = $fields[1]
        }
    }
    foreach ($key in @("lnL", "converged", "iterations", "evaluations", "starts")) {
        if (-not $metadata.ContainsKey($key)) {
            throw "generic Conifer output is missing metadata field $key"
        }
    }
    foreach ($parameter in @("d", "e", "x", "n", "u")) {
        if (-not $parameters.ContainsKey($parameter)) {
            throw "generic Conifer output is missing parameter $parameter"
        }
    }
    if ($metadata.converged -ne "true") {
        throw "generic Conifer optimization did not converge"
    }
    if ([int]$metadata.starts -ne 1) {
        throw "generic Conifer optimization used $($metadata.starts) starts instead of 1"
    }

    $expected = Import-Csv `
        -LiteralPath (Join-Path $repoRoot "validation/golden/rust-dec-xnu-optim.tsv") `
        -Delimiter "`t" |
        Where-Object { $_.case_id -eq "conifer_197tip_xnu_sim_start_true" } |
        Select-Object -First 1
    if ($null -eq $expected) {
        throw "missing Rust Conifer joint-optimization golden"
    }
    $lnLDelta = [Math]::Abs([double]$metadata.lnL - [double]$expected.rust_lnL)
    if ($lnLDelta -gt $RustLnLTolerance) {
        throw "generic Conifer lnL regression delta=$lnLDelta"
    }
    foreach ($parameter in @("d", "e", "x", "n", "u")) {
        $delta = [Math]::Abs($parameters[$parameter] - [double]$expected."rust_$parameter")
        if ($delta -gt $RustParameterTolerance) {
            throw "generic Conifer $parameter regression delta=$delta"
        }
    }

    $bgb = Import-Csv `
        -LiteralPath (Join-Path $repoRoot "validation/golden/biogeobears-dec-xnu-optim.tsv") `
        -Delimiter "`t" |
        Where-Object { $_.case_id -eq "conifer_197tip_xnu_sim_start_true" } |
        Select-Object -First 1
    if ($null -eq $bgb) {
        throw "missing BioGeoBEARS Conifer joint-optimization golden"
    }
    $advantage = [double]$metadata.lnL - [double]$bgb.biogeobears_lnL
    if ($advantage -lt -$BioGeoBEARSAdvantageTolerance) {
        throw "generic Conifer lnL is lower than BioGeoBEARS by $(-$advantage)"
    }

    Write-Host (
        "Passed: lnL={0}, iterations={1}, evaluations={2}, BioGeoBEARS delta={3}." -f `
            $metadata.lnL,
            $metadata.iterations,
            $metadata.evaluations,
            $advantage
    )
}
finally {
    Pop-Location
}

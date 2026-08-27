param(
    [switch]$RefreshBioGeoBEARS
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$manifest = "validation/cladogenesis_parameter_optimization_fixtures.tsv"
$optimizationGolden = "validation/golden/biogeobears-cladogenesis-parameter-optim.tsv"
$profileGolden = "validation/golden/biogeobears-cladogenesis-parameter-profile.tsv"

Push-Location $repoRoot
try {
    if ($RefreshBioGeoBEARS) {
        Write-Host "== Refresh BioGeoBEARS cladogenesis parameter goldens =="
        & Rscript `
            "validation/biogeobears/biogeobears-cladogenesis-parameter-optim-golden.R" `
            $manifest `
            $optimizationGolden `
            $profileGolden
        if ($LASTEXITCODE -ne 0) {
            throw "BioGeoBEARS golden generation failed with exit code $LASTEXITCODE"
        }
    }

    foreach ($path in @($manifest, $optimizationGolden, $profileGolden)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Missing validation input: $path"
        }
    }

    $optimizationRows = @(Import-Csv -LiteralPath $optimizationGolden -Delimiter "`t")
    $profileRows = @(Import-Csv -LiteralPath $profileGolden -Delimiter "`t")
    if ($optimizationRows.Count -ne 8) {
        throw "Expected 8 optimization rows, found $($optimizationRows.Count)"
    }
    if ($profileRows.Count -ne 240) {
        throw "Expected 240 profile rows, found $($profileRows.Count)"
    }
    $failedConvergence = @($optimizationRows | Where-Object { $_.convergence -ne "0" })
    if ($failedConvergence.Count -gt 0) {
        throw "BioGeoBEARS has $($failedConvergence.Count) non-converged optimization rows"
    }
    $unknownSources = @(
        $optimizationRows |
            Where-Object { $_.candidate_source -notin @("optimizer", "profile_grid") }
    )
    if ($unknownSources.Count -gt 0) {
        throw "BioGeoBEARS golden contains an unknown candidate_source"
    }

    Write-Host "== Check Rust profiles and generic optimization =="
    & cargo test `
        -p biogeo-core `
        --test biogeobears_cladogenesis_parameter_optimization `
        -- --nocapture
    if ($LASTEXITCODE -ne 0) {
        throw "Rust cladogenesis parameter validation failed with exit code $LASTEXITCODE"
    }

    $screened = @($optimizationRows | Where-Object { $_.candidate_source -eq "profile_grid" })
    Write-Host (
        "Passed: {0} optimization cases, {1} fixed profile points, {2} profile-screened optimum." -f `
            $optimizationRows.Count,
            $profileRows.Count,
            $screened.Count
    )
}
finally {
    Pop-Location
}

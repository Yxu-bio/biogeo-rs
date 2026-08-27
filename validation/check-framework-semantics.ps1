param(
    [switch]$IncludeLagrangeReference,
    [string]$LagrangeScratchRoot = ""
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

function Invoke-CargoStep {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    Write-Host "`n== $Name =="
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Name failed with exit code $LASTEXITCODE"
    }
}

function Invoke-ScriptStep {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Script,
        [hashtable]$Parameters = @{}
    )

    Write-Host "`n== $Name =="
    & (Join-Path $PSScriptRoot $Script) @Parameters
}

Push-Location $repoRoot
try {
    Invoke-CargoStep -Name "Rust format" -Arguments @("fmt", "--all", "--", "--check")
    Invoke-CargoStep -Name "Rust unit tests" -Arguments @("test", "--workspace", "--locked")

    Invoke-ScriptStep -Name "Rust DEC fixture regression" -Script "check-rust-dec-fixtures.ps1"
    Invoke-ScriptStep `
        -Name "Rust DIVALIKE fixture regression" `
        -Script "check-rust-dec-fixtures.ps1" `
        -Parameters @{
            Manifest = "validation/divalike_fixtures.tsv"
            Command = "divalike"
        }
    Invoke-ScriptStep -Name "Rust DEC+J fixture regression" -Script "check-rust-decj-fixtures.ps1"
    Invoke-ScriptStep `
        -Name "Rust maxent cladogenesis fixture regression" `
        -Script "check-rust-decj-fixtures.ps1" `
        -Parameters @{ Manifest = "validation/maxent_fixtures.tsv" }

    Invoke-ScriptStep -Name "BioGeoBEARS DEC fixed likelihood golden" -Script "compare-biogeobears-dec.ps1"
    Invoke-ScriptStep -Name "BioGeoBEARS DEC ancestral posterior golden" -Script "compare-biogeobears-dec-ancestral.ps1"
    Invoke-ScriptStep -Name "BioGeoBEARS DEC split posterior golden" -Script "compare-biogeobears-dec-split.ps1"
    Invoke-ScriptStep -Name "BioGeoBEARS DEC optimization golden" -Script "compare-biogeobears-dec-optim.ps1"

    Invoke-ScriptStep `
        -Name "BioGeoBEARS DIVALIKE fixed likelihood golden" `
        -Script "compare-biogeobears-dec.ps1" `
        -Parameters @{
            Manifest = "validation/divalike_fixtures.tsv"
            Golden = "validation/golden/biogeobears-divalike.tsv"
            Command = "divalike"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS DIVALIKE split posterior golden" `
        -Script "compare-biogeobears-dec-split.ps1" `
        -Parameters @{
            Manifest = "validation/divalike_fixtures.tsv"
            Golden = "validation/golden/biogeobears-divalike-split.tsv"
            Command = "divalike"
            ProbabilityTolerance = 1e-6
            WeightTolerance = 1e-8
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS DIVALIKE ancestral posterior golden" `
        -Script "compare-biogeobears-dec-ancestral.ps1" `
        -Parameters @{
            Manifest = "validation/divalike_fixtures.tsv"
            Golden = "validation/golden/biogeobears-divalike-ancestral.tsv"
            Command = "divalike"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS DIVALIKE optimization golden" `
        -Script "compare-biogeobears-dec-optim.ps1" `
        -Parameters @{
            Manifest = "validation/divalike_fixtures.tsv"
            Golden = "validation/golden/biogeobears-divalike-optim.tsv"
            Command = "divalike-optimize"
        }

    Invoke-ScriptStep `
        -Name "Rust BAYAREALIKE fixture regression" `
        -Script "check-rust-dec-fixtures.ps1" `
        -Parameters @{
            Manifest = "validation/bayarealike_fixtures.tsv"
            Command = "bayarealike"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS BAYAREALIKE fixed likelihood golden" `
        -Script "compare-biogeobears-dec.ps1" `
        -Parameters @{
            Manifest = "validation/bayarealike_fixtures.tsv"
            Golden = "validation/golden/biogeobears-bayarealike.tsv"
            Command = "bayarealike"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS BAYAREALIKE split posterior golden" `
        -Script "compare-biogeobears-dec-split.ps1" `
        -Parameters @{
            Manifest = "validation/bayarealike_fixtures.tsv"
            Golden = "validation/golden/biogeobears-bayarealike-split.tsv"
            Command = "bayarealike"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS BAYAREALIKE ancestral posterior golden" `
        -Script "compare-biogeobears-dec-ancestral.ps1" `
        -Parameters @{
            Manifest = "validation/bayarealike_fixtures.tsv"
            Golden = "validation/golden/biogeobears-bayarealike-ancestral.tsv"
            Command = "bayarealike"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS BAYAREALIKE optimization golden" `
        -Script "compare-biogeobears-dec-optim.ps1" `
        -Parameters @{
            Manifest = "validation/bayarealike_fixtures.tsv"
            Golden = "validation/golden/biogeobears-bayarealike-optim.tsv"
            Command = "bayarealike-optimize"
        }

    Invoke-ScriptStep `
        -Name "Rust DIVALIKE+J fixture regression" `
        -Script "check-rust-dec-fixtures.ps1" `
        -Parameters @{
            Manifest = "validation/divalikej_fixtures.tsv"
            Command = "divalike"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS DIVALIKE+J fixed likelihood golden" `
        -Script "compare-biogeobears-dec.ps1" `
        -Parameters @{
            Manifest = "validation/divalikej_fixtures.tsv"
            Golden = "validation/golden/biogeobears-divalikej.tsv"
            Command = "divalike"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS DIVALIKE+J split posterior golden" `
        -Script "compare-biogeobears-dec-split.ps1" `
        -Parameters @{
            Manifest = "validation/divalikej_fixtures.tsv"
            Golden = "validation/golden/biogeobears-divalikej-split.tsv"
            Command = "divalike"
            ProbabilityTolerance = 2e-6
            WeightTolerance = 1e-8
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS DIVALIKE+J ancestral posterior golden" `
        -Script "compare-biogeobears-dec-ancestral.ps1" `
        -Parameters @{
            Manifest = "validation/divalikej_fixtures.tsv"
            Golden = "validation/golden/biogeobears-divalikej-ancestral.tsv"
            Command = "divalike"
            ProbabilityTolerance = 2e-6
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS DIVALIKE+J optimization golden" `
        -Script "compare-biogeobears-decj-optim.ps1" `
        -Parameters @{
            Manifest = "validation/divalikej_fixtures.tsv"
            Golden = "validation/golden/biogeobears-divalikej-optim.tsv"
            Command = "divalikej-optimize"
            LnLTolerance = 2e-5
            MultiStartPoints = 2
        }

    Invoke-ScriptStep `
        -Name "Rust BAYAREALIKE+J fixture regression" `
        -Script "check-rust-dec-fixtures.ps1" `
        -Parameters @{
            Manifest = "validation/bayarealikej_fixtures.tsv"
            Command = "bayarealike"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS BAYAREALIKE+J fixed likelihood golden" `
        -Script "compare-biogeobears-dec.ps1" `
        -Parameters @{
            Manifest = "validation/bayarealikej_fixtures.tsv"
            Golden = "validation/golden/biogeobears-bayarealikej.tsv"
            Command = "bayarealike"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS BAYAREALIKE+J split posterior golden" `
        -Script "compare-biogeobears-dec-split.ps1" `
        -Parameters @{
            Manifest = "validation/bayarealikej_fixtures.tsv"
            Golden = "validation/golden/biogeobears-bayarealikej-split.tsv"
            Command = "bayarealike"
            ProbabilityTolerance = 2e-6
            WeightTolerance = 1e-8
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS BAYAREALIKE+J ancestral posterior golden" `
        -Script "compare-biogeobears-dec-ancestral.ps1" `
        -Parameters @{
            Manifest = "validation/bayarealikej_fixtures.tsv"
            Golden = "validation/golden/biogeobears-bayarealikej-ancestral.tsv"
            Command = "bayarealike"
            ProbabilityTolerance = 2e-6
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS BAYAREALIKE+J optimization golden" `
        -Script "compare-biogeobears-decj-optim.ps1" `
        -Parameters @{
            Manifest = "validation/bayarealikej_fixtures.tsv"
            Golden = "validation/golden/biogeobears-bayarealikej-optim.tsv"
            Command = "bayarealikej-optimize"
            LnLTolerance = 2e-5
            MultiStartPoints = 2
        }

    Invoke-ScriptStep `
        -Name "Rust directional dispersal fixture regression" `
        -Script "check-rust-dec-fixtures.ps1" `
        -Parameters @{
            Manifest = "validation/dispersal_fixtures.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS directional dispersal fixed likelihood golden" `
        -Script "compare-biogeobears-dec.ps1" `
        -Parameters @{
            Manifest = "validation/dispersal_fixtures.tsv"
            Golden = "validation/golden/biogeobears-dispersal.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS directional dispersal split posterior golden" `
        -Script "compare-biogeobears-dec-split.ps1" `
        -Parameters @{
            Manifest = "validation/dispersal_fixtures.tsv"
            Golden = "validation/golden/biogeobears-dispersal-split.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS directional dispersal ancestral posterior golden" `
        -Script "compare-biogeobears-dec-ancestral.ps1" `
        -Parameters @{
            Manifest = "validation/dispersal_fixtures.tsv"
            Golden = "validation/golden/biogeobears-dispersal-ancestral.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS directional dispersal optimization golden" `
        -Script "compare-biogeobears-dec-optim.ps1" `
        -Parameters @{
            Manifest = "validation/dispersal_fixtures.tsv"
            Golden = "validation/golden/biogeobears-dispersal-optim.tsv"
            Command = "dec-optimize"
        }

    Invoke-ScriptStep `
        -Name "Rust distance, environment, and extirpation modifier fixture regression" `
        -Script "check-rust-dec-fixtures.ps1" `
        -Parameters @{
            Manifest = "validation/anagenesis_modifier_fixtures.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS distance, environment, and extirpation fixed likelihood golden" `
        -Script "compare-biogeobears-dec.ps1" `
        -Parameters @{
            Manifest = "validation/anagenesis_modifier_fixtures.tsv"
            Golden = "validation/golden/biogeobears-anagenesis-modifiers.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS distance, environment, and extirpation split posterior golden" `
        -Script "compare-biogeobears-dec-split.ps1" `
        -Parameters @{
            Manifest = "validation/anagenesis_modifier_fixtures.tsv"
            Golden = "validation/golden/biogeobears-anagenesis-modifiers-split.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS distance, environment, and extirpation ancestral posterior golden" `
        -Script "compare-biogeobears-dec-ancestral.ps1" `
        -Parameters @{
            Manifest = "validation/anagenesis_modifier_fixtures.tsv"
            Golden = "validation/golden/biogeobears-anagenesis-modifiers-ancestral.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS distance, environment, and extirpation optimization golden" `
        -Script "compare-biogeobears-dec-optim.ps1" `
        -Parameters @{
            Manifest = "validation/anagenesis_modifier_fixtures.tsv"
            Golden = "validation/golden/biogeobears-anagenesis-modifiers-optim.tsv"
            Command = "dec-optimize"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS DEC free x/n/u exponent optimization golden" `
        -Script "compare-biogeobears-dec-exponent-optim.ps1"
    Invoke-ScriptStep `
        -Name "Rust DEC x/n/u pair-profile regression" `
        -Script "check-dec-pair-profiles.ps1"
    Invoke-ScriptStep `
        -Name "BioGeoBEARS DEC pair-profile selected-point optimization golden" `
        -Script "compare-biogeobears-dec-optim.ps1" `
        -Parameters @{
            Manifest = "validation/pair_profile_semantic_fixtures.tsv"
            Golden = "validation/golden/biogeobears-pair-profile-semantic-optim.tsv"
            Command = "dec-optimize"
            MultiStartPoints = 2
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS DEC joint x/n/u fixed-likelihood golden" `
        -Script "compare-biogeobears-dec.ps1" `
        -Parameters @{
            Manifest = "validation/xnu_fixed_fixtures.tsv"
            Golden = "validation/golden/biogeobears-dec-xnu-fixed.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "Rust and BioGeoBEARS DEC joint d/e/x/n/u optimization golden" `
        -Script "compare-biogeobears-dec-xnu-optim.ps1"

    Invoke-ScriptStep `
        -Name "Rust time-stratified dispersal fixture regression" `
        -Script "check-rust-dec-fixtures.ps1" `
        -Parameters @{
            Manifest = "validation/time_stratified_fixtures.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS time-stratified fixed likelihood golden" `
        -Script "compare-biogeobears-dec.ps1" `
        -Parameters @{
            Manifest = "validation/time_stratified_fixtures.tsv"
            Golden = "validation/golden/biogeobears-time-stratified.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS time-stratified split posterior golden" `
        -Script "compare-biogeobears-dec-split.ps1" `
        -Parameters @{
            Manifest = "validation/time_stratified_fixtures.tsv"
            Golden = "validation/golden/biogeobears-time-stratified-split.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS time-stratified ancestral posterior golden" `
        -Script "compare-biogeobears-dec-ancestral.ps1" `
        -Parameters @{
            Manifest = "validation/time_stratified_fixtures.tsv"
            Golden = "validation/golden/biogeobears-time-stratified-ancestral.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS time-stratified optimization golden" `
        -Script "compare-biogeobears-dec-optim.ps1" `
        -Parameters @{
            Manifest = "validation/time_stratified_fixtures.tsv"
            Golden = "validation/golden/biogeobears-time-stratified-optim.tsv"
            Command = "dec-optimize"
        }

    Invoke-ScriptStep `
        -Name "Rust raw time-stratified anagenesis fixture regression" `
        -Script "check-rust-dec-fixtures.ps1" `
        -Parameters @{
            Manifest = "validation/time_stratified_raw_fixtures.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS raw time-stratified fixed likelihood golden" `
        -Script "compare-biogeobears-dec.ps1" `
        -Parameters @{
            Manifest = "validation/time_stratified_raw_fixtures.tsv"
            Golden = "validation/golden/biogeobears-time-stratified-raw.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS raw time-stratified split posterior golden" `
        -Script "compare-biogeobears-dec-split.ps1" `
        -Parameters @{
            Manifest = "validation/time_stratified_raw_fixtures.tsv"
            Golden = "validation/golden/biogeobears-time-stratified-raw-split.tsv"
            Command = "dec"
            ProbabilityTolerance = 2e-5
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS raw time-stratified ancestral posterior golden" `
        -Script "compare-biogeobears-dec-ancestral.ps1" `
        -Parameters @{
            Manifest = "validation/time_stratified_raw_fixtures.tsv"
            Golden = "validation/golden/biogeobears-time-stratified-raw-ancestral.tsv"
            Command = "dec"
            ProbabilityTolerance = 2e-5
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS raw time-stratified optimization golden" `
        -Script "compare-biogeobears-dec-optim.ps1" `
        -Parameters @{
            Manifest = "validation/time_stratified_raw_fixtures.tsv"
            Golden = "validation/golden/biogeobears-time-stratified-raw-optim.tsv"
            Command = "dec-optimize"
            LnLTolerance = 2e-5
        }

    Invoke-ScriptStep `
        -Name "Rust time-stratified range-state constraint regression" `
        -Script "check-rust-dec-fixtures.ps1" `
        -Parameters @{
            Manifest = "validation/state_constraint_fixtures.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS range-state constraint fixed likelihood golden" `
        -Script "compare-biogeobears-dec.ps1" `
        -Parameters @{
            Manifest = "validation/state_constraint_fixtures.tsv"
            Golden = "validation/golden/biogeobears-state-constraints.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS range-state constraint ancestral posterior golden" `
        -Script "compare-biogeobears-dec-ancestral.ps1" `
        -Parameters @{
            Manifest = "validation/state_constraint_fixtures.tsv"
            Golden = "validation/golden/biogeobears-state-constraints-ancestral.tsv"
            Command = "dec"
            ProbabilityTolerance = 2e-5
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS areas-allowed split posterior golden" `
        -Script "compare-biogeobears-dec-split.ps1" `
        -Parameters @{
            Manifest = "validation/state_constraint_fixtures.tsv"
            Golden = "validation/golden/biogeobears-state-constraints-split.tsv"
            Command = "dec"
            ProbabilityTolerance = 2e-5
            WeightTolerance = 1e-8
            IgnoreZeroProbabilityPlaceholders = $true
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS range-state constraint optimization golden" `
        -Script "compare-biogeobears-dec-optim.ps1" `
        -Parameters @{
            Manifest = "validation/state_constraint_fixtures.tsv"
            Golden = "validation/golden/biogeobears-state-constraints-optim.tsv"
            Command = "dec-optimize"
            LnLTolerance = 5e-5
            MultiStartPoints = 2
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS official fossil-tip stochastic-history posterior distribution" `
        -Script "check-fossil-tip-bsm.ps1"

    Invoke-ScriptStep `
        -Name "Rust direct-ancestor fixture regression" `
        -Script "check-rust-dec-fixtures.ps1" `
        -Parameters @{
            Manifest = "validation/direct_ancestor_fixtures.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS direct-ancestor fixed likelihood golden" `
        -Script "compare-biogeobears-dec.ps1" `
        -Parameters @{
            Manifest = "validation/direct_ancestor_fixtures.tsv"
            Golden = "validation/golden/biogeobears-direct-ancestor.tsv"
            Command = "dec"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS direct-ancestor ancestral posterior golden" `
        -Script "compare-biogeobears-dec-ancestral.ps1" `
        -Parameters @{
            Manifest = "validation/direct_ancestor_fixtures.tsv"
            Golden = "validation/golden/biogeobears-direct-ancestor-ancestral.tsv"
            Command = "dec"
            ProbabilityTolerance = 2e-5
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS direct-ancestor split posterior golden" `
        -Script "compare-biogeobears-dec-split.ps1" `
        -Parameters @{
            Manifest = "validation/direct_ancestor_fixtures.tsv"
            Golden = "validation/golden/biogeobears-direct-ancestor-split.tsv"
            Command = "dec"
            ProbabilityTolerance = 2e-5
            WeightTolerance = 1e-8
            IgnoreZeroProbabilityPlaceholders = $true
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS direct-ancestor optimization golden" `
        -Script "compare-biogeobears-dec-optim.ps1" `
        -Parameters @{
            Manifest = "validation/direct_ancestor_fixtures.tsv"
            Golden = "validation/golden/biogeobears-direct-ancestor-optim.tsv"
            Command = "dec-optimize"
            LnLTolerance = 2e-5
            MultiStartPoints = 2
        }

    Invoke-ScriptStep -Name "BioGeoBEARS DEC+J fixed likelihood golden" -Script "compare-biogeobears-decj.ps1"
    Invoke-ScriptStep `
        -Name "BioGeoBEARS DEC+J ancestral posterior golden" `
        -Script "compare-biogeobears-dec-ancestral.ps1" `
        -Parameters @{
            Manifest = "validation/decj_fixtures.tsv"
            Golden = "validation/golden/biogeobears-decj-ancestral.tsv"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS DEC+J split posterior golden" `
        -Script "compare-biogeobears-dec-split.ps1" `
        -Parameters @{
            Manifest = "validation/decj_fixtures.tsv"
            Golden = "validation/golden/biogeobears-decj-split.tsv"
            WeightTolerance = 1e-8
        }
    Invoke-ScriptStep -Name "BioGeoBEARS DEC+J optimization golden" -Script "compare-biogeobears-decj-optim.ps1"

    Invoke-ScriptStep `
        -Name "BioGeoBEARS maxent cladogenesis likelihood golden" `
        -Script "compare-biogeobears-decj.ps1" `
        -Parameters @{
            Manifest = "validation/maxent_fixtures.tsv"
            Golden = "validation/golden/biogeobears-maxent.tsv"
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS maxent cladogenesis split golden" `
        -Script "compare-biogeobears-dec-split.ps1" `
        -Parameters @{
            Manifest = "validation/maxent_fixtures.tsv"
            Golden = "validation/golden/biogeobears-maxent-split.tsv"
            ProbabilityTolerance = 1e-6
            WeightTolerance = 1e-7
        }
    Invoke-ScriptStep `
        -Name "BioGeoBEARS maxent cladogenesis optimization golden" `
        -Script "compare-biogeobears-dec-optim.ps1" `
        -Parameters @{
            Manifest = "validation/maxent_fixtures.tsv"
            Golden = "validation/golden/biogeobears-maxent-optim.tsv"
        }

    Invoke-ScriptStep `
        -Name "Versioned parameter-table framework" `
        -Script "check-parameter-table-framework.ps1"
    Invoke-ScriptStep `
        -Name "Six-preset modifier support and rejection matrix" `
        -Script "check-preset-modifier-matrix.ps1" `
        -Parameters @{ SkipBuild = $true }
    Invoke-ScriptStep `
        -Name "Official and special tree input contract" `
        -Script "check-tree-input-equivalence.ps1" `
        -Parameters @{ SkipBuild = $true }
    Invoke-ScriptStep `
        -Name "Large state-space success and early resource rejection" `
        -Script "check-large-state-space-resources.ps1" `
        -Parameters @{ SkipBuild = $true }

    Invoke-ScriptStep `
        -Name "BioGeoBEARS a/b/w fixed profile golden" `
        -Script "compare-biogeobears-abw-profile.ps1"
    Invoke-ScriptStep `
        -Name "BioGeoBEARS detection fixed profile golden" `
        -Script "compare-biogeobears-detection-profile.ps1"
    Invoke-ScriptStep `
        -Name "BioGeoBEARS detection optimization golden" `
        -Script "compare-biogeobears-detection-optim.ps1"
    Invoke-ScriptStep `
        -Name "BioGeoBEARS detection cross-module fixed golden" `
        -Script "compare-biogeobears-detection-combinations.ps1"
    Invoke-ScriptStep `
        -Name "BioGeoBEARS detection cross-module ancestral posterior golden" `
        -Script "compare-biogeobears-detection-combination-ancestral.ps1"
    Invoke-ScriptStep `
        -Name "BioGeoBEARS detection cross-module split posterior golden" `
        -Script "compare-biogeobears-detection-combination-split.ps1"
    Invoke-ScriptStep `
        -Name "BioGeoBEARS detection cross-module optimization golden" `
        -Script "compare-biogeobears-detection-combination-optim.ps1"
    Invoke-ScriptStep `
        -Name "BioGeoBEARS constrained detection full-stack fixnode posterior golden" `
        -Script "compare-biogeobears-detection-full-stack-fixnode.ps1"
    Invoke-ScriptStep `
        -Name "BioGeoBEARS constrained detection full-stack d/e/x/n/u optimization golden" `
        -Script "check-biogeobears-detection-full-stack-optimization.ps1"
    Invoke-ScriptStep `
        -Name "Constrained detection full-stack stochastic-history distribution" `
        -Script "check-detection-full-stack-bsm-distribution.ps1"

    if ($IncludeLagrangeReference) {
        $lagrangeParams = @{}
        if (-not [string]::IsNullOrWhiteSpace($LagrangeScratchRoot)) {
            $lagrangeParams.ScratchRoot = $LagrangeScratchRoot
        }
        Invoke-ScriptStep `
            -Name "Independent LAGRANGE-ng semantic reference" `
            -Script "compare-lagrange-ng-reference.ps1" `
            -Parameters $lagrangeParams
        Invoke-ScriptStep `
            -Name "Official LAGRANGE-ng example reference" `
            -Script "compare-lagrange-ng-official-reference.ps1" `
            -Parameters $lagrangeParams
    }
}
finally {
    Pop-Location
}

Write-Host "`nFramework semantic validation passed."
if (-not $IncludeLagrangeReference) {
    Write-Host "LAGRANGE-ng reference was not requested and did not affect the golden gate."
}

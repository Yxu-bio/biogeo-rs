param(
    [int]$SampleCount = 5000,
    [switch]$RefreshBioGeoBEARS,
    [int64]$BioGeoBEARSSeed = 20260716,
    [int64]$RustSeed = 20260717,
    [string]$RustThreads = "auto",
    [int]$RustMaxInFlight = 0,
    [int]$RustCheckpointSamples = 0,
    [int]$RustShardSamples = 0,
    [ValidateSet("legacy", "full", "compact", "summary")]
    [string]$RustOutputLevel = "legacy",
    [int]$BioGeoBEARSBatchSize = 100,
    [double]$MaxMeanZ = 5.0,
    [double]$KsMultiplier = 2.0,
    [double]$MaxPeriodShareDifference = 0.02,
    [string]$BioGeoBEARSGolden = "validation/golden/biogeobears-bsm-distribution-samples.tsv",
    [string]$Report = "validation/benchmark-runs/bsm-distribution-comparison.tsv"
)

$ErrorActionPreference = "Stop"
$culture = [Globalization.CultureInfo]::InvariantCulture
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
. (Join-Path $PSScriptRoot "rscript-portable.ps1")

if ($SampleCount -lt 100) {
    throw "SampleCount must be at least 100 for a distribution-level comparison"
}
if ($BioGeoBEARSBatchSize -lt 1) {
    throw "BioGeoBEARSBatchSize must be positive"
}
if ($RustThreads -notmatch "(?i)^(auto|[1-9][0-9]*)$") {
    throw "RustThreads must be auto or a positive integer"
}
if ($RustMaxInFlight -lt 0) {
    throw "RustMaxInFlight must be zero (automatic) or a positive integer"
}
if ($RustCheckpointSamples -lt 0) {
    throw "RustCheckpointSamples must be zero (automatic) or a positive integer"
}
if ($RustShardSamples -lt 0) {
    throw "RustShardSamples must be zero (monolithic) or a positive integer"
}

function Resolve-RepoPath([string]$Path, [bool]$MustExist = $true) {
    $candidate = if ([IO.Path]::IsPathRooted($Path)) {
        $Path
    }
    else {
        Join-Path $repoRoot $Path
    }
    if ($MustExist) {
        return (Resolve-Path $candidate).Path
    }
    return [IO.Path]::GetFullPath($candidate)
}

function Invoke-RScript {
    param(
        [Parameter(Mandatory = $true)][string]$Script,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    $rArguments = @((Join-Path $repoRoot $Script)) + $Arguments
    Invoke-PortableRScript -Arguments $rArguments -FailureMessage "$Script failed"
}

Push-Location $repoRoot
try {
    $goldenPath = Resolve-RepoPath $BioGeoBEARSGolden $false
    if ($RefreshBioGeoBEARS) {
        Write-Host "== Refresh BioGeoBEARS official BSM distribution golden =="
        Invoke-RScript `
            -Script "validation/biogeobears-bsm-distribution.R" `
            -Arguments @(
                $SampleCount.ToString($culture),
                $BioGeoBEARSGolden,
                $BioGeoBEARSSeed.ToString($culture),
                $BioGeoBEARSBatchSize.ToString($culture)
            )
    }
    elseif (-not (Test-Path -LiteralPath $goldenPath -PathType Leaf)) {
        throw "Missing BioGeoBEARS BSM golden: $goldenPath. Run with -RefreshBioGeoBEARS once."
    }

    $bgbSamples = @(Import-Csv -LiteralPath $goldenPath -Delimiter "`t")
    if ($bgbSamples.Count -ne $SampleCount) {
        throw "BioGeoBEARS golden contains $($bgbSamples.Count) samples, expected $SampleCount"
    }
    if (@($bgbSamples | Where-Object { [int]$_.manual_fallback_branches -ne 0 }).Count -gt 0) {
        throw "BioGeoBEARS golden contains manual fallback histories"
    }

    $optimizedPath = Resolve-RepoPath "validation/golden/biogeobears-state-constraints-optim.tsv"
    $optimized = @(
        Import-Csv -LiteralPath $optimizedPath -Delimiter "`t" |
            Where-Object { $_.case_id -eq "bsm_3taxa_official_areas_allowed" }
    )
    if ($optimized.Count -ne 1 -or [int]$optimized[0].convergence -ne 0) {
        throw "Could not load converged official BSM ML parameters"
    }
    $d = [double]::Parse($optimized[0].biogeobears_d, $culture)
    $e = [double]::Parse($optimized[0].biogeobears_e, $culture)

    Write-Host "`n== Build release CLI =="
    & cargo build --release -p biogeo-cli
    if ($LASTEXITCODE -ne 0) {
        throw "Release build failed with exit code $LASTEXITCODE"
    }

    $runRoot = Resolve-RepoPath "validation/benchmark-runs" $false
    [IO.Directory]::CreateDirectory($runRoot) | Out-Null
    $rawRustPath = Join-Path $runRoot "rust-bsm-distribution-run.tsv"
    $rustStreamDir = [IO.Path]::GetFullPath((Join-Path $runRoot "rust-bsm-distribution-stream"))
    $normalizedRunRoot = [IO.Path]::GetFullPath($runRoot).TrimEnd(
        [IO.Path]::DirectorySeparatorChar,
        [IO.Path]::AltDirectorySeparatorChar
    )
    if ([IO.Path]::GetDirectoryName($rustStreamDir) -ne $normalizedRunRoot) {
        throw "Refusing to clean a Rust BSM output directory outside benchmark-runs"
    }
    if (Test-Path -LiteralPath $rustStreamDir) {
        if (-not (Test-Path -LiteralPath $rustStreamDir -PathType Container)) {
            throw "Rust BSM output path exists and is not a directory: $rustStreamDir"
        }
        [IO.Directory]::Delete($rustStreamDir, $true)
    }
    $rustSamplesPath = Join-Path $runRoot "rust-bsm-distribution-samples.tsv"
    $reportPath = Resolve-RepoPath $Report $false
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($reportPath)) | Out-Null

    $treePath = Resolve-RepoPath "validation/fixtures/biogeobears_official/bsm_3taxa_areas_allowed/tree.nwk"
    $rangesPath = Resolve-RepoPath "validation/fixtures/biogeobears_official/bsm_3taxa_areas_allowed/ranges.tsv"
    $strataPath = Resolve-RepoPath "validation/fixtures/biogeobears_official/bsm_3taxa_areas_allowed/anagenetic_strata.tsv"
    $rustExe = Resolve-RepoPath "target/release/biogeo-cli.exe"
    $rustArguments = @(
        "dec",
        "--tree", $treePath,
        "--ranges", $rangesPath,
        "--d", $d.ToString("R", $culture),
        "--e", $e.ToString("R", $culture),
        "--max-range-size", "3",
        "--include-null-range",
        "--root-prior", "flat",
        "--dispersal-strata", $strataPath,
        "--bsm-samples", $SampleCount.ToString($culture),
        "--bsm-output-dir", $rustStreamDir,
        "--bsm-output-level", $RustOutputLevel,
        "--bsm-threads", $RustThreads,
        "--seed", $RustSeed.ToString($culture)
    )
    if ($RustMaxInFlight -gt 0) {
        $rustArguments += @(
            "--bsm-max-in-flight",
            $RustMaxInFlight.ToString($culture)
        )
    }
    if ($RustCheckpointSamples -gt 0) {
        $rustArguments += @(
            "--bsm-checkpoint-samples",
            $RustCheckpointSamples.ToString($culture)
        )
    }
    if ($RustShardSamples -gt 0) {
        $rustArguments += @(
            "--bsm-shard-samples",
            $RustShardSamples.ToString($culture)
        )
    }

    Write-Host "`n== Generate independent Rust BSM sample =="
    $rustElapsed = Measure-Command {
        & $rustExe @rustArguments | Set-Content -LiteralPath $rawRustPath -Encoding utf8
        if ($LASTEXITCODE -ne 0) {
            throw "Rust BSM generation failed with exit code $LASTEXITCODE"
        }
    }
    Write-Host "Rust fixed-model likelihood, stochastic-history sampling, and output: $($rustElapsed.TotalSeconds.ToString('0.###', $culture)) s"

    $rustMetadata = @{}
    foreach ($row in Import-Csv -LiteralPath (Join-Path $rustStreamDir "metadata.tsv") -Delimiter "`t") {
        $rustMetadata[$row.key] = $row.value
    }
    Write-Host "Rust BSM execution: format=$($rustMetadata['format']), output-level=$($rustMetadata['output_level']), protocol=$($rustMetadata['rng_protocol']), threads=$($rustMetadata['threads']), max-in-flight=$($rustMetadata['max_in_flight']), checkpoint-samples=$($rustMetadata['checkpoint_samples']), shard-samples=$($rustMetadata['shard_samples'])"

    Write-Host "`n== Extract Rust stochastic-history summaries =="
    Invoke-RScript `
        -Script "validation/extract-rust-bsm-distribution.R" `
        -Arguments @($rustStreamDir, $rustSamplesPath)

    Write-Host "`n== Compare independent empirical distributions =="
    Invoke-RScript `
        -Script "validation/compare-bsm-distributions.R" `
        -Arguments @(
            $goldenPath,
            $rustSamplesPath,
            $reportPath,
            $MaxMeanZ.ToString("R", $culture),
            $KsMultiplier.ToString("R", $culture),
            $MaxPeriodShareDifference.ToString("R", $culture)
        )

    $metadataPath = Join-Path `
        ([IO.Path]::GetDirectoryName($goldenPath)) `
        (([IO.Path]::GetFileNameWithoutExtension($goldenPath)) + "-metadata.tsv")
    $bgbElapsed = [double]::NaN
    if (Test-Path -LiteralPath $metadataPath -PathType Leaf) {
        $metadata = Import-Csv -LiteralPath $metadataPath -Delimiter "`t"
        if ($null -ne $metadata.elapsed_seconds) {
            $bgbElapsed = [double]::Parse($metadata.elapsed_seconds, $culture)
        }
    }
    $speedup = if ([double]::IsNaN($bgbElapsed)) {
        [double]::NaN
    }
    else {
        $bgbElapsed / $rustElapsed.TotalSeconds
    }

    $timingPath = Join-Path $runRoot "bsm-distribution-timing.tsv"
    $timingValues = @(
        $SampleCount.ToString($culture),
        $(if ([double]::IsNaN($bgbElapsed)) { "NA" } else { $bgbElapsed.ToString("R", $culture) }),
        $rustElapsed.TotalSeconds.ToString("R", $culture),
        $(if ([double]::IsNaN($speedup)) { "NA" } else { $speedup.ToString("R", $culture) }),
        $rustMetadata["rng_protocol"],
        $rustMetadata["threads"],
        $rustMetadata["max_in_flight"]
    )
    $timingLines = @(
        "sample_count`tbiogeobears_history_sampling_seconds`trust_cli_seconds`tobserved_speedup`trust_rng_protocol`trust_threads`trust_max_in_flight",
        ($timingValues -join "`t")
    )
    [IO.File]::WriteAllLines($timingPath, $timingLines, [Text.UTF8Encoding]::new($false))

    if (-not [double]::IsNaN($speedup)) {
        Write-Host "Observed BSM timing ratio (BioGeoBEARS history sampling / Rust CLI): $($speedup.ToString('0.##', $culture))x"
    }
    Write-Host "Comparison report: $reportPath"
    Write-Host "Timing report: $timingPath"
}
finally {
    Pop-Location
}

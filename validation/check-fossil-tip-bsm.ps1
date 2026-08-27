param(
    [int]$SampleCount = 20000,
    [string]$RustThreads = "auto",
    [string]$Report = "validation/benchmark-runs/fossil-tip-bsm-posterior-report.tsv"
)

$ErrorActionPreference = "Stop"
$culture = [Globalization.CultureInfo]::InvariantCulture
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
. (Join-Path $PSScriptRoot "rscript-portable.ps1")

if ($SampleCount -lt 100) {
    throw "SampleCount must be at least 100 for a distribution-level check"
}
if ($RustThreads -notmatch "(?i)^(auto|[1-9][0-9]*)$") {
    throw "RustThreads must be auto or a positive integer"
}

function Resolve-RepoPath([string]$Path, [bool]$MustExist = $true) {
    $candidate = if ([IO.Path]::IsPathRooted($Path)) { $Path } else { Join-Path $repoRoot $Path }
    if ($MustExist) {
        return (Resolve-Path -LiteralPath $candidate).Path
    }
    return [IO.Path]::GetFullPath($candidate)
}

$benchmarkRoot = Resolve-RepoPath "validation/benchmark-runs" $false
[IO.Directory]::CreateDirectory($benchmarkRoot) | Out-Null
$runDir = [IO.Path]::GetFullPath((Join-Path $benchmarkRoot ("fossil-tip-bsm-" + [guid]::NewGuid().ToString("N"))))
if ([IO.Path]::GetDirectoryName($runDir) -ne $benchmarkRoot) {
    throw "Refusing to use a temporary directory outside validation/benchmark-runs"
}
[IO.Directory]::CreateDirectory($runDir) | Out-Null

try {
    Push-Location $repoRoot
    try {
        & cargo build --release -q -p biogeo-cli
        if ($LASTEXITCODE -ne 0) {
            throw "Release build failed with exit code $LASTEXITCODE"
        }

        $bsmDir = Join-Path $runDir "bsm"
        $treePath = Resolve-RepoPath "validation/fixtures/biogeobears_official/bsm_3taxa_fossil/tree.nwk"
        $rangesPath = Resolve-RepoPath "validation/fixtures/biogeobears_official/bsm_3taxa_fossil/ranges.tsv"
        $strataPath = Resolve-RepoPath "validation/fixtures/biogeobears_official/bsm_3taxa_fossil/anagenetic_strata.tsv"
        $rustExe = Resolve-RepoPath "target/release/biogeo-cli.exe"
        $rustArgs = @(
            "dec",
            "--tree", $treePath,
            "--ranges", $rangesPath,
            "--d", "0.1",
            "--e", "0.2",
            "--max-range-size", "3",
            "--include-null-range",
            "--root-prior", "flat",
            "--dispersal-strata", $strataPath,
            "--bsm-samples", $SampleCount.ToString($culture),
            "--bsm-output-dir", $bsmDir,
            "--bsm-threads", $RustThreads,
            "--seed", "20260721"
        )

        $elapsed = Measure-Command {
            & $rustExe @rustArgs | Set-Content -LiteralPath (Join-Path $runDir "cli.tsv") -Encoding utf8
            if ($LASTEXITCODE -ne 0) {
                throw "Rust fossil-tip BSM failed with exit code $LASTEXITCODE"
            }
        }

        $metadata = @{}
        foreach ($row in Import-Csv -LiteralPath (Join-Path $bsmDir "metadata.tsv") -Delimiter "`t") {
            $metadata[$row.key] = $row.value
        }
        if ($metadata["status"] -ne "complete" -or [int]$metadata["completed_samples"] -ne $SampleCount) {
            throw "Rust fossil-tip BSM output is incomplete"
        }

        $expectedBranchTime = 4.91
        $eventCounts = @(Import-Csv -LiteralPath (Join-Path $bsmDir "sample_event_counts.tsv") -Delimiter "`t")
        if ($eventCounts.Count -ne $SampleCount) {
            throw "Event-count table has $($eventCounts.Count) samples, expected $SampleCount"
        }
        foreach ($row in $eventCounts) {
            if ([Math]::Abs([double]$row.total_branch_time - $expectedBranchTime) -gt 1e-10) {
                throw "Sample $($row.sample) occupancy does not sum to fossil-tree branch time"
            }
        }

        $humanSegments = @(
            Import-Csv -LiteralPath (Join-Path $bsmDir "branch_segments.tsv") -Delimiter "`t" |
                Where-Object { $_.child_clade -eq "human" }
        )
        if ($humanSegments.Count -ne 2 * $SampleCount) {
            throw "Human fossil branch must contain exactly two period segments per sample"
        }
        foreach ($row in $humanSegments) {
            $start = [double]$row.start_time_from_parent
            $end = [double]$row.end_time_from_parent
            if ([int]$row.q_index -eq 1) {
                $valid = [Math]::Abs($start) -le 1e-12 -and [Math]::Abs($end - 0.9) -le 1e-12
            }
            elseif ([int]$row.q_index -eq 0) {
                $valid = [Math]::Abs($start - 0.9) -le 1e-12 -and [Math]::Abs($end - 0.91) -le 1e-12
            }
            else {
                $valid = $false
            }
            if (-not $valid) {
                throw "Human fossil branch has an invalid period segment in sample $($row.sample)"
            }
        }

        $humanTipStates = @(
            Import-Csv -LiteralPath (Join-Path $bsmDir "node_states.tsv") -Delimiter "`t" |
                Where-Object { $_.kind -eq "tip" -and $_.clade -eq "human" }
        )
        if ($humanTipStates.Count -ne $SampleCount -or @($humanTipStates | Where-Object { $_.range_bits -ne "4" }).Count -ne 0) {
            throw "Human fossil tip is not fixed to observed range C in every sample"
        }

        $reportPath = Resolve-RepoPath $Report $false
        [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($reportPath)) | Out-Null
        Invoke-PortableRScript `
            -Arguments @(
                (Resolve-RepoPath "validation/compare-detection-full-stack-bsm-to-fixnode.R"),
                $bsmDir,
                (Resolve-RepoPath "validation/golden/biogeobears-state-constraints-ancestral.tsv"),
                (Resolve-RepoPath "validation/golden/biogeobears-state-constraints-split.tsv"),
                $reportPath,
                "7",
                "0.04",
                "0.06",
                "bsm_3taxa_official_fossil_tip"
            ) `
            -FailureMessage "Fossil-tip BSM posterior comparison failed"

        Write-Host "Fossil-tip BSM ok samples=$SampleCount branch_time=$expectedBranchTime elapsed_seconds=$($elapsed.TotalSeconds.ToString('0.###', $culture)) threads=$($metadata['threads'])"
        Write-Host "Comparison report: $reportPath"
    }
    finally {
        Pop-Location
    }
}
finally {
    if ((Test-Path -LiteralPath $runDir -PathType Container) -and [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($runDir)) -eq $benchmarkRoot) {
        Remove-Item -LiteralPath $runDir -Recurse -Force
    }
}

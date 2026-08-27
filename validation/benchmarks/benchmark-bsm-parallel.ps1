param(
    [ValidateSet("official-3taxon", "conifer-197tip")]
    [string]$Workload = "official-3taxon",
    [int]$SampleCount = 0,
    [int[]]$ThreadCounts = @(1, 2, 4, 8, 16),
    [int]$Repetitions = 3,
    [int64]$Seed = 20260717,
    [string]$Report = ""
)

$ErrorActionPreference = "Stop"
$culture = [Globalization.CultureInfo]::InvariantCulture
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$tableNames = @(
    "node_states.tsv",
    "cladogenetic_splits.tsv",
    "branch_segments.tsv",
    "sample_event_counts.tsv",
    "sample_period_event_counts.tsv",
    "sample_state_occupancy.tsv",
    "sample_period_state_occupancy.tsv",
    "anagenetic_events.tsv"
)

if ($SampleCount -eq 0) {
    $SampleCount = if ($Workload -eq "official-3taxon") { 10000 } else { 100 }
}
if ([string]::IsNullOrWhiteSpace($Report)) {
    $Report = "validation/benchmark-runs/bsm-parallel-$Workload.tsv"
}
if ($SampleCount -lt 1) {
    throw "SampleCount must be positive"
}
if ($Repetitions -lt 1) {
    throw "Repetitions must be positive"
}
$ThreadCounts = @($ThreadCounts | Sort-Object -Unique)
if ($ThreadCounts.Count -eq 0 -or @($ThreadCounts | Where-Object { $_ -lt 1 }).Count -gt 0) {
    throw "ThreadCounts must contain positive integers"
}
if (1 -notin $ThreadCounts) {
    throw "ThreadCounts must include 1 for the speedup baseline"
}
if (@($ThreadCounts | Where-Object { $_ -gt $SampleCount }).Count -gt 0) {
    throw "ThreadCounts cannot exceed SampleCount"
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

function Get-DataFingerprint([string]$OutputDir) {
    $parts = foreach ($name in $tableNames) {
        $path = Join-Path $OutputDir $name
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Missing BSM data table: $path"
        }
        "$name=$((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash)"
    }
    $bytes = [Text.Encoding]::UTF8.GetBytes(($parts -join "`n"))
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha256.ComputeHash($bytes))).Replace("-", "")
    }
    finally {
        $sha256.Dispose()
    }
}

function Get-Median([double[]]$Values) {
    $ordered = @($Values | Sort-Object)
    $middle = [int][Math]::Floor($ordered.Count / 2)
    if (($ordered.Count % 2) -eq 1) {
        return $ordered[$middle]
    }
    return ($ordered[$middle - 1] + $ordered[$middle]) / 2.0
}

Push-Location $repoRoot
try {
    Write-Host "== Build release CLI =="
    & cargo build --release -p biogeo-cli
    if ($LASTEXITCODE -ne 0) {
        throw "Release build failed with exit code $LASTEXITCODE"
    }

    if ($Workload -eq "official-3taxon") {
        $optimized = @(
            Import-Csv -LiteralPath (
                Resolve-RepoPath "validation/golden/biogeobears-state-constraints-optim.tsv"
            ) -Delimiter "`t" |
                Where-Object { $_.case_id -eq "bsm_3taxa_official_areas_allowed" }
        )
        if ($optimized.Count -ne 1 -or [int]$optimized[0].convergence -ne 0) {
            throw "Could not load converged official BSM ML parameters"
        }
        $d = [double]::Parse($optimized[0].biogeobears_d, $culture)
        $e = [double]::Parse($optimized[0].biogeobears_e, $culture)
        $treePath = Resolve-RepoPath "validation/fixtures/biogeobears_official/bsm_3taxa_areas_allowed/tree.nwk"
        $rangesPath = Resolve-RepoPath "validation/fixtures/biogeobears_official/bsm_3taxa_areas_allowed/ranges.tsv"
        $modelArguments = @(
            "--include-null-range",
            "--root-prior", "flat",
            "--dispersal-strata", (
                Resolve-RepoPath "validation/fixtures/biogeobears_official/bsm_3taxa_areas_allowed/anagenetic_strata.tsv"
            )
        )
    }
    else {
        $d = 0.0729437109598974
        $e = 0.0268990166943367
        $treePath = Resolve-RepoPath "validation/fixtures/biogeobears_official/conifer_decx/tree.nwk"
        $rangesPath = Resolve-RepoPath "validation/fixtures/biogeobears_official/conifer_decx/ranges.tsv"
        $modelArguments = @("--root-prior", "flat")
    }
    $rustExe = Resolve-RepoPath "target/release/biogeo-cli.exe"
    $benchmarkRoot = Resolve-RepoPath "validation/benchmark-runs" $false
    [IO.Directory]::CreateDirectory($benchmarkRoot) | Out-Null
    $workRoot = [IO.Path]::GetFullPath((Join-Path $benchmarkRoot "bsm-parallel-work"))
    $normalizedBenchmarkRoot = [IO.Path]::GetFullPath($benchmarkRoot).TrimEnd(
        [IO.Path]::DirectorySeparatorChar,
        [IO.Path]::AltDirectorySeparatorChar
    )
    if ([IO.Path]::GetDirectoryName($workRoot) -ne $normalizedBenchmarkRoot) {
        throw "Refusing to manage a BSM benchmark directory outside benchmark-runs"
    }
    if (Test-Path -LiteralPath $workRoot) {
        [IO.Directory]::Delete($workRoot, $true)
    }
    [IO.Directory]::CreateDirectory($workRoot) | Out-Null

    $rows = [Collections.Generic.List[object]]::new()
    $baselineFingerprint = $null
    for ($repetition = 1; $repetition -le $Repetitions; $repetition++) {
        foreach ($threads in $ThreadCounts) {
            $outputDir = Join-Path $workRoot "threads-$threads-run-$repetition"
            $arguments = @(
                "dec",
                "--tree", $treePath,
                "--ranges", $rangesPath,
                "--d", $d.ToString("R", $culture),
                "--e", $e.ToString("R", $culture),
                "--max-range-size", "3",
                $modelArguments
            ) + @(
                "--bsm-samples", $SampleCount.ToString($culture),
                "--bsm-output-dir", $outputDir,
                "--bsm-threads", $threads.ToString($culture),
                "--bsm-max-in-flight", ([Math]::Min($SampleCount, 2 * $threads)).ToString($culture),
                "--seed", $Seed.ToString($culture)
            )

            $elapsed = Measure-Command {
                & $rustExe @arguments | Out-Null
                if ($LASTEXITCODE -ne 0) {
                    throw "Rust BSM benchmark failed with exit code $LASTEXITCODE"
                }
            }
            $metadata = @{}
            foreach ($row in Import-Csv -LiteralPath (Join-Path $outputDir "metadata.tsv") -Delimiter "`t") {
                $metadata[$row.key] = $row.value
            }
            if ($metadata["status"] -ne "complete") {
                throw "Rust BSM benchmark output is incomplete"
            }
            $fingerprint = Get-DataFingerprint $outputDir
            if ($null -eq $baselineFingerprint) {
                $baselineFingerprint = $fingerprint
            }
            elseif ($fingerprint -ne $baselineFingerprint) {
                throw "BSM data tables differ for thread count $threads, repetition $repetition"
            }

            $rows.Add([pscustomobject]@{
                workload = $Workload
                sample_count = $SampleCount
                threads = [int]$metadata["threads"]
                repetition = $repetition
                seconds = $elapsed.TotalSeconds
                max_in_flight = [int]$metadata["max_in_flight"]
                rng_protocol = $metadata["rng_protocol"]
                data_fingerprint = $fingerprint
            })
            Write-Host "threads=$threads run=$repetition seconds=$($elapsed.TotalSeconds.ToString('0.###', $culture))"
            [IO.Directory]::Delete($outputDir, $true)
        }
    }

    $oneThreadMedian = Get-Median @(
        $rows | Where-Object { $_.threads -eq 1 } | ForEach-Object { $_.seconds }
    )
    $summary = foreach ($threads in $ThreadCounts) {
        $threadRows = @($rows | Where-Object { $_.threads -eq [Math]::Min($threads, $SampleCount) })
        $median = Get-Median @($threadRows | ForEach-Object { $_.seconds })
        [pscustomobject]@{
            workload = $Workload
            sample_count = $SampleCount
            threads = [Math]::Min($threads, $SampleCount)
            repetitions = $Repetitions
            median_seconds = $median
            speedup_vs_one_thread = $oneThreadMedian / $median
            rng_protocol = $threadRows[0].rng_protocol
            data_fingerprint = $baselineFingerprint
        }
    }

    $reportPath = Resolve-RepoPath $Report $false
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($reportPath)) | Out-Null
    $summary | Export-Csv -LiteralPath $reportPath -Delimiter "`t" -NoTypeInformation -Encoding utf8
    $rawPath = [IO.Path]::Combine(
        [IO.Path]::GetDirectoryName($reportPath),
        [IO.Path]::GetFileNameWithoutExtension($reportPath) + "-runs.tsv"
    )
    $rows | Export-Csv -LiteralPath $rawPath -Delimiter "`t" -NoTypeInformation -Encoding utf8

    Write-Host ""
    $summary | Format-Table threads, median_seconds, speedup_vs_one_thread -AutoSize
    Write-Host "All runs produced data fingerprint $baselineFingerprint"
    Write-Host "Summary report: $reportPath"
    Write-Host "Raw report: $rawPath"
}
finally {
    if ($null -ne $workRoot -and (Test-Path -LiteralPath $workRoot)) {
        [IO.Directory]::Delete($workRoot, $true)
    }
    Pop-Location
}

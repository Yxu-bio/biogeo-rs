param(
    [int]$Areas = 12,
    [int]$Tips = 64,
    [int]$MaxRangeSize = 3,
    [double]$D = 0.04,
    [double]$E = 0.015,
    [double]$Mx01 = 0.0001,
    [bool]$IncludeNullRange = $true,
    [int]$RustRepeats = 5,
    [int]$BioGeoBEARSRepeats = 3,
    [double]$LikelihoodTolerance = 0.00001
)

$ErrorActionPreference = "Stop"
$culture = [Globalization.CultureInfo]::InvariantCulture

function Format-Number([double]$Value) {
    return $Value.ToString("0.######", $culture)
}

function Get-Binomial([int]$N, [int]$K) {
    if ($K -lt 0 -or $K -gt $N) {
        return 0
    }
    if ($K -eq 0 -or $K -eq $N) {
        return 1
    }

    $K = [Math]::Min($K, $N - $K)
    [double]$result = 1
    for ($i = 1; $i -le $K; $i++) {
        $result = $result * ($N - $K + $i) / $i
    }

    return [int][Math]::Round($result)
}

function Get-StateCount([int]$AreaCount, [int]$MaxSize, [bool]$HasNullRange) {
    $count = 0
    if ($HasNullRange) {
        $count += 1
    }
    for ($size = 1; $size -le $MaxSize; $size++) {
        $count += Get-Binomial -N $AreaCount -K $size
    }
    return $count
}

function Get-BranchLength([int]$Start, [int]$Count, [int]$Depth) {
    return 0.05 + (0.013 * (($Start + (3 * $Count) + (5 * $Depth)) % 19))
}

function New-Subtree([int]$Start, [int]$Count, [int]$Depth, [bool]$IsRoot) {
    if ($Count -eq 1) {
        $tip = "T$($Start.ToString("000"))"
        $length = Format-Number (Get-BranchLength -Start $Start -Count $Count -Depth $Depth)
        return "$tip`:$length"
    }

    $leftCount = [int][Math]::Floor($Count / 2)
    $rightCount = $Count - $leftCount
    $left = New-Subtree -Start $Start -Count $leftCount -Depth ($Depth + 1) -IsRoot $false
    $right = New-Subtree -Start ($Start + $leftCount) -Count $rightCount -Depth ($Depth + 1) -IsRoot $false
    $body = "($left,$right)"
    if ($IsRoot) {
        return $body
    }

    $length = Format-Number (Get-BranchLength -Start $Start -Count $Count -Depth $Depth)
    return "$body`:$length"
}

function Write-StressInputs([string]$TreePath, [string]$RangesPath) {
    $utf8NoBom = [Text.UTF8Encoding]::new($false)
    $tree = (New-Subtree -Start 1 -Count $Tips -Depth 0 -IsRoot $true) + ";"
    [IO.File]::WriteAllText($TreePath, "$tree`n", $utf8NoBom)

    $areaNames = @(1..$Areas | ForEach-Object { "Area$_" })
    $lines = [Collections.Generic.List[string]]::new()
    $lines.Add((@("tip") + $areaNames) -join "`t")

    $rangeSizeCap = [Math]::Min($MaxRangeSize, $Areas)
    for ($tipIndex = 1; $tipIndex -le $Tips; $tipIndex++) {
        $rangeSize = 1 + (($tipIndex - 1) % $rangeSizeCap)
        $bits = @(0) * $Areas
        $start = (($tipIndex * 7) + (3 * $rangeSize)) % $Areas

        for ($offset = 0; $offset -lt $rangeSize; $offset++) {
            $areaIndex = ($start + $offset) % $Areas
            $bits[$areaIndex] = 1
        }

        $tip = "T$($tipIndex.ToString("000"))"
        $lines.Add((@($tip) + $bits) -join "`t")
    }

    [IO.File]::WriteAllLines($RangesPath, [string[]]$lines, $utf8NoBom)
}

function Invoke-RustDec([string]$RustExe, [string]$TreePath, [string]$RangesPath, [int]$Iteration) {
    $args = @(
        "dec",
        "--tree", $TreePath,
        "--ranges", $RangesPath,
        "--d", $D.ToString($culture),
        "--e", $E.ToString($culture),
        "--mx01", $Mx01.ToString($culture),
        "--max-range-size", $MaxRangeSize.ToString($culture),
        "--root-prior", "flat"
    )
    if ($IncludeNullRange) {
        $args += "--include-null-range"
    }

    $sw = [Diagnostics.Stopwatch]::StartNew()
    $output = & $RustExe @args 2>&1
    $exitCode = $LASTEXITCODE
    $sw.Stop()
    if ($exitCode -ne 0) {
        throw "Rust CLI exited with code $exitCode`n$($output -join "`n")"
    }

    $lnLLine = @($output | Where-Object { $_ -like "lnL`t*" })[0]
    if ([string]::IsNullOrWhiteSpace($lnLLine)) {
        throw "Rust CLI output did not contain lnL"
    }
    $lnL = ($lnLLine -split "`t", 2)[1]

    [pscustomobject]@{
        tool = "rust"
        iteration = $Iteration
        seconds = Format-Number $sw.Elapsed.TotalSeconds
        lnL = $lnL
    }
}

function Write-TimingTable($Rows, [string]$Path) {
    $lines = [Collections.Generic.List[string]]::new()
    $lines.Add("tool`titeration`tseconds`tlnL")
    foreach ($row in $Rows) {
        $lines.Add("$($row.tool)`t$($row.iteration)`t$($row.seconds)`t$($row.lnL)")
    }
    Set-Content -Path $Path -Value $lines -Encoding utf8
}

function Get-Mean([double[]]$Values) {
    return (($Values | Measure-Object -Average).Average)
}

function Get-Median([double[]]$Values) {
    $sorted = @($Values | Sort-Object)
    $middle = [int][Math]::Floor($sorted.Count / 2)
    if (($sorted.Count % 2) -eq 1) {
        return $sorted[$middle]
    }
    return (($sorted[$middle - 1] + $sorted[$middle]) / 2.0)
}

if ($Areas -lt 1) {
    throw "Areas must be positive"
}
if ($Tips -lt 2) {
    throw "Tips must be at least 2"
}
if ($MaxRangeSize -lt 1 -or $MaxRangeSize -gt $Areas) {
    throw "MaxRangeSize must be between 1 and Areas"
}
if ([double]::IsNaN($Mx01) -or [double]::IsInfinity($Mx01) -or $Mx01 -lt 0.00001 -or $Mx01 -gt 0.99999) {
    throw "Mx01 must be finite and between 0.00001 and 0.99999"
}
if ($RustRepeats -lt 1 -or $BioGeoBEARSRepeats -lt 0) {
    throw "RustRepeats must be positive and BioGeoBEARSRepeats must be non-negative"
}
if ([double]::IsNaN($LikelihoodTolerance) -or [double]::IsInfinity($LikelihoodTolerance) -or $LikelihoodTolerance -lt 0) {
    throw "LikelihoodTolerance must be finite and non-negative"
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$mxCase = (Format-Number $Mx01) -replace "\.", "p"
$caseId = "dec-stress-$($Areas)a-$($Tips)t-m$MaxRangeSize-mx$mxCase"
$runRoot = Join-Path $repoRoot "validation/benchmark-runs/$caseId"
New-Item -ItemType Directory -Force -Path $runRoot | Out-Null

$treePath = Join-Path $runRoot "tree.nwk"
$rangesPath = Join-Path $runRoot "ranges.tsv"
$rustTimesPath = Join-Path $runRoot "rust-times.tsv"
$bgbTimesPath = Join-Path $runRoot "biogeobears-times.tsv"
$summaryPath = Join-Path $runRoot "summary.tsv"

Write-StressInputs -TreePath $treePath -RangesPath $rangesPath
$stateCount = Get-StateCount -AreaCount $Areas -MaxSize $MaxRangeSize -HasNullRange $IncludeNullRange

Write-Host "Stress case: $caseId states=$stateCount include_null_range=$IncludeNullRange mx01=$(Format-Number $Mx01)"
Write-Host "Tree: $treePath"
Write-Host "Ranges: $rangesPath"

Push-Location $repoRoot
try {
    Write-Host "Building release Rust CLI..."
    & cargo build --release -q -p biogeo-cli
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with code $LASTEXITCODE"
    }

    $rustExe = Join-Path $repoRoot "target/release/biogeo-cli.exe"
    if (-not (Test-Path $rustExe)) {
        throw "Release binary not found: $rustExe"
    }

    Write-Host "Running Rust warmup..."
    [void](Invoke-RustDec -RustExe $rustExe -TreePath $treePath -RangesPath $rangesPath -Iteration 0)

    $rustRows = @()
    for ($iteration = 1; $iteration -le $RustRepeats; $iteration++) {
        $row = Invoke-RustDec -RustExe $rustExe -TreePath $treePath -RangesPath $rangesPath -Iteration $iteration
        $rustRows += $row
        Write-Host "Rust iteration $iteration seconds=$($row.seconds) lnL=$($row.lnL)"
    }
    Write-TimingTable -Rows $rustRows -Path $rustTimesPath

    if ($BioGeoBEARSRepeats -gt 0) {
        Write-Host "Running BioGeoBEARS warm-session benchmark..."
        $rArgs = @(
            "validation/benchmark-biogeobears-dec.R",
            $treePath,
            $rangesPath,
            $D.ToString($culture),
            $E.ToString($culture),
            $MaxRangeSize.ToString($culture),
            $IncludeNullRange.ToString().ToLowerInvariant(),
            $Mx01.ToString($culture),
            $BioGeoBEARSRepeats.ToString($culture),
            $bgbTimesPath
        )
        $rStopwatch = [Diagnostics.Stopwatch]::StartNew()
        $previousErrorActionPreference = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try {
            $rOutput = & Rscript @rArgs 2>&1
            $rExitCode = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $previousErrorActionPreference
        }
        $rStopwatch.Stop()
        if ($rExitCode -ne 0) {
            throw "Rscript exited with code $rExitCode`n$($rOutput -join "`n")"
        }
        $rOutput | ForEach-Object { Write-Host $_ }
    } else {
        Write-Host "Skipping BioGeoBEARS benchmark because BioGeoBEARSRepeats=0"
        "tool`titeration`tseconds`tlnL" | Set-Content -Path $bgbTimesPath -Encoding utf8
        $rStopwatch = [Diagnostics.Stopwatch]::new()
    }
}
finally {
    Pop-Location
}

$rustSeconds = [double[]]@($rustRows | ForEach-Object { [double]::Parse($_.seconds, $culture) })
$rustMean = Get-Mean $rustSeconds
$rustMedian = Get-Median $rustSeconds

if ($BioGeoBEARSRepeats -gt 0) {
    $bgbRows = Import-Csv -Path $bgbTimesPath -Delimiter "`t"
    $bgbSeconds = [double[]]@($bgbRows | ForEach-Object { [double]::Parse($_.seconds, $culture) })
    $bgbMean = Get-Mean $bgbSeconds
    $bgbMedian = Get-Median $bgbSeconds
    $speedupMean = $bgbMean / $rustMean
    $speedupMedian = $bgbMedian / $rustMedian
    $rscriptTotal = $rStopwatch.Elapsed.TotalSeconds
    $bgbLnL = $bgbRows[0].lnL
    $rustLnLNumber = [double]::Parse($rustRows[0].lnL, $culture)
    $bgbLnLNumber = [double]::Parse($bgbLnL, $culture)
    $lnLDelta = [Math]::Abs($rustLnLNumber - $bgbLnLNumber)
    $lnLRelativeDelta = $lnLDelta / [Math]::Max(1.0, [Math]::Abs($bgbLnLNumber))
    $lnLDeltaPerTip = $lnLDelta / $Tips
} else {
    $bgbMean = $null
    $bgbMedian = $null
    $speedupMean = $null
    $speedupMedian = $null
    $rscriptTotal = $null
    $bgbLnL = "NA"
    $lnLDelta = $null
    $lnLRelativeDelta = $null
    $lnLDeltaPerTip = $null
}

$summaryLines = [Collections.Generic.List[string]]::new()
$summaryLines.Add("metric`tvalue")
$summaryLines.Add("case_id`t$caseId")
$summaryLines.Add("areas`t$Areas")
$summaryLines.Add("tips`t$Tips")
$summaryLines.Add("max_range_size`t$MaxRangeSize")
$summaryLines.Add("include_null_range`t$IncludeNullRange")
$summaryLines.Add("states`t$stateCount")
$summaryLines.Add("d`t$(Format-Number $D)")
$summaryLines.Add("e`t$(Format-Number $E)")
$summaryLines.Add("mx01`t$(Format-Number $Mx01)")
$summaryLines.Add("rust_repeats`t$RustRepeats")
$summaryLines.Add("biogeobears_repeats`t$BioGeoBEARSRepeats")
$summaryLines.Add("rust_mean_seconds`t$(Format-Number $rustMean)")
$summaryLines.Add("rust_median_seconds`t$(Format-Number $rustMedian)")
$summaryLines.Add("biogeobears_warm_mean_seconds`t$(if ($null -eq $bgbMean) { 'NA' } else { Format-Number $bgbMean })")
$summaryLines.Add("biogeobears_warm_median_seconds`t$(if ($null -eq $bgbMedian) { 'NA' } else { Format-Number $bgbMedian })")
$summaryLines.Add("warm_mean_speedup_biogeobears_over_rust`t$(if ($null -eq $speedupMean) { 'NA' } else { Format-Number $speedupMean })")
$summaryLines.Add("warm_median_speedup_biogeobears_over_rust`t$(if ($null -eq $speedupMedian) { 'NA' } else { Format-Number $speedupMedian })")
$summaryLines.Add("biogeobears_rscript_total_seconds`t$(if ($null -eq $rscriptTotal) { 'NA' } else { Format-Number $rscriptTotal })")
$summaryLines.Add("rust_lnL`t$($rustRows[0].lnL)")
$summaryLines.Add("biogeobears_lnL`t$bgbLnL")
$summaryLines.Add("lnL_abs_delta`t$(if ($null -eq $lnLDelta) { 'NA' } else { $lnLDelta.ToString('G17', $culture) })")
$summaryLines.Add("lnL_relative_delta`t$(if ($null -eq $lnLRelativeDelta) { 'NA' } else { $lnLRelativeDelta.ToString('G17', $culture) })")
$summaryLines.Add("lnL_abs_delta_per_tip`t$(if ($null -eq $lnLDeltaPerTip) { 'NA' } else { $lnLDeltaPerTip.ToString('G17', $culture) })")
$summaryLines.Add("lnL_tolerance`t$($LikelihoodTolerance.ToString('G17', $culture))")
Set-Content -Path $summaryPath -Value $summaryLines -Encoding utf8

Write-Host ""
Write-Host "Summary written: $summaryPath"
Write-Host "Rust mean seconds: $(Format-Number $rustMean)"
if ($BioGeoBEARSRepeats -gt 0) {
    Write-Host "BioGeoBEARS warm-session mean seconds: $(Format-Number $bgbMean)"
    Write-Host "Warm-session speedup (BioGeoBEARS / Rust): $(Format-Number $speedupMean)x"
    Write-Host "lnL absolute delta: $($lnLDelta.ToString('G17', $culture))"
    if ($lnLDelta -gt $LikelihoodTolerance) {
        throw "lnL mismatch: absolute delta $lnLDelta exceeds tolerance $LikelihoodTolerance"
    }
}

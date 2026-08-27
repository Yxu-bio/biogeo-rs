[CmdletBinding()]
param(
    [string]$CliPath = "target/release/biogeo-cli.exe",
    [double]$DurationMinutes = 120,
    [int]$Cycles = 0,
    [int]$BsmSamples = 4096,
    [int]$BsmThreads = 0,
    [int]$MinimumFreeSpaceMb = 2048,
    [string]$OutputRoot = "",
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$culture = [Globalization.CultureInfo]::InvariantCulture
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Text
    )

    [IO.File]::WriteAllText($Path, $Text, [Text.UTF8Encoding]::new($false))
}

function Resolve-RepoPath {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [bool]$MustExist = $true
    )

    $resolved = if ([IO.Path]::IsPathRooted($Path)) {
        [IO.Path]::GetFullPath($Path)
    }
    else {
        [IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
    }
    if ($MustExist -and -not (Test-Path -LiteralPath $resolved)) {
        throw "Path does not exist: $resolved"
    }
    return $resolved
}

function Set-KeyValue {
    param(
        [Parameter(Mandatory = $true)][string]$Text,
        [Parameter(Mandatory = $true)][string]$Key,
        [Parameter(Mandatory = $true)][string]$Value
    )

    $pattern = "(?m)^$([regex]::Escape($Key))`t[^`r`n]*$"
    $matches = [regex]::Matches($Text, $pattern)
    if ($matches.Count -ne 1) {
        throw "Expected exactly one '$Key' row in the stability request."
    }
    return [regex]::Replace($Text, $pattern, "$Key`t$Value")
}

function Read-KeyValues {
    param([Parameter(Mandatory = $true)][object[]]$Lines)

    $values = @{}
    foreach ($line in $Lines) {
        $parts = ([string]$line) -split "`t", 2
        if ($parts.Count -eq 2 -and -not $values.ContainsKey($parts[0])) {
            $values[$parts[0]] = $parts[1]
        }
    }
    return $values
}

function Convert-ToProcessArgument {
    param([Parameter(Mandatory = $true)][string]$Value)

    if ($Value.Contains('"')) {
        throw "Process arguments must not contain a double quote."
    }
    if ($Value -match '\s') {
        return '"' + $Value + '"'
    }
    return $Value
}

function Invoke-MonitoredCli {
    param(
        [Parameter(Mandatory = $true)][string]$Cli,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath
    )

    $process = Start-Process `
        -FilePath $Cli `
        -ArgumentList @($Arguments | ForEach-Object { Convert-ToProcessArgument $_ }) `
        -RedirectStandardOutput $StdoutPath `
        -RedirectStandardError $StderrPath `
        -WindowStyle Hidden `
        -PassThru
    [uint64]$peakWorkingSet = 0
    while (-not $process.HasExited) {
        try {
            $process.Refresh()
            $peakWorkingSet = [Math]::Max($peakWorkingSet, [uint64]$process.PeakWorkingSet64)
        }
        catch {
            if (-not $process.HasExited) {
                throw
            }
        }
        Start-Sleep -Milliseconds 100
    }
    $process.WaitForExit()
    try {
        $process.Refresh()
        $peakWorkingSet = [Math]::Max($peakWorkingSet, [uint64]$process.PeakWorkingSet64)
    }
    catch {
        # The final sample above is best effort after the process handle has exited.
    }
    $capture = [PSCustomObject]@{
        ExitCode = [int]$process.ExitCode
        PeakWorkingSet = $peakWorkingSet
        Stdout = @([IO.File]::ReadAllLines($StdoutPath, [Text.Encoding]::UTF8))
        Stderr = @([IO.File]::ReadAllLines($StderrPath, [Text.Encoding]::UTF8))
    }
    Write-Output -NoEnumerate $capture
}

function Get-TreeFingerprint {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [string[]]$FileNames = @()
    )

    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/')
    $records = @(
        Get-ChildItem -LiteralPath $rootFull -Recurse -File |
            Where-Object { $FileNames.Count -eq 0 -or $_.Name -in $FileNames } |
            ForEach-Object {
                $relative = $_.FullName.Substring($rootFull.Length + 1).Replace('\', '/')
                $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
                "$relative`t$($_.Length)`t$hash"
            } |
            Sort-Object
    )
    if ($records.Count -eq 0) {
        throw "No files were found below $rootFull"
    }
    $bytes = [Text.Encoding]::UTF8.GetBytes(($records -join "`n"))
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha256.ComputeHash($bytes))).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $sha256.Dispose()
    }
}

function Get-MaxPeakWorkingSet {
    param([Parameter(Mandatory = $true)][string]$ResultRoot)

    $peaks = @(
        Get-ChildItem -LiteralPath $ResultRoot -Recurse -Filter metadata.tsv -File |
            ForEach-Object {
                foreach ($line in [IO.File]::ReadLines($_.FullName)) {
                    if ($line -match '^process_peak_working_set_bytes\t([0-9]+)$') {
                        [uint64]$Matches[1]
                    }
                }
            }
    )
    if ($peaks.Count -eq 0) {
        return [uint64]0
    }
    return [uint64](($peaks | Measure-Object -Maximum).Maximum)
}

function Remove-ValidatedCycle {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$WorkRoot
    )

    $pathFull = [IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
    $workFull = [IO.Path]::GetFullPath($WorkRoot).TrimEnd('\', '/')
    if ([IO.Path]::GetDirectoryName($pathFull) -ne $workFull -or
        [IO.Path]::GetFileName($pathFull) -notmatch '^cycle-[0-9]{6}$') {
        throw "Refusing to remove unvalidated stability-cycle path: $pathFull"
    }
    if (Test-Path -LiteralPath $pathFull) {
        [IO.Directory]::Delete($pathFull, $true)
    }
}

if ($env:OS -ne "Windows_NT") {
    throw "Windows PC stability validation requires Windows."
}
if ($Cycles -lt 0 -or ($Cycles -eq 0 -and $DurationMinutes -le 0)) {
    throw "Use a positive DurationMinutes or a positive Cycles value."
}
if ($BsmSamples -lt 1 -or $MinimumFreeSpaceMb -lt 1 -or $BsmThreads -lt 0) {
    throw "BsmSamples and MinimumFreeSpaceMb must be positive; BsmThreads must be zero or positive."
}
if ($BsmThreads -eq 0) {
    $BsmThreads = [Math]::Max(1, [Environment]::ProcessorCount)
}
$BsmThreads = [Math]::Min($BsmThreads, $BsmSamples)

if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $stamp = [DateTime]::UtcNow.ToString("yyyyMMddTHHmmssZ", $culture)
    $OutputRoot = Join-Path $repoRoot "validation\benchmark-runs\windows-pc-stability-$stamp-$PID"
}
$outputFull = Resolve-RepoPath -Path $OutputRoot -MustExist $false
if (Test-Path -LiteralPath $outputFull) {
    throw "Stability output already exists and will not be overwritten: $outputFull"
}

Push-Location $repoRoot
try {
    if (-not $SkipBuild) {
        & cargo build --release --locked -p biogeo-cli
        if ($LASTEXITCODE -ne 0) {
            throw "Release build failed with exit code $LASTEXITCODE"
        }
    }
    $cli = Resolve-RepoPath -Path $CliPath
    if (-not (Test-Path -LiteralPath $cli -PathType Leaf)) {
        throw "CLI executable does not exist: $cli"
    }
    $cliSha256 = (Get-FileHash -LiteralPath $cli -Algorithm SHA256).Hash.ToLowerInvariant()
    $cliVersionLines = @(& $cli --version)
    if ($LASTEXITCODE -ne 0 -or $cliVersionLines.Count -ne 1 -or
        [string]::IsNullOrWhiteSpace([string]$cliVersionLines[0])) {
        throw "Could not obtain one stable CLI version line."
    }
    $cliVersion = [string]$cliVersionLines[0]

    [IO.Directory]::CreateDirectory($outputFull) | Out-Null
    $inputRoot = Join-Path $outputFull "input"
    $workRoot = Join-Path $outputFull "work"
    $logRoot = Join-Path $outputFull "logs"
    [IO.Directory]::CreateDirectory($workRoot) | Out-Null
    [IO.Directory]::CreateDirectory($logRoot) | Out-Null
    Copy-Item `
        -LiteralPath (Join-Path $repoRoot "validation\fixtures\ponerinae_32tip_7area") `
        -Destination $inputRoot `
        -Recurse

    $requestPath = Join-Path $inputRoot "workflow-stability.tsv"
    $request = [IO.File]::ReadAllText((Join-Path $inputRoot "workflow-resume.tsv"))
    $shardSamples = [Math]::Min(128, $BsmSamples)
    $checkpointSamples = [Math]::Min(32, $shardSamples)
    $request = Set-KeyValue -Text $request -Key "bsm_samples" -Value ([string]$BsmSamples)
    $request = Set-KeyValue -Text $request -Key "bsm_output_level" -Value "compact"
    $request = Set-KeyValue -Text $request -Key "bsm_threads" -Value ([string]$BsmThreads)
    $request = Set-KeyValue -Text $request -Key "bsm_shard_samples" -Value ([string]$shardSamples)
    $request = Set-KeyValue -Text $request -Key "bsm_checkpoint_samples" -Value ([string]$checkpointSamples)
    $request = Set-KeyValue -Text $request -Key "bsm_time_limit_seconds" -Value "86400"
    Write-Utf8NoBom -Path $requestPath -Text $request
    $requestSha256 = (Get-FileHash -LiteralPath $requestPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $inputFingerprint = Get-TreeFingerprint -Root $inputRoot

    $cyclesPath = Join-Path $outputFull "cycles.tsv"
    Write-Utf8NoBom -Path $cyclesPath -Text (
        "cycle`tstarted_utc`telapsed_seconds`toutput_bytes`tpeak_working_set_bytes" +
        "`toptimization_fingerprint`tbsm_fingerprint`tcompleted_samples`tanagenetic_events`n"
    )
    $evidencePath = Join-Path $outputFull "evidence.tsv"
    $started = [DateTime]::UtcNow
    $stopwatch = [Diagnostics.Stopwatch]::StartNew()
    $cycle = 0
    $completedCycles = 0
    $baselineOptimization = $null
    $baselineBsm = $null
    $totalOutputBytes = [uint64]0
    $maxPeakWorkingSet = [uint64]0
    $failureMessage = "none"

    try {
        while ($cycle -eq 0 -or
            ($Cycles -gt 0 -and $cycle -lt $Cycles) -or
            ($Cycles -eq 0 -and $stopwatch.Elapsed.TotalMinutes -lt $DurationMinutes)) {
            $driveRoot = [IO.Path]::GetPathRoot($outputFull)
            $drive = [IO.DriveInfo]::new($driveRoot)
            $freeMb = [Math]::Floor($drive.AvailableFreeSpace / 1MB)
            if ($freeMb -lt $MinimumFreeSpaceMb) {
                throw "Free space fell below ${MinimumFreeSpaceMb} MiB before cycle $($cycle + 1)."
            }

            $cycle += 1
            $cycleStarted = [DateTime]::UtcNow
            $cycleRoot = Join-Path $workRoot ("cycle-{0:D6}" -f $cycle)
            $stdoutPath = Join-Path $logRoot ("cycle-{0:D6}.stdout.tsv" -f $cycle)
            $stderrPath = Join-Path $logRoot ("cycle-{0:D6}.stderr.txt" -f $cycle)
            $cycleWatch = [Diagnostics.Stopwatch]::StartNew()
            $capture = Invoke-MonitoredCli `
                -Cli $cli `
                -Arguments @(
                    "--error-format", "tsv",
                    "model-workflow",
                    "--request", $requestPath,
                    "--output-dir", $cycleRoot
                ) `
                -StdoutPath $stdoutPath `
                -StderrPath $stderrPath
            $stdout = $capture.Stdout
            $exitCode = $capture.ExitCode
            $cycleWatch.Stop()
            if ($exitCode -ne 0) {
                $diagnostics = @($capture.Stdout) + @($capture.Stderr)
                throw "model-workflow cycle $cycle failed with exit code ${exitCode}: $($diagnostics -join ' | ')"
            }
            $fields = Read-KeyValues -Lines $stdout
            foreach ($required in @(
                    @{ Name = "format"; Value = "biogeo-model-workflow-run-v1" },
                    @{ Name = "status"; Value = "complete" },
                    @{ Name = "candidate_models"; Value = "6" },
                    @{ Name = "bsm_status"; Value = "complete" },
                    @{ Name = "bsm_completed_samples"; Value = [string]$BsmSamples },
                    @{ Name = "bsm_validation"; Value = "deep" }
                )) {
                if (-not $fields.ContainsKey($required.Name) -or
                    [string]$fields[$required.Name] -ne [string]$required.Value) {
                    throw "Cycle $cycle returned an unexpected $($required.Name) value."
                }
            }

            $inspection = @(& $cli bsm-inspect --bsm-result (Join-Path $cycleRoot "bsm-result") --deep)
            if ($LASTEXITCODE -ne 0) {
                throw "Deep BSM inspection failed in cycle $cycle."
            }
            $inspectionFields = Read-KeyValues -Lines $inspection
            if ($inspectionFields["status"] -ne "valid" -or
                $inspectionFields["completed_samples"] -ne [string]$BsmSamples) {
                throw "Deep BSM inspection returned an unexpected result in cycle $cycle."
            }

            $optimizationFingerprint = Get-TreeFingerprint `
                -Root (Join-Path $cycleRoot "model-batch") `
                -FileNames @("comparison.tsv", "model-averaged-ancestral-ranges.tsv", "resolved-parameters.tsv")
            $bsmFingerprint = Get-TreeFingerprint `
                -Root (Join-Path $cycleRoot "bsm-result") `
                -FileNames @(
                    "node_states.tsv",
                    "cladogenetic_splits.tsv",
                    "branch_segments.tsv",
                    "sample_event_counts.tsv",
                    "sample_period_event_counts.tsv",
                    "sample_state_occupancy.tsv",
                    "sample_period_state_occupancy.tsv",
                    "anagenetic_events.tsv"
                )
            if ($null -eq $baselineOptimization) {
                $baselineOptimization = $optimizationFingerprint
                $baselineBsm = $bsmFingerprint
            }
            elseif ($optimizationFingerprint -ne $baselineOptimization -or
                $bsmFingerprint -ne $baselineBsm) {
                throw "Scientific output fingerprint changed in cycle $cycle."
            }

            $outputBytes = [uint64]((
                    Get-ChildItem -LiteralPath $cycleRoot -Recurse -File |
                        Measure-Object -Property Length -Sum
                ).Sum)
            $peakWorkingSet = [Math]::Max(
                [uint64]$capture.PeakWorkingSet,
                (Get-MaxPeakWorkingSet -ResultRoot $cycleRoot)
            )
            $totalOutputBytes += $outputBytes
            $maxPeakWorkingSet = [Math]::Max($maxPeakWorkingSet, $peakWorkingSet)
            $row = @(
                $cycle,
                $cycleStarted.ToString("o", $culture),
                $cycleWatch.Elapsed.TotalSeconds.ToString("R", $culture),
                $outputBytes,
                $peakWorkingSet,
                $optimizationFingerprint,
                $bsmFingerprint,
                $fields["bsm_completed_samples"],
                $fields["bsm_completed_anagenetic_events"]
            ) -join "`t"
            [IO.File]::AppendAllText($cyclesPath, "$row`n", [Text.UTF8Encoding]::new($false))
            $completedCycles += 1

            if ($cycle -gt 1) {
                Remove-ValidatedCycle -Path $cycleRoot -WorkRoot $workRoot
            }
        }
    }
    catch {
        $failureMessage = [Uri]::EscapeDataString($_.Exception.Message)
        throw
    }
    finally {
        $stopwatch.Stop()
        $status = if ($failureMessage -eq "none") { "passed" } else { "failed" }
        $cyclesSha256 = (Get-FileHash -LiteralPath $cyclesPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $retainedBaseline = if ($completedCycles -gt 0) { "work/cycle-000001" } else { "none" }
        $evidence = @(
            "key`tvalue",
            "format`tbiogeo-windows-pc-stability-v1",
            "status`t$status",
            "started_utc`t$($started.ToString('o', $culture))",
            "completed_utc`t$([DateTime]::UtcNow.ToString('o', $culture))",
            "cli_version`t$cliVersion",
            "cli_sha256`t$cliSha256",
            "input_fingerprint`t$inputFingerprint",
            "request_sha256`t$requestSha256",
            "process_visible_logical_processors`t$([Environment]::ProcessorCount)",
            "powershell_version`t$($PSVersionTable.PSVersion.ToString())",
            "requested_duration_minutes`t$($DurationMinutes.ToString('R', $culture))",
            "requested_cycles`t$Cycles",
            "completed_cycles`t$completedCycles",
            "elapsed_seconds`t$($stopwatch.Elapsed.TotalSeconds.ToString('R', $culture))",
            "candidate_models_per_cycle`t6",
            "bsm_samples_per_cycle`t$BsmSamples",
            "bsm_threads`t$BsmThreads",
            "total_bsm_samples`t$($completedCycles * $BsmSamples)",
            "total_output_bytes_before_cleanup`t$totalOutputBytes",
            "max_recorded_peak_working_set_bytes`t$maxPeakWorkingSet",
            "optimization_fingerprint`t$baselineOptimization",
            "bsm_fingerprint`t$baselineBsm",
            "failure_message`t$failureMessage",
            "cycles_file`tcycles.tsv",
            "cycles_sha256`t$cyclesSha256",
            "retained_baseline`t$retainedBaseline"
        ) -join "`n"
        Write-Utf8NoBom -Path $evidencePath -Text "$evidence`n"
    }

    Get-Content -LiteralPath $evidencePath | Select-Object -Skip 1
    Write-Output "output_root`t$outputFull"
}
finally {
    Pop-Location
}

param(
    [string]$Output = "validation/lagrange-ng-official-output.tsv",
    [string]$ScratchRoot = ""
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$outputPath = Join-Path $repoRoot $Output

function Test-AsciiPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    return $Path -cmatch '^[\x00-\x7F]+$'
}

if ([string]::IsNullOrWhiteSpace($ScratchRoot)) {
    if (-not [string]::IsNullOrWhiteSpace($env:BGB_LAGRANGE_SCRATCH)) {
        $ScratchRoot = $env:BGB_LAGRANGE_SCRATCH
    } elseif (-not [string]::IsNullOrWhiteSpace($env:TEMP) -and (Test-AsciiPath $env:TEMP)) {
        $ScratchRoot = $env:TEMP
    } elseif (Test-Path -LiteralPath "C:\Temp" -PathType Container) {
        $ScratchRoot = "C:\Temp"
    } else {
        throw "No ASCII scratch directory found. Set BGB_LAGRANGE_SCRATCH to an ASCII-only path."
    }
}

if (-not (Test-AsciiPath $ScratchRoot)) {
    throw "LAGRANGE-ng scratch path must be ASCII-only: $ScratchRoot"
}

$lagrangeExe = & (Join-Path $PSScriptRoot "find-lagrange-ng.ps1")
$sourceRoot = Join-Path $repoRoot "validation/r-cache/lagrange-ng-src"
if (-not (Test-Path -LiteralPath (Join-Path $sourceRoot ".git") -PathType Container)) {
    New-Item -ItemType Directory -Force -Path (Join-Path $repoRoot "validation/r-cache") | Out-Null
    git clone https://github.com/computations/lagrange-ng.git $sourceRoot
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to clone official LAGRANGE-ng source"
    }
}

$runId = Get-Date -Format "yyyyMMdd-HHmmss"
$runRoot = Join-Path $ScratchRoot "lagrange-ng-official-audit-$runId"
New-Item -ItemType Directory -Force -Path (Join-Path $runRoot "example") | Out-Null

Copy-Item -LiteralPath (Join-Path $sourceRoot "example/example.nwk") -Destination (Join-Path $runRoot "example/example.nwk") -Force
Copy-Item -LiteralPath (Join-Path $sourceRoot "example/example.phy") -Destination (Join-Path $runRoot "example/example.phy") -Force
Copy-Item -LiteralPath (Join-Path $sourceRoot "example/example.conf") -Destination (Join-Path $runRoot "repo-example.conf") -Force

@(
    "treefile = example/example.nwk",
    "datafile = example/example.phy",
    "areanames = RA RB RC RD RE",
    "states",
    "workers = 1",
    "threads-per-worker = 1",
    "prefix = readme-minimal"
) | Set-Content -LiteralPath (Join-Path $runRoot "readme-minimal.conf") -Encoding ASCII

@(
    "treefile = example/example.nwk",
    "datafile = example/example.phy",
    "areanames = RA RB RC RD RE",
    "states",
    "workers = 1",
    "threads-per-worker = 1",
    "mode = evaluate",
    "dispersion = 0.1",
    "extinction = 0.2",
    "prefix = eval-fixed"
) | Set-Content -LiteralPath (Join-Path $runRoot "eval-fixed.conf") -Encoding ASCII

@(
    "treefile = example/example.nwk",
    "datafile = example/example.phy",
    "areanames = RA RB RC RD RE",
    "states",
    "workers = 1",
    "threads-per-worker = 1",
    "mode = evaluate",
    "dispersion = 2.0",
    "extinction = 3.0",
    "prefix = eval-huge"
) | Set-Content -LiteralPath (Join-Path $runRoot "eval-huge.conf") -Encoding ASCII

function Match-Number {
    param(
        [Parameter(Mandatory = $true)][string]$Text,
        [Parameter(Mandatory = $true)][string]$Pattern
    )
    $match = [regex]::Match($Text, $Pattern)
    if ($match.Success) {
        return $match.Groups[1].Value
    }
    return ""
}

function Find-ResultJson {
    param(
        [Parameter(Mandatory = $true)][string]$Text,
        [Parameter(Mandatory = $true)][string]$RunRoot
    )
    $match = [regex]::Match($Text, "Writing results to\s+(.+\.results\.json)")
    if (-not $match.Success) {
        return $null
    }
    $rawPath = $match.Groups[1].Value.Trim()
    if ([System.IO.Path]::IsPathRooted($rawPath)) {
        return $rawPath
    }
    return Join-Path $RunRoot $rawPath
}

$cases = @(
    [pscustomobject]@{
        case_id = "readme_minimal_optimize"
        config = "readme-minimal.conf"
        requested_d = ""
        requested_e = ""
        readme_expected_initial = "-66.235818"
        readme_expected_final = "-31.424296"
    },
    [pscustomobject]@{
        case_id = "repo_example_optimize"
        config = "repo-example.conf"
        requested_d = ""
        requested_e = ""
        readme_expected_initial = ""
        readme_expected_final = ""
    },
    [pscustomobject]@{
        case_id = "readme_evaluate_d0.1_e0.2"
        config = "eval-fixed.conf"
        requested_d = "0.1"
        requested_e = "0.2"
        readme_expected_initial = ""
        readme_expected_final = ""
    },
    [pscustomobject]@{
        case_id = "readme_evaluate_d2_e3"
        config = "eval-huge.conf"
        requested_d = "2.0"
        requested_e = "3.0"
        readme_expected_initial = ""
        readme_expected_final = ""
    }
)

$rows = New-Object System.Collections.Generic.List[object]
Push-Location $runRoot
try {
    foreach ($case in $cases) {
        $logLines = & $lagrangeExe $case.config 2>&1
        $exitCode = $LASTEXITCODE
        $logPath = Join-Path $runRoot "$($case.config).stdout.txt"
        $logLines | Set-Content -LiteralPath $logPath -Encoding UTF8
        $text = $logLines -join "`n"

        $jsonPath = Find-ResultJson -Text $text -RunRoot $runRoot
        $jsonParams = $null
        if ($jsonPath -and (Test-Path -LiteralPath $jsonPath -PathType Leaf)) {
            $json = Get-Content -Raw -LiteralPath $jsonPath | ConvertFrom-Json
            $jsonParams = $json.params | Select-Object -First 1
        }

        $actualD = Match-Number -Text $text -Pattern "Period:\s*[^,]+,\s*Dispersion:\s*(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)"
        $actualE = Match-Number -Text $text -Pattern "Period:\s*[^,]+,\s*Dispersion:\s*-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?,\s*Extinction:\s*(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)"
        if ($actualD -eq "" -and $jsonParams) {
            $actualD = "{0:R}" -f [double]$jsonParams.dispersion
        }
        if ($actualE -eq "" -and $jsonParams) {
            $actualE = "{0:R}" -f [double]$jsonParams.extinction
        }

        $observedMode = if ($text -match "Final LLH:") {
            "optimize"
        } elseif ($text -match "LLH:") {
            "evaluate"
        } else {
            "unknown"
        }

        $parameterStatus = "not_applicable"
        if ($case.requested_d -ne "") {
            $rateDelta = [Math]::Max(
                [Math]::Abs(([double]$actualD) - ([double]$case.requested_d)),
                [Math]::Abs(([double]$actualE) - ([double]$case.requested_e))
            )
            if ($rateDelta -le 1e-12) {
                $parameterStatus = "requested_rates_used"
            } else {
                $parameterStatus = "requested_rates_ignored"
            }
        }

        $rows.Add([pscustomobject]@{
            case_id = $case.case_id
            exit_code = $exitCode
            observed_mode = $observedMode
            initial_llh = Match-Number -Text $text -Pattern "Initial LLH:\s*(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)"
            final_llh = Match-Number -Text $text -Pattern "Final LLH:\s*(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)"
            evaluate_llh = Match-Number -Text $text -Pattern "(?m)\bLLH:\s*(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)"
            requested_d = $case.requested_d
            requested_e = $case.requested_e
            actual_d = $actualD
            actual_e = $actualE
            parameter_status = $parameterStatus
            readme_expected_initial = $case.readme_expected_initial
            readme_expected_final = $case.readme_expected_final
            config = $case.config
            scratch_dir = $runRoot
        })
    }
}
finally {
    Pop-Location
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $outputPath) | Out-Null
$rows | Export-Csv -Path $outputPath -Delimiter "`t" -NoTypeInformation
Write-Host "Wrote $outputPath"

param(
    [string]$GoldenPath = "validation/golden/biogeobears-mx01r-audit.tsv"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$expectedPath = (Resolve-Path (Join-Path $repoRoot $GoldenPath)).Path
$tempPath = Join-Path ([System.IO.Path]::GetTempPath()) (
    "biogeobears-mx01r-" + [guid]::NewGuid().ToString("N") + ".tsv"
)

Push-Location $repoRoot
try {
    $rOutput = @(& Rscript validation/biogeobears/biogeobears-mx01r-audit.R $tempPath 2>&1)
    $rExitCode = $LASTEXITCODE
    if ($rExitCode -ne 0) {
        throw "BioGeoBEARS mx01r audit exited with code $rExitCode`n$($rOutput -join "`n")"
    }

    $expectedLength = (Get-Item -LiteralPath $expectedPath).Length
    $actualLength = (Get-Item -LiteralPath $tempPath).Length
    $expectedHash = (Get-FileHash -LiteralPath $expectedPath -Algorithm SHA256).Hash
    $actualHash = (Get-FileHash -LiteralPath $tempPath -Algorithm SHA256).Hash
    if ($expectedLength -ne $actualLength -or $expectedHash -ne $actualHash) {
        throw "BioGeoBEARS mx01r audit output differs from frozen golden: $GoldenPath"
    }

    $rows = Import-Csv -LiteralPath $tempPath -Delimiter "`t"
    if ($rows.Count -ne 6 -or ($rows | Where-Object exactly_unchanged -ne "TRUE")) {
        throw "BioGeoBEARS mx01r audit did not produce six unchanged rows"
    }
    Write-Host "BioGeoBEARS mx01r audit ok: 6 rows, all extracted deltas exactly zero"
}
finally {
    Pop-Location
    if (Test-Path -LiteralPath $tempPath) {
        Remove-Item -LiteralPath $tempPath -Force
    }
}
